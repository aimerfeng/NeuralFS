# External Binaries

This directory contains external binaries bundled with NeuralFS.

## Watchdog

The watchdog process monitors the main NeuralFS application and:
- Restarts it if it crashes
- Restores Windows Explorer if needed
- Provides heartbeat monitoring

### Building Watchdog

The watchdog binary will be built as part of the project:

```bash
cargo build --bin watchdog --release
```

### Platform-specific naming

Tauri expects binaries with platform-specific suffixes:
- Windows: `watchdog-x86_64-pc-windows-msvc.exe`
- Linux: `watchdog-x86_64-unknown-linux-gnu`
- macOS: `watchdog-x86_64-apple-darwin` or `watchdog-aarch64-apple-darwin`
