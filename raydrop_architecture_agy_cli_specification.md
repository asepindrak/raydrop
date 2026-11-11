# Raydrop - Complete Technical Specification & AGY CLI Execution Blueprint

> **Product Name:** Raydrop  
> **Tagline:** Universal, Zero-Server-Storage P2P File Transfer for Windows, macOS, & Linux (LAN + Direct WAN)  
> **Tech Stack:** Tauri v2 (Rust Backend + Svelte/Tailwind Frontend) + Lightweight Rust Signaling/STUN Server (Dokploy + Traefik Ready)  
> **Architecture Goal:** 100% P2P Local LAN & Direct WAN Transfer (NAT Hole-Punching via QUIC/UDP + TCP Fallback), zero server file storage/bandwidth load, Single-Instance IPC for native OS context menus (files & folders), system tray control, Ed25519/X25519 authenticated identity, and on-the-fly BLAKE3 + ChaCha20-Poly1305 encrypted chunked streaming.

---

## 1. Project Directory Structure

```text
raydrop/
├── package.json
├── vite.config.ts
├── tailwind.config.js
├── postcss.config.js
├── tsconfig.json
├── src/                                # Frontend (Svelte + Tailwind + TypeScript)
│   ├── app.postcss
│   ├── App.svelte
│   ├── main.ts
│   └── lib/
│       ├── components/
│       │   ├── Header.svelte           # Discoverability Toggle, LAN/WAN Status, Device ID Copy
│       │   ├── ContactList.svelte      # LAN Peers + Saved WAN Peers (Add by Device ID)
│       │   ├── TransferModal.svelte    # Incoming Transfer Prompt + Target Path Picker
│       │   ├── ActiveTransfers.svelte  # Real-time Progress, Speed (MB/s), Pause/Resume/Cancel
│       │   └── TraySettings.svelte     # Signaling URL, STUN Server, Auto-Accept & Context Menu Setup
│       ├── stores/
│       │   ├── state.ts                # Discoverability, Identity & Connection State
│       │   ├── contacts.ts             # LAN & WAN Peer Store
│       │   └── transfers.ts            # Active & Historical Transfers Store
│       └── utils/
│           └── tauri-ipc.ts            # Typed Tauri invoke & event listeners
├── src-tauri/                          # Desktop Backend (Rust - Tauri v2)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── capabilities/
│   │   └── default.json                # Tauri v2 Permission Capabilities
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── lib.rs                      # Single-Instance CLI handler + Tray Builder + Setup
│       ├── state.rs                    # AppState, Keypair, Active Sessions & Channels
│       ├── crypto/
│       │   ├── mod.rs
│       │   └── identity.rs             # Ed25519 Identity, X25519 ECDH, ChaCha20-Poly1305 AEAD
│       ├── network/
│       │   ├── mod.rs
│       │   ├── mdns.rs                 # Local LAN Peer Discovery (_raydrop._udp.local.)
│       │   ├── signaling.rs            # WAN WSS Client (Presence, Ed25519 Auth & ICE/Candidate Exchange)
│       │   ├── nat.rs                  # STUN Public Endpoint Discovery & UDP Hole Punching
│       │   └── transport.rs            # QUIC (quinn) + TCP Fallback Chunked Stream Transceiver
│       ├── db/
│       │   ├── mod.rs
│       │   └── storage.rs              # SQLite Local Database (Identity, Contacts, Resume State & History)
│       ├── os/
│       │   ├── mod.rs
│       │   ├── win_registry.rs         # Windows Explorer Context Menu (Files, Folders & SendTo)
│       │   ├── mac_finder.rs           # macOS Finder Quick Action / Service Workflow Installer
│       │   └── linux_nautilus.rs       # Ubuntu Nautilus Script Generator
│       └── commands.rs                 # Tauri IPC Command Handlers
└── signal-server/                      # Self-Hosted WAN Signaling & STUN Reflector (Dokploy + Traefik)
    ├── Cargo.toml
    ├── Dockerfile
    ├── docker-compose.yml              # Dokploy & Traefik Production Compose (WSS + UDP 3478)
    ├── .env.example
    └── src/
        └── main.rs                     # Stateless Axum WSS Signaling + UDP STUN Endpoint Reflector
```

---

## 2. Dependencies & Core Configurations

### 2.1 `package.json` (Frontend Environment)

