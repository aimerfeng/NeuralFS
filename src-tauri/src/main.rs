//! NeuralFS - AI-driven immersive file system shell
//! 
//! Main entry point for the Tauri application.

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use neural_fs::core::{RuntimeDependencies, RuntimeStatus};
use neural_fs::logging::{LoggingSystem, LoggingConfig, LogLevel, LogOutput};
use neural_fs::commands::{
    // Search commands
    search_files, get_search_suggestions,
    // Tag commands
    get_tags, get_file_tags, add_tag, remove_tag, confirm_tag, reject_tag, create_tag,
    // Relation commands
    get_relations, confirm_relation, reject_relation, block_relation, create_relation,
    get_block_rules, remove_block_rule,
    // Config commands
    get_config, set_config, get_cloud_status, set_cloud_enabled,
    add_monitored_directory, remove_monitored_directory,
    set_theme, set_language, export_config, import_config, reset_config,
    list_config_backups, restore_config_backup, init_config, ConfigState,
    // Status commands
    get_index_status, get_system_status, get_dead_letter_tasks, get_dead_letter_stats,
    retry_dead_letter, retry_all_dead_letter, clear_dead_letter,
    pause_indexing, resume_indexing,
    // Protocol commands
    get_session_token_cmd, build_thumbnail_url_cmd, build_preview_url_cmd,
    build_file_url_cmd, get_asset_server_port, is_protocol_ready,
    // Onboarding commands
    check_first_launch, get_suggested_directories, browse_directory,
    save_onboarding_config, start_initial_scan, get_scan_progress, complete_onboarding,
};
use neural_fs::protocol::{
    register_custom_protocol, ProtocolState,
};
use neural_fs::asset::AssetServerConfig;

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
    // Initialize logging system with configuration
    // Requirements 24.1: Structured logs with configurable verbosity levels
    // Requirements 24.2: Log rotation to prevent disk space exhaustion
    // Requirements 24.7: Performance metrics for bottleneck identification
    let logging_config = if cfg!(debug_assertions) {
        LoggingConfig::development()
    } else {
        LoggingConfig::production()
    };

    // Initialize the logging system
    // Note: We keep the _logging_system alive for the duration of the application
    // to ensure log file handles are properly managed
    let _logging_system = match LoggingSystem::init(logging_config) {
        Ok(system) => {
            tracing::info!("Logging system initialized successfully");
            Some(system)
        }
        Err(e) => {
            // Fall back to basic logging if advanced logging fails
            eprintln!("Failed to initialize logging system: {}. Using basic logging.", e);
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive(tracing::Level::INFO.into()),
                )
                .init();
            None
        }
    };

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

    // Create config state
    let config_state = ConfigState::new();

    // Create protocol state with default configuration
    // This generates the session token that will be used for asset requests
    let asset_config = AssetServerConfig::default();
    let protocol_state = ProtocolState::new(
        neural_fs::asset::AssetServerState::new(asset_config)
    );

    tracing::info!(
        "Protocol state initialized with session token: {}...",
        &protocol_state.get_session_token()[..8]
    );

    // Build and run Tauri application with custom protocol
    let builder = tauri::Builder::default()
        .manage(app_state)
        .manage(config_state)
        .manage(protocol_state.clone());

    // Register the nfs:// custom protocol
    let builder = register_custom_protocol(builder, protocol_state);

    builder
        .invoke_handler(tauri::generate_handler![
            // Core commands
            get_runtime_status,
            health_check,
            // Protocol commands (for frontend handshake - Requirements: Asset Server Security)
            get_session_token_cmd,
            build_thumbnail_url_cmd,
            build_preview_url_cmd,
            build_file_url_cmd,
            get_asset_server_port,
            is_protocol_ready,
            // Search commands (Requirements 2.1, 2.2)
            search_files,
            get_search_suggestions,
            // Tag commands (Requirements 5.1, Human-in-the-Loop)
            get_tags,
            get_file_tags,
            add_tag,
            remove_tag,
            confirm_tag,
            reject_tag,
            create_tag,
            // Relation commands (Requirements 6.1, Human-in-the-Loop)
            get_relations,
            confirm_relation,
            reject_relation,
            block_relation,
            create_relation,
            get_block_rules,
            remove_block_rule,
            // Config commands (Requirements 15.1, 15.2, 15.7)
            init_config,
            get_config,
            set_config,
            get_cloud_status,
            set_cloud_enabled,
            add_monitored_directory,
            remove_monitored_directory,
            set_theme,
            set_language,
            export_config,
            import_config,
            reset_config,
            list_config_backups,
            restore_config_backup,
            // Status commands (Requirements 16.1, Indexer Resilience)
            get_index_status,
            get_system_status,
            get_dead_letter_tasks,
            get_dead_letter_stats,
            retry_dead_letter,
            retry_all_dead_letter,
            clear_dead_letter,
            pause_indexing,
            resume_indexing,
            // Onboarding commands (Requirements 17.1, 17.2, 17.3, 17.4, 17.5)
            check_first_launch,
            get_suggested_directories,
            browse_directory,
            save_onboarding_config,
            start_initial_scan,
            get_scan_progress,
            complete_onboarding,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
