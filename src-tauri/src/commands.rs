use crate::db::storage::{now_unix_secs, AppSettings, ChatMessage, Contact, TransferRecord};
use crate::network::transport::{initiate_peer_transfer, resume_peer_transfer};
use crate::os::{register_context_menu, set_autostart};
use crate::state::{AppState, IncomingRequestDecision};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{atomic::Ordering, Arc};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
pub struct AppStateSnapshot {
    pub settings: AppSettings,
    pub ed25519_pubkey: String,
    pub os_type: String,
    pub local_ip: String,
    pub listen_port: u16,
    pub wan_reflexive_addr: Option<String>,
    pub signaling_connected: bool,
    pub queued_cli_files: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CloudDeviceItem {
    pub device_id: String,
    pub user_email: String,
    pub user_name: String,
    pub device_name: String,
    pub os_type: String,
    pub ed25519_pubkey: String,
    pub last_ip: Option<String>,
    pub is_online: bool,
}

#[derive(Debug, Deserialize)]
pub struct CloudContactItem {
    pub email: String,
    pub name: String,
    pub trust_level: String,
    pub devices: Vec<CloudDeviceItem>,
}

#[tauri::command]
pub fn get_app_state(state: State<'_, Arc<AppState>>) -> Result<AppStateSnapshot, String> {
    let settings = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let wan_reflexive_addr = state
        .wan_reflexive_addr
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let mut queued = state
        .queued_cli_files
        .lock()
        .map_err(|e| e.to_string())?;
    let queued_cli_files = std::mem::take(&mut *queued);

    Ok(AppStateSnapshot {
        settings,
        ed25519_pubkey: state.identity.public_hex(),
        os_type: std::env::consts::OS.to_string(),
        local_ip: state.local_ip.clone(),
        listen_port: state.listen_port,
        wan_reflexive_addr,
        signaling_connected: state.is_signaling_connected(),
        queued_cli_files,
    })
}

#[tauri::command]
pub fn save_cloud_session(
    state: State<'_, Arc<AppState>>,
    user_id: String,
    user_email: String,
    user_name: String,
    auth_token: String,
    device_name: Option<String>,
) -> Result<AppSettings, String> {
    state.db.set_setting("user_id", &user_id)?;
    state.db.set_setting("user_email", &user_email)?;
    state.db.set_setting("user_name", &user_name)?;
    state.db.set_setting("auth_token", &auth_token)?;

    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.user_id = Some(user_id);
    s.user_email = Some(user_email);
    s.user_name = Some(user_name);
    s.auth_token = Some(auth_token.clone());

    if let Some(dn) = device_name {
        let trimmed = dn.trim();
        if !trimmed.is_empty() {
            state.db.set_setting("alias_name", trimmed)?;
            s.alias_name = trimmed.to_string();
        }
    }

    // Re-send authenticated REGISTER packet over active signaling WebSocket
    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let ts = now_unix_secs();
        let sign_payload = format!("RAYDROP_REGISTER:{}:{ts}", state.identity.device_id);
        let sig = state.identity.sign_message(sign_payload.as_bytes());
        let reg_msg = json!({
            "type": "REGISTER",
            "device_id": state.identity.device_id,
            "alias_name": s.alias_name,
            "ed25519_pubkey": state.identity.public_hex(),
            "timestamp": ts,
            "signature": sig,
            "auth_token": auth_token
        })
        .to_string();
        let _ = tx.send(reg_msg);
    }

    Ok(s.clone())
}

#[tauri::command]
pub fn logout_cloud_session(state: State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    state.db.set_setting("user_id", "")?;
    state.db.set_setting("user_email", "")?;
    state.db.set_setting("user_name", "")?;
    state.db.set_setting("auth_token", "")?;

    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.user_id = None;
    s.user_email = None;
    s.user_name = None;
    s.auth_token = None;
    Ok(s.clone())
}

