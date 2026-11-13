use crate::crypto::identity::{decrypt_chunk, encrypt_chunk, DeviceIdentity, EphemeralDh};
use crate::db::storage::{now_unix_secs, Contact, TransferRecord};
use crate::network::nat::{bind_reusable_udp_socket, discover_wan_reflexive_address};
use crate::state::{AppState, IncomingRequestDecision, ManifestFileEntry};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

const CHUNK_SIZE: usize = 512 * 1024; // 512 KB plaintext per frame
const HEADER_LEN: usize = 36;
const MAGIC_BYTES: &[u8; 4] = b"RAYD";
const PARALLEL_WORKERS: usize = 6; // IDM-style 6 parallel chunk workers for large files
const PARALLEL_THRESHOLD_BYTES: u64 = 4 * 1024 * 1024; // 4 MB threshold for parallel multi-segment streaming

/// Forcefully pops up and focuses the main Raydrop window on the Win32 main UI thread,
/// even if the app was minimized in the System Tray or behind another foreground application.
pub fn popup_and_focus_main_window(app: &AppHandle) {
    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app_handle.get_webview_window("main") {
            let _ = w.set_always_on_top(true);
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
            let _ = w.request_user_attention(Some(tauri::UserAttentionType::Critical));
            let w_clone = w.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(1800)).await;
                let _ = w_clone.set_always_on_top(false);
                let _ = w_clone.set_focus();
            });
        }
    });
}

/// Computes the full BLAKE3 digest of a file on disk asynchronously.
async fn compute_file_blake3(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).await.map_err(|e| e.to_string())?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[derive(Debug, Serialize, Deserialize)]
struct HelloFrame {
    #[serde(rename = "type")]
    frame_type: String,
    session_id: String,
    sender_device_id: String,
    sender_alias: String,
    ed25519_pubkey: String,
    x25519_ephemeral_pubkey: String,
    signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HelloAckFrame {
    #[serde(rename = "type")]
    frame_type: String,
    session_id: String,
    receiver_device_id: String,
    receiver_alias: String,
    ed25519_pubkey: String,
    x25519_ephemeral_pubkey: String,
    signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ManifestFrame {
    #[serde(rename = "type")]
    frame_type: String,
    session_id: String,
    total_files: usize,
    total_size_bytes: u64,
    files: Vec<ManifestFileEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResumeOffsetEntry {
    pub file_id: u32,
    pub offset_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct HandshakeResponseFrame {
    #[serde(rename = "type")]
    frame_type: String,
    session_id: String,
    status: String, // "ACCEPTED" or "REJECTED"
    resume_offsets: Vec<ResumeOffsetEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileCompleteFrame {
    #[serde(rename = "type")]
    frame_type: String,
    session_id: String,
    file_id: u32,
    blake3_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IncomingTransferEvent {
    pub session_id: String,
    pub sender_device_id: String,
    pub sender_alias: String,
    pub network_route: String,
    pub total_files: usize,
    pub total_size_bytes: u64,
    pub files: Vec<ManifestFileEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferProgressEvent {
    pub session_id: String,
    pub peer_device_id: String,
    pub peer_alias: String,
    pub direction: String,
    pub network_route: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub speed_mbps: f64,
    pub eta_seconds: u64,
    pub status: String,
}

/// Sanitizes relative path from untrusted peer manifest to prevent directory traversal (`../`).
pub fn sanitize_relative_path(raw: &str) -> Result<PathBuf, String> {
    let normalized = raw.replace('\\', "/");
    let path = Path::new(&normalized);
    let mut clean = PathBuf::new();

    for comp in path.components() {
        match comp {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Unsafe relative path rejected: {raw}"));
            }
        }
    }

    if clean.as_os_str().is_empty() {
        return Err("Empty relative file path".to_string());
    }
    Ok(clean)
}

async fn write_json_packet<W: AsyncWrite + Unpin>(writer: &mut W, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let len = bytes.len() as u32;
    writer
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| e.to_string())?;
    writer.write_all(&bytes).await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn read_json_packet<R: AsyncRead + Unpin, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T, String> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| e.to_string())?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        return Err("Control packet exceeds maximum size (16MB)".to_string());
    }
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

fn build_quic_endpoints(port: u16) -> Result<quinn::Endpoint, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert = rcgen::generate_simple_self_signed(vec!["raydrop.local".to_string()])
        .map_err(|e| e.to_string())?;
    let cert_der = rustls::pki_types::CertificateDer::from(cert.cert);
    let priv_key = rustls::pki_types::PrivateKeyDer::Pkcs8(
        rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()),
    );

    let server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], priv_key)
        .map_err(|e| e.to_string())?;

    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
            .map_err(|e| e.to_string())?,
    ));

    let std_udp = bind_reusable_udp_socket(port)?;
    let runtime = quinn::default_runtime().ok_or_else(|| "No Tokio runtime".to_string())?;
    let mut endpoint = quinn::Endpoint::new(
        quinn::EndpointConfig::default(),
        Some(server_config),
        std_udp,
        runtime,
    )
    .map_err(|e| e.to_string())?;

    // Configure permissive client config (peer authentication is enforced at the Raydrop Ed25519 layer)
    let client_crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();
    let client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
            .map_err(|e| e.to_string())?,
    ));
    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
}

#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

pub fn start_transport_listeners(app: AppHandle, state: Arc<AppState>) {
    // 1. Start Direct TCP Listener (LAN high-speed transport)
    let app_tcp = app.clone();
    let state_tcp = state.clone();
    tauri::async_runtime::spawn(async move {
        let bind_addr = format!("0.0.0.0:{}", state_tcp.listen_port);
        match TcpListener::bind(&bind_addr).await {
            Ok(listener) => {
                info!("Raydrop TCP transport listening on {bind_addr}");
                while let Ok((stream, peer_addr)) = listener.accept().await {
                    let _ = stream.set_nodelay(true);
                    let app_c = app_tcp.clone();
                    let state_c = state_tcp.clone();
                    tauri::async_runtime::spawn(async move {
                        let (reader, writer) = tokio::io::split(stream);
                        if let Err(e) = handle_incoming_stream(
                            app_c,
                            state_c,
                            reader,
                            writer,
                            peer_addr,
                            "lan_direct".to_string(),
                        )
                        .await
                        {
                            warn!("Incoming TCP transfer error from {peer_addr}: {e}");
                        }
                    });
                }
            }
            Err(e) => warn!("Failed to bind TCP listener on {bind_addr}: {e}"),
        }
    });

    // 2. Start QUIC over UDP Listener (WAN NAT hole-punched & LAN QUIC transport)
    tauri::async_runtime::spawn(async move {
        match build_quic_endpoints(state.listen_port) {
            Ok(endpoint) => {
                *state.quic_endpoint.lock().unwrap() = Some(endpoint.clone());
                info!(
                    "Raydrop QUIC/UDP transport listening on 0.0.0.0:{}",
                    state.listen_port
                );
                while let Some(incoming) = endpoint.accept().await {
                    let app_c = app.clone();
                    let state_c = state.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(conn) = incoming.await {
                            let peer_addr = conn.remote_address();
                            if let Ok((send_stream, recv_stream)) = conn.accept_bi().await {
                                if let Err(e) = handle_incoming_stream(
                                    app_c,
                                    state_c,
                                    recv_stream,
                                    send_stream,
                                    peer_addr,
                                    "wan_p2p".to_string(),
                                )
                                .await
                                {
                                    warn!("Incoming QUIC transfer error from {peer_addr}: {e}");
                                }
                            }
                        }
                    });
                }
            }
            Err(e) => warn!("Failed to initialize QUIC endpoint: {e}"),
        }
    });
}

