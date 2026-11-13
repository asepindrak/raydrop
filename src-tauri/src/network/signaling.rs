use crate::db::storage::now_unix_secs;
use crate::network::nat::{discover_wan_reflexive_address, execute_udp_hole_punch};
use crate::state::{AppState, WanCandidateResponse};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};

pub fn start_signaling_daemon(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        loop {
            let (sig_url, stun_addr, alias_name, auth_token) = {
                let s = state.settings.lock().unwrap();
                (
                    s.signaling_url.clone(),
                    s.stun_addr.clone(),
                    s.alias_name.clone(),
                    s.auth_token.clone(),
                )
            };

            if sig_url.trim().is_empty() {
                state.signaling_connected.store(false, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            // Refresh our public WAN candidate via STUN before registering
            if let Some(reflexive) =
                discover_wan_reflexive_address(state.listen_port, &stun_addr).await
            {
                *state.wan_reflexive_addr.lock().unwrap() = Some(reflexive);
            }

            match connect_async(&sig_url).await {
                Ok((ws_stream, _)) => {
                    info!("Connected to WAN signaling server: {sig_url}");
                    state.signaling_connected.store(true, Ordering::Relaxed);
                    let _ = app.emit("signaling_status_changed", true);

                    let (mut ws_tx, mut ws_rx) = ws_stream.split();
                    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
                    *state.signaling_tx.lock().unwrap() = Some(tx.clone());

                    // Send authenticated REGISTER message
                    let ts = now_unix_secs();
                    let sign_payload =
                        format!("RAYDROP_REGISTER:{}:{ts}", state.identity.device_id);
                    let sig = state.identity.sign_message(sign_payload.as_bytes());

                    let reg_msg = json!({
                        "type": "REGISTER",
                        "device_id": state.identity.device_id,
                        "alias_name": alias_name,
                        "ed25519_pubkey": state.identity.public_hex(),
                        "timestamp": ts,
                        "signature": sig,
                        "auth_token": auth_token
                    })
                    .to_string();
                    let _ = tx.send(reg_msg);

                    // Query online status of all saved contacts
                    if let Ok(contacts) = state.db.list_contacts() {
                        for c in contacts {
                            let lookup = json!({
                                "type": "PEER_LOOKUP",
                                "target_device_id": c.device_id
                            })
                            .to_string();
                            let _ = tx.send(lookup);
                        }
                    }

                    let writer = tokio::spawn(async move {
                        while let Some(outbound) = rx.recv().await {
                            if ws_tx.send(Message::Text(outbound)).await.is_err() {
                                break;
                            }
                        }
                    });

                    while let Some(Ok(msg)) = ws_rx.next().await {
                        if let Message::Text(text) = msg {
                            handle_signaling_message(&app, &state, &text).await;
                        }
                    }

                    writer.abort();
                    state.signaling_connected.store(false, Ordering::Relaxed);
                    *state.signaling_tx.lock().unwrap() = None;
                    let _ = app.emit("signaling_status_changed", false);
                }
                Err(e) => {
                    warn!("Signaling connection failed ({sig_url}): {e}");
                    state.signaling_connected.store(false, Ordering::Relaxed);
                    let _ = app.emit("signaling_status_changed", false);
                }
            }

            tokio::time::sleep(Duration::from_secs(6)).await;
        }
    });
}