#[tauri::command]
pub fn sync_cloud_data_to_local(
    state: State<'_, Arc<AppState>>,
    my_devices: Vec<CloudDeviceItem>,
    contacts: Vec<CloudContactItem>,
) -> Result<Vec<Contact>, String> {
    let now = now_unix_secs();

    // 1. Upsert all other devices belonging to the user's own account (My Devices / Tailnet)
    for dev in my_devices {
        if dev.device_id == state.identity.device_id {
            continue;
        }
        let existing = state.db.get_contact(&dev.device_id).ok().flatten();
        let net_type = existing
            .as_ref()
            .map(|c| c.network_type.clone())
            .unwrap_or_else(|| "wan".to_string());
        let last_port = existing.as_ref().and_then(|c| c.last_known_port);
        let last_ip = existing
            .as_ref()
            .and_then(|c| c.last_known_ip.clone())
            .or(dev.last_ip);

        let c = Contact {
            device_id: dev.device_id,
            alias_name: dev.device_name,
            user_email: Some(dev.user_email),
            user_name: Some(dev.user_name),
            os_type: Some(dev.os_type),
            is_own_device: true,
            ed25519_pubkey: Some(dev.ed25519_pubkey),
            network_type: net_type,
            last_known_ip: last_ip,
            last_known_port: last_port,
            trust_level: "trusted_auto_accept".to_string(),
            is_online: dev.is_online,
            last_seen_at: now,
            created_at: now,
        };
        let _ = state.db.upsert_contact(&c);
    }

    // 2. Upsert all devices belonging to saved Cloud Contacts
    for contact_group in contacts {
        for dev in contact_group.devices {
            if dev.device_id == state.identity.device_id {
                continue;
            }
            let existing = state.db.get_contact(&dev.device_id).ok().flatten();
            let net_type = existing
                .as_ref()
                .map(|c| c.network_type.clone())
                .unwrap_or_else(|| "wan".to_string());
            let last_port = existing.as_ref().and_then(|c| c.last_known_port);
            let last_ip = existing
                .as_ref()
                .and_then(|c| c.last_known_ip.clone())
                .or(dev.last_ip);

            let c = Contact {
                device_id: dev.device_id,
                alias_name: dev.device_name,
                user_email: Some(contact_group.email.clone()),
                user_name: Some(contact_group.name.clone()),
                os_type: Some(dev.os_type),
                is_own_device: false,
                ed25519_pubkey: Some(dev.ed25519_pubkey),
                network_type: net_type,
                last_known_ip: last_ip,
                last_known_port: last_port,
                trust_level: contact_group.trust_level.clone(),
                is_online: dev.is_online,
                last_seen_at: now,
                created_at: now,
            };
            let _ = state.db.upsert_contact(&c);
        }
    }

    state.db.list_contacts()
}

