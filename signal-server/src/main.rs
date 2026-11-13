use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, DefaultBodyLimit, Path, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{delete, get, patch, post},
    Router,
};
use dashmap::DashMap;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{net::UdpSocket, sync::mpsc};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use uuid::Uuid;

const MAX_HTTP_BODY_BYTES: usize = 64 * 1024; // 64 KB max JSON body
const MAX_WS_MESSAGE_BYTES: usize = 2 * 1024 * 1024; // 2 MB max WebSocket frame
const PEER_CHANNEL_CAPACITY: usize = 1024; // Bounded per-peer queue to buffer high-throughput E2EE relay stream frames
const AUTH_RATE_WINDOW_SECS: u64 = 300; // 5-minute window
const MAX_AUTH_ATTEMPTS_PER_WINDOW: u32 = 10; // Max 10 failed login/register attempts per 5 mins per IP
const AUTH_LOCKOUT_SECS: u64 = 600; // 10-minute temporary ban on brute-force
const MAX_WS_MSGS_PER_SEC: u32 = 2500; // Per-socket message flood limit (generous for high-throughput E2EE relay stream frames)

#[derive(Clone)]
struct RateLimitBucket {
    failures: u32,
    window_start: u64,
    locked_until: u64,
}

#[derive(Clone)]
struct ServerState {
    peers: Arc<DashMap<String, PeerSession>>,
    auth_limiter: Arc<DashMap<String, RateLimitBucket>>,
    db: Arc<Mutex<Connection>>,
}

