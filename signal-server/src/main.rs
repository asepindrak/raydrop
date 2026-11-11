use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use dashmap::DashMap;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::UdpSocket, sync::mpsc};
use tracing::{info, warn};

#[derive(Clone)]
struct ServerState {
    peers: Arc<DashMap<String, PeerSession>>,
}

struct PeerSession {
    alias_name: String,
    ed25519_pubkey: String,
    tx: mpsc::UnboundedSender<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
enum SignalMessage {
    #[serde(rename = "REGISTER")]
    Register {
        device_id: String,
        alias_name: String,
        ed25519_pubkey: String,
        timestamp: u64,
        signature: String,
    },
    #[serde(rename = "PEER_LOOKUP")]
    PeerLookup { target_device_id: String },
    #[serde(rename = "CONNECT_INTENT")]
    ConnectIntent {
        session_id: String,
        from_device_id: String,
        from_alias: String,
        from_ed25519_pubkey: String,
        target_device_id: String,
        lan_candidates: Vec<String>,
        wan_candidate: Option<String>,
    },
    #[serde(rename = "CANDIDATE_ANSWER")]
    CandidateAnswer {
        session_id: String,
        from_device_id: String,
        from_ed25519_pubkey: String,
        target_device_id: String,
        lan_candidates: Vec<String>,
        wan_candidate: Option<String>,
    },
    #[serde(rename = "RELAY_CONTROL")]
    RelayControl {
        session_id: String,
        from_device_id: String,
        target_device_id: String,
        payload_json: String,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let http_addr = std::env::var("SIGNAL_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let udp_addr = std::env::var("SIGNAL_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:3478".to_string());

    let state = ServerState {
        peers: Arc::new(DashMap::new()),
    };

    // Spawn UDP STUN / Reflexive Address Discovery Listener
    let udp_bind = udp_addr.clone();
    tokio::spawn(async move {
        match UdpSocket::bind(&udp_bind).await {
            Ok(sock) => {
                info!("Raydrop UDP STUN reflector listening on {udp_bind}");
                let mut buf = [0u8; 1024];
                loop {
                    if let Ok((len, peer_addr)) = sock.recv_from(&mut buf).await {
                        let req = &buf[..len];
                        if req.starts_with(b"RAYD_STUN") {
                            let resp = json!({
                                "type": "STUN_REFLEXIVE",
                                "reflexive_addr": peer_addr.to_string(),
                                "ip": peer_addr.ip().to_string(),
                                "port": peer_addr.port()
                            })
                            .to_string();
                            let _ = sock.send_to(resp.as_bytes(), peer_addr).await;
                        } else if len >= 20 && req[0] == 0x00 && req[1] == 0x01 {
                            // Minimal RFC 5389 STUN Binding Success Response (0x0101)
                            let mut stun_resp = Vec::with_capacity(32);
                            stun_resp.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]); // Type + placeholder len
                            stun_resp.extend_from_slice(&req[4..20]); // Magic Cookie + Transaction ID
                            let resp_json = peer_addr.to_string();
                            let _ = sock.send_to(resp_json.as_bytes(), peer_addr).await;
                        }
                    }
                }
            }
            Err(e) => warn!("Failed to bind UDP STUN reflector on {udp_bind}: {e}"),
        }
    });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&http_addr)
        .await
        .expect("Failed to bind HTTP signaling port");
    info!("Raydrop WSS Signaling server listening on {http_addr}");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

async fn health_handler(State(state): State<ServerState>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "raydrop-signal-server",
        "online_peers": state.peers.len()
    }))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

fn verify_registration(
    device_id: &str,
    ed25519_pubkey_hex: &str,
    timestamp: u64,
    signature_hex: &str,
) -> bool {
    let Ok(pub_bytes) = hex::decode(ed25519_pubkey_hex) else {
        return false;
    };
    let Ok(pub_arr): Result<[u8; 32], _> = pub_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&pub_arr) else {
        return false;
    };

    // Verify device_id matches BLAKE3(ed25519_pubkey)
    let hash = blake3::hash(&pub_arr);
    let hex_hash = hash.to_hex().to_uppercase();
    let expected_id = format!(
        "RAY-{}-{}-{}",
        &hex_hash[0..4],
        &hex_hash[4..8],
        &hex_hash[8..12]
    );
    if device_id != expected_id {
        return false;
    }

    let Ok(sig_bytes) = hex::decode(signature_hex) else {
        return false;
    };
    let Ok(sig) = Signature::from_slice(&sig_bytes) else {
        return false;
    };

    let msg = format!("RAYDROP_REGISTER:{device_id}:{timestamp}");
    verifying_key.verify(msg.as_bytes(), &sig).is_ok()
}

