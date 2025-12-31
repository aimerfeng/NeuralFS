//! Protocol-related Tauri commands
//!
//! This module provides commands for the frontend to interact with the
//! custom protocol system, including session token retrieval and URL building.

use uuid::Uuid;
use crate::protocol::{ProtocolState, SessionTokenResponse, build_thumbnail_url, build_preview_url, build_file_url};

/// Get the session token for authenticating asset requests.
///
/// This command MUST be called by the frontend during app initialization,
/// BEFORE any asset loading occurs. Otherwise, all asset requests will
/// return 403 Forbidden.
///
/// # Returns
///
/// A `SessionTokenResponse` containing:
/// - `token`: The session token string
/// - `protocol_url`: The base URL for the nfs:// protocol ("nfs://")
/// - `http_url`: The base URL for the HTTP asset server (fallback)
///
/// # Example (Frontend)
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/tauri';
///
/// const response = await invoke<SessionTokenResponse>('get_session_token');
/// console.log('Token:', response.token);
/// console.log('Protocol URL:', response.protocol_url);
/// console.log('HTTP URL:', response.http_url);
/// ```
#[tauri::command]
pub fn get_session_token_cmd(state: tauri::State<ProtocolState>) -> SessionTokenResponse {
    SessionTokenResponse {
        token: state.get_session_token().to_string(),
        protocol_url: "nfs://".to_string(),
        http_url: format!("http://127.0.0.1:{}", state.asset_state.config.port),
    }
}

/// Build a thumbnail URL for a given file UUID.
///
/// # Arguments
///
/// * `uuid` - The file UUID
/// * `use_protocol` - Whether to use the nfs:// protocol (true) or HTTP fallback (false)
///
/// # Returns
///
/// The complete URL with session token included.
#[tauri::command]
pub fn build_thumbnail_url_cmd(
    state: tauri::State<ProtocolState>,
    uuid: String,
    use_protocol: Option<bool>,
) -> Result<String, String> {
    let uuid = Uuid::parse_str(&uuid).map_err(|e| format!("Invalid UUID: {}", e))?;
    let use_protocol = use_protocol.unwrap_or(true);
    Ok(build_thumbnail_url(&state, uuid, use_protocol))
}

/// Build a preview URL for a given file UUID.
///
/// # Arguments
///
/// * `uuid` - The file UUID
/// * `use_protocol` - Whether to use the nfs:// protocol (true) or HTTP fallback (false)
///
/// # Returns
///
/// The complete URL with session token included.
#[tauri::command]
pub fn build_preview_url_cmd(
    state: tauri::State<ProtocolState>,
    uuid: String,
    use_protocol: Option<bool>,
) -> Result<String, String> {
    let uuid = Uuid::parse_str(&uuid).map_err(|e| format!("Invalid UUID: {}", e))?;
    let use_protocol = use_protocol.unwrap_or(true);
    Ok(build_preview_url(&state, uuid, use_protocol))
}

/// Build a file URL for a given file UUID.
///
/// # Arguments
///
/// * `uuid` - The file UUID
/// * `use_protocol` - Whether to use the nfs:// protocol (true) or HTTP fallback (false)
///
/// # Returns
///
/// The complete URL with session token included.
#[tauri::command]
pub fn build_file_url_cmd(
    state: tauri::State<ProtocolState>,
    uuid: String,
    use_protocol: Option<bool>,
) -> Result<String, String> {
    let uuid = Uuid::parse_str(&uuid).map_err(|e| format!("Invalid UUID: {}", e))?;
    let use_protocol = use_protocol.unwrap_or(true);
    Ok(build_file_url(&state, uuid, use_protocol))
}

/// Get the asset server port.
///
/// # Returns
///
/// The port number the asset server is listening on.
#[tauri::command]
pub fn get_asset_server_port(state: tauri::State<ProtocolState>) -> u16 {
    state.asset_state.config.port
}

/// Check if the protocol state is initialized.
///
/// # Returns
///
/// Always returns true if the command can be called (state is managed).
#[tauri::command]
pub fn is_protocol_ready(_state: tauri::State<ProtocolState>) -> bool {
    true
}
