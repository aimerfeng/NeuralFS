//! NeuralFS - AI-driven immersive file system shell
//! 
//! Main entry point for the Tauri application.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use neural_fs::core::{RuntimeDependencies, RuntimeStatus};

/// Application state
pub struct AppState {
    pub runtime_status: RuntimeStatus,
}

/// Get runtime status command
#[tauri::command]
fn get_runtime_status(state: tauri::State<AppState>) -> RuntimeStatus {
    state.runtime_status.clone()
}

/// Health check command
#[tauri::command]
fn health_check() -> String {
    "NeuralFS is running".to_string()
}

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting NeuralFS...");

    // Check runtime dependencies
    let runtime_status = RuntimeDependencies::check_all();
    tracing::info!(
        "Runtime status: GPU={}, Provider={}",
        runtime_status.has_gpu_acceleration(),
        runtime_status.recommended_provider()
    );

    // Create app state
    let app_state = AppState { runtime_status };

    // Build and run Tauri application
    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![get_runtime_status, health_check])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
