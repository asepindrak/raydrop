pub mod commands;
pub mod crypto;
pub mod db;
pub mod network;
pub mod os;
pub mod state;
pub mod tray_badge;

use crate::db::storage::Database;
use crate::network::{
    mdns::start_mdns_daemon, signaling::start_signaling_daemon, transport::start_transport_listeners,
};
use crate::state::AppState;
use std::collections::HashMap;
use std::net::TcpListener as StdTcpListener;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
#[cfg(desktop)]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
#[allow(unused_imports)]
use tauri::{Emitter, Manager, WindowEvent};
#[allow(unused_imports)]
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
    let start_minimized =
        initial_args.iter().any(|a| a == "--minimized") && initial_send_paths.is_empty();

    let app_state = Arc::new(AppState {
        db: Arc::new(db),
        identity,
        settings: Mutex::new(settings),
        local_ip,
        listen_port,
        wan_reflexive_addr: Mutex::new(None),
        signaling_connected: AtomicBool::new(false),
        signaling_tx: Mutex::new(None),
        quic_endpoint: Mutex::new(None),
        relay_streams: Mutex::new(HashMap::new()),
        pending_requests: Mutex::new(HashMap::new()),
        active_cancellations: Mutex::new(HashMap::new()),
        pending_wan_candidates: Mutex::new(HashMap::new()),
        queued_cli_files: Mutex::new(initial_send_paths.clone()),
        active_web_share: Mutex::new(None),
    });

    #[cfg(desktop)]
    let state_for_single_instance = app_state.clone();
    let state_for_setup = app_state.clone();

    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(move |app, argv, _cwd| {
        let paths = extract_send_paths(&argv);
        crate::network::transport::popup_and_focus_main_window(app);
        if !paths.is_empty() {
            info!("Received CLI --send paths from secondary instance: {:?}", paths);
            if let Ok(mut q) = state_for_single_instance.queued_cli_files.lock() {
                q.extend(paths.clone());
            }
            let _ = app.emit("cli_send_files", paths);
        }
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .setup(move |app| {
            let handle = app.handle().clone();

            #[cfg(desktop)]
            {
                // Build System Tray Menu
                let is_disc = state_for_setup.is_discoverable();
                let is_autostart = state_for_setup
                    .settings
                    .lock()
                    .map(|s| s.autostart)
                    .unwrap_or(true);
                let toggle_label = if is_disc {
                    "Status: Discoverable (ON)"
                } else {
                    "Status: Hidden (OFF)"
                };
                let autostart_label = if is_autostart {
                    "Auto-Start at Boot: ON"
                } else {
                    "Auto-Start at Boot: OFF"
                };
                let toggle_item =
                    MenuItem::with_id(app, "toggle_disc", toggle_label, true, None::<&str>)?;
                let autostart_item =
                    MenuItem::with_id(app, "toggle_autostart", autostart_label, true, None::<&str>)?;
                let open_item =
                    MenuItem::with_id(app, "open_dashboard", "Open Dashboard", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Quit Raydrop", true, None::<&str>)?;
                let menu = Menu::with_items(
                    app,
                    &[&toggle_item, &autostart_item, &open_item, &quit_item],
                )?;

                let state_tray = state_for_setup.clone();
                let toggle_item_clone = toggle_item.clone();
                let autostart_item_clone = autostart_item.clone();
                let state_menu = state_tray.clone();

                let mut tray_builder = TrayIconBuilder::with_id("main_tray")
                    .menu(&menu)
                    .tooltip("Raydrop — Universal P2P File Transfer")
                    .on_menu_event(move |app_handle, event| match event.id.as_ref() {
                        "toggle_disc" => {
                            let next = !state_menu.is_discoverable();
                            let _ = state_menu
                                .db
                                .set_setting("discoverable", if next { "true" } else { "false" });
                            if let Ok(mut s) = state_menu.settings.lock() {
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
                        "toggle_autostart" => {
                            let current = state_menu
                                .settings
                                .lock()
                                .map(|s| s.autostart)
                                .unwrap_or(true);
                            let next = !current;
                            let _ = crate::os::set_autostart(next);
                            let _ = state_menu
                                .db
                                .set_setting("autostart", if next { "true" } else { "false" });
                            if let Ok(mut s) = state_menu.settings.lock() {
                                s.autostart = next;
                            }
                            let new_label = if next {
                                "Auto-Start at Boot: ON"
                            } else {
                                "Auto-Start at Boot: OFF"
                            };
                            let _ = autostart_item_clone.set_text(new_label);
                            let _ = app_handle.emit("autostart_changed", next);
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

                if let Ok(tray_img) =
                    tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
                {
                    tray_builder = tray_builder.icon(tray_img);
                } else if let Some(icon) = app.default_window_icon() {
                    tray_builder = tray_builder.icon(icon.clone());
                }
                let _ = tray_builder.build(app)?;

                // Initialize System Tray badge with unread messages count on startup
                if let Ok(counts) = state_tray.db.get_unread_chat_counts() {
                    let total: u32 = counts.values().sum();
                    crate::tray_badge::update_tray_unread_badge(&app.handle(), total);
                }

                // Auto-register OS Right-Click Context Menu ("Send with Raydrop") on startup
                match crate::os::register_context_menu() {
                    Ok(msg) => info!("{msg}"),
                    Err(e) => info!("OS Context Menu registration skipped: {e}"),
                }

                // Ensure OS Autorun at Startup matches user preference (points to current_exe)
                if is_autostart {
                    match crate::os::set_autostart(true) {
                        Ok(msg) => info!("{msg}"),
                        Err(e) => info!("OS Autostart registration skipped: {e}"),
                    }
                }
            }

            // Show & focus main window on startup (unless launched with --minimized at boot)
            if let Some(main_win) = app.get_webview_window("main") {
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                if let Ok(win_img) =
                    tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png"))
                {
                    let _ = main_win.set_icon(win_img);
                }
                if start_minimized {
                    let _ = main_win.hide();
                } else {
                    let _ = main_win.show();
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    let _ = main_win.unminimize();
                    let _ = main_win.set_focus();
                    if !initial_send_paths.is_empty() {
                        crate::network::transport::popup_and_focus_main_window(&handle);
                    }
                }
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    let win_clone = main_win.clone();
                    main_win.on_window_event(move |event| {
                        if let WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            let _ = win_clone.hide();
                        }
                    });
                }
            }

            // Start Background Networking Daemons (TCP + QUIC + mDNS + WAN Signaling/STUN)
            start_transport_listeners(handle.clone(), state_for_setup.clone());
            start_mdns_daemon(handle.clone(), state_for_setup.clone());
            start_signaling_daemon(handle, state_for_setup);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::save_cloud_session,
            commands::logout_cloud_session,
            commands::sync_cloud_data_to_local,
            commands::delete_local_contact,
            commands::toggle_discoverability,
            commands::toggle_autostart,
            commands::update_settings,
            commands::get_contacts,
            commands::add_wan_contact,
            commands::update_contact_trust,
            commands::initiate_transfer,
            commands::resume_transfer,
            commands::pause_transfer,
            commands::respond_transfer_request,
            commands::cancel_transfer,
            commands::get_transfer_history,
            commands::register_os_context_menu,
            commands::open_file_path,
            commands::open_folder_path,
            commands::get_win11_direct_menu,
            commands::toggle_win11_direct_menu,
            commands::check_app_update,
            commands::download_and_install_update,
            commands::send_chat_message,
            commands::get_chat_messages,
            commands::mark_chat_messages_read,
            commands::clear_chat_messages,
            commands::get_unread_chat_counts,
            commands::update_tray_badge,
            commands::open_external_url,
            commands::get_file_thumbnail,
            commands::start_web_share,
            commands::stop_web_share,
            commands::read_system_clipboard,
            commands::write_system_clipboard,
            commands::broadcast_clipboard,
            commands::set_speed_limit,
            commands::list_folder_sync_tasks,
            commands::add_folder_sync_task,
            commands::toggle_folder_sync_auto,
            commands::delete_folder_sync_task,
            commands::trigger_folder_sync,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Raydrop Tauri application");
}

