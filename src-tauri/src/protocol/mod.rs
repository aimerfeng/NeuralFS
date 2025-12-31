//! Custom Protocol Registration for NeuralFS
//!
//! This module provides the `nfs://` custom protocol for serving assets
//! (thumbnails, previews, files) directly to the frontend without HTTP overhead.
//!
//! The protocol integrates with the existing SecureAssetStreamServer for:
//! - Session token validation
//! - Thumbnail serving: `nfs://thumbnail/{uuid}`
//! - Preview serving: `nfs://preview/{uuid}`
//! - File serving: `nfs://file/{uuid}`

mod handler;
#[cfg(test)]
mod tests;

pub use handler::{
    register_custom_protocol, ProtocolState, ProtocolConfig,
    get_session_token, AssetProtocolHandler,
};
