use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, State,
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
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{net::UdpSocket, sync::mpsc};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone)]
struct ServerState {
    peers: Arc<DashMap<String, PeerSession>>,
    db: Arc<Mutex<Connection>>,
}

#[allow(dead_code)]
struct PeerSession {
    user_id: Option<String>,
    user_email: Option<String>,
    alias_name: String,
    ed25519_pubkey: String,
    tx: mpsc::UnboundedSender<String>,
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

fn hash_password(password: &str, salt_hex: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"RAYDROP_AUTH_V2:");
    hasher.update(salt_hex.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    // Perform 10,000 rounds of iterated BLAKE3 key stretching
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
        db: Arc::new(Mutex::new(conn)),
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

async fn health_handler(State(state): State<ServerState>) -> impl IntoResponse {
    let total_users: i64 = state
        .db
        .lock()
        .ok()
        .and_then(|c| c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).ok())
        .unwrap_or(0);

    Json(json!({
        "status": "ok",
        "service": "raydrop-signal-server",
        "version": "2.0.0",
        "online_peers": state.peers.len(),
        "registered_users": total_users
    }))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
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
    let now = now_secs() as i64;
    conn.execute(
        "INSERT INTO devices (device_id, user_id, device_name, os_type, ed25519_pubkey, last_ip, last_seen_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(device_id) DO UPDATE SET
            user_id = excluded.user_id,
            device_name = excluded.device_name,
            os_type = excluded.os_type,
            ed25519_pubkey = excluded.ed25519_pubkey,
            last_ip = COALESCE(excluded.last_ip, devices.last_ip),
            last_seen_at = excluded.last_seen_at",
        params![
            device_id,
            user_id,
            device_name,
            os_type,
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ServerState>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    let email = req.email.trim().to_lowercase();
    let name = req.name.trim().to_string();
    if !email.contains('@') || name.is_empty() || req.password.len() < 4 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Please provide a valid email, name, and password (min 4 chars)." })),
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

    let ip_str = addr.ip().to_string();
    let _ = upsert_device_record(
        &conn,
        &req.device_id,
        &user_id,
        &req.device_name,
        &req.os_type,
        &req.ed25519_pubkey,
        Some(&ip_str),
    );

    let token = generate_random_hex(32);
    let _ = conn.execute(
        "INSERT INTO sessions (token, user_id, device_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![&token, &user_id, &req.device_id, now],
    );

    let my_devices = query_user_devices(&conn, &state.peers, &user_id);
    let contacts = query_user_contacts(&conn, &state.peers, &user_id);

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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<ServerState>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
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
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Account not found for this email." })),
        );
    };

    let computed = hash_password(&req.password, &salt);
    if computed != expected_hash {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid password." })),
        );
    }

    let ip_str = addr.ip().to_string();
    let _ = upsert_device_record(
        &conn,
        &req.device_id,
        &user_id,
        &req.device_name,
        &req.os_type,
        &req.ed25519_pubkey,
        Some(&ip_str),
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

    let _ = conn.execute(
        "UPDATE devices SET device_name = ?1 WHERE device_id = ?2 AND user_id = ?3",
        params![req.device_name.trim(), &target_device_id, &user.id],
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

    let trust = req.trust_level.unwrap_or_else(|| "ask".to_string());
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
        params![&req.trust_level, &user.id, &target_email],
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
        let _ = entry.value().tx.send(notify_msg.clone());
    }
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
    let mut registered_user_id: Option<String> = None;

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
                        auth_token,
                    } => {
                        if verify_registration(&device_id, &ed25519_pubkey, timestamp, &signature) {
                            let mut uid_opt = None;
                            let mut email_opt = None;

                            if let Some(tok) = auth_token {
                                let conn = state.db.lock().unwrap();
                                if let Some(u) = authenticate_token(&conn, &tok) {
                                    let ip_s = addr.ip().to_string();
                                    let _ = conn.execute(
                                        "UPDATE devices SET last_ip = ?1, last_seen_at = ?2 WHERE device_id = ?3",
                                        params![&ip_s, now_secs() as i64, &device_id],
                                    );
                                    uid_opt = Some(u.id);
                                    email_opt = Some(u.email);
                                }
                            }

                            info!(
                                "Peer registered: {device_id} ({alias_name}, email={:?}) from {addr}",
                                email_opt
                            );
                            state.peers.insert(
                                device_id.clone(),
                                PeerSession {
                                    user_id: uid_opt.clone(),
                                    user_email: email_opt,
                                    alias_name: alias_name.clone(),
                                    ed25519_pubkey: ed25519_pubkey.clone(),
                                    tx: tx.clone(),
                                },
                            );
                            registered_id = Some(device_id.clone());
                            registered_user_id = uid_opt.clone();

                            let _ = tx.send(
                                json!({
                                    "type": "REGISTER_OK",
                                    "device_id": device_id,
                                    "public_ip": addr.ip().to_string()
                                })
                                .to_string(),
                            );

                            if let Some(ref uid) = uid_opt {
                                broadcast_presence_update(&state, uid);
                            }
                        } else {
                            warn!("Invalid Ed25519 registration signature from {addr}");
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
        if let Some(uid) = registered_user_id {
            broadcast_presence_update(&state, &uid);
        }
    }
    writer_task.abort();
}
