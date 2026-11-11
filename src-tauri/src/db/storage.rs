use crate::crypto::identity::DeviceIdentity;
use directories::{ProjectDirs, UserDirs};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub device_id: String,
    pub alias_name: String,
    pub ed25519_pubkey: Option<String>,
    pub network_type: String, // "lan" or "wan"
    pub last_known_ip: Option<String>,
    pub last_known_port: Option<u16>,
    pub trust_level: String, // "ask", "trusted_auto_accept", "blocked"
    pub is_online: bool,
    pub last_seen_at: u64,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub id: String,
    pub peer_device_id: String,
    pub peer_alias: String,
    pub direction: String,     // "incoming" or "outgoing"
    pub network_route: String, // "lan_direct" or "wan_p2p"
    pub total_files: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub status: String, // "in_progress", "paused", "completed", "cancelled", "failed", "rejected"
    pub target_path: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub device_id: String,
    pub alias_name: String,
    pub discoverable: bool,
    pub download_dir: String,
    pub signaling_url: String,
    pub stun_addr: String,
}

pub struct Database {
    conn: Mutex<Connection>,
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Database {
    pub fn init() -> Result<(Self, DeviceIdentity, AppSettings), String> {
        let proj_dirs = ProjectDirs::from("com", "raydrop", "Raydrop")
            .ok_or_else(|| "Failed to resolve application data directory".to_string())?;
        let data_dir = proj_dirs.data_dir();
        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;

        let db_path = data_dir.join("storage.db");
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS local_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS contacts (
                device_id TEXT PRIMARY KEY,
                alias_name TEXT NOT NULL,
                ed25519_pubkey TEXT,
                network_type TEXT NOT NULL,
                last_known_ip TEXT,
                last_known_port INTEGER,
                trust_level TEXT DEFAULT 'ask',
                is_online INTEGER DEFAULT 0,
                last_seen_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS transfer_history (
                id TEXT PRIMARY KEY,
                peer_device_id TEXT NOT NULL,
                peer_alias TEXT NOT NULL,
                direction TEXT NOT NULL,
                network_route TEXT NOT NULL,
                total_files INTEGER NOT NULL,
                total_bytes INTEGER NOT NULL,
                transferred_bytes INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                target_path TEXT,
                created_at INTEGER NOT NULL
            );
            ",
        )
        .map_err(|e| e.to_string())?;

        let db = Self {
            conn: Mutex::new(conn),
        };

        let identity = match db.get_setting("ed25519_secret_hex")? {
            Some(sec) => DeviceIdentity::from_secret_hex(&sec)?,
            None => {
                let id = DeviceIdentity::generate();
                db.set_setting("ed25519_secret_hex", &id.secret_hex())?;
                db.set_setting("ed25519_public_hex", &id.public_hex())?;
                db.set_setting("device_id", &id.device_id)?;
                id
            }
        };

        let default_alias = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| "Raydrop-Device".to_string());

        let alias_name = match db.get_setting("alias_name")? {
            Some(a) => a,
            None => {
                db.set_setting("alias_name", &default_alias)?;
                default_alias
            }
        };

        let discoverable = match db.get_setting("discoverable")? {
            Some(v) => v == "true",
            None => {
                db.set_setting("discoverable", "true")?;
                true
            }
        };

        let default_dl = UserDirs::new()
            .and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string();

        let download_dir = match db.get_setting("download_dir")? {
            Some(d) => d,
            None => {
                db.set_setting("download_dir", &default_dl)?;
                default_dl
            }
        };

        let signaling_url = match db.get_setting("signaling_url")? {
            Some(s) => s,
            None => {
                let def = "wss://signal.example.com/ws";
                db.set_setting("signaling_url", def)?;
                def.to_string()
            }
        };

        let stun_addr = match db.get_setting("stun_addr")? {
            Some(s) => s,
            None => {
                let def = "signal.example.com:3478";
                db.set_setting("stun_addr", def)?;
                def.to_string()
            }
        };

        let settings = AppSettings {
            device_id: identity.device_id.clone(),
            alias_name,
            discoverable,
            download_dir,
            signaling_url,
            stun_addr,
        };

        Ok((db, identity, settings))
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT value FROM local_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO local_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn upsert_contact(&self, c: &Contact) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO contacts (
                device_id, alias_name, ed25519_pubkey, network_type,
                last_known_ip, last_known_port, trust_level, is_online, last_seen_at, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(device_id) DO UPDATE SET
                alias_name = excluded.alias_name,
                ed25519_pubkey = COALESCE(excluded.ed25519_pubkey, contacts.ed25519_pubkey),
                network_type = excluded.network_type,
                last_known_ip = COALESCE(excluded.last_known_ip, contacts.last_known_ip),
                last_known_port = COALESCE(excluded.last_known_port, contacts.last_known_port),
                is_online = excluded.is_online,
                last_seen_at = excluded.last_seen_at",
            params![
                c.device_id,
                c.alias_name,
                c.ed25519_pubkey,
                c.network_type,
                c.last_known_ip,
                c.last_known_port,
                c.trust_level,
                if c.is_online { 1 } else { 0 },
                c.last_seen_at as i64,
                c.created_at as i64,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_contact_trust(&self, device_id: &str, trust_level: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE contacts SET trust_level = ?1 WHERE device_id = ?2",
            params![trust_level, device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_contact_online(&self, device_id: &str, is_online: bool) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE contacts SET is_online = ?1, last_seen_at = ?2 WHERE device_id = ?3",
            params![if is_online { 1 } else { 0 }, now_unix_secs() as i64, device_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_contact(&self, device_id: &str) -> Result<Option<Contact>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT device_id, alias_name, ed25519_pubkey, network_type, last_known_ip,
                    last_known_port, trust_level, is_online, last_seen_at, created_at
             FROM contacts WHERE device_id = ?1",
            params![device_id],
            |row| {
                Ok(Contact {
                    device_id: row.get(0)?,
                    alias_name: row.get(1)?,
                    ed25519_pubkey: row.get(2)?,
                    network_type: row.get(3)?,
                    last_known_ip: row.get(4)?,
                    last_known_port: row.get::<_, Option<i64>>(5)?.map(|p| p as u16),
                    trust_level: row.get(6)?,
                    is_online: row.get::<_, i64>(7)? != 0,
                    last_seen_at: row.get::<_, i64>(8)? as u64,
                    created_at: row.get::<_, i64>(9)? as u64,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
    }

    pub fn list_contacts(&self) -> Result<Vec<Contact>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT device_id, alias_name, ed25519_pubkey, network_type, last_known_ip,
                        last_known_port, trust_level, is_online, last_seen_at, created_at
                 FROM contacts ORDER BY is_online DESC, last_seen_at DESC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(Contact {
                    device_id: row.get(0)?,
                    alias_name: row.get(1)?,
                    ed25519_pubkey: row.get(2)?,
                    network_type: row.get(3)?,
                    last_known_ip: row.get(4)?,
                    last_known_port: row.get::<_, Option<i64>>(5)?.map(|p| p as u16),
                    trust_level: row.get(6)?,
                    is_online: row.get::<_, i64>(7)? != 0,
                    last_seen_at: row.get::<_, i64>(8)? as u64,
                    created_at: row.get::<_, i64>(9)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }

    pub fn upsert_transfer(&self, t: &TransferRecord) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO transfer_history (
                id, peer_device_id, peer_alias, direction, network_route,
                total_files, total_bytes, transferred_bytes, status, target_path, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
                transferred_bytes = excluded.transferred_bytes,
                status = excluded.status,
                target_path = COALESCE(excluded.target_path, transfer_history.target_path)",
            params![
                t.id,
                t.peer_device_id,
                t.peer_alias,
                t.direction,
                t.network_route,
                t.total_files as i64,
                t.total_bytes as i64,
                t.transferred_bytes as i64,
                t.status,
                t.target_path,
                t.created_at as i64,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_transfers(&self) -> Result<Vec<TransferRecord>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, peer_device_id, peer_alias, direction, network_route,
                        total_files, total_bytes, transferred_bytes, status, target_path, created_at
                 FROM transfer_history ORDER BY created_at DESC LIMIT 100",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok(TransferRecord {
                    id: row.get(0)?,
                    peer_device_id: row.get(1)?,
                    peer_alias: row.get(2)?,
                    direction: row.get(3)?,
                    network_route: row.get(4)?,
                    total_files: row.get::<_, i64>(5)? as usize,
                    total_bytes: row.get::<_, i64>(6)? as u64,
                    transferred_bytes: row.get::<_, i64>(7)? as u64,
                    status: row.get(8)?,
                    target_path: row.get(9)?,
                    created_at: row.get::<_, i64>(10)? as u64,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(|e| e.to_string())?);
        }
        Ok(list)
    }
}

