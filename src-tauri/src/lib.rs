pub mod commands;
pub mod crypto;
pub mod db;
pub mod network;
pub mod os;
pub mod state;

use crate::db::storage::Database;
use crate::network::{
    mdns::start_mdns_daemon, signaling::start_signaling_daemon, transport::start_transport_listeners,
};
use crate::state::AppState;
use std::collections::HashMap;
use std::net::TcpListener as StdTcpListener;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tracing::info;

fn extract_send_paths(args: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut after_send = false;
    for arg in args {
        if after_send {
            if !arg.starts_with("--") {
                paths.push(arg.clone());
            }
        } else if arg == "--send" {
            after_send = true;
        }
    }
    paths
}

fn allocate_dynamic_port() -> u16 {
    StdTcpListener::bind("0.0.0.0:0")
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .unwrap_or(53317)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    let (db, identity, settings) =
        Database::init().expect("Failed to initialize Raydrop SQLite database");

    let local_ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let listen_port = allocate_dynamic_port();

    let initial_args: Vec<String> = std::env::args().collect();
    let initial_send_paths = extract_send_paths(&initial_args);

    let app_state = Arc::new(AppState {
        db: Arc::new(db),
        identity,
        settings: Mutex::new(settings),
        local_ip,
        listen_port,
        wan_reflexive_addr: Mutex::new(None),
        signaling_connected: AtomicBool::new(false),
        signaling_tx: Mutex::new(None),
        pending_requests: Mutex::new(HashMap::new()),
        active_cancellations: Mutex::new(HashMap::new()),
        pending_wan_candidates: Mutex::new(HashMap::new()),
        queued_cli_files: Mutex::new(initial_send_paths),
    });

    let state_for_single_instance = app_state.clone();
    let state_for_setup = app_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(move |app, argv, _cwd| {
            let paths = extract_send_paths(&argv);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            if !paths.is_empty() {
                info!("Received CLI --send paths from secondary instance: {:?}", paths);
                if let Ok(mut q) = state_for_single_instance.queued_cli_files.lock() {
                    q.extend(paths.clone());
                }
                let _ = app.emit("cli_send_files", paths);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .setup(move |app| {
            let handle = app.handle().clone();

            // Build System Tray Menu
            let is_disc = state_for_setup.is_discoverable();
            let toggle_label = if is_disc {
                "Status: Discoverable (ON)"
            } else {
                "Status: Hidden (OFF)"
            };
            let toggle_item =
                MenuItem::with_id(app, "toggle_disc", toggle_label, true, None::<&str>)?;
            let open_item =
                MenuItem::with_id(app, "open_dashboard", "Open Dashboard", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit Raydrop", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_item, &open_item, &quit_item])?;

            let state_tray = state_for_setup.clone();
            let toggle_item_clone = toggle_item.clone();

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Raydrop — Universal P2P File Transfer")
                .on_menu_event(move |app_handle, event| match event.id.as_ref() {
                    "toggle_disc" => {
                        let next = !state_tray.is_discoverable();
                        let _ = state_tray
                            .db
                            .set_setting("discoverable", if next { "true" } else { "false" });
                        if let Ok(mut s) = state_tray.settings.lock() {
                            s.discoverable = next;
                        }
                        let new_label = if next {
                            "Status: Discoverable (ON)"
                        } else {
                            "Status: Hidden (OFF)"
                        };
                        let _ = toggle_item_clone.set_text(new_label);
                        let _ = app_handle.emit("discoverability_changed", next);
                    }
                    "open_dashboard" => {
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app_handle = tray.app_handle();
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let _ = tray_builder.build(app)?;

            // Minimize to System Tray on window close instead of exiting background daemon
            if let Some(main_win) = app.get_webview_window("main") {
                let win_clone = main_win.clone();
                main_win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win_clone.hide();
                    }
                });
            }

            // Start Background Networking Daemons (TCP + QUIC + mDNS + WAN Signaling/STUN)
            start_transport_listeners(handle.clone(), state_for_setup.clone());
            start_mdns_daemon(handle.clone(), state_for_setup.clone());
            start_signaling_daemon(handle, state_for_setup);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::toggle_discoverability,
            commands::update_settings,
            commands::get_contacts,
            commands::add_wan_contact,
            commands::update_contact_trust,
            commands::initiate_transfer,
            commands::respond_transfer_request,
            commands::cancel_transfer,
            commands::get_transfer_history,
            commands::register_os_context_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Raydrop Tauri application");
}

