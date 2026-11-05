use amber_core::ai::AiError;
use amber_core::{ai, auth, dbconn, detect, files, report, shellenv};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
use tauri_plugin_sql::{Migration, MigrationKind};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// appDataDir 기준 상대경로를 macOS 휴지통으로 이동(영구 삭제 대신 복구 가능). 규칙은 amber-core 의 files.rs.
/// `async` + `spawn_blocking` 이어야 한다 — Tauri v2 는 non-async 커맨드를 메인(이벤트 루프)
/// 스레드에서 돌리고, `trash` 의 macOS 기본 경로는 Finder 에 AppleEvent 를 보내 응답까지
/// 블로킹한다. 동기로 두면 노트 하나 지울 때마다 메인 창과 위젯이 함께 얼어붙는다
/// (첫 호출 때 뜨는 자동화 권한 다이얼로그 동안 특히 길다).
#[tauri::command]
async fn move_to_trash(app: tauri::AppHandle, rel_path: String) -> Result<(), AiError> {
    let failed = |e: tauri::Error| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string());
    let base = app.path().app_data_dir().map_err(failed)?;
    let home = app.path().home_dir().map_err(failed)?;
    tokio::task::spawn_blocking(move || files::move_to_trash(&base, &home, &rel_path))
        .await
        .map_err(|e| AiError::detailed("TRASH_FAILED", e.to_string(), e.to_string()))?
}

/// 백업: 사용자가 고른 폴더 아래 `amber-backup-<로컬 타임스탬프>/` 를 만들고
/// DB 스냅샷(amber.db)과 vault/ 사본을 넣는다(amber-core 의 files.rs). 성공 시 만들어진 폴더의 절대경로.
/// tz_offset_min 은 JS Date.getTimezoneOffset() 값(report.rs 와 같은 부호 규약).
#[tauri::command]
async fn create_backup(
    app: tauri::AppHandle,
    dest_dir: String,
    tz_offset_min: i32,
    extra_roots: Option<Vec<(String, String)>>,
) -> Result<String, AiError> {
    // DB 는 sql 플러그인이 app_config_dir 기준으로 열고, vault 는 프론트가 appdata 기준으로 쓴다
    // (macOS 에선 같은 폴더지만 각자의 기준을 그대로 따른다).
    let path = |e: tauri::Error| AiError::new("BACKUP_PATH", e.to_string());
    let db = app.path().app_config_dir().map_err(path)?.join("amber.db");
    let data_dir = app.path().app_data_dir().map_err(path)?;
    files::create_backup(db, data_dir, dest_dir, tz_offset_min, extra_roots).await
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
        Migration {
            version: 15,
            description: "add_db_connections",
            sql: include_str!("../migrations/0015_db_connections.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 16,
            description: "add_todo_parked",
            sql: include_str!("../migrations/0016_todo_parked.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 17,
            description: "add_db_credential_command",
            sql: include_str!("../migrations/0017_db_credential_command.sql"),
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
            // 사용자 셸 PATH 를 미리 읽어 둔다 — Dock 으로 뜬 앱도 터미널과 같은 곳에서 CLI 를 찾게(shellenv.rs).
            // 첫 CLI 호출이 셸 기동을 기다리지 않는다
            tauri::async_runtime::spawn(async {
                shellenv::path().await;
            });
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
            ai::ai_note_edit_span,
            ai::ai_note_ask,
            ai::ai_todo_suggest,
            ai::ai_erd_generate_stream,
            ai::ai_health,
            ai::ai_cancel,
            auth::ai_auth_status,
            auth::ai_auth_login,
            auth::ai_auth_code,
            auth::ai_auth_cancel,
            detect::detect_ai_clis,
            detect::codex_models,
            report::report_collect,
            report::report_generate,
            report::report_generate_weekly,
            report::detect_report_tools,
            report::report_mcp_servers,
            report::report_gh_accounts,
            report::report_check_github,
            dbconn::db_secret_set,
            dbconn::db_secret_delete,
            dbconn::db_test,
            dbconn::db_introspect
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
