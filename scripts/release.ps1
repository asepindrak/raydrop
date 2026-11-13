# ==============================================================================
# Raydrop Automated GitHub Release & Landing Page Sync Script
# ==============================================================================
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/release.ps1
#
# This script:
#   1. Reads version from package.json automatically
#   2. Syncs fallback links in landing/index.html
#   3. Builds installer & portable binaries if needed
#   4. Pushes git commits & tags to GitHub
#   5. Creates GitHub Release & uploads .msi and .exe binaries
#   6. Landing page (raydrop.web.id) updates in real-time via GitHub API
# ==============================================================================

$ErrorActionPreference = "Stop"

$repoRoot = (Get-Item $PSScriptRoot).Parent.FullName
Set-Location $repoRoot

# 1. Read Version
$pkgJson = Get-Content "$repoRoot/package.json" -Raw | ConvertFrom-Json
$ver = $pkgJson.version
$tag = "v$ver"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Publishing Raydrop $tag Release to GitHub" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 2. Update landing/index.html fallback values
$landingHtmlPath = "$repoRoot/landing/index.html"
if (Test-Path $landingHtmlPath) {
    Write-Host "Syncing landing page static fallback to $tag..." -ForegroundColor Yellow
    $landingContent = Get-Content $landingHtmlPath -Raw
    $landingContent = $landingContent -replace 'Raydrop_[\d\.]+_x64_en-US\.msi', "Raydrop_${ver}_x64_en-US.msi"
    $landingContent = $landingContent -replace 'Raydrop_[\d\.]+_x64-portable\.exe', "Raydrop_${ver}_x64-portable.exe"
    $landingContent = $landingContent -replace 'download/v[\d\.]+/Raydrop', "download/$tag/Raydrop"
    Set-Content -Path $landingHtmlPath -Value $landingContent -Encoding UTF8
}

# 3. Verify Artifacts
$msiPath = "$repoRoot/src-tauri/target/release/bundle/msi/Raydrop_${ver}_x64_en-US.msi"
$exePath = "$repoRoot/src-tauri/target/release/raydrop.exe"

if (-not (Test-Path $msiPath)) {
    Write-Host "Installer not found at $msiPath. Building now..." -ForegroundColor Yellow
    npm run tauri build
}

if (-not (Test-Path $msiPath) -or -not (Test-Path $exePath)) {
    Write-Error "Build output missing. Expected:`n  $msiPath`n  $exePath"
}

# 4. Get GitHub Credentials
$credInput = "protocol=https`nhost=github.com`n`n"
$credOutput = $credInput | git credential fill
$token = ($credOutput | Select-String "^password=(.+)$").Matches.Groups[1].Value

if (-not $token) {
    Write-Error "GitHub personal access token not found in git credentials. Please sign in with git credential-manager."
}

$headers = @{
    "Authorization" = "Bearer $token"
    "Accept"        = "application/vnd.github+json"
    "User-Agent"    = "Raydrop-Release-Automation"
}

$repo = "asepindrak/raydrop"

# 5. Git Commit & Push Tag
Write-Host "Ensuring git tag $tag is pushed..." -ForegroundColor Yellow
git tag -f $tag
git push origin main
git push origin $tag --force

# 6. Generate Release Notes
$bodyText = @"
## Raydrop $tag Release

<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

### Download (Windows 64-bit / x64)
- **Windows Setup Installer (`.msi` — Recommended):** \`Raydrop_${ver}_x64_en-US.msi\`
- **Windows Portable App (`.exe` — Standalone / No Install):** \`Raydrop_${ver}_x64-portable.exe\`

### Features & Highlights
- **Zero-Server High-Speed File Transfer:** True peer-to-peer over local LAN and global WAN.
- **Dynamic 6-Stream Parallel Workers:** IDM-style chunk acceleration for massive files (up to 125 MB/s).
- **Auto-Resumable Chunk Engine:** Reconnect seamlessly after Wi-Fi drops without restarting from scratch.
- **End-to-End Cryptographic Security:** BLAKE3 checksums, X25519 ECDH key exchange, and ChaCha20-Poly1305.
- **Windows 11 Native Context Menu:** Direct 1-click right-click integration on Windows Explorer.
- **In-App Auto-Updater:** One-click seamless upgrade notification for all peers.
- **Live Download Landing Page:** Automatically synced at [raydrop.web.id](https://raydrop.web.id).
"@

# 7. Create or Get GitHub Release
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/tags/$tag" -Headers $headers -Method Get
    Write-Host "Found existing release: $($release.html_url)" -ForegroundColor Green
} catch {
    $bodyObj = @{
        tag_name   = $tag
        name       = "Raydrop $tag"
        body       = $bodyText
        draft      = $false
        prerelease = $false
    }
    $jsonBody = $bodyObj | ConvertTo-Json -Depth 5
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases" -Headers $headers -Method Post -Body ([System.Text.Encoding]::UTF8.GetBytes($jsonBody)) -ContentType "application/json; charset=utf-8"
    Write-Host "Created new GitHub release: $($release.html_url)" -ForegroundColor Green
}

# Remove existing assets for idempotency
foreach ($asset in $release.assets) {
    Write-Host "Removing existing asset: $($asset.name)" -ForegroundColor Gray
    Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/assets/$($asset.id)" -Headers $headers -Method Delete
}

# 8. Upload Assets
$uploadUrlBase = $release.upload_url -replace '\{\?name,label\}$', ''
$uploads = @(
    @{ Path = $msiPath; Name = "Raydrop_${ver}_x64_en-US.msi" },
    @{ Path = $exePath; Name = "Raydrop_${ver}_x64-portable.exe" }
)

foreach ($u in $uploads) {
    Write-Host "Uploading $($u.Name)..." -ForegroundColor Cyan
    $uploadUri = "${uploadUrlBase}?name=$([uri]::EscapeDataString($u.Name))"
    $uploaded = Invoke-RestMethod -Uri $uploadUri -Headers $headers -Method Post -InFile $u.Path -ContentType "application/octet-stream"
    Write-Host "Uploaded: $($uploaded.browser_download_url)" -ForegroundColor Green
}

Write-Host ""
Write-Host "==========================================================" -ForegroundColor Green
Write-Host " Release $tag is LIVE!" -ForegroundColor Green
Write-Host " GitHub: $($release.html_url)" -ForegroundColor Green
Write-Host " Landing: https://raydrop.web.id (Auto-synced in real-time)" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Green

