//! NeuralFS Watchdog Process
//!
//! Independent supervisor process that monitors the main NeuralFS application
//! via shared memory heartbeat and can restart it if it becomes unresponsive.
//!
//! Usage:
//!   watchdog.exe --executable <path> [--args <args>...]
//!
//! The watchdog will:
//! 1. Open shared memory created by the main process
//! 2. Monitor heartbeat timestamps
//! 3. Restart the main process if heartbeat times out
//! 4. Restore Windows Explorer if the main process crashes

use std::path::PathBuf;
use std::time::Duration;

use neural_fs::watchdog::{Watchdog, WatchdogConfig, WatchdogError, WatchdogState};

/// Command line arguments
struct Args {
    /// Path to the main executable
    executable: PathBuf,
    /// Arguments to pass to the main executable
    args: Vec<String>,
    /// Heartbeat timeout in milliseconds
    timeout_ms: u64,
    /// Maximum restart attempts
    max_restarts: u32,
    /// Enable verbose logging
    verbose: bool,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = std::env::args().skip(1);
        let mut executable = None;
        let mut exec_args = Vec::new();
        let mut timeout_ms = 3000u64;
        let mut max_restarts = 3u32;
        let mut verbose = false;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--executable" | "-e" => {
                    executable = args.next().map(PathBuf::from);
                }
                "--args" | "-a" => {
                    // Collect remaining arguments for the main process
                    exec_args.extend(args.by_ref());
                }
                "--timeout" | "-t" => {
                    if let Some(val) = args.next() {
                        timeout_ms = val.parse().map_err(|_| "Invalid timeout value")?;
                    }
                }
                "--max-restarts" | "-m" => {
                    if let Some(val) = args.next() {
                        max_restarts = val.parse().map_err(|_| "Invalid max-restarts value")?;
                    }
                }
                "--verbose" | "-v" => {
                    verbose = true;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {
                    return Err(format!("Unknown argument: {}", arg));
                }
            }
        }

        let executable = executable.ok_or("--executable is required")?;

        Ok(Self {
            executable,
            args: exec_args,
            timeout_ms,
            max_restarts,
            verbose,
        })
    }
}

fn print_help() {
    println!(
        r#"NeuralFS Watchdog - Process Supervisor

USAGE:
    watchdog [OPTIONS] --executable <PATH>

OPTIONS:
    -e, --executable <PATH>    Path to the main NeuralFS executable (required)
    -a, --args <ARGS>...       Arguments to pass to the main executable
    -t, --timeout <MS>         Heartbeat timeout in milliseconds (default: 3000)
    -m, --max-restarts <N>     Maximum restart attempts (default: 3)
    -v, --verbose              Enable verbose logging
    -h, --help                 Print this help message

DESCRIPTION:
    The watchdog monitors the main NeuralFS process via shared memory heartbeat.
    If the main process becomes unresponsive (no heartbeat within timeout),
    the watchdog will:
    1. Restore Windows Explorer (if applicable)
    2. Terminate the unresponsive process
    3. Restart the main process
    4. Notify the user about the restart

    The watchdog will give up after max-restarts consecutive failures.
"#
    );
}

fn init_logging(verbose: bool) {
    let level = if verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();
}

fn run_watchdog(args: Args) -> Result<(), WatchdogError> {
    let config = WatchdogConfig {
        main_executable: args.executable.clone(),
        main_args: args.args,
        timeout_ms: args.timeout_ms,
        max_restart_attempts: args.max_restarts,
        restore_explorer_on_crash: true,
        ..Default::default()
    };

    tracing::info!("Starting NeuralFS Watchdog");
    tracing::info!("Monitoring: {:?}", args.executable);
    tracing::info!("Timeout: {}ms", args.timeout_ms);
    tracing::info!("Max restarts: {}", args.max_restarts);

    let mut watchdog = Watchdog::new(config);
    
    // Start monitoring
    watchdog.start()?;
    tracing::info!("Watchdog started, waiting for main process heartbeat...");

    // Main monitoring loop
    loop {
        match watchdog.tick() {
            Ok(true) => {
                // Continue monitoring
                std::thread::sleep(watchdog.check_interval());
            }
            Ok(false) => {
                // Watchdog stopped
                tracing::info!("Watchdog stopped");
                break;
            }
            Err(WatchdogError::MaxRestartsExceeded) => {
                tracing::error!("Max restart attempts exceeded, watchdog giving up");
                return Err(WatchdogError::MaxRestartsExceeded);
            }
            Err(e) => {
                tracing::error!("Watchdog error: {}", e);
                // Continue monitoring despite errors
                std::thread::sleep(Duration::from_secs(1));
            }
        }

        // Log state periodically
        if watchdog.state() == WatchdogState::Monitoring {
            tracing::debug!("Heartbeat OK");
        }
    }

    Ok(())
}

fn main() {
    let args = match Args::parse() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("Use --help for usage information");
            std::process::exit(1);
        }
    };

    init_logging(args.verbose);

    if let Err(e) = run_watchdog(args) {
        tracing::error!("Watchdog failed: {}", e);
        std::process::exit(1);
    }
}
