mod ai;
mod detect;
mod report;

use ai::AiError;
use std::path::Path;
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
use tauri_plugin_sql::{Migration, MigrationKind};
use tokio::process::Command;
use tokio::time::timeout;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// appDataDir 기준 상대경로를 macOS 휴지통으로 이동(영구 삭제 대신 복구 가능).
/// vault 밖 경로는 거부한다. 대상이 없으면 멱등 성공.
/// `async` + `spawn_blocking` 이어야 한다 — Tauri v2 는 non-async 커맨드를 메인(이벤트 루프)
/// 스레드에서 돌리고, `trash` 의 macOS 기본 경로는 Finder 에 AppleEvent 를 보내 응답까지
/// 블로킹한다. 동기로 두면 노트 하나 지울 때마다 메인 창과 위젯이 함께 얼어붙는다
/// (첫 호출 때 뜨는 자동화 권한 다이얼로그 동안 특히 길다).
#[tauri::command]
async fn move_to_trash(app: tauri::AppHandle, rel_path: String) -> Result<(), AiError> {
    // rel_path 는 appdata 상대경로(기본 보관함) 또는 절대경로("폴더 열기"로 연 워크스페이스).
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string()))?;
    let home = app
        .path()
        .home_dir()
        .map_err(|e| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string()))?;

    tokio::task::spawn_blocking(move || {
        let p = std::path::PathBuf::from(&rel_path);
        let target = if p.is_absolute() { p } else { base.join(&p) };
        if !target.exists() {
            return Ok(());
        }
        let io = |e: std::io::Error| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string());
        let canon_target = target.canonicalize().map_err(io)?;
        let canon_base = base.canonicalize().map_err(io)?;
        let canon_home = home.canonicalize().map_err(io)?;
        // 허용: appdata 하위, 또는 홈 하위(홈 자체는 금지) — 시스템 경로 오삭제 방지
        let allowed = canon_target.starts_with(&canon_base)
            || (canon_target.starts_with(&canon_home) && canon_target != canon_home);
        if !allowed {
            return Err(AiError::new("TRASH_FORBIDDEN", "허용되지 않은 경로입니다."));
        }
        trash::delete(&canon_target)
            .map_err(|e| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string()))
    })
    .await
    .map_err(|e| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string()))?
}

