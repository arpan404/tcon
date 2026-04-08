param(
  [string]$Prefix = "$HOME\AppData\Local\tcon"
)

$ErrorActionPreference = "Stop"

Write-Host "Building tcon (release)..."
cargo build --release

$binDir = Join-Path $Prefix "bin"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null
Copy-Item "target/release/tcon.exe" (Join-Path $binDir "tcon.exe") -Force

Write-Host "Installed:" (Join-Path $binDir "tcon.exe")
Write-Host "Add '$binDir' to PATH if needed."