#[tauri::command]
pub fn delete_local_contact(
    state: State<'_, Arc<AppState>>,
    device_id: Option<String>,
    email: Option<String>,
) -> Result<(), String> {
    if let Some(did) = device_id {
        state.db.delete_contact_by_device_id(&did)?;
    }
    if let Some(em) = email {
        state.db.delete_contacts_by_email(&em)?;
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_discoverability(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    state
        .db
        .set_setting("discoverable", if enabled { "true" } else { "false" })?;
    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.discoverable = enabled;
    Ok(enabled)
}

#[tauri::command]
pub fn toggle_autostart(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    let _ = set_autostart(enabled)?;
    state
        .db
        .set_setting("autostart", if enabled { "true" } else { "false" })?;
    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.autostart = enabled;
    Ok(enabled)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, Arc<AppState>>,
    alias_name: String,
    download_dir: String,
    signaling_url: String,
    stun_addr: String,
    autostart: Option<bool>,
    speed_limit_mbps: Option<u32>,
) -> Result<AppSettings, String> {
    let resolved_stun = if stun_addr.trim().is_empty() {
        let host = signaling_url
            .trim()
            .trim_start_matches("wss://")
            .trim_start_matches("ws://")
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("stun.raydrop.web.id")
            .split(':')
            .next()
            .unwrap_or("stun.raydrop.web.id");
        format!("{host}:3478")
    } else {
        stun_addr.trim().to_string()
    };

    state.db.set_setting("alias_name", &alias_name)?;
    state.db.set_setting("download_dir", &download_dir)?;
    state.db.set_setting("signaling_url", signaling_url.trim())?;
    state.db.set_setting("stun_addr", &resolved_stun)?;

    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.alias_name = alias_name;
    s.download_dir = download_dir;
    s.signaling_url = signaling_url.trim().to_string();
    s.stun_addr = resolved_stun;

    if let Some(limit) = speed_limit_mbps {
        state.db.set_setting("speed_limit_mbps", &limit.to_string())?;
        s.speed_limit_mbps = limit;
    }

    if let Some(auto) = autostart {
        let _ = set_autostart(auto);
        state
            .db
            .set_setting("autostart", if auto { "true" } else { "false" })?;
        s.autostart = auto;
    }

    Ok(s.clone())
}

#[tauri::command]
pub fn set_speed_limit(state: State<'_, Arc<AppState>>, speed_limit_mbps: u32) -> Result<u32, String> {
    state.db.set_setting("speed_limit_mbps", &speed_limit_mbps.to_string())?;
    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.speed_limit_mbps = speed_limit_mbps;
    Ok(speed_limit_mbps)
}

#[tauri::command]
pub fn get_contacts(state: State<'_, Arc<AppState>>) -> Result<Vec<Contact>, String> {
    state.db.list_contacts()
}

#[tauri::command]
pub fn add_wan_contact(
    state: State<'_, Arc<AppState>>,
    device_id: String,
    alias_name: String,
    direct_ip_port: Option<String>,
) -> Result<Contact, String> {
    let clean_id = device_id.trim().to_uppercase();
    if !clean_id.starts_with("RAY-") {
        return Err("Device ID must start with RAY-XXXX-XXXX-XXXX".to_string());
    }

    let (last_known_ip, last_known_port) = if let Some(addr_str) = direct_ip_port {
        let trimmed = addr_str.trim();
        if let Some((ip, port_s)) = trimmed.rsplit_once(':') {
            (Some(ip.to_string()), port_s.parse::<u16>().ok())
        } else if !trimmed.is_empty() {
            (Some(trimmed.to_string()), None)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let contact = Contact {
        device_id: clean_id.clone(),
        alias_name: alias_name.trim().to_string(),
        user_email: None,
        user_name: None,
        os_type: None,
        is_own_device: false,
        ed25519_pubkey: None,
        network_type: "wan".to_string(),
        last_known_ip,
        last_known_port,
        trust_level: "ask".to_string(),
        is_online: false,
        last_seen_at: now_unix_secs(),
        created_at: now_unix_secs(),
    };
    state.db.upsert_contact(&contact)?;

    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let lookup = json!({
            "type": "PEER_LOOKUP",
            "target_device_id": clean_id
        })
        .to_string();
        let _ = tx.send(lookup);
    }

    Ok(contact)
}

#[tauri::command]
pub fn update_contact_trust(
    state: State<'_, Arc<AppState>>,
    device_id: String,
    trust_level: String,
    user_email: Option<String>,
) -> Result<(), String> {
    if let Some(email) = user_email {
        if !email.trim().is_empty() {
            state.db.update_contact_trust_by_email(&email, &trust_level)?;
            return Ok(());
        }
    }
    state.db.update_contact_trust(&device_id, &trust_level)
}

#[tauri::command]
pub async fn initiate_transfer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    target_device_id: String,
    file_paths: Vec<String>,
) -> Result<String, String> {
    initiate_peer_transfer(app, state.inner().clone(), target_device_id, file_paths).await
}

#[tauri::command]
pub async fn resume_transfer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<String, String> {
    resume_peer_transfer(app, state.inner().clone(), session_id).await
}

#[tauri::command]
pub fn pause_transfer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    if let Some(flag) = state
        .active_cancellations
        .lock()
        .map_err(|e| e.to_string())?
        .get(&session_id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    if let Ok(Some(mut rec)) = state.db.get_transfer(&session_id) {
        rec.status = "paused".to_string();
        let _ = state.db.upsert_transfer(&rec);
        let _ = app.emit("transfer_state_changed", &rec);
    }
    Ok(())
}

#[tauri::command]
pub fn respond_transfer_request(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    accepted: bool,
    target_directory: Option<String>,
) -> Result<(), String> {
    if let Some(sender) = state
        .pending_requests
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&session_id)
    {
        let _ = sender.send(IncomingRequestDecision {
            accepted,
            target_directory,
        });
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_transfer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    if let Some(flag) = state
        .active_cancellations
        .lock()
        .map_err(|e| e.to_string())?
        .get(&session_id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    if let Ok(Some(mut rec)) = state.db.get_transfer(&session_id) {
        rec.status = "cancelled".to_string();
        let _ = state.db.upsert_transfer(&rec);
        let _ = app.emit("transfer_state_changed", &rec);
    }
    Ok(())
}

#[tauri::command]
pub fn get_transfer_history(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<TransferRecord>, String> {
    state.db.list_transfers()
}

#[tauri::command]
pub fn register_os_context_menu() -> Result<String, String> {
    register_context_menu()
}

#[tauri::command]
pub fn get_win11_direct_menu() -> bool {
    crate::os::get_win11_direct_menu_status()
}

#[tauri::command]
pub fn toggle_win11_direct_menu(enabled: bool) -> Result<bool, String> {
    crate::os::set_win11_direct_menu(enabled)
}

#[cfg(target_os = "windows")]
mod win_shell {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::process::CommandExt;
    use std::path::Path;
    use std::ptr;

    #[link(name = "shell32")]
    extern "system" {
        fn ILCreateFromPathW(psz_path: *const u16) -> *mut std::ffi::c_void;
        fn ILFree(pidl: *mut std::ffi::c_void);
        fn SHOpenFolderAndSelectItems(
            pidl_folder: *const std::ffi::c_void,
            cidl: u32,
            apidl: *const *const std::ffi::c_void,
            dw_flags: u32,
        ) -> i32;
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            lpOperation: *const u16,
            lpFile: *const u16,
            lpParameters: *const u16,
            lpDirectory: *const u16,
            nShowCmd: i32,
        ) -> usize;
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pv_reserved: *mut std::ffi::c_void, dw_co_init: u32) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn AllowSetForegroundWindow(dw_process_id: u32) -> i32;
    }

    pub fn allow_foreground() {
        unsafe {
            let _ = AllowSetForegroundWindow(0xFFFF_FFFF);
        }
    }

    pub fn open_file_with_default_app(path: &Path) -> Result<(), String> {
        let clean_str = path
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', r"\");
        let wide: Vec<u16> = OsStr::new(&clean_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let open_op: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let _ = AllowSetForegroundWindow(0xFFFF_FFFF);
            let res = ShellExecuteW(
                ptr::null_mut(),
                open_op.as_ptr(),
                wide.as_ptr(),
                ptr::null(),
                ptr::null(),
                1, // SW_SHOWNORMAL
            );
            if res > 32 {
                return Ok(());
            }
        }

        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &clean_str])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
        Ok(())
    }

    pub fn reveal_and_select_file(path: &Path) -> Result<(), String> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let clean_str = canonical
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', r"\");
        let wide: Vec<u16> = OsStr::new(&clean_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let _ = AllowSetForegroundWindow(0xFFFF_FFFF);
            let _ = CoInitializeEx(ptr::null_mut(), 0x2); // COINIT_APARTMENTTHREADED
            let pidl = ILCreateFromPathW(wide.as_ptr());
            if !pidl.is_null() {
                let hr = SHOpenFolderAndSelectItems(pidl, 0, ptr::null(), 0);
                ILFree(pidl);
                if hr >= 0 {
                    return Ok(());
                }
            }
        }

        // Fallback to explorer.exe /select,"<path>"
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", clean_str))
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
        Ok(())
    }
}