/// Creates a virtual full-duplex `AsyncRead + AsyncWrite` stream bridged over `RELAY_CONTROL`
/// WebSocket messages. All payload bytes written to this stream are already E2EE encrypted
/// (Ed25519 + X25519 ECDH + ChaCha20-Poly1305 AEAD).
pub(crate) fn create_ws_relay_stream(
    state: Arc<AppState>,
    session_id: String,
    peer_device_id: String,
) -> (ReadHalf<DuplexStream>, WriteHalf<DuplexStream>) {
    let (local_stream, bridge_stream) = tokio::io::duplex(512 * 1024);
    let (in_tx, mut in_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    state
        .relay_streams
        .lock()
        .unwrap()
        .insert(session_id.clone(), in_tx);

    let (mut bridge_read, mut bridge_write) = tokio::io::split(bridge_stream);

    // Pump inbound bytes received from signaling WebSocket into the local duplex reader
    tauri::async_runtime::spawn(async move {
        while let Some(bytes) = in_rx.recv().await {
            if bytes.is_empty() {
                let _ = bridge_write.shutdown().await;
                break;
            }
            if bridge_write.write_all(&bytes).await.is_err() {
                break;
            }
            let _ = bridge_write.flush().await;
        }
    });

    // Pump outbound bytes written by the local transfer engine into `RELAY_CONTROL` WebSocket frames
    let state_out = state.clone();
    let sid_out = session_id.clone();
    let my_id = state.identity.device_id.clone();
    let target_id = peer_device_id;

    tauri::async_runtime::spawn(async move {
        let mut buf = vec![0u8; 24 * 1024];
        loop {
            match bridge_read.read(&mut buf).await {
                Ok(0) | Err(_) => {
                    if let Some(tx) = state_out.signaling_tx.lock().unwrap().as_ref() {
                        let eof_msg = json!({
                            "type": "RELAY_CONTROL",
                            "session_id": sid_out,
                            "from_device_id": my_id,
                            "target_device_id": target_id,
                            "payload_json": ""
                        })
                        .to_string();
                        let _ = tx.send(eof_msg);
                    }
                    break;
                }
                Ok(n) => {
                    let hex_data = hex::encode(&buf[..n]);
                    let sent_ok = if let Some(tx) = state_out.signaling_tx.lock().unwrap().as_ref()
                    {
                        let msg = json!({
                            "type": "RELAY_CONTROL",
                            "session_id": sid_out,
                            "from_device_id": my_id,
                            "target_device_id": target_id,
                            "payload_json": hex_data
                        })
                        .to_string();
                        tx.send(msg).is_ok()
                    } else {
                        false
                    };
                    if !sent_ok {
                        break;
                    }
                }
            }
        }
        state_out.relay_streams.lock().unwrap().remove(&sid_out);
    });

    tokio::io::split(local_stream)
}

pub(crate) async fn handle_incoming_stream<R, W>(
    app: AppHandle,
    state: Arc<AppState>,
    mut reader: R,
    mut writer: W,
    peer_addr: SocketAddr,
    network_route: String,
) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    // Privacy Rule #1: Check if Discoverability is ON
    if !state.is_discoverable() {
        info!("Auto-dropping incoming connection from {peer_addr} (Discoverability OFF)");
        return Ok(());
    }

    // Step 1: Read & verify HELLO frame
    let hello: HelloFrame = read_json_packet(&mut reader).await?;
    if hello.frame_type != "HELLO" {
        return Err("Expected HELLO frame".to_string());
    }

    let session_uuid = Uuid::parse_str(&hello.session_id).map_err(|e| e.to_string())?;
    let session_id_bytes = *session_uuid.as_bytes();

    let signed_msg = format!("{}:{}", hello.session_id, hello.x25519_ephemeral_pubkey);
    DeviceIdentity::verify_peer_identity(
        &hello.sender_device_id,
        &hello.ed25519_pubkey,
        signed_msg.as_bytes(),
        &hello.signature,
    )?;

    // Privacy Rule #2: Check Contact Trust Level (Own devices under same email auto-accept!)
    let existing_contact = state.db.get_contact(&hello.sender_device_id)?;
    let is_own_device = existing_contact
        .as_ref()
        .map(|c| c.is_own_device)
        .unwrap_or(false);
    let trust_level = if is_own_device {
        "trusted_auto_accept".to_string()
    } else {
        existing_contact
            .as_ref()
            .map(|c| c.trust_level.clone())
            .unwrap_or_else(|| "ask".to_string())
    };

    if trust_level == "blocked" {
        info!("Blocked peer {} attempted connection", hello.sender_device_id);
        return Ok(());
    }

    // Upsert contact with verified Ed25519 public key (TOFU)
    let contact = Contact {
        device_id: hello.sender_device_id.clone(),
        alias_name: hello.sender_alias.clone(),
        user_email: existing_contact.as_ref().and_then(|c| c.user_email.clone()),
        user_name: existing_contact.as_ref().and_then(|c| c.user_name.clone()),
        os_type: existing_contact.as_ref().and_then(|c| c.os_type.clone()),
        is_own_device,
        ed25519_pubkey: Some(hello.ed25519_pubkey.clone()),
        network_type: if network_route == "lan_direct" {
            "lan".to_string()
        } else {
            "wan".to_string()
        },
        last_known_ip: if peer_addr.port() == 0 {
            existing_contact.as_ref().and_then(|c| c.last_known_ip.clone())
        } else {
            Some(peer_addr.ip().to_string())
        },
        last_known_port: if peer_addr.port() == 0 {
            existing_contact.as_ref().and_then(|c| c.last_known_port)
        } else {
            Some(peer_addr.port())
        },
        trust_level: trust_level.clone(),
        is_online: true,
        last_seen_at: now_unix_secs(),
        created_at: now_unix_secs(),
    };
    let _ = state.db.upsert_contact(&contact);
    let _ = app.emit("contacts_updated", ());

    // Step 2: Generate ephemeral X25519 and reply with HELLO_ACK
    let my_dh = EphemeralDh::generate();
    let my_eph_hex = my_dh.public_hex();
    let ack_sign_msg = format!("{}:{}", hello.session_id, my_eph_hex);
    let my_sig = state.identity.sign_message(ack_sign_msg.as_bytes());
    let my_alias = state.settings.lock().unwrap().alias_name.clone();

    let hello_ack = HelloAckFrame {
        frame_type: "HELLO_ACK".to_string(),
        session_id: hello.session_id.clone(),
        receiver_device_id: state.identity.device_id.clone(),
        receiver_alias: my_alias,
        ed25519_pubkey: state.identity.public_hex(),
        x25519_ephemeral_pubkey: my_eph_hex,
        signature: my_sig,
    };
    write_json_packet(&mut writer, &hello_ack).await?;

    let session_key = my_dh.derive_session_key(&hello.x25519_ephemeral_pubkey, &session_id_bytes)?;

    // Step 3: Read MANIFEST frame
    let manifest: ManifestFrame = read_json_packet(&mut reader).await?;
    if manifest.frame_type != "MANIFEST" {
        return Err("Expected MANIFEST frame".to_string());
    }

    // Determine Acceptance:
    // 1. If this session_id was already accepted earlier (resuming after Wi-Fi drop or PC restart), auto-resume into saved target_path!
    // 2. If peer is trusted_auto_accept (or own device), auto-accept into default_dl!
    // 3. Otherwise, pop up main window to the foreground and prompt the user via IncomingTransferEvent modal!
    let default_dl = state.settings.lock().unwrap().download_dir.clone();
    let existing_session_dir = state
        .db
        .get_transfer(&hello.session_id)
        .ok()
        .flatten()
        .and_then(|t| {
            if t.status != "rejected" && t.status != "cancelled" {
                t.target_path
            } else {
                None
            }
        });

    let decision = if let Some(saved_dir) = existing_session_dir {
        info!(
            "Auto-resuming previously accepted transfer session {} into {}",
            hello.session_id, saved_dir
        );
        IncomingRequestDecision {
            accepted: true,
            target_directory: Some(saved_dir),
        }
    } else if trust_level == "trusted_auto_accept" {
        popup_and_focus_main_window(&app);
        IncomingRequestDecision {
            accepted: true,
            target_directory: Some(default_dl.clone()),
        }
    } else {
        let (tx, rx) = oneshot::channel();
        state
            .pending_requests
            .lock()
            .unwrap()
            .insert(hello.session_id.clone(), tx);

        let evt = IncomingTransferEvent {
            session_id: hello.session_id.clone(),
            sender_device_id: hello.sender_device_id.clone(),
            sender_alias: hello.sender_alias.clone(),
            network_route: network_route.clone(),
            total_files: manifest.total_files,
            total_size_bytes: manifest.total_size_bytes,
            files: manifest.files.clone(),
        };
        popup_and_focus_main_window(&app);
        let _ = app.emit("incoming_transfer_request", &evt);

        match tokio::time::timeout(Duration::from_secs(120), rx).await {
            Ok(Ok(dec)) => dec,
            _ => {
                state
                    .pending_requests
                    .lock()
                    .unwrap()
                    .remove(&hello.session_id);
                IncomingRequestDecision {
                    accepted: false,
                    target_directory: None,
                }
            }
        }
    };

    if !decision.accepted {
        let reject_resp = HandshakeResponseFrame {
            frame_type: "HANDSHAKE_RESPONSE".to_string(),
            session_id: hello.session_id.clone(),
            status: "REJECTED".to_string(),
            resume_offsets: vec![],
        };
        let _ = write_json_packet(&mut writer, &reject_resp).await;

        let record = TransferRecord {
            id: hello.session_id,
            peer_device_id: hello.sender_device_id,
            peer_alias: hello.sender_alias,
            direction: "incoming".to_string(),
            network_route,
            total_files: manifest.total_files,
            total_bytes: manifest.total_size_bytes,
            transferred_bytes: 0,
            status: "rejected".to_string(),
            target_path: None,
            source_paths: None,
            created_at: now_unix_secs(),
        };
        let _ = state.db.upsert_transfer(&record);
        let _ = app.emit("transfer_state_changed", &record);
        return Ok(());
    }

    let target_root = PathBuf::from(decision.target_directory.unwrap_or(default_dl));
    std::fs::create_dir_all(&target_root).map_err(|e| e.to_string())?;

    // Check existing `.raypart` or completed files for instant O(1) resume offset calculation
    let mut resume_offsets = Vec::new();
    let mut file_map: HashMap<u32, (PathBuf, PathBuf, u64)> = HashMap::new();
    let mut already_received_bytes: u64 = 0;

    for f in &manifest.files {
        let rel = sanitize_relative_path(&f.relative_path)?;
        let final_path = target_root.join(&rel);
        if let Some(parent) = final_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let part_name = format!(
            "{}.raypart",
            final_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
        );
        let part_path = final_path.with_file_name(part_name);

        let mut offset_bytes = 0u64;

        if part_path.exists() {
            if let Ok(meta) = std::fs::metadata(&part_path) {
                let existing_len = meta.len();
                if existing_len > 0 && existing_len <= f.size_bytes {
                    // Roll back by up to (PARALLEL_WORKERS * CHUNK_SIZE) aligned to CHUNK_SIZE
                    // so any in-flight parallel tail chunks from an interrupted session are cleanly overwritten
                    let safety_margin = (PARALLEL_WORKERS as u64) * (CHUNK_SIZE as u64);
                    let safe_offset = if f.size_bytes >= PARALLEL_THRESHOLD_BYTES {
                        if existing_len > safety_margin {
                            ((existing_len - safety_margin) / (CHUNK_SIZE as u64))
                                * (CHUNK_SIZE as u64)
                        } else {
                            0
                        }
                    } else {
                        existing_len.min(f.size_bytes.saturating_sub(1))
                    };

                    if safe_offset > 0 {
                        if let Ok(f_handle) =
                            std::fs::OpenOptions::new().write(true).open(&part_path)
                        {
                            let _ = f_handle.set_len(safe_offset);
                        }
                        offset_bytes = safe_offset;
                        already_received_bytes += safe_offset;
                    } else {
                        let _ = std::fs::remove_file(&part_path);
                    }
                } else {
                    let _ = std::fs::remove_file(&part_path);
                }
            }
        }

        resume_offsets.push(ResumeOffsetEntry {
            file_id: f.file_id,
            offset_bytes,
        });
        file_map.insert(f.file_id, (part_path, final_path, offset_bytes));
    }

    let accept_resp = HandshakeResponseFrame {
        frame_type: "HANDSHAKE_RESPONSE".to_string(),
        session_id: hello.session_id.clone(),
        status: "ACCEPTED".to_string(),
        resume_offsets,
    };
    write_json_packet(&mut writer, &accept_resp).await?;

    let cancel_flag = Arc::new(AtomicBool::new(false));
    state
        .active_cancellations
        .lock()
        .unwrap()
        .insert(hello.session_id.clone(), cancel_flag.clone());

    let mut record = TransferRecord {
        id: hello.session_id.clone(),
        peer_device_id: hello.sender_device_id.clone(),
        peer_alias: hello.sender_alias.clone(),
        direction: "incoming".to_string(),
        network_route: network_route.clone(),
        total_files: manifest.total_files,
        total_bytes: manifest.total_size_bytes,
        transferred_bytes: already_received_bytes,
        status: "in_progress".to_string(),
        target_path: Some(target_root.to_string_lossy().to_string()),
        source_paths: None,
        created_at: now_unix_secs(),
    };
    let _ = state.db.upsert_transfer(&record);
    let _ = app.emit("transfer_state_changed", &record);

    let start_time = Instant::now();
    let mut last_emit = Instant::now();
    let mut last_db_sync = Instant::now();
    let mut completed_files = 0usize;
    let mut open_handles: HashMap<u32, File> = HashMap::new();

    // Receive binary chunks (supporting out-of-order parallel segment offsets) & FILE_COMPLETE frames
    while completed_files < manifest.total_files {
        if cancel_flag.load(Ordering::Relaxed) {
            for (_, mut h) in open_handles.drain() {
                let _ = h.flush().await;
            }
            let db_status = state
                .db
                .get_transfer(&record.id)
                .ok()
                .flatten()
                .map(|r| r.status)
                .unwrap_or_else(|| "paused".to_string());
            record.status = if db_status == "cancelled" {
                "cancelled".to_string()
            } else {
                "paused".to_string()
            };
            let _ = state.db.upsert_transfer(&record);
            let _ = app.emit("transfer_state_changed", &record);
            return Err("Transfer paused/cancelled".to_string());
        }

        let mut header = [0u8; HEADER_LEN];
        if let Err(e) = reader.read_exact(&mut header).await {
            for (_, mut h) in open_handles.drain() {
                let _ = h.flush().await;
            }
            record.status = "paused".to_string();
            let _ = state.db.upsert_transfer(&record);
            let _ = app.emit("transfer_state_changed", &record);
            return Err(format!("Stream disconnected (saved .raypart for resume): {e}"));
        }

        if &header[0..4] != MAGIC_BYTES {
            return Err("Invalid frame magic bytes (expected RAYD)".to_string());
        }
        if &header[4..20] != session_id_bytes {
            return Err("Session ID mismatch in binary frame".to_string());
        }

        let file_id = u32::from_be_bytes(header[20..24].try_into().unwrap());
        let offset = u64::from_be_bytes(header[24..32].try_into().unwrap());
        let cipher_len = u32::from_be_bytes(header[32..36].try_into().unwrap()) as usize;

        if cipher_len > CHUNK_SIZE + 1024 {
            return Err("Encrypted chunk exceeds maximum allowed size".to_string());
        }

        let mut ciphertext = vec![0u8; cipher_len];
        if let Err(e) = reader.read_exact(&mut ciphertext).await {
            for (_, mut h) in open_handles.drain() {
                let _ = h.flush().await;
            }
            record.status = "paused".to_string();
            let _ = state.db.upsert_transfer(&record);
            let _ = app.emit("transfer_state_changed", &record);
            return Err(format!("Stream disconnected mid-chunk (saved .raypart for resume): {e}"));
        }

        let plaintext = decrypt_chunk(&session_key, file_id, offset, &header, &ciphertext)?;

        if file_id == 0 {
            // Control Frame: FILE_COMPLETE
            let complete: FileCompleteFrame =
                serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
            if complete.frame_type == "FILE_COMPLETE" {
                if let Some(mut handle) = open_handles.remove(&complete.file_id) {
                    let _ = handle.flush().await;
                    let _ = handle.sync_all().await;
                    drop(handle);
                }
                if let Some((part_path, final_path, _)) = file_map.remove(&complete.file_id) {
                    let computed_hash = compute_file_blake3(&part_path).await?;
                    if computed_hash != complete.blake3_hash {
                        record.status = "failed".to_string();
                        let _ = state.db.upsert_transfer(&record);
                        let _ = app.emit("transfer_state_changed", &record);
                        return Err(format!(
                            "BLAKE3 integrity mismatch on file_id {}: expected {}, got {}",
                            complete.file_id, complete.blake3_hash, computed_hash
                        ));
                    }
                    if final_path.exists() {
                        let _ = tokio::fs::remove_file(&final_path).await;
                    }
                    tokio::fs::rename(&part_path, &final_path)
                        .await
                        .map_err(|e| e.to_string())?;
                    completed_files += 1;
                }
            }
        } else {
            // Data Chunk for `file_id` at exact `offset` (supports parallel multi-segment writes)
            if !open_handles.contains_key(&file_id) {
                if let Some((part_path, _, _)) = file_map.get(&file_id) {
                    let file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .read(true)
                        .open(part_path)
                        .await
                        .map_err(|e| e.to_string())?;
                    open_handles.insert(file_id, file);
                }
            }

            if let Some(file) = open_handles.get_mut(&file_id) {
                file.seek(std::io::SeekFrom::Start(offset))
                    .await
                    .map_err(|e| e.to_string())?;
                file.write_all(&plaintext)
                    .await
                    .map_err(|e| e.to_string())?;
                record.transferred_bytes =
                    (record.transferred_bytes + plaintext.len() as u64).min(record.total_bytes);
            }

            if last_db_sync.elapsed() >= Duration::from_millis(1200) {
                let _ = state.db.upsert_transfer(&record);
                last_db_sync = Instant::now();
            }

            if last_emit.elapsed() >= Duration::from_millis(120) {
                let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
                let session_delta = record
                    .transferred_bytes
                    .saturating_sub(already_received_bytes);
                let speed_mbps = (session_delta as f64 / (1024.0 * 1024.0)) / elapsed;
                let remaining = record
                    .total_bytes
                    .saturating_sub(record.transferred_bytes);
                let eta_seconds = if speed_mbps > 0.01 {
                    ((remaining as f64 / (1024.0 * 1024.0)) / speed_mbps) as u64
                } else {
                    0
                };

                let _ = app.emit(
                    "transfer_progress",
                    &TransferProgressEvent {
                        session_id: record.id.clone(),
                        peer_device_id: record.peer_device_id.clone(),
                        peer_alias: record.peer_alias.clone(),
                        direction: "incoming".to_string(),
                        network_route: network_route.clone(),
                        transferred_bytes: record.transferred_bytes,
                        total_bytes: record.total_bytes,
                        speed_mbps,
                        eta_seconds,
                        status: "in_progress".to_string(),
                    },
                );
                last_emit = Instant::now();
            }
        }
    }

    state
        .active_cancellations
        .lock()
        .unwrap()
        .remove(&record.id);

    record.status = "completed".to_string();
    let _ = state.db.upsert_transfer(&record);
    let _ = app.emit("transfer_state_changed", &record);
    Ok(())
}

