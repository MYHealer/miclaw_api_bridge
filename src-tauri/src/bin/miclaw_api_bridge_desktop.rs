#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use miclaw_api_bridge_lib::error::{BridgeError, Result};
use miclaw_api_bridge_lib::server::{start_http, HttpServer, ServerConfig};
use miclaw_api_bridge_lib::state::BridgeState;
use std::net::{IpAddr, Ipv4Addr};

#[cfg(not(target_os = "linux"))]
enum UserEvent {
    Menu(tray_icon::menu::MenuEvent),
}

fn main() {
    miclaw_api_bridge_lib::init_tracing();
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn start_runtime_and_server() -> Result<(tokio::runtime::Runtime, HttpServer, String)> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(BridgeError::other)?;
    let state = BridgeState::new()?;
    let port = state.storage.settings().proxy_port;
    let server = runtime.block_on(start_http(
        state,
        ServerConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
        },
    ))?;
    let webui_url = server.webui_url();
    Ok((runtime, server, webui_url))
}

#[cfg(target_os = "linux")]
fn run() -> Result<()> {
    use ksni::menu::StandardItem;
    use std::sync::mpsc;

    enum Command {
        OpenWebui,
        Quit,
    }

    struct LinuxTray {
        tx: mpsc::Sender<Command>,
        icon: Vec<ksni::Icon>,
    }

    impl ksni::Tray for LinuxTray {
        fn id(&self) -> String {
            "miclaw_api_bridge".into()
        }

        fn title(&self) -> String {
            "miclaw_api_bridge".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon.clone()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "miclaw_api_bridge".into(),
                description: "Local mimo API bridge".into(),
                icon_pixmap: self.icon.clone(),
                ..Default::default()
            }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.tx.send(Command::OpenWebui);
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            let open_tx = self.tx.clone();
            let quit_tx = self.tx.clone();
            vec![
                StandardItem {
                    label: "打开webui".into(),
                    activate: Box::new(move |_| {
                        let _ = open_tx.send(Command::OpenWebui);
                    }),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "退出".into(),
                    icon_name: "application-exit".into(),
                    activate: Box::new(move |_| {
                        let _ = quit_tx.send(Command::Quit);
                    }),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    let (_runtime, server, webui_url) = start_runtime_and_server()?;
    let (tx, rx) = mpsc::channel();
    let service = ksni::TrayService::new(LinuxTray {
        tx,
        icon: load_ksni_icons()?,
    });
    let handle = service.handle();
    std::thread::spawn(move || {
        if let Err(e) = service.run() {
            tracing::error!(target = "desktop", "linux tray service failed: {e}");
        }
    });

    open::that(&webui_url).map_err(BridgeError::other)?;

    let mut server = Some(server);
    while let Ok(command) = rx.recv() {
        match command {
            Command::OpenWebui => {
                let _ = open::that(&webui_url);
            }
            Command::Quit => {
                handle.shutdown();
                if let Some(server) = server.take() {
                    server.shutdown();
                }
                break;
            }
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn run() -> Result<()> {
    use tao::event::Event;
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::TrayIconBuilder;

    struct DesktopRuntime {
        _tokio: tokio::runtime::Runtime,
        server: Option<HttpServer>,
        webui_url: String,
        _tray: tray_icon::TrayIcon,
    }

    impl DesktopRuntime {
        fn shutdown(&mut self) {
            if let Some(server) = self.server.take() {
                server.shutdown();
            }
        }
    }

    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    configure_platform_event_loop(&mut event_loop);

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

    let (runtime, server, webui_url) = start_runtime_and_server()?;
    let open_item = MenuItem::with_id("open_webui", "打开webui", true, None);
    let quit_item = MenuItem::with_id("quit", "退出", true, None);
    let tray_menu = Menu::with_items(&[&open_item, &quit_item]).map_err(BridgeError::other)?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("miclaw_api_bridge")
        .with_menu(Box::new(tray_menu))
        .with_icon(load_tray_icon()?)
        .with_menu_on_left_click(true)
        .build()
        .map_err(BridgeError::other)?;

    open::that(&webui_url).map_err(BridgeError::other)?;

    let mut desktop = DesktopRuntime {
        _tokio: runtime,
        server: Some(server),
        webui_url,
        _tray: tray,
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::UserEvent(UserEvent::Menu(event)) = event {
            match event.id().as_ref() {
                "open_webui" => {
                    let _ = open::that(&desktop.webui_url);
                }
                "quit" => {
                    desktop.shutdown();
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            }
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn load_tray_icon() -> Result<tray_icon::Icon> {
    let image = image::load_from_memory(include_bytes!("../../icons/icon.png"))
        .map_err(BridgeError::other)?
        .into_rgba8();
    let (width, height) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), width, height).map_err(BridgeError::other)
}

#[cfg(target_os = "linux")]
fn load_ksni_icons() -> Result<Vec<ksni::Icon>> {
    let image = image::load_from_memory(include_bytes!("../../icons/icon.png"))
        .map_err(BridgeError::other)?
        .resize(64, 64, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let (width, height) = image.dimensions();
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for pixel in image.pixels() {
        let [r, g, b, a] = pixel.0;
        data.extend_from_slice(&[a, r, g, b]);
    }
    Ok(vec![ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    }])
}

#[cfg(target_os = "macos")]
fn configure_platform_event_loop(event_loop: &mut tao::event_loop::EventLoop<UserEvent>) {
    use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};

    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    event_loop.set_dock_visibility(false);
}

#[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
fn configure_platform_event_loop(_event_loop: &mut tao::event_loop::EventLoop<UserEvent>) {}
