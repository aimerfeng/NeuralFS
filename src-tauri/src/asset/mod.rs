//! Secure Asset Streaming Server
//!
//! This module provides a secure HTTP server for streaming assets (thumbnails, previews, files)
//! to the frontend without IPC serialization overhead.
//!
//! Security features:
//! - Session token validation (generated at startup)
//! - CSRF protection via Origin/Referer checking
//! - Security response headers (X-Content-Type-Options, X-Frame-Options, etc.)
//! - Localhost-only binding

mod error;
mod server;
mod routes;
#[cfg(test)]
mod tests;

pub use error::AssetError;
pub use server::{
    SecureAssetStreamServer, AssetServerConfig, AssetServerState,
    CachedThumbnail, CachedPreview,
};
pub use routes::{TokenParams, serve_thumbnail, serve_preview, serve_file};
