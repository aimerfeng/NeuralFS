//! Highlight Navigator Module
//!
//! Provides functionality for:
//! - Opening files in appropriate applications
//! - Navigating to specific content locations (line, page, region)
//! - Launching files with system default or custom applications
//! - "Open with" menu support
//!
//! Requirements: 7.2, 7.3, 14.1, 14.3

mod navigator;
mod launcher;
#[cfg(test)]
mod tests;

pub use navigator::{
    HighlightNavigator, NavigatorConfig, NavigationTarget, NavigationResult,
    NavigationError, FileOpenMode,
};
pub use launcher::{
    AppLauncher, AppInfo, LaunchResult, LaunchError,
    InstalledApp, FileTypeAssociation,
};