#[tauri::command]
pub fn open_file_path(app: AppHandle, path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("File or folder not found: {path}"));
    }

    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(false);
    }

    #[cfg(target_os = "windows")]
    {
        win_shell::open_file_with_default_app(&p)?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&p)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&p)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
pub fn open_folder_path(app: AppHandle, path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);

    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(false);
    }

    // If the file is still downloading/paused as `.raypart`, select the `.raypart` file in Explorer!
    let target_item = if p.exists() {
        Some(p.clone())
    } else {
        let part_candidate = p.with_file_name(format!(
            "{}.raypart",
            p.file_name().and_then(|n| n.to_str()).unwrap_or("file")
        ));
        if part_candidate.exists() {
            Some(part_candidate)
        } else {
            None
        }
    };

    #[cfg(target_os = "windows")]
    {
        win_shell::allow_foreground();
        if let Some(ref item) = target_item {
            if item.is_file() {
                let item_for_ui = item.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = app.run_on_main_thread(move || {
                    let res = win_shell::reveal_and_select_file(&item_for_ui);
                    let _ = tx.send(res);
                });
                if let Ok(res) = rx.recv_timeout(std::time::Duration::from_secs(3)) {
                    return res;
                }
                return win_shell::reveal_and_select_file(item);
            }
        }

        let dir_to_open = if let Some(ref item) = target_item {
            if item.is_dir() {
                item.clone()
            } else if let Some(parent) = item.parent() {
                parent.to_path_buf()
            } else {
                return Err(format!("Folder not found: {path}"));
            }
        } else if let Some(parent) = p.parent() {
            if parent.exists() {
                parent.to_path_buf()
            } else {
                return Err(format!("Folder not found: {path}"));
            }
        } else {
            return Err(format!("Folder not found: {path}"));
        };

        let canonical = std::fs::canonicalize(&dir_to_open).unwrap_or(dir_to_open);
        let win_dir = canonical
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\");
        std::process::Command::new("explorer.exe")
            .arg(&win_dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(ref item) = target_item {
            if item.is_file() {
                std::process::Command::new("open")
                    .arg("-R")
                    .arg(item)
                    .spawn()
                    .map_err(|e| format!("Failed to open folder: {e}"))?;
                return Ok(());
            }
        }
        let dir_to_open = if p.exists() && p.is_dir() {
            p
        } else if let Some(parent) = p.parent() {
            parent.to_path_buf()
        } else {
            return Err(format!("Folder not found: {path}"));
        };
        std::process::Command::new("open")
            .arg(&dir_to_open)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        let dir_to_open = if p.exists() && p.is_dir() {
            p
        } else if let Some(parent) = p.parent() {
            parent.to_path_buf()
        } else {
            return Err(format!("Folder not found: {path}"));
        };
        std::process::Command::new("xdg-open")
            .arg(&dir_to_open)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_title: String,
    pub release_notes: String,
    pub release_date: String,
    pub html_url: String,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProgressPayload {
    pub downloaded: u64,
    pub total: u64,
    pub percent: f64,
    pub speed_bytes_sec: u64,
}

fn parse_semver(v: &str) -> Vec<u64> {
    let clean = v.trim_start_matches('v').trim_start_matches('V');
    clean
        .split(|c: char| c == '.' || c == '-' || c == '+')
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let l_parts = parse_semver(latest);
    let c_parts = parse_semver(current);
    let max_len = l_parts.len().max(c_parts.len());
    for i in 0..max_len {
        let l = l_parts.get(i).copied().unwrap_or(0);
        let c = c_parts.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        } else if l < c {
            return false;
        }
    }
    false
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app.package_info().version.to_string();

    let client = reqwest::Client::builder()
        .user_agent("Raydrop-Desktop-App")
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let res = client
        .get("https://api.github.com/repos/asepindrak/raydrop/releases/latest")
        .send()
        .await
        .map_err(|e| format!("Unable to reach GitHub update server: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("Update check returned HTTP {}", res.status()));
    }

    let release_val: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse release response: {e}"))?;

    let tag_name = release_val["tag_name"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let release_title = release_val["name"]
        .as_str()
        .unwrap_or(&tag_name)
        .to_string();
    let release_notes = release_val["body"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let release_date = release_val["published_at"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let html_url = release_val["html_url"]
        .as_str()
        .unwrap_or("https://github.com/asepindrak/raydrop/releases")
        .to_string();

    let clean_latest = tag_name.trim_start_matches('v').trim_start_matches('V').to_string();
    let has_update = is_newer_version(&clean_latest, &current_version);

    let mut download_url = None;
    let mut asset_name = None;
    let mut asset_size = None;

    if let Some(assets) = release_val["assets"].as_array() {
        // Priority 1: .msi installer
        let msi_asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.to_lowercase().ends_with(".msi"))
                .unwrap_or(false)
        });

        // Priority 2: .exe
        let exe_asset = assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.to_lowercase().ends_with(".exe"))
                .unwrap_or(false)
        });

        let target_asset = msi_asset.or(exe_asset);
        if let Some(a) = target_asset {
            download_url = a["browser_download_url"].as_str().map(String::from);
            asset_name = a["name"].as_str().map(String::from);
            asset_size = a["size"].as_u64();
        }
    }

    Ok(UpdateCheckResult {
        has_update,
        current_version,
        latest_version: tag_name,
        release_title,
        release_notes,
        release_date,
        html_url,
        download_url,
        asset_name,
        asset_size,
    })
}

