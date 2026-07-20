mod claude;
mod detect;
mod report;

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

/// appDataDir 기준 상대경로를 macOS 휴지통으로 이동(영구 삭제 대신 복구 가능).
/// vault 밖 경로는 거부한다. 대상이 없으면 멱등 성공.
#[tauri::command]
fn move_to_trash(app: tauri::AppHandle, rel_path: String) -> Result<(), String> {
    // rel_path 는 appdata 상대경로(기본 보관함) 또는 절대경로("폴더 열기"로 연 워크스페이스).
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let p = std::path::PathBuf::from(&rel_path);
    let target = if p.is_absolute() { p } else { base.join(&p) };
    if !target.exists() {
        return Ok(());
    }
    let canon_target = target.canonicalize().map_err(|e| e.to_string())?;
    let canon_base = base.canonicalize().map_err(|e| e.to_string())?;
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let canon_home = home.canonicalize().map_err(|e| e.to_string())?;
    // 허용: appdata 하위, 또는 홈 하위(홈 자체는 금지) — 시스템 경로 오삭제 방지
    let allowed = canon_target.starts_with(&canon_base)
        || (canon_target.starts_with(&canon_home) && canon_target != canon_home);
    if !allowed {
        return Err("허용되지 않은 경로입니다.".into());
    }
    trash::delete(&canon_target).map_err(|e| e.to_string())?;
    Ok(())
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
            // 구 프로젝트명(til) 잔재 정리: til.db → amber.db 로 이전.
            // DB 는 프론트의 Database.load 시점에 열리므로, 그 전(setup)에 파일명을 바꿔 데이터를 보존한다.
            // amber.db 가 아직 없을 때만(최초 1회), WAL/SHM 까지 함께 옮긴다.
            if let Ok(dir) = app.path().app_config_dir() {
                if dir.join("til.db").exists() && !dir.join("amber.db").exists() {
                    for (old, new) in [
                        ("til.db", "amber.db"),
                        ("til.db-wal", "amber.db-wal"),
                        ("til.db-shm", "amber.db-shm"),
                    ] {
                        let from = dir.join(old);
                        if from.exists() {
                            let _ = std::fs::rename(&from, dir.join(new));
                        }
                    }
                }
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
                .icon(app.default_window_icon().unwrap().clone())
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
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(handle)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            move_to_trash,
            claude::claude_generate,
            claude::claude_augment,
            claude::claude_note_compose,
            claude::claude_note_compose_stream,
            claude::claude_note_ask,
            claude::claude_health,
            detect::detect_ai_clis,
            report::report_collect,
            report::report_generate,
            report::detect_report_tools
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