#[allow(dead_code)]
struct PeerSession {
    conn_id: Uuid,
    user_id: Option<String>,
    user_email: Option<String>,
    alias_name: String,
    ed25519_pubkey: String,
    tx: mpsc::Sender<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserProfile {
    pub id: String,
    pub email: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceNode {
    pub device_id: String,
    pub user_id: String,
    pub user_email: String,
    pub user_name: String,
    pub device_name: String,
    pub os_type: String,
    pub ed25519_pubkey: String,
    pub last_ip: Option<String>,
    pub is_online: bool,
    pub last_seen_at: u64,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContactWithDevices {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub trust_level: String,
    pub is_online: bool,
    pub devices: Vec<DeviceNode>,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    email: String,
    name: String,
    password: String,
    device_id: String,
    device_name: String,
    os_type: String,
    ed25519_pubkey: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
    device_id: String,
    device_name: String,
    os_type: String,
    ed25519_pubkey: String,
}

#[derive(Debug, Deserialize)]
struct RenameDeviceRequest {
    device_name: String,
}

#[derive(Debug, Deserialize)]
struct AddContactRequest {
    email: String,
    trust_level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateContactTrustRequest {
    email: String,
    trust_level: String,
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
        auth_token: Option<String>,
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

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Extracts real client IP behind Traefik / reverse proxy (`X-Forwarded-For` / `X-Real-IP`)
fn extract_client_ip(headers: &HeaderMap, fallback_addr: SocketAddr) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first_ip) = xff.split(',').next() {
            let trimmed = first_ip.trim();
            if !trimmed.is_empty() && trimmed.len() <= 64 {
                return trimmed.to_string();
            }
        }
    }
    if let Some(xri) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = xri.trim();
        if !trimmed.is_empty() && trimmed.len() <= 64 {
            return trimmed.to_string();
        }
    }
    fallback_addr.ip().to_string()
}

/// Checks if `client_ip` is currently rate-limited / locked out from auth endpoints.
fn check_auth_rate_limit(state: &ServerState, client_ip: &str) -> Result<(), u64> {
    let now = now_secs();
    if let Some(mut entry) = state.auth_limiter.get_mut(client_ip) {
        if entry.locked_until > now {
            return Err(entry.locked_until - now);
        }
        if now.saturating_sub(entry.window_start) > AUTH_RATE_WINDOW_SECS {
            entry.failures = 0;
            entry.window_start = now;
            entry.locked_until = 0;
        }
    }
    Ok(())
}

/// Records a failed auth attempt or rate-limited action for `client_ip`.
fn record_auth_failure(state: &ServerState, client_ip: &str) {
    let now = now_secs();
    let mut entry = state
        .auth_limiter
        .entry(client_ip.to_string())
        .or_insert(RateLimitBucket {
            failures: 0,
            window_start: now,
            locked_until: 0,
        });
    if now.saturating_sub(entry.window_start) > AUTH_RATE_WINDOW_SECS {
        entry.failures = 1;
        entry.window_start = now;
        entry.locked_until = 0;
    } else {
        entry.failures += 1;
        if entry.failures >= MAX_AUTH_ATTEMPTS_PER_WINDOW {
            entry.locked_until = now + AUTH_LOCKOUT_SECS;
            warn!(
                "SECURITY: IP {} locked out for {}s due to repeated auth failures",
                client_ip, AUTH_LOCKOUT_SECS
            );
        }
    }
}

fn clear_auth_failures(state: &ServerState, client_ip: &str) {
    state.auth_limiter.remove(client_ip);
}

/// Constant-time string comparison to prevent timing side-channel attacks on password hashes and tokens.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn hash_password(password: &str, salt_hex: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"RAYDROP_AUTH_V2:");
    hasher.update(salt_hex.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    // Perform 10,000 rounds of iterated BLAKE3 key stretching (compatible with existing DB hashes)
    let mut current = hasher.finalize();
    for _ in 0..10_000 {
        let mut h = blake3::Hasher::new();
        h.update(current.as_bytes());
        h.update(salt_hex.as_bytes());
        current = h.finalize();
    }
    current.to_hex().to_string()
}

fn generate_random_hex(bytes_len: usize) -> String {
    let mut buf = vec![0u8; bytes_len];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Verifies that `device_id` (`RAY-XXXX-XXXX-XXXX`) is mathematically derived from `ed25519_pubkey_hex`.
fn verify_device_id_matches_pubkey(device_id: &str, ed25519_pubkey_hex: &str) -> bool {
    let Ok(pub_bytes) = hex::decode(ed25519_pubkey_hex) else {
        return false;
    };
    let Ok(pub_arr): Result<[u8; 32], _> = pub_bytes.try_into() else {
        return false;
    };
    if VerifyingKey::from_bytes(&pub_arr).is_err() {
        return false;
    }
    let hash = blake3::hash(&pub_arr);
    let hex_hash = hash.to_hex().to_uppercase();
    let expected_id = format!(
        "RAY-{}-{}-{}",
        &hex_hash[0..4],
        &hex_hash[4..8],
        &hex_hash[8..12]
    );
    device_id == expected_id
}

fn init_database(db_path: &str) -> Connection {
    if let Some(parent) = std::path::Path::new(db_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(db_path)
        .or_else(|_| Connection::open("./signal.db"))
        .expect("Failed to open SQLite database");

    let _ = conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,
            name TEXT NOT NULL,
            password_salt TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS devices (
            device_id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            device_name TEXT NOT NULL,
            os_type TEXT NOT NULL,
            ed25519_pubkey TEXT NOT NULL,
            last_ip TEXT,
            last_seen_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS user_contacts (
            owner_user_id TEXT NOT NULL,
            contact_user_id TEXT NOT NULL,
            trust_level TEXT NOT NULL DEFAULT 'ask',
            created_at INTEGER NOT NULL,
            PRIMARY KEY (owner_user_id, contact_user_id),
            FOREIGN KEY(owner_user_id) REFERENCES users(id) ON DELETE CASCADE,
            FOREIGN KEY(contact_user_id) REFERENCES users(id) ON DELETE CASCADE
        );
        ",
    );

    info!("Initialized persistent SQLite database at {db_path}");
    conn
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let http_addr = std::env::var("SIGNAL_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let udp_addr = std::env::var("SIGNAL_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:3478".to_string());
    let db_path = std::env::var("SIGNAL_DB_PATH").unwrap_or_else(|_| "/data/signal.db".to_string());

    let conn = init_database(&db_path);
    let state = ServerState {
        peers: Arc::new(DashMap::new()),
        auth_limiter: Arc::new(DashMap::new()),
        db: Arc::new(Mutex::new(conn)),
    };

    // Periodically prune expired rate-limit entries every 10 minutes
    let limiter_cleanup = state.auth_limiter.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
            let now = now_secs();
            limiter_cleanup.retain(|_, v| {
                v.locked_until > now || now.saturating_sub(v.window_start) <= AUTH_RATE_WINDOW_SECS
            });
        }
    });

    // Spawn UDP STUN / Reflexive Address Discovery Listener
    let udp_bind = udp_addr.clone();
    tokio::spawn(async move {
        match UdpSocket::bind(&udp_bind).await {
            Ok(sock) => {
                info!("Raydrop UDP STUN reflector listening on {udp_bind}");
                let mut buf = [0u8; 512];
                loop {
                    if let Ok((len, peer_addr)) = sock.recv_from(&mut buf).await {
                        if len > 256 {
                            continue; // Drop oversized UDP packets to prevent amplification
                        }
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
        .route("/api/auth/register", post(register_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/state", get(get_cloud_state_handler))
        .route(
            "/api/devices/:device_id",
            patch(rename_device_handler).delete(delete_device_handler),
        )
        .route(
            "/api/contacts",
            post(add_contact_handler).patch(update_contact_trust_handler),
        )
        .route("/api/contacts/:email", delete(delete_contact_handler))
        .layer(DefaultBodyLimit::max(MAX_HTTP_BODY_BYTES))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&http_addr)
        .await
        .expect("Failed to bind HTTP signaling port");
    info!("Raydrop WSS Signaling & Cloud Account server listening on {http_addr}");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "raydrop-signal-server",
        "version": "2.1.0"
    }))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
        .filter(|t| t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()))
}

fn authenticate_token(conn: &Connection, token: &str) -> Option<UserProfile> {
    conn.query_row(
        "SELECT u.id, u.email, u.name
         FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.token = ?1",
        params![token],
        |row| {
            Ok(UserProfile {
                id: row.get(0)?,
                email: row.get(1)?,
                name: row.get(2)?,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

fn query_user_devices(
    conn: &Connection,
    peers: &DashMap<String, PeerSession>,
    user_id: &str,
) -> Vec<DeviceNode> {
    let mut stmt = match conn.prepare(
        "SELECT d.device_id, d.user_id, u.email, u.name, d.device_name, d.os_type,
                d.ed25519_pubkey, d.last_ip, d.last_seen_at, d.created_at
         FROM devices d
         JOIN users u ON u.id = d.user_id
         WHERE d.user_id = ?1
         ORDER BY d.last_seen_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map(params![user_id], |row| {
        let dev_id: String = row.get(0)?;
        let is_online = peers.contains_key(&dev_id);
        Ok(DeviceNode {
            device_id: dev_id,
            user_id: row.get(1)?,
            user_email: row.get(2)?,
            user_name: row.get(3)?,
            device_name: row.get(4)?,
            os_type: row.get(5)?,
            ed25519_pubkey: row.get(6)?,
            last_ip: row.get(7)?,
            is_online,
            last_seen_at: row.get::<_, i64>(8)? as u64,
            created_at: row.get::<_, i64>(9)? as u64,
        })
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

fn query_user_contacts(
    conn: &Connection,
    peers: &DashMap<String, PeerSession>,
    owner_user_id: &str,
) -> Vec<ContactWithDevices> {
    let mut stmt = match conn.prepare(
        "SELECT u.id, u.email, u.name, uc.trust_level
         FROM user_contacts uc
         JOIN users u ON u.id = uc.contact_user_id
         WHERE uc.owner_user_id = ?1
         ORDER BY u.name ASC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let base_contacts: Vec<(String, String, String, String)> = match stmt.query_map(
        params![owner_user_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => return vec![],
    };

    base_contacts
        .into_iter()
        .map(|(uid, email, name, trust_level)| {
            let devices = query_user_devices(conn, peers, &uid);
            let is_online = devices.iter().any(|d| d.is_online);
            ContactWithDevices {
                user_id: uid,
                email,
                name,
                trust_level,
                is_online,
                devices,
            }
        })
        .collect()
}

fn upsert_device_record(
    conn: &Connection,
    device_id: &str,
    user_id: &str,
    device_name: &str,
    os_type: &str,
    ed25519_pubkey: &str,
    last_ip: Option<&str>,
) -> Result<(), String> {
    if !verify_device_id_matches_pubkey(device_id, ed25519_pubkey) {
        return Err("Device ID does not match Ed25519 public key".to_string());
    }

    // Prevent key spoofing: if device_id already exists, its Ed25519 public key must match
    let existing_pubkey: Option<String> = conn
        .query_row(
            "SELECT ed25519_pubkey FROM devices WHERE device_id = ?1",
            params![device_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if let Some(saved_pk) = existing_pubkey {
        if !constant_time_eq(saved_pk.as_bytes(), ed25519_pubkey.as_bytes()) {
            return Err("Cryptographic key mismatch for existing device_id".to_string());
        }
    }

    let now = now_secs() as i64;
    let safe_name: String = device_name.trim().chars().take(64).collect();
    let safe_os: String = os_type.trim().chars().take(32).collect();

    conn.execute(
        "INSERT INTO devices (device_id, user_id, device_name, os_type, ed25519_pubkey, last_ip, last_seen_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(device_id) DO UPDATE SET
            user_id = excluded.user_id,
            device_name = excluded.device_name,
            os_type = excluded.os_type,
            last_ip = COALESCE(excluded.last_ip, devices.last_ip),
            last_seen_at = excluded.last_seen_at",
        params![
            device_id,
            user_id,
            safe_name,
            safe_os,
            ed25519_pubkey,
            last_ip,
            now,
            now
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn register_handler(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ServerState>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    let client_ip = extract_client_ip(&headers, addr);
    if let Err(wait_secs) = check_auth_rate_limit(&state, &client_ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": format!("Terlalu banyak percobaan. Silakan tunggu {wait_secs} detik.")
            })),
        );
    }

    let email = req.email.trim().to_lowercase();
    let name: String = req.name.trim().chars().take(80).collect();
    if !email.contains('@')
        || email.len() > 128
        || name.is_empty()
        || req.password.len() < 4
        || req.password.len() > 256
    {
        record_auth_failure(&state, &client_ip);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Please provide a valid email, name, and password (4-256 chars)." })),
        );
    }

    if !verify_device_id_matches_pubkey(&req.device_id, &req.ed25519_pubkey) {
        record_auth_failure(&state, &client_ip);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid cryptographic device identity." })),
        );
    }

    let conn = state.db.lock().unwrap();
    let exists: Option<String> = conn
        .query_row(
            "SELECT id FROM users WHERE email = ?1",
            params![&email],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);

    if exists.is_some() {
        record_auth_failure(&state, &client_ip);
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "Email is already registered. Please sign in instead." })),
        );
    }

    let user_id = Uuid::new_v4().to_string();
    let salt = generate_random_hex(16);
    let pw_hash = hash_password(&req.password, &salt);
    let now = now_secs() as i64;

    if let Err(e) = conn.execute(
        "INSERT INTO users (id, email, name, password_salt, password_hash, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![&user_id, &email, &name, &salt, &pw_hash, now],
    ) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Database error: {e}") })),
        );
    }

    let _ = upsert_device_record(
        &conn,
        &req.device_id,
        &user_id,
        &req.device_name,
        &req.os_type,
        &req.ed25519_pubkey,
        Some(&client_ip),
    );

    let token = generate_random_hex(32);
    let _ = conn.execute(
        "INSERT INTO sessions (token, user_id, device_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![&token, &user_id, &req.device_id, now],
    );

    let my_devices = query_user_devices(&conn, &state.peers, &user_id);
    let contacts = query_user_contacts(&conn, &state.peers, &user_id);
    clear_auth_failures(&state, &client_ip);

    info!("Registered new user: {email} ({name}) on device {}", req.device_id);

    (
        StatusCode::OK,
        Json(json!({
            "token": token,
            "user": {
                "id": user_id,
                "email": email,
                "name": name
            },
            "my_devices": my_devices,
            "contacts": contacts
        })),
    )
}

async fn login_handler(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ServerState>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let client_ip = extract_client_ip(&headers, addr);
    if let Err(wait_secs) = check_auth_rate_limit(&state, &client_ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": format!("Terlalu banyak percobaan login gagal. Silakan tunggu {wait_secs} detik.")
            })),
        );
    }

    if req.password.len() > 256 || req.email.len() > 128 {
        record_auth_failure(&state, &client_ip);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid credentials." })),
        );
    }

    if !verify_device_id_matches_pubkey(&req.device_id, &req.ed25519_pubkey) {
        record_auth_failure(&state, &client_ip);
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid cryptographic device identity." })),
        );
    }

    let email = req.email.trim().to_lowercase();
    let conn = state.db.lock().unwrap();

    let row: Option<(String, String, String, String)> = conn
        .query_row(
            "SELECT id, name, password_salt, password_hash FROM users WHERE email = ?1",
            params![&email],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .unwrap_or(None);

    let Some((user_id, name, salt, expected_hash)) = row else {
        record_auth_failure(&state, &client_ip);
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Email atau password salah." })),
        );
    };

    let computed = hash_password(&req.password, &salt);
    if !constant_time_eq(computed.as_bytes(), expected_hash.as_bytes()) {
        record_auth_failure(&state, &client_ip);
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Email atau password salah." })),
        );
    }

    clear_auth_failures(&state, &client_ip);

    let _ = upsert_device_record(
        &conn,
        &req.device_id,
        &user_id,
        &req.device_name,
        &req.os_type,
        &req.ed25519_pubkey,
        Some(&client_ip),
    );

    let token = generate_random_hex(32);
    let now = now_secs() as i64;
    let _ = conn.execute(
        "INSERT INTO sessions (token, user_id, device_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![&token, &user_id, &req.device_id, now],
    );

    let my_devices = query_user_devices(&conn, &state.peers, &user_id);
    let contacts = query_user_contacts(&conn, &state.peers, &user_id);

    info!("User logged in: {email} on device {}", req.device_id);
    drop(conn);
    broadcast_presence_update(&state, &user_id);

    (
        StatusCode::OK,
        Json(json!({
            "token": token,
            "user": {
                "id": user_id,
                "email": email,
                "name": name
            },
            "my_devices": my_devices,
            "contacts": contacts
        })),
    )
}

async fn get_cloud_state_handler(
    headers: HeaderMap,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    let Some(token) = extract_bearer_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Missing Authorization Bearer token" })),
        );
    };

    let conn = state.db.lock().unwrap();
    let Some(user) = authenticate_token(&conn, &token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Session expired or invalid token" })),
        );
    };

    let my_devices = query_user_devices(&conn, &state.peers, &user.id);
    let contacts = query_user_contacts(&conn, &state.peers, &user.id);

    (
        StatusCode::OK,
        Json(json!({
            "user": user,
            "my_devices": my_devices,
            "contacts": contacts
        })),
    )
}

