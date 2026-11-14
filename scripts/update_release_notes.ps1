# Update GitHub Release notes with rich formatting, logo, and release details
$ErrorActionPreference = "Stop"

$credInput = @"
protocol=https
host=github.com

"@

$credOutput = $credInput | git credential fill
$token = ($credOutput | Select-String "^password=(.+)$").Matches.Groups[1].Value

if (-not $token) {
    Write-Error "GitHub access token not found in git credentials."
    exit 1
}

$headers = @{
    "Authorization" = "Bearer $token"
    "Accept"        = "application/vnd.github+json"
    "User-Agent"    = "Raydrop-Release-Updater"
}

$repo = "asepindrak/raydrop"
$tag = "v1.2.6"

$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/tags/$tag" -Headers $headers -Method Get

Write-Host "Found release: $($release.name) (ID: $($release.id))" -ForegroundColor Cyan

$releaseBody = @"
<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop ${tag}: Universal Clipboard Sync & Mobile P2P Enhancements</h2>

<p align="center">
  <strong>Fast, Resumable, End-to-End Encrypted File Sharing Across Devices with Zero Server Storage.</strong><br/>
  Available natively on <b>Windows</b>, <b>macOS</b>, <b>Linux</b>, and <b>Android</b>.
</p>

---

### 📦 Available Downloads

| Operating System | Package | Architecture | Download Link |
| :--- | :---: | :---: | :--- |
| **Windows** | `.msi` | x64 (64-bit) | [**Raydrop_1.2.6_x64_en-US.msi**](https://github.com/asepindrak/raydrop/releases/download/$tag/Raydrop_1.2.6_x64_en-US.msi) |
| **macOS** | `.dmg` | Universal (Apple Silicon & Intel) | [**Raydrop_1.2.6_universal.dmg**](https://github.com/asepindrak/raydrop/releases/download/$tag/Raydrop_1.2.6_universal.dmg) |
| **Linux (Debian / Ubuntu)** | `.deb` | x64 (amd64) | [**Raydrop_1.2.6_amd64.deb**](https://github.com/asepindrak/raydrop/releases/download/$tag/Raydrop_1.2.6_amd64.deb) |
| **Linux (Universal)** | `.AppImage` | x64 (amd64) | [**Raydrop_1.2.6_amd64.AppImage**](https://github.com/asepindrak/raydrop/releases/download/$tag/Raydrop_1.2.6_amd64.AppImage) |
| **Android** | `.apk` | ARM64 (\`arm64-v8a\`) | [**Raydrop-arm64-v8a.apk**](https://github.com/asepindrak/raydrop/releases/download/$tag/Raydrop-arm64-v8a.apk) |

---

### 🚀 What's New & Highlights in v1.2.6

#### 📋 Universal Clipboard Sync (Seamless Auto-Paste & Native Image Support)
- **Instant Cross-Device Auto-Paste:** When sending clipboard text or links to trusted devices, content is automatically written straight into the recipient's system clipboard. You can immediately press **Ctrl+V** (or paste on mobile) in any target application (Word, browser, messaging apps) without having to copy it again from chat bubbles.
- **Full Clipboard Image & Screenshot Sync:** Copy any screenshot or image to your OS clipboard on PC 1, trigger Universal Clipboard Sync, and immediately paste the actual native image bitmap into Photoshop, WhatsApp, Discord, or graphics editors on PC 2 or mobile devices.
- **Interactive Image Preview & Quick Paste:** The Universal Clipboard dialog features image thumbnail previews and live paste interception (\`Ctrl+V\`) for rapid broadcast across peers.

#### 📱 Android & Mobile P2P File Transfer Enhancements
- **Smart Magic-Bytes Header Inspection:** Resolved an issue where Android MediaStore URIs caused transferred files to default to \`file_1.bin\`. The backend now inspects file magic bytes (JPEG, PNG, GIF, WebP, PDF, MP4, MP3, WAV, ZIP/DOCX/XLSX, MKV) to guarantee exact file extensions and recognizable filenames (\`Image_...\`, \`Video_...\`, etc.).
- **Multi-File Queueing on Mobile:** Added an interactive **"+ Add More"** file queue picker in the transfer modal, enabling users to pick and bundle multiple items before dispatching.
- **Android Storage & Folder Handling:** Integrated system event handlers for "Open File" and "Open Folder", directing users directly to their native Download directory (\`/storage/emulated/0/Download\`) and native Android file manager.
- **SAF Sandbox Fallback:** Replaced unsupported folder browsing on mobile with seamless multi-file selection to comply with Android Storage Access Framework (SAF) security constraints.

#### ⚡ Performance & Core P2P Features
- **Parallel Multi-Segment Engine:** Up to 6 simultaneous streaming workers for rapid chunk delivery.
- **Bit-Perfect BLAKE3 Hash Verification:** Dynamic chunk-level integrity verification.
- **O(1) Instant Auto-Resume (\`.raypart\`):** Interrupted downloads pick up right where they left off.
- **End-to-End Encryption (E2EE):** Every packet is secured with ChaCha20-Poly1305 and X25519 ephemeral Diffie-Hellman keys.
- **Precision Speed Limiter (QoS):** Flexible bandwidth throttling to keep Wi-Fi networks stable.
- **Zero Cloud Storage Guarantee:** 100% direct peer-to-peer transmission.
"@

$patchBody = @{
    name = "Raydrop ${tag}: Universal Clipboard Auto-Paste, Image Sync & Mobile P2P"
    body = $releaseBody
} | ConvertTo-Json -Depth 5

$updateRes = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/$($release.id)" `
    -Headers $headers `
    -Method Patch `
    -Body ([System.Text.Encoding]::UTF8.GetBytes($patchBody)) `
    -ContentType "application/json; charset=utf-8"

Write-Host "Successfully updated release notes for $tag!" -ForegroundColor Green
Write-Host "Release URL: $($updateRes.html_url)" -ForegroundColor Green