#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    download_url: String,
    asset_name: String,
) -> Result<String, String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .user_agent("Raydrop-Desktop-App")
        .build()
        .map_err(|e| format!("Failed to create download client: {e}"))?;

    let res = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to initiate download: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("Download failed with HTTP {}", res.status()));
    }

    let total_bytes = res.content_length().unwrap_or(0);
    let temp_file_path = std::env::temp_dir().join(&asset_name);

    let mut file = tokio::fs::File::create(&temp_file_path)
        .await
        .map_err(|e| format!("Failed to create temporary file: {e}"))?;

    let mut stream = res.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    let mut last_bytes: u64 = 0;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("Error downloading chunk: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write chunk to disk: {e}"))?;
        downloaded += chunk.len() as u64;

        if last_emit.elapsed() >= std::time::Duration::from_millis(150) || downloaded == total_bytes {
            let elapsed_sec = last_emit.elapsed().as_secs_f64();
            let speed = if elapsed_sec > 0.0 {
                ((downloaded.saturating_sub(last_bytes)) as f64 / elapsed_sec) as u64
            } else {
                0
            };
            last_bytes = downloaded;
            last_emit = std::time::Instant::now();

            let percent = if total_bytes > 0 {
                (downloaded as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };

            let _ = app.emit(
                "app_update_progress",
                UpdateProgressPayload {
                    downloaded,
                    total: total_bytes,
                    percent,
                    speed_bytes_sec: speed,
                },
            );
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush downloaded file: {e}"))?;
    drop(file);

    #[cfg(target_os = "windows")]
    {
        let is_msi = asset_name.to_lowercase().ends_with(".msi");
        let path_str = temp_file_path.to_string_lossy().to_string();

        if is_msi {
            std::process::Command::new("msiexec.exe")
                .arg("/i")
                .arg(&path_str)
                .spawn()
                .map_err(|e| format!("Failed to launch MSI installer: {e}"))?;

            let app_exit = app.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                app_exit.exit(0);
            });

            return Ok("Installer launched. Upgrading Raydrop...".to_string());
        } else {
            std::process::Command::new("explorer.exe")
                .arg(format!("/select,{}", path_str))
                .spawn()
                .ok();
            return Ok(format!("Updated executable saved to {}", path_str));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        return Ok(format!("Downloaded update to {}", temp_file_path.to_string_lossy()));
    }
}

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

