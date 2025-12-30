# Build script for NeuralFS Watchdog Sidecar
# This script compiles the watchdog binary and places it in the correct location
# for Tauri's sidecar mechanism.
#
# Tauri requires sidecar binaries to follow strict naming conventions:
# - Windows: watchdog-x86_64-pc-windows-msvc.exe
# - macOS x64: watchdog-x86_64-apple-darwin
# - macOS ARM: watchdog-aarch64-apple-darwin
# - Linux: watchdog-x86_64-unknown-linux-gnu

param(
    [switch]$Release = $false,
    [string]$Target = ""
)

$ErrorActionPreference = "Stop"

# Determine build profile
$Profile = if ($Release) { "release" } else { "debug" }
$ProfileFlag = if ($Release) { "--release" } else { "" }

# Determine target triple
if ([string]::IsNullOrEmpty($Target)) {
    $Target = rustc -vV | Select-String "host:" | ForEach-Object { $_.Line.Split(":")[1].Trim() }
}

Write-Host "Building watchdog sidecar..." -ForegroundColor Cyan
Write-Host "  Profile: $Profile" -ForegroundColor Gray
Write-Host "  Target: $Target" -ForegroundColor Gray

# Navigate to src-tauri directory
Push-Location src-tauri

try {
    # Build the watchdog binary
    if ($ProfileFlag) {
        cargo build $ProfileFlag --bin watchdog --target $Target
    } else {
        cargo build --bin watchdog --target $Target
    }

    if ($LASTEXITCODE -ne 0) {
        throw "Cargo build failed with exit code $LASTEXITCODE"
    }

    # Determine source and destination paths
    $Extension = if ($Target -like "*windows*") { ".exe" } else { "" }
    $SourcePath = "target/$Target/$Profile/watchdog$Extension"
    $DestDir = "binaries"
    $DestPath = "$DestDir/watchdog-$Target$Extension"

    # Create binaries directory if it doesn't exist
    if (-not (Test-Path $DestDir)) {
        New-Item -ItemType Directory -Path $DestDir -Force | Out-Null
        Write-Host "  Created directory: $DestDir" -ForegroundColor Gray
    }

    # Copy the binary
    if (Test-Path $SourcePath) {
        Copy-Item -Path $SourcePath -Destination $DestPath -Force
        Write-Host "  Copied: $SourcePath -> $DestPath" -ForegroundColor Green
    } else {
        throw "Built binary not found at: $SourcePath"
    }

    Write-Host "Watchdog sidecar build complete!" -ForegroundColor Green

} finally {
    Pop-Location
}