/// 백업: 사용자가 고른 폴더 아래 `amber-backup-<로컬 타임스탬프>/` 를 만들고
/// DB 스냅샷(amber.db)과 vault/ 사본을 넣는다. 성공 시 만들어진 폴더의 절대경로를 돌려준다.
/// tz_offset_min 은 JS Date.getTimezoneOffset() 값(report.rs 와 같은 부호 규약).
#[tauri::command]
async fn create_backup(
    app: tauri::AppHandle,
    dest_dir: String,
    tz_offset_min: i32,
    // "폴더 열기"로 바꾼 작업 폴더들(섹션명, 절대경로). 기본 보관함이면 프론트가 넘기지 않는다.
    // 이걸 안 받으면 노트를 git 으로 관리하려고 폴더를 연 사용자가 노트 0개짜리 백업을 받는다.
    extra_roots: Option<Vec<(String, String)>>,
) -> Result<String, AiError> {
    // DB 는 sql 플러그인이 app_config_dir 기준으로 열고, vault 는 프론트가 appdata 기준으로 쓴다
    // (macOS 에선 같은 폴더지만 각자의 기준을 그대로 따른다).
    let db = app
        .path()
        .app_config_dir()
        .map_err(|e| AiError::new("BACKUP_PATH", e.to_string()))?
        .join("amber.db");
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AiError::new("BACKUP_PATH", e.to_string()))?;
    let vault = data_dir.join("vault");

    let dest = std::path::PathBuf::from(dest_dir.trim());
    if !dest.is_dir() {
        return Err(AiError::new("BACKUP_NO_DEST", "백업할 폴더를 찾을 수 없습니다."));
    }
    // 백업 대상이 앱 데이터 폴더 안이면 vault 를 자기 자신 안으로 무한 복사하게 된다
    if let (Ok(d), Ok(base)) = (dest.canonicalize(), data_dir.canonicalize()) {
        if d.starts_with(&base) {
            return Err(AiError::new(
                "BACKUP_INSIDE_APPDATA",
                "앱 데이터 폴더 밖의 위치를 선택해 주세요.",
            ));
        }
    }

    // 같은 폴더에 여러 번 백업해도 덮어쓰지 않게 하위 폴더로 나눈다
    // (VACUUM INTO 는 대상 파일이 이미 있으면 실패한다).
    let root = dest.join(format!("amber-backup-{}", local_stamp(tz_offset_min)));
    std::fs::create_dir_all(&root)
        .map_err(|e| AiError::detailed("BACKUP_MKDIR", e.to_string(), e.to_string()))?;

    let roots = extra_roots.unwrap_or_default();
    let write = async {
        if db.exists() {
            vacuum_into(&db, &root.join("amber.db")).await?;
        }
        if vault.is_dir() {
            let (from, to) = (vault.clone(), root.join("vault"));
            // 파일 복사는 블로킹이라 워커 스레드로 — 그동안 UI(IPC)가 멈추지 않게
            tokio::task::spawn_blocking(move || copy_dir(&from, &to))
                .await
                .map_err(|e| e.to_string())??;
        }
        // 커스텀 작업 폴더는 roots/<섹션>/ 아래로. 어디서 왔는지는 manifest.json 에 남긴다.
        let mut manifest = String::from("{\n  \"version\": 1,\n  \"roots\": {");
        for (i, (section, path)) in roots.iter().enumerate() {
            let src = std::path::PathBuf::from(path);
            if !src.is_dir() {
                continue;
            }
            // 백업 대상 폴더가 이 루트 안이면 자기 자신을 재귀 복사하게 된다
            if let (Ok(d), Ok(r)) = (dest.canonicalize(), src.canonicalize()) {
                if d.starts_with(&r) {
                    return Err(format!(
                        "백업 위치가 '{section}' 작업 폴더 안이라 복사할 수 없습니다."
                    ));
                }
            }
            let to = root.join("roots").join(section);
            tokio::task::spawn_blocking(move || copy_dir(&src, &to))
                .await
                .map_err(|e| e.to_string())??;
            manifest.push_str(&format!(
                "{}\n    \"{}\": \"{}\"",
                if i == 0 { "" } else { "," },
                section,
                path.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }
        manifest.push_str("\n  }\n}\n");
        if !roots.is_empty() {
            std::fs::write(root.join("manifest.json"), manifest).map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    };

    if let Err(e) = write.await {
        // 반쪽짜리 폴더를 남기면 온전한 백업으로 오인한다 — 방금 만든 폴더만 되돌린다
        let _ = std::fs::remove_dir_all(&root);
        // vacuum_into/copy_dir 의 사유는 그대로 detail 로 넘긴다(프론트가 코드로 문구를 만든다)
        return Err(AiError::detailed("BACKUP_WRITE", e.clone(), e));
    }
    Ok(root.to_string_lossy().into_owned())
}

/// DB 스냅샷. journal_mode 가 WAL 이라 amber.db 파일 복사는 최근 쓰기를 놓치므로,
/// 앱이 DB 를 연 채로도 일관된 사본을 만드는 `VACUUM INTO` 를 쓴다.
/// sqlx 를 직접 의존하지 않으므로 macOS 기본 제공 sqlite3 CLI 로 실행한다.
async fn vacuum_into(db: &Path, out: &Path) -> Result<(), String> {
    // 경로는 SQL 문자열 리터럴로 들어가므로 작은따옴표만 이스케이프(셸은 거치지 않는다)
    let sql = format!(
        "VACUUM INTO '{}'",
        out.to_string_lossy().replace('\'', "''")
    );
    let res = timeout(
        Duration::from_secs(120),
        Command::new("/usr/bin/sqlite3")
            .arg(db)
            .arg(&sql)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "DB 백업이 시간 안에 끝나지 않았습니다.".to_string())?;

    let output = res.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "시스템 sqlite3 를 찾을 수 없어 DB 를 백업하지 못했습니다.".to_string()
        } else {
            format!("sqlite3 실행 실패: {e}")
        }
    })?;
    if !output.status.success() {
        return Err(format!(
            "DB 백업 실패: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// 재귀 복사 — tauri-plugin-fs 에 재귀 복사가 없어 직접 구현.
/// 심볼릭 링크는 건너뛴다(백업본이 원본 밖을 가리키는 것 방지).
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let dst = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &dst)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 백업 폴더 이름용 로컬 타임스탬프(YYYYMMDD-HHMMSS). chrono 없이 epoch 초에서 직접 환산.
/// tz_offset_min = JS getTimezoneOffset()(UTC 기준 분, KST=-540) — report.rs fmt_hhmm 과 동일 산술.
fn local_stamp(tz_offset_min: i32) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        - i64::from(tz_offset_min) * 60;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // 3월을 연초로 두는 민력(civil) 역산 — 윤년 분기를 한 번에 처리한다
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = era * 400 + yoe + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![
        Migration {
            version: 1,
            description: "init_schema",
            sql: include_str!("../migrations/0001_init.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "add_todos",
            sql: include_str!("../migrations/0002_todos.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 3,
            description: "add_todo_sort_order",
            sql: include_str!("../migrations/0003_todo_order.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 4,
            description: "add_todo_parent",
            sql: include_str!("../migrations/0004_todo_parent.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 5,
            description: "add_daily_reports",
            sql: include_str!("../migrations/0005_daily_reports.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 6,
            description: "rename_claude_invocations_to_ai",
            sql: include_str!("../migrations/0006_rename_invocations.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 7,
            description: "add_time_blocks",
            sql: include_str!("../migrations/0007_time_blocks.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 8,
            description: "add_todo_carries",
            sql: include_str!("../migrations/0008_todo_carries.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 9,
            description: "add_todo_soft_delete",
            sql: include_str!("../migrations/0009_todo_soft_delete.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 10,
            description: "add_vacations",
            sql: include_str!("../migrations/0010_vacations.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 11,
            description: "add_weekly_reports",
            sql: include_str!("../migrations/0011_weekly_reports.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 12,
            description: "add_todo_scope",
            sql: include_str!("../migrations/0012_todo_scope.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 13,
            description: "week_start_sunday",
            sql: include_str!("../migrations/0013_week_start_sunday.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 14,
            description: "carry_snapshot",
            sql: include_str!("../migrations/0014_carry_snapshot.sql"),
            kind: MigrationKind::Up,
        },
    ];

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_sql::Builder::new()
                .add_migrations("sqlite:amber.db", migrations)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            // 창을 닫으면 종료 대신 숨김 — 앱은 트레이에 상주
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // 위젯은 모든 스페이스에 표시 — 데스크탑을 전환하거나 다른 앱이 전체화면이어도 사라지지 않는다
            // (PRD §8.1: JSON 키가 아니라 런타임 API). 메인 창에는 걸지 않는다.
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.set_visible_on_all_workspaces(true);
            }

            let handle = app.handle();
            let toggle_widget = MenuItem::with_id(
                handle,
                "toggle_widget",
                "위젯 열기/숨기기",
                true,
                None::<&str>,
            )?;
            let open_main =
                MenuItem::with_id(handle, "open_main", "메인 창 열기", true, None::<&str>)?;
            let quit = MenuItem::with_id(handle, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(handle, &[&toggle_widget, &open_main, &quit])?;

            let _tray = TrayIconBuilder::new()
                // 메뉴바(macOS)용 단색 template 아이콘 — 앱 컬러 아이콘과 별도.
                // icon_as_template=true 면 macOS 가 알파를 마스크로 라이트/다크 메뉴바에 맞춰 렌더한다.
                .icon(tauri::include_image!("icons/tray.png"))
                .icon_as_template(true)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "toggle_widget" => {
                        if let Some(w) = app.get_webview_window("widget") {
                            if w.is_visible().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                    "open_main" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        // app.exit 는 process::exit 라 Drop 이 돌지 않는다 → kill_on_drop 도 발화 안 함.
                        // 정리를 안 하면 `claude -p` 와 그것이 띄운 MCP 서버가 앱보다 오래 산다.
                        ai::kill_all();
                        app.exit(0)
                    }
                    _ => {}
                })
                .build(handle)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            move_to_trash,
            create_backup,
            ai::ai_generate,
            ai::ai_augment,
            ai::ai_note_compose,
            ai::ai_note_compose_stream,
            ai::ai_note_ask,
            ai::ai_erd_generate_stream,
            ai::ai_health,
            ai::ai_cancel,
            detect::detect_ai_clis,
            report::report_collect,
            report::report_generate,
            report::report_generate_weekly,
            report::detect_report_tools,
            report::report_mcp_servers,
            report::report_gh_accounts
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, event| {
            // ⌘Q·독 종료 등 트레이 메뉴를 거치지 않는 종료 경로에서도 자식 프로세스를 정리한다
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                ai::kill_all();
            }
        });
}