pub fn generate_image_thumbnail(file_path: &str, max_width: u32, max_height: u32) -> Result<String, String> {
    let path = std::path::Path::new(file_path);
    if !path.exists() {
        return Err("File not found".to_string());
    }

    let img = image::open(path).map_err(|e| format!("Failed to open image: {e}"))?;
    let thumb = img.thumbnail(max_width, max_height);

    let mut buf = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode JPEG: {e}"))?;

    let encoded = BASE64_STANDARD.encode(buf.into_inner());
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

#[tauri::command]
pub fn get_file_thumbnail(path: String) -> Result<String, String> {
    generate_image_thumbnail(&path, 320, 240)
}

#[tauri::command]
pub fn send_chat_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
    content: String,
    msg_type: Option<String>,
    transfer_session_id: Option<String>,
    files_json: Option<String>,
) -> Result<ChatMessage, String> {
    let now = now_unix_secs();
    let msg_id = uuid::Uuid::new_v4().to_string();
    let effective_type = msg_type.unwrap_or_else(|| "text".to_string());

    // Enrich files_json with real file size and fast lightweight thumbnails (max 300x220)
    let enriched_files_json = if let Some(ref raw_json) = files_json {
        if let Ok(mut items) = serde_json::from_str::<Vec<serde_json::Value>>(raw_json) {
            for item in &mut items {
                if let Some(obj) = item.as_object_mut() {
                    let path_str = obj
                        .get("path")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let name_str = obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_lowercase();

                    if let Some(ref p) = path_str {
                        // 1. Populate real file size if missing or 0
                        let current_size = obj.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                        if current_size == 0 {
                            if let Ok(meta) = std::fs::metadata(p) {
                                obj.insert("size".to_string(), json!(meta.len()));
                            }
                        }

                        // 2. Generate lightweight base64 thumbnail if it's an image
                        let is_img = name_str.ends_with(".png")
                            || name_str.ends_with(".jpg")
                            || name_str.ends_with(".jpeg")
                            || name_str.ends_with(".webp")
                            || name_str.ends_with(".bmp")
                            || name_str.ends_with(".gif");

                        if is_img && obj.get("thumbnail").is_none() {
                            if let Ok(thumb_data) = generate_image_thumbnail(p, 300, 220) {
                                obj.insert("thumbnail".to_string(), json!(thumb_data));
                            }
                        }
                    }
                }
            }
            serde_json::to_string(&items).ok()
        } else {
            files_json.clone()
        }
    } else {
        None
    };

    let chat_msg = ChatMessage {
        id: msg_id.clone(),
        peer_device_id: peer_device_id.clone(),
        direction: "outgoing".to_string(),
        content: content.clone(),
        msg_type: effective_type.clone(),
        transfer_session_id: transfer_session_id.clone(),
        files_json: enriched_files_json.clone(),
        is_read: true,
        created_at: now,
    };

    state.db.insert_chat_message(&chat_msg)?;

    let payload_obj = json!({
        "id": msg_id,
        "content": content,
        "msg_type": effective_type,
        "transfer_session_id": transfer_session_id,
        "files_json": enriched_files_json,
        "from_alias": state.settings.lock().unwrap().alias_name,
        "created_at": now
    })
    .to_string();

    let hex_payload = hex::encode(payload_obj.as_bytes());

    // 1. Send via active WAN signaling relay
    if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
        let packet = json!({
            "type": "RELAY_CONTROL",
            "session_id": format!("chat:{msg_id}"),
            "from_device_id": state.identity.device_id,
            "target_device_id": peer_device_id,
            "payload_json": hex_payload
        })
        .to_string();
        let _ = tx.send(packet);
    }

    // 2. Also attempt direct LAN / Tailscale TCP delivery if contact has known local IP & port
    if let Ok(Some(contact)) = state.db.get_contact(&peer_device_id) {
        if let (Some(ip), Some(port)) = (contact.last_known_ip, contact.last_known_port) {
            let target_addr = format!("{}:{}", ip, port);
            let hex_p = hex_payload.clone();
            let my_dev_id = state.identity.device_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(Ok(mut stream)) = tokio::time::timeout(
                    std::time::Duration::from_millis(1500),
                    tokio::net::TcpStream::connect(&target_addr),
                )
                .await
                {
                    let packet_val = json!({
                        "type": "CHAT",
                        "from_device_id": my_dev_id,
                        "payload_json": hex_p
                    });
                    if let Ok(data) = serde_json::to_vec(&packet_val) {
                        let len_prefix = (data.len() as u32).to_be_bytes();
                        use tokio::io::AsyncWriteExt;
                        let _ = stream.write_all(&len_prefix).await;
                        let _ = stream.write_all(&data).await;
                        let _ = stream.flush().await;
                    }
                }
            });
        }
    }

    let _ = app.emit("chat_message_sent", &chat_msg);
    Ok(chat_msg)
}

