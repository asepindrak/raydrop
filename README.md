<p align="center">
  <img src="assets/logo.png" alt="Raydrop Logo" width="200" />
</p>

<h1 align="center">Raydrop</h1>
<p align="center">
  <strong>Universal P2P File Transfer (LAN + Direct WAN)</strong><br />
  Fast, Encrypted, Multi-Segment Parallel File Sharing Across Devices
</p>

Raydrop is a zero-server-storage P2P file transfer desktop application for **Windows, macOS, and Linux** built with **Tauri v2 (Rust + Svelte + Tailwind CSS)**.

## Key Features
- **Instant Local LAN Discovery:** Uses mDNS (`_raydrop._tcp.local.`) to automatically detect nearby peers on the same Wi-Fi/LAN.
- **Direct WAN P2P via NAT Hole-Punching:** Connects across the internet using a self-hosted **Signaling + STUN Reflector (`signal-server/`)** deployed on **Dokploy + Traefik**, establishing direct **QUIC over UDP** streams with **0 server bandwidth/storage load**.
- **End-to-End Authenticated Encryption:**
  - `Ed25519` device identity (`RAY-XXXX-XXXX-XXXX` derived from `BLAKE3(pubkey)`).
  - Ephemeral `X25519` Diffie-Hellman key exchange + `HKDF-SHA256` session key derivation.
  - `ChaCha20-Poly1305` AEAD encryption per 512 KB chunk (`Nonce = file_id (4B) || offset (8B)`).
  - On-the-fly streaming `BLAKE3` verification (`FILE_COMPLETE` frame) with `.raypart` partial file resume support.
- **Single-Instance OS Context Menu Integration:** Right-click any file, folder, or multi-file selection (`SendTo`) in Windows Explorer, Ubuntu Nautilus, or macOS Finder to queue transfers directly into the running System Tray instance.

---

## 1. Deploying `signal-server/` on Dokploy + Traefik

1. In **Dokploy**, create a new **Docker Compose** or **Application** pointing to the `signal-server/` directory.
2. Set the environment variable:
   ```env
   SIGNAL_DOMAIN=signal.yourdomain.com
   SIGNAL_HTTP_ADDR=0.0.0.0:8080
   SIGNAL_UDP_ADDR=0.0.0.0:3478
   ```
3. Ensure port **`3478/udp`** is allowed on your VPS firewall (`sudo ufw allow 3478/udp`) so STUN public `IP:Port` discovery works alongside Traefik's HTTPS/WSS proxy (`wss://signal.yourdomain.com/ws`).

---

## 2. Building & Running the Desktop App

### Prerequisites
- **Node.js** 18+
- **Rust** toolchain (`rustup` / `cargo`)

### Development
```bash
npm install
npm run tauri dev
```

### Production Installer Build
```bash
npm run tauri build
```

