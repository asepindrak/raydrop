use crate::db::storage::{now_unix_secs, AppSettings, Contact, TransferRecord};
use crate::network::transport::{initiate_peer_transfer, resume_peer_transfer};
use crate::os::{register_context_menu, set_autostart};
use crate::state::{AppState, IncomingRequestDecision};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{atomic::Ordering, Arc};
use tauri::{AppHandle, Emitter, State};

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
            .unwrap_or("raydrop.gapleh.online")
            .split(':')
            .next()
            .unwrap_or("raydrop.gapleh.online");
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
pub fn open_file_path(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("File atau folder tidak ditemukan: {path}"));
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let win_path = p.to_string_lossy().replace('/', "\\");
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &win_path])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Gagal membuka file: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&p)
            .spawn()
            .map_err(|e| format!("Gagal membuka file: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&p)
            .spawn()
            .map_err(|e| format!("Gagal membuka file: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
pub fn open_folder_path(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        if p.exists() && p.is_file() {
            let win_path = p.to_string_lossy().replace('/', "\\");
            std::process::Command::new("explorer.exe")
                .raw_arg(format!("/select,\"{}\"", win_path))
                .spawn()
                .map_err(|e| format!("Gagal membuka folder: {e}"))?;
            return Ok(());
        }
        let dir_to_open = if p.exists() && p.is_dir() {
            p
        } else if let Some(parent) = p.parent() {
            if parent.exists() {
                parent.to_path_buf()
            } else {
                return Err(format!("Folder tidak ditemukan: {path}"));
            }
        } else {
            return Err(format!("Folder tidak ditemukan: {path}"));
        };
        let win_dir = dir_to_open.to_string_lossy().replace('/', "\\");
        std::process::Command::new("explorer.exe")
            .arg(&win_dir)
            .spawn()
            .map_err(|e| format!("Gagal membuka folder: {e}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        if p.exists() && p.is_file() {
            std::process::Command::new("open")
                .args(["-R", &path])
                .spawn()
                .map_err(|e| format!("Gagal membuka folder: {e}"))?;
            return Ok(());
        }
        let dir_to_open = if p.exists() && p.is_dir() {
            p
        } else if let Some(parent) = p.parent() {
            parent.to_path_buf()
        } else {
            return Err(format!("Folder tidak ditemukan: {path}"));
        };
        std::process::Command::new("open")
            .arg(&dir_to_open)
            .spawn()
            .map_err(|e| format!("Gagal membuka folder: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        let dir_to_open = if p.exists() && p.is_dir() {
            p
        } else if let Some(parent) = p.parent() {
            parent.to_path_buf()
        } else {
            return Err(format!("Folder tidak ditemukan: {path}"));
        };
        std::process::Command::new("xdg-open")
            .arg(&dir_to_open)
            .spawn()
            .map_err(|e| format!("Gagal membuka folder: {e}"))?;
    }

    Ok(())
}