```json
{
  "name": "raydrop",
  "private": true,
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.1.0",
    "@tauri-apps/plugin-dialog": "^2.0.0",
    "@tauri-apps/plugin-fs": "^2.0.0",
    "@tauri-apps/plugin-notification": "^2.0.0",
    "@tauri-apps/plugin-process": "^2.0.0",
    "@tauri-apps/plugin-shell": "^2.0.0",
    "clsx": "^2.1.0",
    "lucide-svelte": "^0.460.0",
    "svelte": "^4.2.19"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.1.0",
    "@sveltejs/vite-plugin-svelte": "^3.1.2",
    "autoprefixer": "^10.4.20",
    "postcss": "^8.4.49",
    "tailwindcss": "^3.4.15",
    "typescript": "^5.6.3",
    "vite": "^5.4.11"
  }
}
```

### 2.2 `src-tauri/Cargo.toml` (Rust Desktop Backend Environment)

```toml
[package]
name = "raydrop"
version = "1.0.0"
description = "Universal Zero-Server P2P File Transfer System (LAN + WAN)"
authors = ["Raydrop Team"]
edition = "2021"

[build-dependencies]
tauri-build = { version = "2.0.0", features = [] }

[dependencies]
tauri = { version = "2.1.0", features = ["tray-icon", "image-png"] }
tauri-plugin-single-instance = "2.0.0"
tauri-plugin-dialog = "2.0.0"
tauri-plugin-fs = "2.0.0"
tauri-plugin-notification = "2.0.0"
tauri-plugin-process = "2.0.0"
tauri-plugin-shell = "2.0.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.41", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec", "io"] }
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"] }
futures-util = "0.3"
quinn = "0.11"
rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }
rcgen = "0.13"
socket2 = { version = "0.5", features = ["all"] }
mdns-sd = "0.11"
rusqlite = { version = "0.32", features = ["bundled"] }
blake3 = "1.5"
chacha20poly1305 = "0.10"
ed25519-dalek = { version = "2.1", features = ["rand_core"] }
x25519-dalek = { version = "2.0", features = ["static_secrets"] }
hkdf = "0.12"
sha2 = "0.10"
hex = "0.4"
rand = "0.8"
uuid = { version = "1.11", features = ["v4", "serde"] }
tracing = "0.1"
tracing-subscriber = "0.3"
directories = "5.0"
walkdir = "2.5"

[target.'cfg(target_os = "windows")'.dependencies]
winreg = "0.52"

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

### 2.3 `src-tauri/capabilities/default.json` (Tauri v2 Permissions)

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for Raydrop main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-show",
    "core:window:allow-hide",
    "core:window:allow-set-focus",
    "core:event:default",
    "dialog:default",
    "fs:default",
    "notification:default",
    "process:default",
    "shell:default"
  ]
}
```

---

## 3. Core Protocols, Cryptography & WAN NAT Traversal

### 3.1 Cryptographic Identity & Session Key Derivation (`crypto/identity.rs`)

1. **Persistent Identity (`Ed25519`):**
   * On first launch, Raydrop generates an `Ed25519` keypair stored in SQLite (`local_identity`).
   * `device_id` is the first 16 bytes (32 hex chars, formatted as `RAY-XXXX-XXXX-XXXX`) of `BLAKE3(ed25519_public_key)`. This prevents `device_id` spoofing: any peer can verify that `BLAKE3(pubkey) == device_id`.
2. **Authenticated Diffie-Hellman Handshake (`HELLO` / `HELLO_ACK`):**
   * Before sending file manifests, initiator and responder exchange ephemeral `X25519` public keys signed by their long-term `Ed25519` identity keys.
   * Both sides compute `shared_secret = X25519(my_ephemeral_secret, peer_ephemeral_pub)` and derive a 256-bit `session_key` via `HKDF-SHA256(salt = session_id_bytes, ikm = shared_secret, info = b"raydrop-v1-chunk-key")`.
3. **Deterministic Chunk AEAD Encryption (`ChaCha20-Poly1305`):**
   * Every binary chunk is encrypted with `ChaCha20-Poly1305` using `session_key`.
   * The 12-byte Nonce is constructed deterministically with zero collision risk:
     `Nonce (12B) = file_id (4B BigEndian) || chunk_offset (8B BigEndian)`.
   * Additional Authenticated Data (`AAD`) binds the 36-byte header so header tampering is immediately rejected.

---