async fn rename_device_handler(
    headers: HeaderMap,
    Path(target_device_id): Path<String>,
    State(state): State<ServerState>,
    Json(req): Json<RenameDeviceRequest>,
) -> impl IntoResponse {
    let Some(token) = extract_bearer_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };
    let conn = state.db.lock().unwrap();
    let Some(user) = authenticate_token(&conn, &token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };

    let safe_name: String = req.device_name.trim().chars().take(64).collect();
    let _ = conn.execute(
        "UPDATE devices SET device_name = ?1 WHERE device_id = ?2 AND user_id = ?3",
        params![safe_name, &target_device_id, &user.id],
    );

    let my_devices = query_user_devices(&conn, &state.peers, &user.id);
    drop(conn);
    broadcast_presence_update(&state, &user.id);

    (StatusCode::OK, Json(json!({ "my_devices": my_devices })))
}

async fn delete_device_handler(
    headers: HeaderMap,
    Path(target_device_id): Path<String>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    let Some(token) = extract_bearer_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };
    let conn = state.db.lock().unwrap();
    let Some(user) = authenticate_token(&conn, &token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };

    let _ = conn.execute(
        "DELETE FROM devices WHERE device_id = ?1 AND user_id = ?2",
        params![&target_device_id, &user.id],
    );
    let _ = conn.execute(
        "DELETE FROM sessions WHERE device_id = ?1 AND user_id = ?2",
        params![&target_device_id, &user.id],
    );

    let my_devices = query_user_devices(&conn, &state.peers, &user.id);
    drop(conn);
    broadcast_presence_update(&state, &user.id);

    (StatusCode::OK, Json(json!({ "my_devices": my_devices })))
}

