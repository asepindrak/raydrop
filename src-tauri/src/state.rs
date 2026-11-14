use crate::crypto::identity::DeviceIdentity;
use crate::db::storage::{AppSettings, Database};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFileEntry {
    pub file_id: u32,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingRequestDecision {
    pub accepted: bool,
    pub target_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WanCandidateResponse {
    pub lan_candidates: Vec<String>,
    pub wan_candidate: Option<String>,
    pub peer_ed25519_pubkey: String,
}

pub struct AppState {
    pub db: Arc<Database>,
    pub identity: DeviceIdentity,
    pub settings: Mutex<AppSettings>,
    pub local_ip: String,
    pub listen_port: u16,
    pub wan_reflexive_addr: Mutex<Option<String>>,
    pub signaling_connected: AtomicBool,
    pub signaling_tx: Mutex<Option<mpsc::UnboundedSender<String>>>,
    pub quic_endpoint: Mutex<Option<quinn::Endpoint>>,
    pub relay_streams: Mutex<HashMap<String, mpsc::UnboundedSender<Vec<u8>>>>,
    pub pending_requests: Mutex<HashMap<String, oneshot::Sender<IncomingRequestDecision>>>,
    pub active_cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub pending_wan_candidates: Mutex<HashMap<String, oneshot::Sender<WanCandidateResponse>>>,
    pub queued_cli_files: Mutex<Vec<String>>,
    pub active_web_share: Mutex<Option<Arc<AtomicBool>>>,
}

impl AppState {
    pub fn is_discoverable(&self) -> bool {
        self.settings
            .lock()
            .map(|s| s.discoverable)
            .unwrap_or(false)
    }

    pub fn is_signaling_connected(&self) -> bool {
        self.signaling_connected.load(Ordering::Relaxed)
    }
}

