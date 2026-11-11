use crate::db::storage::{now_unix_secs, Contact};
use crate::state::AppState;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

const SERVICE_TYPE: &str = "_raydrop._tcp.local.";

pub fn start_mdns_daemon(app: AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        let Ok(mdns) = ServiceDaemon::new() else {
            warn!("Failed to initialize mDNS daemon");
            return;
        };

        let settings = state.settings.lock().unwrap().clone();
        let instance_name = format!(
            "{}-{}",
            settings.alias_name.replace(' ', "-"),
            &state.identity.device_id[4..8]
        );
        let host_name = format!("{}.local.", instance_name);

        let mut properties = HashMap::new();
        properties.insert("device_id".to_string(), state.identity.device_id.clone());
        properties.insert("alias".to_string(), settings.alias_name.clone());
        properties.insert("pubkey".to_string(), state.identity.public_hex());
        properties.insert("os".to_string(), std::env::consts::OS.to_string());
        if let Some(ref email) = settings.user_email {
            properties.insert("email".to_string(), email.clone());
        }
        if let Some(ref name) = settings.user_name {
            properties.insert("user_name".to_string(), name.clone());
        }

        if let Ok(service_info) = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &host_name,
            &state.local_ip,
            state.listen_port,
            Some(properties),
        ) {
            let service_info = service_info.enable_addr_auto();
            if let Err(e) = mdns.register(service_info) {
                warn!("mDNS service registration failed: {e}");
            } else {
                info!(
                    "mDNS registered: {} on {}:{}",
                    state.identity.device_id, state.local_ip, state.listen_port
                );
            }
        }

        let Ok(receiver) = mdns.browse(SERVICE_TYPE) else {
            warn!("Failed to start mDNS browse");
            return;
        };

        while let Ok(event) = receiver.recv() {
            if let ServiceEvent::ServiceResolved(info) = event {
                let Some(peer_id) = info.get_property_val_str("device_id") else {
                    continue;
                };
                if peer_id == state.identity.device_id {
                    continue;
                }

                let alias = info
                    .get_property_val_str("alias")
                    .unwrap_or("Unknown-LAN-Peer")
                    .to_string();
                let pubkey = info.get_property_val_str("pubkey").map(|s| s.to_string());
                let peer_email = info.get_property_val_str("email").map(|s| s.to_string());
                let peer_user_name = info
                    .get_property_val_str("user_name")
                    .map(|s| s.to_string());
                let peer_os = info.get_property_val_str("os").map(|s| s.to_string());
                let port = info.get_port();
                let ip = info
                    .get_addresses()
                    .iter()
                    .find(|ip| ip.is_ipv4())
                    .or_else(|| info.get_addresses().iter().next())
                    .map(|ip| ip.to_string());

                if let Some(ip_str) = ip {
                    let my_email = state.settings.lock().unwrap().user_email.clone();
                    let is_own_device = match (&my_email, &peer_email) {
                        (Some(me), Some(pe)) if !me.is_empty() => me.eq_ignore_ascii_case(pe),
                        _ => false,
                    };

                    let existing = state.db.get_contact(peer_id).ok().flatten();
                    let existing_trust = existing
                        .as_ref()
                        .map(|c| c.trust_level.clone())
                        .unwrap_or_else(|| {
                            if is_own_device {
                                "trusted_auto_accept".to_string()
                            } else {
                                "ask".to_string()
                            }
                        });

                    let contact = Contact {
                        device_id: peer_id.to_string(),
                        alias_name: alias,
                        user_email: peer_email.or_else(|| existing.as_ref().and_then(|c| c.user_email.clone())),
                        user_name: peer_user_name.or_else(|| existing.as_ref().and_then(|c| c.user_name.clone())),
                        os_type: peer_os.or_else(|| existing.as_ref().and_then(|c| c.os_type.clone())),
                        is_own_device: is_own_device || existing.as_ref().map(|c| c.is_own_device).unwrap_or(false),
                        ed25519_pubkey: pubkey,
                        network_type: "lan".to_string(),
                        last_known_ip: Some(ip_str),
                        last_known_port: Some(port),
                        trust_level: existing_trust,
                        is_online: true,
                        last_seen_at: now_unix_secs(),
                        created_at: now_unix_secs(),
                    };

                    let _ = state.db.upsert_contact(&contact);
                    let _ = app.emit("peer_discovered", &contact);
                }
            }
        }
    });
}