async fn add_contact_handler(
    headers: HeaderMap,
    State(state): State<ServerState>,
    Json(req): Json<AddContactRequest>,
) -> impl IntoResponse {
    let Some(token) = extract_bearer_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };
    let target_email = req.email.trim().to_lowercase();
    let conn = state.db.lock().unwrap();
    let Some(user) = authenticate_token(&conn, &token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };

    if target_email == user.email {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Your own email is already managed under 'My Devices'." })),
        );
    }

    let target_user: Option<(String, String)> = conn
        .query_row(
            "SELECT id, name FROM users WHERE email = ?1",
            params![&target_email],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .unwrap_or(None);

    let Some((target_uid, _target_name)) = target_user else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("No registered Raydrop user found with email '{target_email}'.") })),
        );
    };

    let raw_trust = req.trust_level.unwrap_or_else(|| "ask".to_string());
    let trust = match raw_trust.as_str() {
        "ask" | "trusted_auto_accept" | "blocked" => raw_trust,
        _ => "ask".to_string(),
    };
    let now = now_secs() as i64;
    let _ = conn.execute(
        "INSERT INTO user_contacts (owner_user_id, contact_user_id, trust_level, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(owner_user_id, contact_user_id) DO UPDATE SET trust_level = excluded.trust_level",
        params![&user.id, &target_uid, &trust, now],
    );

    // Also add reverse contact entry (default 'ask') if not present so both users see each other
    let _ = conn.execute(
        "INSERT OR IGNORE INTO user_contacts (owner_user_id, contact_user_id, trust_level, created_at)
         VALUES (?1, ?2, 'ask', ?3)",
        params![&target_uid, &user.id, now],
    );

    let contacts = query_user_contacts(&conn, &state.peers, &user.id);
    drop(conn);
    broadcast_presence_update(&state, &target_uid);

    (StatusCode::OK, Json(json!({ "contacts": contacts })))
}

