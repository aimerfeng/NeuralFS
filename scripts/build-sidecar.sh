#!/bin/bash
# Build script for NeuralFS Watchdog Sidecar
# This script compiles the watchdog binary and places it in the correct location
# for Tauri's sidecar mechanism.
#
# Tauri requires sidecar binaries to follow strict naming conventions:
# - Windows: watchdog-x86_64-pc-windows-msvc.exe
# - macOS x64: watchdog-x86_64-apple-darwin
# - macOS ARM: watchdog-aarch64-apple-darwin
# - Linux: watchdog-x86_64-unknown-linux-gnu

set -e

# Parse arguments
RELEASE=false
TARGET=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            RELEASE=true
            shift
            ;;
        --target)
            TARGET="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Determine build profile
if [ "$RELEASE" = true ]; then
    PROFILE="release"
    PROFILE_FLAG="--release"
else
    PROFILE="debug"
    PROFILE_FLAG=""
fi

# Determine target triple
if [ -z "$TARGET" ]; then
    TARGET=$(rustc -vV | grep "host:" | cut -d' ' -f2)
fi

echo "Building watchdog sidecar..."
echo "  Profile: $PROFILE"
echo "  Target: $TARGET"

# Navigate to src-tauri directory
cd src-tauri

# Build the watchdog binary
if [ -n "$PROFILE_FLAG" ]; then
    cargo build $PROFILE_FLAG --bin watchdog --target "$TARGET"
else
    cargo build --bin watchdog --target "$TARGET"
fi

# Determine source and destination paths
if [[ "$TARGET" == *"windows"* ]]; then
    EXTENSION=".exe"
else
    EXTENSION=""
fi

SOURCE_PATH="target/$TARGET/$PROFILE/watchdog$EXTENSION"
DEST_DIR="binaries"
DEST_PATH="$DEST_DIR/watchdog-$TARGET$EXTENSION"

# Create binaries directory if it doesn't exist
mkdir -p "$DEST_DIR"

# Copy the binary
if [ -f "$SOURCE_PATH" ]; then
    cp "$SOURCE_PATH" "$DEST_PATH"
    echo "  Copied: $SOURCE_PATH -> $DEST_PATH"
else
    echo "Error: Built binary not found at: $SOURCE_PATH"
    exit 1
fi

echo "Watchdog sidecar build complete!"
