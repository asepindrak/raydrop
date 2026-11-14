# Update GitHub Release notes with rich formatting, logo, and release details (UTF-8 safe)
param(
    [string]$Tag = "v1.2.6"
)
$ErrorActionPreference = "Stop"

$repoRoot = (Get-Item $PSScriptRoot).Parent.FullName
Set-Location $repoRoot

python scripts/update_release_notes.py $Tag