async fn update_contact_trust_handler(
    headers: HeaderMap,
    State(state): State<ServerState>,
    Json(req): Json<UpdateContactTrustRequest>,
) -> impl IntoResponse {
    let Some(token) = extract_bearer_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };
    let trust = match req.trust_level.as_str() {
        "ask" | "trusted_auto_accept" | "blocked" => req.trust_level.as_str(),
        _ => "ask",
    };
    let target_email = req.email.trim().to_lowercase();
    let conn = state.db.lock().unwrap();
    let Some(user) = authenticate_token(&conn, &token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };

    let _ = conn.execute(
        "UPDATE user_contacts
         SET trust_level = ?1
         WHERE owner_user_id = ?2
           AND contact_user_id = (SELECT id FROM users WHERE email = ?3)",
        params![trust, &user.id, &target_email],
    );

    let contacts = query_user_contacts(&conn, &state.peers, &user.id);
    (StatusCode::OK, Json(json!({ "contacts": contacts })))
}

async fn delete_contact_handler(
    headers: HeaderMap,
    Path(target_email): Path<String>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    let Some(token) = extract_bearer_token(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };
    let clean_email = target_email.trim().to_lowercase();
    let conn = state.db.lock().unwrap();
    let Some(user) = authenticate_token(&conn, &token) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    };

    let _ = conn.execute(
        "DELETE FROM user_contacts
         WHERE owner_user_id = ?1
           AND contact_user_id = (SELECT id FROM users WHERE email = ?2)",
        params![&user.id, &clean_email],
    );

    let contacts = query_user_contacts(&conn, &state.peers, &user.id);
    (StatusCode::OK, Json(json!({ "contacts": contacts })))
}

