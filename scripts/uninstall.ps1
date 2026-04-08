param(
  [string]$Prefix = "$HOME\AppData\Local\tcon"
)

$ErrorActionPreference = "Stop"

$bin = Join-Path (Join-Path $Prefix "bin") "tcon.exe"
if (Test-Path $bin) {
  Remove-Item $bin -Force
  Write-Host "Removed: $bin"
} else {
  Write-Host "Not installed at: $bin"
}