### 3.2 Hybrid LAN + WAN Connection Architecture (`network/signaling.rs` & `network/nat.rs`)

```text
┌───────────────────┐         1. WSS Presence & Ed25519 Auth          ┌────────────────────────────────┐
│   Peer A (Sender) │ ◄─────────────────────────────────────────────► │   Raydrop Signal/STUN Server   │
│  Behind Home NAT  │                                                 │  (Deployed on Dokploy/Traefik) │
└─────────┬─────────┘         2. STUN UDP :3478 (Discover Public IP)  └───────────────▲────────────────┘
          │           ◄───────────────────────────────────────────────────────────────┘
          │                                                                           │
          │                   3. Exchange Candidates (LAN IP + WAN Reflexive IP:Port) │
          │                                                                           ▼
          │                   4. Simultaneous QUIC/UDP Hole Punching         ┌────────┴──────────┐
          └────────────────────────────────────────────────────────────────► │ Peer B (Receiver) │
                              Direct Encrypted P2P File Stream               │ Behind Office NAT │
                              (Zero Server Bandwidth / Storage)              └───────────────────┘
```

1. **Dual-Stack Socket Sharing (`socket2`):**
   * Raydrop binds a single UDP socket (`0.0.0.0:ephemeral_port`) with `SO_REUSEADDR` for both STUN reflection and the `quinn` QUIC endpoint, plus a local TCP listener on the same port number for ultra-fast LAN TCP fallback.
2. **WAN Signaling Protocol (WSS JSON Messages over `wss://signal.yourdomain.com/ws`):**
   * `REGISTER`: `{ "type": "REGISTER", "device_id": "...", "ed25519_pubkey": "...", "timestamp": 1730000000, "signature": "..." }`
   * `PEER_STATUS`: Query or subscribe to online status of saved WAN contacts.
   * `CONNECT_INTENT`: Sender requests connection to target `device_id`, supplying its gathered candidates:
     * `lan_candidates`: `["192.168.1.15:54321"]`
     * `wan_candidate`: `"203.0.113.42:54321"` (discovered via the UDP STUN reflector on `signal.yourdomain.com:3478`).
   * `PUNCH_SYNC`: Signaling server forwards candidates to Receiver; both sides simultaneously send 5 UDP hole-punch packets (`"RAYD_PUNCH" + session_id`) to each other's `wan_candidate` every 100ms before initiating the QUIC connection, opening stateful NAT firewall pinholes on both sides.

---

### 3.3 Network Handshake & Streaming Wire Format (`network/transport.rs`)

#### A. Cryptographic Hello (`JSON` Frame)
```json
{
  "type": "HELLO",
  "session_id": "c1f7b824-a78b-4b1a-8219-482a5ad6c510",
  "sender_device_id": "RAY-A4F8-9D12-77C1",
  "sender_alias": "Satria-MacBook",
  "ed25519_pubkey": "hex_encoded_32_bytes",
  "x25519_ephemeral_pubkey": "hex_encoded_32_bytes",
  "signature": "hex_ed25519_sig_over(session_id + x25519_ephemeral_pubkey)"
}
```

#### B. Manifest Payload (`JSON` Frame - Instant, Non-Blocking)
Sent immediately after `HELLO_ACK`. Note: `blake3_hash` is computed **on-the-fly during streaming** rather than blocking the UI before transfer starts!
```json
{
  "type": "MANIFEST",
  "session_id": "c1f7b824-a78b-4b1a-8219-482a5ad6c510",
  "total_files": 2,
  "total_size_bytes": 104857600,
  "files": [
    {
      "file_id": 1,
      "relative_path": "Documents/report.pdf",
      "size_bytes": 52428800,
      "modified_unix_ms": 1731350000000
    }
  ]
}
```
> **Security Rule (Path Sanitization):** The receiver MUST sanitize `relative_path` by stripping leading `/`, `\`, drive letters (`C:`), and rejecting any path containing `..` (`Component::ParentDir`) before joining with `target_directory`.

#### C. Handshake Response (`JSON` Frame)
```json
{
  "type": "HANDSHAKE_RESPONSE",
  "session_id": "c1f7b824-a78b-4b1a-8219-482a5ad6c510",
  "status": "ACCEPTED",
  "resume_offsets": [
    {
      "file_id": 1,
      "offset_bytes": 10485760,
      "partial_blake3_hash": "optional_hex_hash_of_existing_prefix"
    }
  ]
}
```

#### D. Binary Chunk Framing (Streaming Payload - 512 KB Plaintext Chunks)
Each binary frame sent over the stream has a 36-byte fixed header followed by the encrypted payload + 16-byte Poly1305 tag:
```text
┌──────────────────┬─────────────────┬─────────────────┬─────────────────┬───────────────────────────┐
│ Magic (4 Bytes)  │ Session ID (16B)│ File ID (4B)    │ Offset (8B)     │ Ciphertext Length (4B)    │
│ "RAYD"           │ UUID Bytes      │ u32 BigEndian   │ u64 BigEndian   │ u32 BigEndian (Len + 16B) │
├──────────────────┴─────────────────┴─────────────────┴─────────────────┴───────────────────────────┤
│ Encrypted Chunk Data (ChaCha20-Poly1305 Ciphertext + 16-Byte Auth Tag, Nonce = FileID || Offset)   │
└────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