#[tauri::command]
pub fn get_chat_messages(
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<ChatMessage>, String> {
    state
        .db
        .list_chat_messages(&peer_device_id, limit.unwrap_or(200), offset.unwrap_or(0))
}

#[tauri::command]
pub fn mark_chat_messages_read(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
) -> Result<(), String> {
    state.db.mark_chat_read(&peer_device_id)?;
    if let Ok(counts) = state.db.get_unread_chat_counts() {
        let total: u32 = counts.values().sum();
        crate::tray_badge::update_tray_unread_badge(&app, total);
    }
    Ok(())
}

#[tauri::command]
pub fn update_tray_badge(app: AppHandle, count: u32) -> Result<(), String> {
    crate::tray_badge::update_tray_unread_badge(&app, count);
    Ok(())
}

#[tauri::command]
pub fn clear_chat_messages(
    state: State<'_, Arc<AppState>>,
    peer_device_id: String,
) -> Result<(), String> {
    state.db.clear_chat_messages(&peer_device_id)
}

#[tauri::command]
pub fn get_unread_chat_counts(
    state: State<'_, Arc<AppState>>,
) -> Result<std::collections::HashMap<String, u32>, String> {
    state.db.get_unread_chat_counts()
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Invalid URL protocol".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn start_web_share(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> Result<crate::network::web_share::WebShareInfo, String> {
    crate::network::web_share::start_web_share_server(app, state.inner().clone(), paths).await
}

#[tauri::command]
pub fn stop_web_share(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    crate::network::web_share::stop_web_share_server(&state);
    Ok(())
}

#[tauri::command]
pub fn read_system_clipboard() -> Result<String, String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    cb.get_text().map_err(|e| format!("Clipboard read error: {e}"))
}

