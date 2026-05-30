use miclaw_api_bridge_lib::error::BridgeError;
use miclaw_api_bridge_lib::server::{start_http, HttpServer, ServerConfig};
use miclaw_api_bridge_lib::state::BridgeState;
use parking_lot::Mutex;
use std::net::{IpAddr, Ipv4Addr};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

struct DesktopState {
    server: Mutex<Option<HttpServer>>,
    webui_url: String,
}

fn main() {
    miclaw_api_bridge_lib::init_tracing();
    tauri::Builder::default()
        .setup(|app| {
            let state = BridgeState::new()?;
            let port = state.storage.settings().proxy_port;
            let server = tauri::async_runtime::block_on(start_http(
                state,
                ServerConfig {
                    host: IpAddr::V4(Ipv4Addr::LOCALHOST),
                    port,
                },
            ))?;
            let webui_url = server.webui_url();

            app.manage(DesktopState {
                server: Mutex::new(Some(server)),
                webui_url: webui_url.clone(),
            });

            let open_item = MenuItem::with_id(app, "open_webui", "打开webui", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../../icons/icon.png"))?;

            TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("miclaw_api_bridge")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open_webui" => {
                        if let Some(state) = app.try_state::<DesktopState>() {
                            let _ = open::that(&state.webui_url);
                        }
                    }
                    "quit" => {
                        if let Some(state) = app.try_state::<DesktopState>() {
                            if let Some(server) = state.server.lock().take() {
                                server.shutdown();
                            }
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            open::that(&webui_url).map_err(BridgeError::other)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running desktop tray application");
}
