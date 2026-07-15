mod claude;

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
    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let target = base.join(&rel_path);
    if !target.exists() {
        return Ok(());
    }
    let vault = base.join("vault");
    let canon_target = target.canonicalize().map_err(|e| e.to_string())?;
    let canon_vault = vault.canonicalize().map_err(|e| e.to_string())?;
    if !canon_target.starts_with(&canon_vault) {
        return Err("허용되지 않은 경로입니다.".into());
    }
    trash::delete(&canon_target).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![Migration {
        version: 1,
        description: "init_schema",
        sql: include_str!("../migrations/0001_init.sql"),
        kind: MigrationKind::Up,
    }];

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_sql::Builder::new()
                .add_migrations("sqlite:til.db", migrations)
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
            claude::claude_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
