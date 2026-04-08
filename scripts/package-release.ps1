param(
  [Parameter(Mandatory = $true)]
  [string]$Version,
  [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"

New-Item -ItemType Directory -Force -Path "dist" | Out-Null

Write-Host "Building target: $Target"
rustup target add $Target | Out-Null
cargo build --release --target $Target

$bin = "target/$Target/release/tcon.exe"
if (!(Test-Path $bin)) {
  throw "Missing binary: $bin"
}

$zipPath = "dist/tcon-$Version-$Target.zip"
if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
Compress-Archive -Path $bin -DestinationPath $zipPath

$hash = Get-FileHash $zipPath -Algorithm SHA256
"$($hash.Hash.ToLower())  $(Split-Path $zipPath -Leaf)" | Out-File -Append -Encoding ascii "dist/checksums-$Version.txt"

Write-Host "Artifacts written to dist/"