#### E. File Completion & On-The-Fly Integrity Frame (`JSON` Control Frame, `File ID = 0`)
When all chunks of a file are sent, Sender sends a control frame with the final streamed BLAKE3 hash. Receiver finalizes its incremental `blake3::Hasher`, verifies the digest, and renames `<filename>.raypart` to `<filename>`:
```json
{
  "type": "FILE_COMPLETE",
  "session_id": "c1f7b824-a78b-4b1a-8219-482a5ad6c510",
  "file_id": 1,
  "blake3_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

---

### 3.4 Local Database Schema (`db/storage.rs`)

SQLite Database created at `%APPDATA%/Raydrop/storage.db` (Windows) or `~/.config/raydrop/storage.db` (Linux/macOS):

```sql
CREATE TABLE IF NOT EXISTS local_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Stores: device_id, alias_name, ed25519_secret_hex, ed25519_public_hex,
--         discoverable ('true'/'false'), download_dir, signaling_url, stun_addr

CREATE TABLE IF NOT EXISTS contacts (
    device_id TEXT PRIMARY KEY,
    alias_name TEXT NOT NULL,
    ed25519_pubkey TEXT,                -- Pinned on first verified connection (TOFU)
    network_type TEXT NOT NULL,         -- 'lan' or 'wan'
    last_known_ip TEXT,
    last_known_port INTEGER,
    trust_level TEXT DEFAULT 'ask',     -- 'ask', 'trusted_auto_accept', 'blocked'
    is_online INTEGER DEFAULT 0,
    last_seen_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS transfer_history (
    id TEXT PRIMARY KEY,                -- session_id
    peer_device_id TEXT NOT NULL,
    peer_alias TEXT NOT NULL,
    direction TEXT NOT NULL,            -- 'incoming' or 'outgoing'
    network_route TEXT NOT NULL,        -- 'lan_direct' or 'wan_p2p'
    total_files INTEGER NOT NULL,
    total_bytes INTEGER NOT NULL,
    transferred_bytes INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,               -- 'in_progress', 'paused', 'completed', 'cancelled', 'failed', 'rejected'
    target_path TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(peer_device_id) REFERENCES contacts(device_id)
);
```

---

## 4. Single-Instance CLI & OS Context Menu Integrations

### 4.1 Single-Instance Argument Forwarding (`lib.rs`)
Raydrop uses `tauri-plugin-single-instance`.
* If Raydrop is **already running in the System Tray** and the user right-clicks a file/folder in Explorer/Finder/Nautilus (`raydrop --send "C:\file1.zip" "C:\folder2"`):
  1. The second process forwards its `argv` list to the running primary instance via the single-instance callback and exits immediately.
  2. The primary instance brings the main window to focus, parses all paths after `--send`, recursively indexes any folders using `walkdir`, and opens the "Select Recipient Peer" modal with the files queued.
* If Raydrop is **not running**, it starts normally, initializes the tray & network daemons, and queues the `--send` paths immediately once the UI loads.

### 4.2 Windows Registry & SendTo Module (`os/win_registry.rs`)
Registers context menu entries under `HKEY_CURRENT_USER` (no Admin UAC prompt required):
1. **Single/Multiple Files:** `HKCU\Software\Classes\*\shell\RaydropSend\command` -> `"\"<exe_path>\" --send \"%1\""`
2. **Directories/Folders:** `HKCU\Software\Classes\Directory\shell\RaydropSend\command` -> `"\"<exe_path>\" --send \"%1\""`
3. **Multi-File Batch Selection (`SendTo`):** Creates a shortcut/wrapper in `%APPDATA%\Microsoft\Windows\SendTo\Raydrop.cmd` executing `"<exe_path>" --send %*` so selecting 20 files at once invokes Raydrop **only once** with all 20 paths.

### 4.3 Ubuntu / Linux Nautilus Module (`os/linux_nautilus.rs`)
Generates `~/.local/share/nautilus/scripts/Send with Raydrop`:
```bash
#!/usr/bin/env bash
# Generated by Raydrop
IFS=$'\n'
if [ -n "$NAUTILUS_SCRIPT_SELECTED_FILE_PATHS" ]; then
    raydrop --send $NAUTILUS_SCRIPT_SELECTED_FILE_PATHS
else
    raydrop --send "$@"
fi
```
Applies `0o755` (`chmod +x`) permissions automatically during initialization.

### 4.4 macOS Finder Quick Action (`os/mac_finder.rs`)
Creates a macOS Finder Service workflow script / helper in `~/Library/Services/Send with Raydrop.workflow` (or registers custom URL scheme `raydrop://send`) that forwards selected Finder items to `/Applications/Raydrop.app/Contents/MacOS/raydrop --send "$@"`.

---

## 5. Self-Hosted WAN Signaling & STUN Server (`signal-server/` for Dokploy + Traefik)

To enable WAN transfers across the internet with **zero file storage and zero bandwidth relay cost**, `signal-server/` provides a single ultra-lightweight Rust binary (~15 MB RAM usage) that runs:
1. **HTTP/WSS Signaling Server (`0.0.0.0:8080`)**: Routed through **Traefik** in **Dokploy** with automatic Let's Encrypt SSL (`wss://signal.yourdomain.com/ws`).
2. **UDP STUN / Public Endpoint Reflector (`0.0.0.0:3478/udp`)**: Exposed directly on port `3478/udp` on the VPS so peers can discover their public NAT reflexive `IP:Port` (`XOR-MAPPED-ADDRESS` / JSON UDP echo).

### 5.1 `signal-server/Cargo.toml`
```toml
[package]
name = "raydrop-signal-server"
version = "1.0.0"
edition = "2021"

[dependencies]
axum = { version = "0.7", features = ["ws"] }
tokio = { version = "1.41", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
dashmap = "6.1"
ed25519-dalek = "2.1"
blake3 = "1.5"
hex = "0.4"
tracing = "0.1"
tracing-subscriber = "0.3"
```

### 5.2 `signal-server/Dockerfile`
```dockerfile
FROM rust:1.82-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM alpine:3.20
RUN apk add --no-cache ca-certificates
WORKDIR /app
COPY --from=builder /app/target/release/raydrop-signal-server /usr/local/bin/raydrop-signal-server
EXPOSE 8080/tcp
EXPOSE 3478/udp
ENV SIGNAL_HTTP_ADDR=0.0.0.0:8080
ENV SIGNAL_UDP_ADDR=0.0.0.0:3478
CMD ["raydrop-signal-server"]
```

### 5.3 `signal-server/docker-compose.yml` (Dokploy + Traefik Ready)
```yaml
services:
  raydrop-signal:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: raydrop-signal
    restart: unless-stopped
    environment:
      - SIGNAL_HTTP_ADDR=0.0.0.0:8080
      - SIGNAL_UDP_ADDR=0.0.0.0:3478
      - RUST_LOG=info
    ports:
      # Direct UDP port for STUN / NAT Public IP:Port Discovery (bypasses HTTP proxy)
      - "3478:3478/udp"
    networks:
      - dokploy-network
    labels:
      - "traefik.enable=true"
      - "traefik.docker.network=dokploy-network"
      - "traefik.http.routers.raydrop-signal.rule=Host(`${SIGNAL_DOMAIN:-signal.example.com}`)"
      - "traefik.http.routers.raydrop-signal.entrypoints=websecure"
      - "traefik.http.routers.raydrop-signal.tls.certresolver=letsencrypt"
      - "traefik.http.services.raydrop-signal.loadbalancer.server.port=8080"

networks:
  dokploy-network:
    external: true
```
> **Dokploy Deployment Note:** Simply deploy `signal-server/` in Dokploy as a Docker Compose or Dockerfile service, set `SIGNAL_DOMAIN=signal.yourdomain.com`, and ensure UDP port `3478` is open on your VPS firewall (`ufw allow 3478/udp`).

---

## 6. System Tray & Privacy Rules Engine

```text
                               ┌───────────────────────────┐
                               │  Incoming File Connection │
                               │   (LAN mDNS or WAN P2P)   │
                               └─────────────┬─────────────┘
                                             │
                                             ▼
                             Is Raydrop Discoverability ON?
                                     │               │
                            [NO]     │               │ [YES]
                             ┌───────┘               └───────┐
                             ▼                               ▼
                      Auto-Reject Frame           Verify Ed25519 Signature
                      (Silent Drop)               & Check Device ID Trust
                                                             │
                                     ┌───────────────────────┼───────────────────────┐
                                     ▼                       ▼                       ▼
                            Trust: "blocked"      Trust: "trusted_auto_accept"  Trust: "ask"
                                     │                       │                       │
                                     ▼                       ▼                       ▼
                              Auto-Reject Frame    Auto-Accept Transfer     Show Receiver Modal UI
                                                   (Save to default dir)    (User prompt: Accept/Reject)
```

---

## 7. AGY CLI Execution Prompts (Step-by-Step Implementation Guide)

Provide these prompts sequentially to AGY CLI / AI coding agent to construct both the Desktop Client and the Dokploy Signaling/STUN Server.

---

### Phase 1: Initialize Tauri v2 Project, Capabilities & Cryptography Core

```text
PROMPT 1:
Set up the Tauri v2 project named "raydrop" using Svelte, Tailwind CSS, and TypeScript according to `raydrop_architecture_agy_cli_specification.md`:
1. Create `package.json`, `vite.config.ts`, `tailwind.config.js`, `postcss.config.js`, `tsconfig.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and `src-tauri/capabilities/default.json`.
2. Implement `src-tauri/src/crypto/identity.rs`:
   - Generate and persist an Ed25519 identity keypair and derive `device_id` (`RAY-XXXX-XXXX-XXXX` from BLAKE3 of the public key).
   - Implement ephemeral X25519 Diffie-Hellman key exchange signed by Ed25519, HKDF-SHA256 session key derivation, and ChaCha20-Poly1305 chunk encryption/decryption using deterministic 12-byte nonces (`file_id (4B) || offset (8B)`) and 36-byte header AAD.
3. Implement `src-tauri/src/db/storage.rs` with SQLite tables (`local_settings`, `contacts`, `transfer_history`) and full CRUD helpers.
```

---

### Phase 2: Self-Hosted WAN Signaling & STUN Server (`signal-server/`)

```text
PROMPT 2:
Create the self-hosted WAN Signaling & STUN Reflector service in `signal-server/` ready for Dokploy and Traefik deployment:
1. Create `signal-server/Cargo.toml`, `signal-server/Dockerfile`, `signal-server/docker-compose.yml` (with `dokploy-network` and Traefik labels), and `signal-server/.env.example`.
2. Implement `signal-server/src/main.rs`:
   - Run a UDP reflector socket on `SIGNAL_UDP_ADDR` (`0.0.0.0:3478`) that responds to `"RAYD_STUN"` packets with `{ "reflexive_addr": "<peer_ip>:<peer_port>" }` and also responds to standard STUN Binding Requests (`0x0001`).
   - Run an Axum HTTP + WebSocket server on `SIGNAL_HTTP_ADDR` (`0.0.0.0:8080`) with `/health` and `/ws` endpoints.
   - Maintain a stateless in-memory `DashMap<String, mpsc::UnboundedSender<Message>>` mapping verified `device_id` -> WebSocket sender.
   - Verify `Ed25519` signatures on `REGISTER`, broadcast `PEER_STATUS` updates, and route `CONNECT_INTENT`, `CANDIDATE_ANSWER`, and `PUNCH_SYNC` messages between peers for WAN NAT hole-punching.
```

---

### Phase 3: Rust Hybrid Network Engine (LAN mDNS + WAN Hole-Punching + Streaming Transport)

```text
PROMPT 3:
Implement the hybrid LAN/WAN networking stack in `src-tauri/src/network/`:
1. `mdns.rs`: Broadcast and discover `_raydrop._udp.local.` using `mdns-sd` with `device_id`, `alias_name`, `ed25519_pubkey`, and listening port. Emit Tauri event `peer_discovered`.
2. `nat.rs`: Bind a shared UDP socket via `socket2` (`SO_REUSEADDR`), query the UDP reflector on `3478/udp` to discover the public WAN `IP:Port`, and implement simultaneous UDP hole-punching (`RAYD_PUNCH` bursts).
3. `signaling.rs`: Connect to the configured `signaling_url` (`wss://...`) with automatic reconnection, authenticate via `Ed25519`, track online WAN contacts, and coordinate candidate exchange + `PUNCH_SYNC`.
4. `transport.rs`:
   - Support both QUIC (`quinn` over the punched UDP socket) and direct TCP streams for LAN/WAN transfers.
   - Implement the `HELLO` -> `HELLO_ACK` -> `MANIFEST` -> `HANDSHAKE_RESPONSE` handshake, strict path traversal sanitization, 512KB binary chunk framing (`RAYD` magic header + ChaCha20-Poly1305 payload), resume offset support (`.raypart` partial files), and on-the-fly streaming `BLAKE3` verification finalized via the `FILE_COMPLETE` frame.
   - Emit real-time Tauri events: `incoming_transfer_request`, `transfer_progress`, and `transfer_state_changed`.
```

---

### Phase 4: Single-Instance IPC, System Tray & OS Context Menus

```text
PROMPT 4:
Implement the OS integration, CLI handler, and Tauri commands in `src-tauri/src/`:
1. `os/win_registry.rs`: Register/unregister "Send with Raydrop" in Windows Registry under `HKCU\Software\Classes\*\shell\RaydropSend` (files), `HKCU\Software\Classes\Directory\shell\RaydropSend` (folders), and `%APPDATA%\Microsoft\Windows\SendTo\Raydrop.cmd` (multi-file selection).
2. `os/linux_nautilus.rs` & `os/mac_finder.rs`: Implement Nautilus script generator (`~/.local/share/nautilus/scripts/Send with Raydrop`) and macOS Finder Service workflow helper.
3. `commands.rs`: Expose Tauri IPC commands: `get_app_state`, `toggle_discoverability`, `update_settings`, `get_contacts`, `add_wan_contact`, `update_contact_trust`, `initiate_transfer`, `respond_transfer_request`, `cancel_transfer`, `get_transfer_history`, and `register_os_context_menu`.
4. `lib.rs`: Configure `tauri-plugin-single-instance` to capture `--send <path1> <path2>...` arguments from secondary launches, focus the main window, and emit `cli_send_files` to the frontend. Set up the System Tray icon with "Status: Discoverable (Toggle)", "Open Raydrop", and "Quit".
```

---

### Phase 5: Frontend Development (Svelte + Tailwind UI)

```text
PROMPT 5:
Develop the complete Raydrop frontend in `src/` using Svelte and Tailwind CSS (modern dark slate/cyan theme):
1. `Header.svelte`: Show local Device ID (`RAY-XXXX-...` with copy button), LAN IP, WAN Signaling connection badge (Connected/Offline), and the prominent "Discoverable (ON/OFF)" toggle.
2. `ContactList.svelte`: Tabs or sections for "Nearby LAN Devices" (auto-discovered via mDNS) and "Saved & WAN Contacts" (with "+ Add WAN Peer by Device ID" modal, online indicator, trust level selector: Ask / Auto-Accept / Blocked, and drag-and-drop file/folder zone).
3. `TransferModal.svelte`: Popup for both incoming transfer requests (`incoming_transfer_request` with Accept/Reject + folder picker) and outgoing CLI `--send` file queues (`cli_send_files` peer selector).
4. `ActiveTransfers.svelte`: Real-time progress bars, transfer route badge (`LAN` vs `WAN P2P`), throughput speed (MB/s), ETA, and Cancel button, plus recent transfer history.
5. `TraySettings.svelte`: Configure Device Alias, Default Download Directory, WAN Signaling Server URL (`wss://...`), STUN Server (`host:3478`), and one-click "Register OS Right-Click Menu" button.
```

---

### Phase 6: End-to-End Verification & Build Check

```text
PROMPT 6:
Verify the entire workspace:
1. Run `cargo check` inside `signal-server/` and verify the Signaling/STUN server compiles cleanly.
2. Run `cargo check` inside `src-tauri/` and `npm run build` in the root directory, fixing any Rust borrow-checker, Tauri v2 API, or TypeScript errors until both build with zero errors.
```