/// Collects files (and recursively walks folders) into a manifest list alongside their local source paths.
fn collect_outgoing_files(
    raw_paths: &[String],
) -> Result<(Vec<(u32, PathBuf, ManifestFileEntry)>, u64), String> {
    let mut items = Vec::new();
    let mut total_bytes = 0u64;
    let mut next_id = 1u32;

    for raw in raw_paths {
        let p = PathBuf::from(raw);
        if !p.exists() {
            continue;
        }
        if p.is_file() {
            let meta = std::fs::metadata(&p).map_err(|e| e.to_string())?;
            let size = meta.len();
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let rel_name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();

            items.push((
                next_id,
                p,
                ManifestFileEntry {
                    file_id: next_id,
                    relative_path: rel_name,
                    size_bytes: size,
                    modified_unix_ms: modified,
                },
            ));
            total_bytes += size;
            next_id += 1;
        } else if p.is_dir() {
            let base_parent = p.parent().unwrap_or(Path::new(""));
            for entry in WalkDir::new(&p).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let fp = entry.path().to_path_buf();
                    let meta = entry.metadata().map_err(|e| e.to_string())?;
                    let size = meta.len();
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    let rel = fp
                        .strip_prefix(base_parent)
                        .unwrap_or(&fp)
                        .to_string_lossy()
                        .replace('\\', "/");

                    items.push((
                        next_id,
                        fp,
                        ManifestFileEntry {
                            file_id: next_id,
                            relative_path: rel,
                            size_bytes: size,
                            modified_unix_ms: modified,
                        },
                    ));
                    total_bytes += size;
                    next_id += 1;
                }
            }
        }
    }

    if items.is_empty() {
        return Err("No valid files found in selection".to_string());
    }
    Ok((items, total_bytes))
}