fn broadcast_presence_update(state: &ServerState, changed_user_id: &str) {
    let notify_msg = json!({
        "type": "CLOUD_STATE_CHANGED",
        "user_id": changed_user_id
    })
    .to_string();

    for entry in state.peers.iter() {
        let _ = entry.value().tx.try_send(notify_msg.clone());
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    let client_ip = extract_client_ip(&headers, addr);
    ws.max_message_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, client_ip, state))
}

fn verify_registration(
    device_id: &str,
    ed25519_pubkey_hex: &str,
    timestamp: u64,
    signature_hex: &str,
) -> bool {
    // Anti-Replay protection: registration timestamp must be within 10 minutes of server clock
    let now = now_secs();
    if now.abs_diff(timestamp) > 600 {
        return false;
    }

    let Ok(pub_bytes) = hex::decode(ed25519_pubkey_hex) else {
        return false;
    };
    let Ok(pub_arr): Result<[u8; 32], _> = pub_bytes.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&pub_arr) else {
        return false;
    };

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

async fn handle_socket(socket: WebSocket, client_ip: String, state: ServerState) {
    let conn_id = Uuid::new_v4();
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::channel::<String>(PEER_CHANNEL_CAPACITY);

    let writer_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let mut registered_id: Option<String> = None;
    let mut registered_pubkey: Option<String> = None;
    let mut registered_user_id: Option<String> = None;
    let mut rate_window = Instant::now();
    let mut msg_count_in_sec: u32 = 0;

    while let Some(Ok(msg)) = ws_rx.next().await {
        if rate_window.elapsed().as_secs() >= 1 {
            rate_window = Instant::now();
            msg_count_in_sec = 0;
        }
        msg_count_in_sec += 1;
        if msg_count_in_sec > MAX_WS_MSGS_PER_SEC {
            warn!("Rate limit exceeded on WebSocket from {client_ip}");
            break;
        }

        if let Message::Text(text) = msg {
            if text.len() > MAX_WS_MESSAGE_BYTES {
                break;
            }
            if let Ok(parsed) = serde_json::from_str::<SignalMessage>(&text) {
                match parsed {
                    SignalMessage::Register {
                        device_id,
                        alias_name,
                        ed25519_pubkey,
                        timestamp,
                        signature,
                        auth_token,
                    } => {
                        if verify_registration(&device_id, &ed25519_pubkey, timestamp, &signature) {
                            let mut uid_opt = None;
                            let mut email_opt = None;
                            let safe_alias: String = alias_name.trim().chars().take(64).collect();

                            if let Some(tok) = auth_token {
                                let conn = state.db.lock().unwrap();
                                if let Some(u) = authenticate_token(&conn, &tok) {
                                    let _ = conn.execute(
                                        "UPDATE devices SET last_ip = ?1, last_seen_at = ?2 WHERE device_id = ?3 AND user_id = ?4",
                                        params![&client_ip, now_secs() as i64, &device_id, &u.id],
                                    );
                                    uid_opt = Some(u.id);
                                    email_opt = Some(u.email);
                                }
                            }

                            info!(
                                "Peer registered: {device_id} ({safe_alias}, email={:?}) from {client_ip}",
                                email_opt
                            );
                            state.peers.insert(
                                device_id.clone(),
                                PeerSession {
                                    conn_id,
                                    user_id: uid_opt.clone(),
                                    user_email: email_opt,
                                    alias_name: safe_alias,
                                    ed25519_pubkey: ed25519_pubkey.clone(),
                                    tx: tx.clone(),
                                },
                            );
                            registered_id = Some(device_id.clone());
                            registered_pubkey = Some(ed25519_pubkey);
                            registered_user_id = uid_opt.clone();

                            let _ = tx.try_send(
                                json!({
                                    "type": "REGISTER_OK",
                                    "device_id": device_id,
                                    "public_ip": client_ip
                                })
                                .to_string(),
                            );

                            if let Some(ref uid) = uid_opt {
                                broadcast_presence_update(&state, uid);
                            }
                        } else {
                            warn!("Invalid Ed25519 registration signature from {client_ip}");
                        }
                    }
                    SignalMessage::PeerLookup { target_device_id } => {
                        if registered_id.is_none() {
                            continue;
                        }
                        if let Some(peer) = state.peers.get(&target_device_id) {
                            let _ = tx.try_send(
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
                            let _ = tx.try_send(
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
                        // Anti-spoofing: sender must be cryptographically registered as `from_device_id`
                        let Some(ref authed_id) = registered_id else {
                            continue;
                        };
                        let Some(ref authed_pk) = registered_pubkey else {
                            continue;
                        };
                        if &from_device_id != authed_id || &from_ed25519_pubkey != authed_pk {
                            warn!("Blocked spoofed CONNECT_INTENT from {authed_id} claiming {from_device_id}");
                            continue;
                        }
                        if let Some(target) = state.peers.get(&target_device_id) {
                            let _ = target.tx.try_send(
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
                        let Some(ref authed_id) = registered_id else {
                            continue;
                        };
                        let Some(ref authed_pk) = registered_pubkey else {
                            continue;
                        };
                        if &from_device_id != authed_id || &from_ed25519_pubkey != authed_pk {
                            warn!("Blocked spoofed CANDIDATE_ANSWER from {authed_id}");
                            continue;
                        }
                        if let Some(initiator) = state.peers.get(&target_device_id) {
                            let _ = initiator.tx.try_send(
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
                            let punch_msg = json!({
                                "type": "PUNCH_SYNC",
                                "session_id": session_id,
                                "peer_wan_candidate": wan_candidate
                            })
                            .to_string();
                            let _ = initiator.tx.try_send(punch_msg);
                        }
                    }
                    SignalMessage::RelayControl {
                        session_id,
                        from_device_id,
                        target_device_id,
                        payload_json,
                    } => {
                        let Some(ref authed_id) = registered_id else {
                            continue;
                        };
                        if &from_device_id != authed_id {
                            warn!("Blocked spoofed RELAY_CONTROL from {authed_id}");
                            continue;
                        }
                        if let Some(target) = state.peers.get(&target_device_id) {
                            let msg_str = json!({
                                "type": "RELAY_CONTROL",
                                "session_id": session_id,
                                "from_device_id": from_device_id,
                                "target_device_id": target_device_id,
                                "payload_json": payload_json
                            })
                            .to_string();
                            let target_tx = target.tx.clone();
                            tokio::spawn(async move {
                                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), target_tx.send(msg_str)).await;
                            });
                        }
                    }
                }
            }
        }
    }

    if let Some(id) = registered_id {
        let is_current = if let Some(peer) = state.peers.get(&id) {
            peer.conn_id == conn_id
        } else {
            false
        };
        if is_current {
            state.peers.remove(&id);
            info!("Peer disconnected: {id}");
            if let Some(uid) = registered_user_id {
                broadcast_presence_update(&state, &uid);
            }
        } else {
            info!("Stale peer session closed for {id}, keeping active connection.");
        }
    }
    writer_task.abort();
}
