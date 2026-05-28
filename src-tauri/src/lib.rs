//! miclaw_api_bridge: bridge Xiaomi mimo into local OpenAI/Claude compatible APIs.

pub mod auth;
pub mod commands;
pub mod error;
pub mod mimo;
pub mod proxy;
pub mod state;
pub mod storage;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,miclaw_api_bridge_lib=debug")
            }),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let state = Arc::new(AppState::new(app.handle().clone())?);
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth_status,
            commands::login,
            commands::send_two_factor_ticket,
            commands::verify_two_factor,
            commands::refresh_session,
            commands::logout,
            commands::proxy_status,
            commands::start_proxy,
            commands::stop_proxy,
            commands::set_proxy_port,
            commands::list_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