pub async fn initiate_peer_transfer(
    app: AppHandle,
    state: Arc<AppState>,
    target_device_id: String,
    file_paths: Vec<String>,
) -> Result<String, String> {
    start_or_resume_transfer(app, state, None, target_device_id, file_paths).await
}

pub async fn resume_peer_transfer(
    app: AppHandle,
    state: Arc<AppState>,
    session_id: String,
) -> Result<String, String> {
    let existing = state
        .db
        .get_transfer(&session_id)?
        .ok_or_else(|| format!("Transfer session {session_id} not found"))?;
    let source_json = existing
        .source_paths
        .ok_or_else(|| "This transfer does not have local source paths to resume".to_string())?;
    let file_paths: Vec<String> =
        serde_json::from_str(&source_json).map_err(|e| e.to_string())?;
    start_or_resume_transfer(
        app,
        state,
        Some(session_id),
        existing.peer_device_id,
        file_paths,
    )
    .await
}

async fn start_or_resume_transfer(
    app: AppHandle,
    state: Arc<AppState>,
    existing_session_id: Option<String>,
    target_device_id: String,
    file_paths: Vec<String>,
) -> Result<String, String> {
    let contact = match state.db.get_contact(&target_device_id)? {
        Some(c) => c,
        None => {
            let fallback = Contact {
                device_id: target_device_id.clone(),
                alias_name: target_device_id.clone(),
                user_email: None,
                user_name: None,
                os_type: None,
                is_own_device: false,
                ed25519_pubkey: None,
                network_type: "wan".to_string(),
                last_known_ip: None,
                last_known_port: None,
                trust_level: "ask".to_string(),
                is_online: true,
                last_seen_at: now_unix_secs(),
                created_at: now_unix_secs(),
            };
            let _ = state.db.upsert_contact(&fallback);
            fallback
        }
    };

    let (files_with_paths, total_bytes) = collect_outgoing_files(&file_paths)?;
    let session_uuid = match &existing_session_id {
        Some(sid) => Uuid::parse_str(sid).unwrap_or_else(|_| Uuid::new_v4()),
        None => Uuid::new_v4(),
    };
    let session_id = session_uuid.to_string();
    let source_paths_json = serde_json::to_string(&file_paths).ok();

    let prev_transferred = existing_session_id
        .as_ref()
        .and_then(|sid| state.db.get_transfer(sid).ok().flatten())
        .map(|t| t.transferred_bytes)
        .unwrap_or(0);

    // Immediately insert & emit the outgoing TransferRecord so the UI displays the progress bar right away
    let initial_record = TransferRecord {
        id: session_id.clone(),
        peer_device_id: contact.device_id.clone(),
        peer_alias: contact.alias_name.clone(),
        direction: "outgoing".to_string(),
        network_route: if contact.network_type == "lan" {
            "lan_direct".to_string()
        } else {
            "wan_p2p".to_string()
        },
        total_files: files_with_paths.len(),
        total_bytes,
        transferred_bytes: prev_transferred,
        status: "in_progress".to_string(),
        target_path: None,
        source_paths: source_paths_json.clone(),
        created_at: now_unix_secs(),
    };
    let _ = state.db.upsert_transfer(&initial_record);
    let _ = app.emit("transfer_state_changed", &initial_record);

    let cancel_flag = Arc::new(AtomicBool::new(false));
    state
        .active_cancellations
        .lock()
        .unwrap()
        .insert(session_id.clone(), cancel_flag.clone());

    let app_c = app.clone();
    let state_c = state.clone();
    let sid_c = session_id.clone();

    tauri::async_runtime::spawn(async move {
        // Automatic Reconnect & Resume Loop (up to 15 attempts if Wi-Fi drops mid-transfer)
        let max_attempts = 15usize;
        for attempt in 1..=max_attempts {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }

            match run_outgoing_session(
                app_c.clone(),
                state_c.clone(),
                sid_c.clone(),
                session_uuid,
                contact.clone(),
                files_with_paths.clone(),
                total_bytes,
                source_paths_json.clone(),
                cancel_flag.clone(),
            )
            .await
            {
                Ok(()) => return,
                Err(e) => {
                    if cancel_flag.load(Ordering::Relaxed) {
                        warn!("Outgoing transfer {sid_c} paused/cancelled by user.");
                        if let Ok(Some(mut rec)) = state_c.db.get_transfer(&sid_c) {
                            if rec.status == "in_progress" {
                                rec.status = "paused".to_string();
                                let _ = state_c.db.upsert_transfer(&rec);
                                let _ = app_c.emit("transfer_state_changed", &rec);
                            }
                        }
                        return;
                    }

                    warn!(
                        "Outgoing transfer {sid_c} interrupted (attempt {attempt}/{max_attempts}): {e}"
                    );
                    if let Ok(Some(mut rec)) = state_c.db.get_transfer(&sid_c) {
                        rec.status = "paused".to_string();
                        let _ = state_c.db.upsert_transfer(&rec);
                        let _ = app_c.emit("transfer_state_changed", &rec);
                    }

                    if attempt < max_attempts {
                        // Wait for Wi-Fi / network to stabilize, then automatically resume from `.raypart` offset
                        tokio::time::sleep(Duration::from_secs(4)).await;
                        if !cancel_flag.load(Ordering::Relaxed) {
                            if let Ok(Some(mut rec)) = state_c.db.get_transfer(&sid_c) {
                                rec.status = "in_progress".to_string();
                                let _ = state_c.db.upsert_transfer(&rec);
                                let _ = app_c.emit("transfer_state_changed", &rec);
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(session_id)
}

#[allow(clippy::too_many_arguments)]
async fn run_outgoing_session(
    app: AppHandle,
    state: Arc<AppState>,
    session_id: String,
    session_uuid: Uuid,
    contact: Contact,
    files_with_paths: Vec<(u32, PathBuf, ManifestFileEntry)>,
    total_bytes: u64,
    source_paths_json: Option<String>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<(), String> {
    // Refresh latest known contact info from DB in case mDNS or Signaling updated IP/Port
    let fresh_contact = state
        .db
        .get_contact(&contact.device_id)
        .ok()
        .flatten()
        .unwrap_or(contact);

    // 1. Try direct LAN TCP connection if LAN IP:Port is already known
    if let (Some(ip), Some(port)) = (&fresh_contact.last_known_ip, fresh_contact.last_known_port) {
        let target_addr = format!("{ip}:{port}");
        if let Ok(Ok(tcp_stream)) = tokio::time::timeout(
            Duration::from_millis(1200),
            TcpStream::connect(&target_addr),
        )
        .await
        {
            let _ = tcp_stream.set_nodelay(true);
            let (reader, writer) = tokio::io::split(tcp_stream);
            return stream_outgoing_files(
                app,
                state,
                session_id,
                session_uuid,
                fresh_contact,
                files_with_paths,
                total_bytes,
                source_paths_json,
                cancel_flag,
                reader,
                writer,
                "lan_direct".to_string(),
            )
            .await;
        }
    }

    // 2. Coordinate Candidate Exchange & NAT Hole-Punching via Signaling Server
    let signaling_tx = state
        .signaling_tx
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Cannot reach peer on LAN and WAN Signaling Server is not connected".to_string())?;

    let stun_addr = state.settings.lock().unwrap().stun_addr.clone();
    let my_wan = discover_wan_reflexive_address(state.listen_port, &stun_addr)
        .await
        .or_else(|| state.wan_reflexive_addr.lock().unwrap().clone());
    let my_lan = vec![format!("{}:{}", state.local_ip, state.listen_port)];
    let my_alias = state.settings.lock().unwrap().alias_name.clone();

    let (cand_tx, cand_rx) = oneshot::channel();
    state
        .pending_wan_candidates
        .lock()
        .unwrap()
        .insert(session_id.clone(), cand_tx);

    let intent_msg = json!({
        "type": "CONNECT_INTENT",
        "session_id": session_id,
        "from_device_id": state.identity.device_id,
        "from_alias": my_alias,
        "from_ed25519_pubkey": state.identity.public_hex(),
        "target_device_id": fresh_contact.device_id,
        "lan_candidates": my_lan,
        "wan_candidate": my_wan
    })
    .to_string();
    let _ = signaling_tx.send(intent_msg);

    if let Ok(Ok(answer)) = tokio::time::timeout(Duration::from_millis(3500), cand_rx).await {
        // 2a. Try Peer's LAN Candidates via Direct TCP first (works when both PCs are on the same Wi-Fi/LAN)
        for lan_cand in &answer.lan_candidates {
            if let Ok(Ok(tcp_stream)) = tokio::time::timeout(
                Duration::from_millis(1000),
                TcpStream::connect(lan_cand),
            )
            .await
            {
                let _ = tcp_stream.set_nodelay(true);
                let (reader, writer) = tokio::io::split(tcp_stream);
                return stream_outgoing_files(
                    app,
                    state,
                    session_id,
                    session_uuid,
                    fresh_contact,
                    files_with_paths,
                    total_bytes,
                    source_paths_json,
                    cancel_flag,
                    reader,
                    writer,
                    "lan_direct".to_string(),
                )
                .await;
            }
        }

        // 2b. Try Direct WAN QUIC over hole-punched UDP using the shared QUIC endpoint
        if let Some(target_wan_str) = answer.wan_candidate {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if let Ok(target_addr) = target_wan_str.parse::<SocketAddr>() {
                let endpoint_opt = state.quic_endpoint.lock().unwrap().clone();
                if let Some(endpoint) = endpoint_opt {
                    if let Ok(connecting) = endpoint.connect(target_addr, "raydrop.local") {
                        if let Ok(Ok(quic_conn)) =
                            tokio::time::timeout(Duration::from_millis(2000), connecting).await
                        {
                            if let Ok((send_stream, recv_stream)) = quic_conn.open_bi().await {
                                return stream_outgoing_files(
                                    app,
                                    state,
                                    session_id,
                                    session_uuid,
                                    fresh_contact,
                                    files_with_paths,
                                    total_bytes,
                                    source_paths_json,
                                    cancel_flag,
                                    recv_stream,
                                    send_stream,
                                    "wan_p2p".to_string(),
                                )
                                .await;
                            }
                        }
                    }
                }
            }
        }
    } else {
        state
            .pending_wan_candidates
            .lock()
            .unwrap()
            .remove(&session_id);
    }

    // 3. Guaranteed Fallback: Stream End-to-End Encrypted (Ed25519 + X25519 + ChaCha20-Poly1305)
    // frames over the active WSS Signaling tunnel (`RELAY_CONTROL`) when direct sockets are blocked by NAT/Firewall
    info!(
        "Direct socket blocked by NAT/Firewall for session {session_id}; using E2EE WSS stream tunnel to {}",
        fresh_contact.device_id
    );
    let (relay_reader, relay_writer) =
        create_ws_relay_stream(state.clone(), session_id.clone(), fresh_contact.device_id.clone());

    stream_outgoing_files(
        app,
        state,
        session_id,
        session_uuid,
        fresh_contact,
        files_with_paths,
        total_bytes,
        source_paths_json,
        cancel_flag,
        relay_reader,
        relay_writer,
        "wan_p2p".to_string(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn stream_outgoing_files<R, W>(
    app: AppHandle,
    state: Arc<AppState>,
    session_id: String,
    session_uuid: Uuid,
    contact: Contact,
    files_with_paths: Vec<(u32, PathBuf, ManifestFileEntry)>,
    total_bytes: u64,
    source_paths_json: Option<String>,
    cancel_flag: Arc<AtomicBool>,
    mut reader: R,
    mut writer: W,
    network_route: String,
) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let session_id_bytes = *session_uuid.as_bytes();
    let my_dh = EphemeralDh::generate();
    let my_eph_hex = my_dh.public_hex();
    let sign_msg = format!("{session_id}:{my_eph_hex}");
    let sig = state.identity.sign_message(sign_msg.as_bytes());
    let my_alias = state.settings.lock().unwrap().alias_name.clone();

    // Step 1: Send HELLO
    let hello = HelloFrame {
        frame_type: "HELLO".to_string(),
        session_id: session_id.clone(),
        sender_device_id: state.identity.device_id.clone(),
        sender_alias: my_alias,
        ed25519_pubkey: state.identity.public_hex(),
        x25519_ephemeral_pubkey: my_eph_hex,
        signature: sig,
    };
    write_json_packet(&mut writer, &hello).await?;

    // Step 2: Receive & Verify HELLO_ACK
    let hello_ack: HelloAckFrame = read_json_packet(&mut reader).await?;
    if hello_ack.frame_type != "HELLO_ACK" {
        return Err("Expected HELLO_ACK from receiver".to_string());
    }
    let ack_signed_msg = format!("{}:{}", session_id, hello_ack.x25519_ephemeral_pubkey);
    DeviceIdentity::verify_peer_identity(
        &hello_ack.receiver_device_id,
        &hello_ack.ed25519_pubkey,
        ack_signed_msg.as_bytes(),
        &hello_ack.signature,
    )?;

    let session_key =
        my_dh.derive_session_key(&hello_ack.x25519_ephemeral_pubkey, &session_id_bytes)?;

    // Step 3: Send Instant MANIFEST (no pre-hashing delay)
    let manifest_entries: Vec<ManifestFileEntry> =
        files_with_paths.iter().map(|(_, _, m)| m.clone()).collect();
    let manifest = ManifestFrame {
        frame_type: "MANIFEST".to_string(),
        session_id: session_id.clone(),
        total_files: manifest_entries.len(),
        total_size_bytes: total_bytes,
        files: manifest_entries,
    };
    write_json_packet(&mut writer, &manifest).await?;

    // Step 4: Wait for Receiver HANDSHAKE_RESPONSE
    let hs_resp: HandshakeResponseFrame = read_json_packet(&mut reader).await?;
    if hs_resp.status != "ACCEPTED" {
        let rejected_record = TransferRecord {
            id: session_id.clone(),
            peer_device_id: contact.device_id.clone(),
            peer_alias: contact.alias_name.clone(),
            direction: "outgoing".to_string(),
            network_route: network_route.clone(),
            total_files: files_with_paths.len(),
            total_bytes,
            transferred_bytes: 0,
            status: "rejected".to_string(),
            target_path: None,
            source_paths: source_paths_json,
            created_at: now_unix_secs(),
        };
        let _ = state.db.upsert_transfer(&rejected_record);
        let _ = app.emit("transfer_state_changed", &rejected_record);
        return Ok(());
    }

    let resume_map: HashMap<u32, u64> = hs_resp
        .resume_offsets
        .into_iter()
        .map(|r| (r.file_id, r.offset_bytes))
        .collect();

    let already_sent_bytes: u64 = files_with_paths
        .iter()
        .map(|(fid, _, entry)| resume_map.get(fid).copied().unwrap_or(0).min(entry.size_bytes))
        .sum();

    let mut record = TransferRecord {
        id: session_id.clone(),
        peer_device_id: contact.device_id.clone(),
        peer_alias: contact.alias_name.clone(),
        direction: "outgoing".to_string(),
        network_route: network_route.clone(),
        total_files: files_with_paths.len(),
        total_bytes,
        transferred_bytes: already_sent_bytes,
        status: "in_progress".to_string(),
        target_path: None,
        source_paths: source_paths_json,
        created_at: now_unix_secs(),
    };
    let _ = state.db.upsert_transfer(&record);
    let _ = app.emit("transfer_state_changed", &record);

    let start_time = Instant::now();
    let mut last_emit = Instant::now();
    let mut last_db_sync = Instant::now();

    // Step 5: Stream each file using IDM-style Parallel Multi-Segment Reader + Encryptor Workers
    for (file_id, path, entry) in files_with_paths {
        let resume_offset = resume_map
            .get(&file_id)
            .copied()
            .unwrap_or(0)
            .min(entry.size_bytes);

        let path_for_hash = path.clone();
        let hash_task = tauri::async_runtime::spawn(async move {
            compute_file_blake3(&path_for_hash).await
        });

        if resume_offset < entry.size_bytes {
            let remaining_file_bytes = entry.size_bytes - resume_offset;
            let worker_count = if remaining_file_bytes >= PARALLEL_THRESHOLD_BYTES {
                PARALLEL_WORKERS
            } else {
                1
            };

            let next_offset = Arc::new(AtomicU64::new(resume_offset));
            let (frame_tx, mut frame_rx) =
                mpsc::channel::<Result<(Vec<u8>, Vec<u8>, usize), String>>(worker_count * 2);

            for _ in 0..worker_count {
                let path_w = path.clone();
                let next_offset_w = next_offset.clone();
                let cancel_w = cancel_flag.clone();
                let tx_w = frame_tx.clone();
                let file_size = entry.size_bytes;

                tauri::async_runtime::spawn(async move {
                    let mut file = match File::open(&path_w).await {
                        Ok(f) => f,
                        Err(e) => {
                            let _ = tx_w.send(Err(e.to_string())).await;
                            return;
                        }
                    };
                    let mut buf = vec![0u8; CHUNK_SIZE];

                    loop {
                        if cancel_w.load(Ordering::Relaxed) {
                            break;
                        }
                        let chunk_offset =
                            next_offset_w.fetch_add(CHUNK_SIZE as u64, Ordering::SeqCst);
                        if chunk_offset >= file_size {
                            break;
                        }

                        let to_read = ((file_size - chunk_offset) as usize).min(CHUNK_SIZE);
                        if let Err(e) = file.seek(std::io::SeekFrom::Start(chunk_offset)).await {
                            let _ = tx_w.send(Err(e.to_string())).await;
                            break;
                        }
                        if let Err(e) = file.read_exact(&mut buf[..to_read]).await {
                            let _ = tx_w.send(Err(e.to_string())).await;
                            break;
                        }

                        let cipher_len = (to_read + 16) as u32;
                        let mut header = [0u8; HEADER_LEN];
                        header[0..4].copy_from_slice(MAGIC_BYTES);
                        header[4..20].copy_from_slice(&session_id_bytes);
                        header[20..24].copy_from_slice(&file_id.to_be_bytes());
                        header[24..32].copy_from_slice(&chunk_offset.to_be_bytes());
                        header[32..36].copy_from_slice(&cipher_len.to_be_bytes());

                        match encrypt_chunk(
                            &session_key,
                            file_id,
                            chunk_offset,
                            &header,
                            &buf[..to_read],
                        ) {
                            Ok(ciphertext) => {
                                if tx_w
                                    .send(Ok((header.to_vec(), ciphertext, to_read)))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx_w.send(Err(e)).await;
                                break;
                            }
                        }
                    }
                });
            }
            drop(frame_tx);

            while let Some(frame_res) = frame_rx.recv().await {
                if cancel_flag.load(Ordering::Relaxed) {
                    let db_status = state
                        .db
                        .get_transfer(&record.id)
                        .ok()
                        .flatten()
                        .map(|r| r.status)
                        .unwrap_or_else(|| "paused".to_string());
                    record.status = if db_status == "cancelled" {
                        "cancelled".to_string()
                    } else {
                        "paused".to_string()
                    };
                    let _ = state.db.upsert_transfer(&record);
                    let _ = app.emit("transfer_state_changed", &record);
                    return Err("Outgoing transfer paused/cancelled".to_string());
                }

                let (header, ciphertext, plain_len) = frame_res?;
                writer
                    .write_all(&header)
                    .await
                    .map_err(|e| e.to_string())?;
                writer
                    .write_all(&ciphertext)
                    .await
                    .map_err(|e| e.to_string())?;

                record.transferred_bytes =
                    (record.transferred_bytes + plain_len as u64).min(record.total_bytes);

                if last_db_sync.elapsed() >= Duration::from_millis(1200) {
                    let _ = state.db.upsert_transfer(&record);
                    last_db_sync = Instant::now();
                }

                if last_emit.elapsed() >= Duration::from_millis(120) {
                    let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
                    let session_delta = record
                        .transferred_bytes
                        .saturating_sub(already_sent_bytes);
                    let speed_mbps = (session_delta as f64 / (1024.0 * 1024.0)) / elapsed;
                    let remaining = record
                        .total_bytes
                        .saturating_sub(record.transferred_bytes);
                    let eta_seconds = if speed_mbps > 0.01 {
                        ((remaining as f64 / (1024.0 * 1024.0)) / speed_mbps) as u64
                    } else {
                        0
                    };

                    let _ = app.emit(
                        "transfer_progress",
                        &TransferProgressEvent {
                            session_id: record.id.clone(),
                            peer_device_id: record.peer_device_id.clone(),
                            peer_alias: record.peer_alias.clone(),
                            direction: "outgoing".to_string(),
                            network_route: network_route.clone(),
                            transferred_bytes: record.transferred_bytes,
                            total_bytes: record.total_bytes,
                            speed_mbps,
                            eta_seconds,
                            status: "in_progress".to_string(),
                        },
                    );
                    last_emit = Instant::now();
                }
            }
        }

        // Send encrypted FILE_COMPLETE control frame (file_id = 0, offset = file_id as u64)
        let final_hash = hash_task
            .await
            .map_err(|e| e.to_string())??;
        let complete_frame = FileCompleteFrame {
            frame_type: "FILE_COMPLETE".to_string(),
            session_id: session_id.clone(),
            file_id,
            blake3_hash: final_hash,
        };
        let complete_bytes = serde_json::to_vec(&complete_frame).map_err(|e| e.to_string())?;
        let ctrl_cipher_len = (complete_bytes.len() + 16) as u32;
        let ctrl_offset = file_id as u64;

        let mut ctrl_header = [0u8; HEADER_LEN];
        ctrl_header[0..4].copy_from_slice(MAGIC_BYTES);
        ctrl_header[4..20].copy_from_slice(&session_id_bytes);
        ctrl_header[20..24].copy_from_slice(&0u32.to_be_bytes());
        ctrl_header[24..32].copy_from_slice(&ctrl_offset.to_be_bytes());
        ctrl_header[32..36].copy_from_slice(&ctrl_cipher_len.to_be_bytes());

        let ctrl_ciphertext = encrypt_chunk(
            &session_key,
            0,
            ctrl_offset,
            &ctrl_header,
            &complete_bytes,
        )?;
        writer
            .write_all(&ctrl_header)
            .await
            .map_err(|e| e.to_string())?;
        writer
            .write_all(&ctrl_ciphertext)
            .await
            .map_err(|e| e.to_string())?;
        writer.flush().await.map_err(|e| e.to_string())?;
    }

    state
        .active_cancellations
        .lock()
        .unwrap()
        .remove(&record.id);

    record.status = "completed".to_string();
    let _ = state.db.upsert_transfer(&record);
    let _ = app.emit("transfer_state_changed", &record);
    Ok(())
}