#[tauri::command]
pub fn write_system_clipboard(text: String) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    cb.set_text(text).map_err(|e| format!("Clipboard write error: {e}"))
}

#[tauri::command]
pub fn broadcast_clipboard(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    content: String,
    target_device_ids: Option<Vec<String>>,
) -> Result<u32, String> {
    if content.trim().is_empty() {
        return Err("Clipboard text cannot be empty".to_string());
    }

    let targets: Vec<String> = if let Some(ids) = target_device_ids {
        if !ids.is_empty() {
            ids
        } else {
            let all = state.db.list_contacts().map_err(|e| e.to_string())?;
            all.into_iter()
                .filter(|c| c.is_own_device || c.trust_level == "trusted_auto_accept")
                .map(|c| c.device_id)
                .collect()
        }
    } else {
        let all = state.db.list_contacts().map_err(|e| e.to_string())?;
        all.into_iter()
            .filter(|c| c.is_own_device || c.trust_level == "trusted_auto_accept")
            .map(|c| c.device_id)
            .collect()
    };

    if targets.is_empty() {
        return Err("No trusted devices found. Add a contact or device first.".to_string());
    }

    let mut sent_count = 0;
    for peer_id in targets {
        if send_chat_message(
            app.clone(),
            state.clone(),
            peer_id,
            content.clone(),
            Some("clipboard".to_string()),
            None,
            None,
        )
        .is_ok()
        {
            sent_count += 1;
        }
    }

    Ok(sent_count)
}

#[tauri::command]
pub fn list_folder_sync_tasks(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::db::storage::FolderSyncTask>, String> {
    state.db.list_folder_sync_tasks()
}

#[tauri::command]
pub fn add_folder_sync_task(
    state: State<'_, Arc<AppState>>,
    local_path: String,
    target_device_id: String,
    is_auto_sync: bool,
) -> Result<crate::db::storage::FolderSyncTask, String> {
    let p = std::path::Path::new(&local_path);
    if !p.exists() || !p.is_dir() {
        return Err("Selected path does not exist or is not a directory".to_string());
    }

    let contact = state
        .db
        .get_contact(&target_device_id)?
        .ok_or_else(|| "Target device not found in contacts".to_string())?;

    let task = crate::db::storage::FolderSyncTask {
        id: uuid::Uuid::new_v4().to_string(),
        local_path,
        target_device_id,
        target_alias: contact.alias_name,
        is_auto_sync,
        last_sync_at: None,
        created_at: now_unix_secs(),
    };

    state.db.insert_folder_sync_task(&task)?;
    Ok(task)
}

#[tauri::command]
pub fn toggle_folder_sync_auto(
    state: State<'_, Arc<AppState>>,
    task_id: String,
    is_auto: bool,
) -> Result<(), String> {
    state.db.update_folder_sync_auto_flag(&task_id, is_auto)
}

#[tauri::command]
pub fn delete_folder_sync_task(
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> Result<(), String> {
    state.db.delete_folder_sync_task(&task_id)
}

#[tauri::command]
pub async fn trigger_folder_sync(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> Result<String, String> {
    let task = state
        .db
        .get_folder_sync_task(&task_id)?
        .ok_or_else(|| "Folder sync task not found".to_string())?;

    let p = std::path::Path::new(&task.local_path);
    if !p.exists() || !p.is_dir() {
        return Err("Local folder no longer exists on disk".to_string());
    }

    let session_id = initiate_peer_transfer(
        app,
        state.inner().clone(),
        task.target_device_id,
        vec![task.local_path],
    )
    .await?;

    let _ = state.db.update_folder_sync_last_time(&task_id, now_unix_secs());
    Ok(session_id)
}