async fn handle_signaling_message(app: &AppHandle, state: &Arc<AppState>, text: &str) {
    let Ok(val) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let Some(msg_type) = val.get("type").and_then(|v| v.as_str()) else {
        return;
    };

    match msg_type {
        "CLOUD_STATE_CHANGED" => {
            let _ = app.emit("cloud_state_changed", ());
        }
        "PEER_STATUS" => {
            if let (Some(device_id), Some(is_online)) = (
                val.get("device_id").and_then(|v| v.as_str()),
                val.get("is_online").and_then(|v| v.as_bool()),
            ) {
                let _ = state.db.update_contact_online(device_id, is_online);
                let _ = app.emit("contacts_updated", ());
            }
        }
        "CONNECT_INTENT" => {
            if !state.is_discoverable() {
                return;
            }
            let session_id = val
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let from_device_id = val
                .get("from_device_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let peer_wan = val
                .get("wan_candidate")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let stun_addr = state.settings.lock().unwrap().stun_addr.clone();
            let my_wan = discover_wan_reflexive_address(state.listen_port, &stun_addr)
                .await
                .or_else(|| state.wan_reflexive_addr.lock().unwrap().clone());

            let my_lan = crate::network::transport::collect_all_lan_addresses(state.listen_port);

            if let Some(tx) = state.signaling_tx.lock().unwrap().as_ref() {
                let answer = json!({
                    "type": "CANDIDATE_ANSWER",
                    "session_id": session_id,
                    "from_device_id": state.identity.device_id,
                    "from_ed25519_pubkey": state.identity.public_hex(),
                    "target_device_id": from_device_id,
                    "lan_candidates": my_lan,
                    "wan_candidate": my_wan
                })
                .to_string();
                let _ = tx.send(answer);
            }

            if let Some(peer_wan_addr) = peer_wan {
                let port = state.listen_port;
                let sid = session_id.clone();
                tokio::spawn(async move {
                    execute_udp_hole_punch(port, &peer_wan_addr, &sid).await;
                });
            }
        }
        "CANDIDATE_ANSWER" => {
            let session_id = val
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let lan_candidates: Vec<String> = val
                .get("lan_candidates")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let wan_candidate = val
                .get("wan_candidate")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let peer_ed25519_pubkey = val
                .get("from_ed25519_pubkey")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            if let Some(ref peer_wan_addr) = wan_candidate {
                let port = state.listen_port;
                let addr = peer_wan_addr.clone();
                let sid = session_id.clone();
                tokio::spawn(async move {
                    execute_udp_hole_punch(port, &addr, &sid).await;
                });
            }

            if let Some(waiter) = state
                .pending_wan_candidates
                .lock()
                .unwrap()
                .remove(&session_id)
            {
                let _ = waiter.send(WanCandidateResponse {
                    lan_candidates,
                    wan_candidate,
                    peer_ed25519_pubkey,
                });
            }
        }
        "RELAY_CONTROL" => {
            let session_id = val
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let from_device_id = val
                .get("from_device_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let payload_json = val
                .get("payload_json")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            if session_id.is_empty() || from_device_id.is_empty() {
                return;
            }

            // Real-time P2P Chat Packet Handler (Zero Server DB, E2EE Relay / Direct)
            if session_id.starts_with("chat:") {
                if let Ok(Some(contact)) = state.db.get_contact(&from_device_id) {
                    if contact.trust_level == "blocked" {
                        warn!("Dropped chat packet from blocked peer: {from_device_id}");
                        return;
                    }
                }

                let bytes = match hex::decode(payload_json) {
                    Ok(b) => b,
                    Err(_) => return,
                };
                let payload_str = match String::from_utf8(bytes) {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let Ok(chat_val) = serde_json::from_str::<Value>(&payload_str) else {
                    return;
                };

                let msg_id = chat_val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| session_id.trim_start_matches("chat:"))
                    .to_string();
                let content = chat_val
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let msg_type = chat_val
                    .get("msg_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("text")
                    .to_string();
                let transfer_session_id = chat_val
                    .get("transfer_session_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let files_json = chat_val
                    .get("files_json")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let created_at = chat_val
                    .get("created_at")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_else(now_unix_secs);

                let chat_msg = crate::db::storage::ChatMessage {
                    id: msg_id,
                    peer_device_id: from_device_id.clone(),
                    direction: "incoming".to_string(),
                    content,
                    msg_type,
                    transfer_session_id,
                    files_json,
                    is_read: false,
                    created_at,
                };

                let _ = state.db.insert_chat_message(&chat_msg);
                let _ = app.emit("chat_message_received", &chat_msg);
                return;
            }

            let existing_tx = state
                .relay_streams
                .lock()
                .unwrap()
                .get(&session_id)
                .cloned();

            let tx = if let Some(tx) = existing_tx {
                tx
            } else if !payload_json.is_empty() && state.is_discoverable() {
                let (reader, writer) = crate::network::transport::create_ws_relay_stream(
                    state.clone(),
                    session_id.clone(),
                    from_device_id,
                );
                let app_c = app.clone();
                let state_c = state.clone();
                tauri::async_runtime::spawn(async move {
                    let dummy_addr: std::net::SocketAddr = "0.0.0.0:0".parse().unwrap();
                    if let Err(e) = crate::network::transport::handle_incoming_stream(
                        app_c,
                        state_c,
                        reader,
                        writer,
                        dummy_addr,
                        "wan_p2p".to_string(),
                    )
                    .await
                    {
                        warn!("Incoming WSS relay stream error: {e}");
                    }
                });
                match state.relay_streams.lock().unwrap().get(&session_id).cloned() {
                    Some(t) => t,
                    None => return,
                }
            } else {
                return;
            };

            if payload_json.is_empty() {
                let _ = tx.send(Vec::new());
            } else if let Ok(bytes) = hex::decode(payload_json) {
                let _ = tx.send(bytes);
            }
        }
        _ => {}
    }
}
