use crate::db::storage::{now_unix_secs, AppSettings, Contact, TransferRecord};
use crate::network::transport::initiate_peer_transfer;
use crate::os::register_context_menu;
use crate::state::{AppState, IncomingRequestDecision};
use serde::Serialize;
use serde_json::json;
use std::sync::{atomic::Ordering, Arc};
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct AppStateSnapshot {
    pub settings: AppSettings,
    pub local_ip: String,
    pub listen_port: u16,
    pub wan_reflexive_addr: Option<String>,
    pub signaling_connected: bool,
    pub queued_cli_files: Vec<String>,
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
        local_ip: state.local_ip.clone(),
        listen_port: state.listen_port,
        wan_reflexive_addr,
        signaling_connected: state.is_signaling_connected(),
        queued_cli_files,
    })
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
pub fn update_settings(
    state: State<'_, Arc<AppState>>,
    alias_name: String,
    download_dir: String,
    signaling_url: String,
    stun_addr: String,
) -> Result<AppSettings, String> {
    state.db.set_setting("alias_name", &alias_name)?;
    state.db.set_setting("download_dir", &download_dir)?;
    state.db.set_setting("signaling_url", &signaling_url)?;
    state.db.set_setting("stun_addr", &stun_addr)?;

    let mut s = state.settings.lock().map_err(|e| e.to_string())?;
    s.alias_name = alias_name;
    s.download_dir = download_dir;
    s.signaling_url = signaling_url;
    s.stun_addr = stun_addr;
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

    // Immediately query online status via signaling server if connected
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
) -> Result<(), String> {
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