async fn handle_socket(socket: WebSocket, addr: SocketAddr, state: ServerState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    let writer_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let mut registered_id: Option<String> = None;

    while let Some(Ok(msg)) = ws_rx.next().await {
        if let Message::Text(text) = msg {
            if let Ok(parsed) = serde_json::from_str::<SignalMessage>(&text) {
                match parsed {
                    SignalMessage::Register {
                        device_id,
                        alias_name,
                        ed25519_pubkey,
                        timestamp,
                        signature,
                    } => {
                        if verify_registration(&device_id, &ed25519_pubkey, timestamp, &signature) {
                            info!("Peer registered: {device_id} ({alias_name}) from {addr}");
                            state.peers.insert(
                                device_id.clone(),
                                PeerSession {
                                    alias_name: alias_name.clone(),
                                    ed25519_pubkey: ed25519_pubkey.clone(),
                                    tx: tx.clone(),
                                },
                            );
                            registered_id = Some(device_id.clone());
                            let _ = tx.send(
                                json!({
                                    "type": "REGISTER_OK",
                                    "device_id": device_id,
                                    "public_ip": addr.ip().to_string()
                                })
                                .to_string(),
                            );
                        } else {
                            warn!("Invalid Ed25519 registration signature from {addr}");
                            let _ = tx.send(
                                json!({
                                    "type": "ERROR",
                                    "message": "Ed25519 identity signature verification failed"
                                })
                                .to_string(),
                            );
                        }
                    }
                    SignalMessage::PeerLookup { target_device_id } => {
                        if let Some(peer) = state.peers.get(&target_device_id) {
                            let _ = tx.send(
                                json!({
                                    "type": "PEER_STATUS",
                                    "device_id": target_device_id,
                                    "alias_name": peer.alias_name,
                                    "ed25519_pubkey": peer.ed25519_pubkey,
                                    "is_online": true
                                })
                                .to_string(),
                            );
                        } else {
                            let _ = tx.send(
                                json!({
                                    "type": "PEER_STATUS",
                                    "device_id": target_device_id,
                                    "is_online": false
                                })
                                .to_string(),
                            );
                        }
                    }
                    SignalMessage::ConnectIntent {
                        session_id,
                        from_device_id,
                        from_alias,
                        from_ed25519_pubkey,
                        target_device_id,
                        lan_candidates,
                        wan_candidate,
                    } => {
                        if let Some(target) = state.peers.get(&target_device_id) {
                            let _ = target.tx.send(
                                json!({
                                    "type": "CONNECT_INTENT",
                                    "session_id": session_id,
                                    "from_device_id": from_device_id,
                                    "from_alias": from_alias,
                                    "from_ed25519_pubkey": from_ed25519_pubkey,
                                    "target_device_id": target_device_id,
                                    "lan_candidates": lan_candidates,
                                    "wan_candidate": wan_candidate
                                })
                                .to_string(),
                            );
                        }
                    }
                    SignalMessage::CandidateAnswer {
                        session_id,
                        from_device_id,
                        from_ed25519_pubkey,
                        target_device_id,
                        lan_candidates,
                        wan_candidate,
                    } => {
                        if let Some(initiator) = state.peers.get(&target_device_id) {
                            let _ = initiator.tx.send(
                                json!({
                                    "type": "CANDIDATE_ANSWER",
                                    "session_id": session_id,
                                    "from_device_id": from_device_id,
                                    "from_ed25519_pubkey": from_ed25519_pubkey,
                                    "target_device_id": target_device_id,
                                    "lan_candidates": lan_candidates,
                                    "wan_candidate": wan_candidate
                                })
                                .to_string(),
                            );
                            // Trigger simultaneous UDP hole-punching on both sides
                            let punch_msg = json!({
                                "type": "PUNCH_SYNC",
                                "session_id": session_id,
                                "peer_wan_candidate": wan_candidate
                            })
                            .to_string();
                            let _ = initiator.tx.send(punch_msg);
                        }
                    }
                    SignalMessage::RelayControl {
                        session_id,
                        from_device_id,
                        target_device_id,
                        payload_json,
                    } => {
                        if let Some(target) = state.peers.get(&target_device_id) {
                            let _ = target.tx.send(
                                json!({
                                    "type": "RELAY_CONTROL",
                                    "session_id": session_id,
                                    "from_device_id": from_device_id,
                                    "target_device_id": target_device_id,
                                    "payload_json": payload_json
                                })
                                .to_string(),
                            );
                        }
                    }
                }
            }
        }
    }

    if let Some(id) = registered_id {
        state.peers.remove(&id);
        info!("Peer disconnected: {id}");
    }
    writer_task.abort();
}

