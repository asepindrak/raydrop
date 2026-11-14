#!/usr/bin/env python3
import json
import subprocess
import urllib.request
import sys

def get_git_token():
    cred_proc = subprocess.Popen(
        ["git", "credential", "fill"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    stdout, _ = cred_proc.communicate("protocol=https\nhost=github.com\n\n")
    for line in stdout.splitlines():
        if line.startswith("password="):
            return line[len("password="):].strip()
    return None

def update_release(tag="v1.2.6", title=None, body=None):
    token = get_git_token()
    if not token:
        print("ERROR: GitHub token not found in git credentials.", file=sys.stderr)
        sys.exit(1)

    repo = "asepindrak/raydrop"
    url = f"https://api.github.com/repos/{repo}/releases/tags/{tag}"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "User-Agent": "Raydrop-Release-Updater",
    }

    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req) as resp:
        release_data = json.loads(resp.read().decode("utf-8"))

    release_id = release_data["id"]
    patch_url = f"https://api.github.com/repos/{repo}/releases/{release_id}"

    payload = {}
    if title:
        payload["name"] = title
    if body:
        payload["body"] = body

    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(patch_url, data=data, headers=headers, method="PATCH")
    req.add_header("Content-Type", "application/json; charset=utf-8")

    with urllib.request.urlopen(req) as resp:
        result = json.loads(resp.read().decode("utf-8"))
        print(f"Successfully updated release {tag}!")
        print(f"URL: {result.get('html_url')}")

if __name__ == "__main__":
    tag = sys.argv[1] if len(sys.argv) > 1 else "v1.2.6"
    title = f"Raydrop {tag}: Universal Clipboard Auto-Paste, Image Sync & Mobile P2P"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: Universal Clipboard Sync & Mobile P2P Enhancements</h2>

<p align="center">
  <strong>Fast, Resumable, End-to-End Encrypted File Sharing Across Devices with Zero Server Storage.</strong><br/>
  Available natively on <b>Windows</b>, <b>macOS</b>, <b>Linux</b>, and <b>Android</b>.
</p>

---

### 📦 Available Downloads

| Operating System | Package | Architecture | Download Link |
| :--- | :---: | :---: | :--- |
| **Windows** | `.msi` | x64 (64-bit) | [**Raydrop_1.2.6_x64_en-US.msi**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop_1.2.6_x64_en-US.msi) |
| **macOS** | `.dmg` | Universal (Apple Silicon & Intel) | [**Raydrop_1.2.6_universal.dmg**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop_1.2.6_universal.dmg) |
| **Linux (Debian / Ubuntu)** | `.deb` | x64 (amd64) | [**Raydrop_1.2.6_amd64.deb**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop_1.2.6_amd64.deb) |
| **Linux (Universal)** | `.AppImage` | x64 (amd64) | [**Raydrop_1.2.6_amd64.AppImage**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop_1.2.6_amd64.AppImage) |
| **Android** | `.apk` | ARM64 (`arm64-v8a`) | [**Raydrop-arm64-v8a.apk**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop-arm64-v8a.apk) |

---

### 🚀 What's New & Highlights in {tag}

#### 📋 Universal Clipboard Sync (Seamless Auto-Paste & Native Image Support)
- **Instant Cross-Device Auto-Paste:** When sending clipboard text or links to trusted devices, content is automatically written straight into the recipient's system clipboard. You can immediately press **Ctrl+V** (or paste on mobile) in any target application (Word, browser, messaging apps) without having to copy it again from chat bubbles.
- **Full Clipboard Image & Screenshot Sync:** Copy any screenshot or image to your OS clipboard on PC 1, trigger Universal Clipboard Sync, and immediately paste the actual native image bitmap into Photoshop, WhatsApp, Discord, or graphics editors on PC 2 or mobile devices.
- **Interactive Image Preview & Quick Paste:** The Universal Clipboard dialog features image thumbnail previews and live paste interception (`Ctrl+V`) for rapid broadcast across peers.

#### 📱 Android & Mobile P2P File Transfer Enhancements
- **Smart Magic-Bytes Header Inspection:** Resolved an issue where Android MediaStore URIs caused transferred files to default to `file_1.bin`. The backend now inspects file magic bytes (JPEG, PNG, GIF, WebP, PDF, MP4, MP3, WAV, ZIP/DOCX/XLSX, MKV) to guarantee exact file extensions and recognizable filenames (`Image_...`, `Video_...`, etc.).
- **Multi-File Queueing on Mobile:** Added an interactive **"+ Add More"** file queue picker in the transfer modal, enabling users to pick and bundle multiple items before dispatching.
- **Android Storage & Folder Handling:** Integrated system event handlers for "Open File" and "Open Folder", directing users directly to their native Download directory (`/storage/emulated/0/Download`) and native Android file manager.
- **SAF Sandbox Fallback:** Replaced unsupported folder browsing on mobile with seamless multi-file selection to comply with Android Storage Access Framework (SAF) security constraints.

#### ⚡ Performance & Core P2P Features
- **Parallel Multi-Segment Engine:** Up to 6 simultaneous streaming workers for rapid chunk delivery.
- **Bit-Perfect BLAKE3 Hash Verification:** Dynamic chunk-level integrity verification.
- **O(1) Instant Auto-Resume (`.raypart`):** Interrupted downloads pick up right where they left off.
- **End-to-End Encryption (E2EE):** Every packet is secured with ChaCha20-Poly1305 and X25519 ephemeral Diffie-Hellman keys.
- **Precision Speed Limiter (QoS):** Flexible bandwidth throttling to keep Wi-Fi networks stable.
- **Zero Cloud Storage Guarantee:** 100% direct peer-to-peer transmission.
"""
    update_release(tag=tag, title=title, body=body)
