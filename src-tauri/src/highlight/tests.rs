//! Tests for the Highlight Navigator module

use super::*;
use std::io::Write;
use tempfile::NamedTempFile;
use uuid::Uuid;

#[tokio::test]
async fn test_navigation_target_builder() {
    let target = NavigationTarget::new("/path/to/file.txt")
        .with_file_id(Uuid::new_v4())
        .with_query("search term");

    assert_eq!(target.path.to_str().unwrap(), "/path/to/file.txt");
    assert!(target.file_id.is_some());
    assert_eq!(target.query, Some("search term".to_string()));
    assert!(target.location.is_none());
}

#[tokio::test]
async fn test_navigation_target_with_location() {
    use crate::core::types::chunk::ChunkLocation;

    let location = ChunkLocation {
        start_offset: 100,
        end_offset: 200,
        start_line: Some(10),
        end_line: Some(15),
        page_number: None,
        bounding_box: None,
    };

    let target = NavigationTarget::new("/path/to/file.txt")
        .with_location(location.clone());

    assert!(target.location.is_some());
    let loc = target.location.unwrap();
    assert_eq!(loc.start_line, Some(10));
    assert_eq!(loc.end_line, Some(15));
}

#[tokio::test]
async fn test_navigator_config_default() {
    let config = NavigatorConfig::default();
    
    assert!(config.use_system_default);
    assert!(config.app_overrides.is_empty());
    assert!(config.track_sessions);
    assert_eq!(config.launch_timeout_ms, 5000);
}

#[tokio::test]
async fn test_navigator_creation() {
    let navigator = HighlightNavigator::new();
    assert!(navigator.config().use_system_default);
}

#[tokio::test]
async fn test_navigator_with_custom_config() {
    let mut config = NavigatorConfig::default();
    config.use_system_default = false;
    config.launch_timeout_ms = 10000;

    let navigator = HighlightNavigator::with_config(config);
    
    assert!(!navigator.config().use_system_default);
    assert_eq!(navigator.config().launch_timeout_ms, 10000);
}

#[tokio::test]
async fn test_navigator_app_override() {
    let mut navigator = HighlightNavigator::new();
    
    navigator.add_app_override("txt", "/usr/bin/vim");
    
    assert!(navigator.config().app_overrides.contains_key("txt"));
    
    let removed = navigator.remove_app_override("txt");
    assert!(removed.is_some());
    assert!(!navigator.config().app_overrides.contains_key("txt"));
}

#[tokio::test]
async fn test_navigate_file_not_found() {
    let navigator = HighlightNavigator::new();
    let target = NavigationTarget::new("/nonexistent/file.txt");

    let result = navigator.navigate(&target).await;
    
    assert!(result.is_err());
    match result {
        Err(NavigationError::FileNotFound { path }) => {
            assert!(path.contains("nonexistent"));
        }
        _ => panic!("Expected FileNotFound error"),
    }
}

#[tokio::test]
async fn test_navigate_existing_file() {
    // Create a temporary file
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "Test content").unwrap();

    let navigator = HighlightNavigator::new();
    let target = NavigationTarget::new(temp_file.path());

    // Note: This test may actually open the file on the system
    // In CI environments, this might fail if no default app is configured
    // We're mainly testing that the code path works without panicking
    let result = navigator.navigate(&target).await;
    
    // The result depends on the system configuration
    // On systems with proper file associations, this should succeed
    // On minimal CI systems, it might fail
    match result {
        Ok(nav_result) => {
            assert!(nav_result.success);
        }
        Err(NavigationError::OpenFailed { .. }) => {
            // This is acceptable in CI environments
        }
        Err(e) => {
            // Other errors are unexpected
            panic!("Unexpected error: {:?}", e);
        }
    }
}

// AppLauncher tests

#[tokio::test]
async fn test_app_launcher_creation() {
    let launcher = AppLauncher::new();
    assert!(!launcher.cache_populated);
}

#[tokio::test]
async fn test_open_with_default_file_not_found() {
    let launcher = AppLauncher::new();
    let result = launcher.open_with_default(std::path::Path::new("/nonexistent/file.txt")).await;
    
    assert!(result.is_err());
    match result {
        Err(LaunchError::FileNotFound { path }) => {
            assert!(path.contains("nonexistent"));
        }
        _ => panic!("Expected FileNotFound error"),
    }
}

#[tokio::test]
async fn test_get_apps_for_extension() {
    let mut launcher = AppLauncher::new();
    
    let result = launcher.get_apps_for_extension("txt").await;
    
    assert!(result.is_ok());
    let association = result.unwrap();
    assert_eq!(association.extension, "txt");
    assert_eq!(association.mime_type, Some("text/plain".to_string()));
}

#[tokio::test]
async fn test_get_apps_for_pdf() {
    let mut launcher = AppLauncher::new();
    
    let result = launcher.get_apps_for_extension("pdf").await;
    
    assert!(result.is_ok());
    let association = result.unwrap();
    assert_eq!(association.extension, "pdf");
    assert_eq!(association.mime_type, Some("application/pdf".to_string()));
}

#[tokio::test]
async fn test_get_apps_for_image() {
    let mut launcher = AppLauncher::new();
    
    let result = launcher.get_apps_for_extension("png").await;
    
    assert!(result.is_ok());
    let association = result.unwrap();
    assert_eq!(association.extension, "png");
    assert_eq!(association.mime_type, Some("image/png".to_string()));
}

#[tokio::test]
async fn test_get_apps_for_unknown_extension() {
    let mut launcher = AppLauncher::new();
    
    let result = launcher.get_apps_for_extension("xyz123").await;
    
    assert!(result.is_ok());
    let association = result.unwrap();
    assert_eq!(association.extension, "xyz123");
    assert!(association.mime_type.is_none());
}

#[tokio::test]
async fn test_open_with_app_not_found() {
    let launcher = AppLauncher::new();
    
    // Create a temp file
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "Test content").unwrap();
    
    let app_info = AppInfo {
        name: "NonexistentApp".to_string(),
        path: std::path::PathBuf::from("/nonexistent/app"),
        is_default: false,
    };
    
    let result = launcher.open_with_app(temp_file.path(), &app_info).await;
    
    assert!(result.is_err());
    match result {
        Err(LaunchError::AppNotFound { app }) => {
            assert_eq!(app, "NonexistentApp");
        }
        _ => panic!("Expected AppNotFound error"),
    }
}

#[tokio::test]
async fn test_file_type_association_structure() {
    let association = FileTypeAssociation {
        extension: "rs".to_string(),
        mime_type: Some("text/x-rust".to_string()),
        default_app: Some(AppInfo {
            name: "VS Code".to_string(),
            path: std::path::PathBuf::from("/usr/bin/code"),
            is_default: true,
        }),
        available_apps: vec![
            AppInfo {
                name: "VS Code".to_string(),
                path: std::path::PathBuf::from("/usr/bin/code"),
                is_default: true,
            },
            AppInfo {
                name: "Vim".to_string(),
                path: std::path::PathBuf::from("/usr/bin/vim"),
                is_default: false,
            },
        ],
    };

    assert_eq!(association.extension, "rs");
    assert!(association.default_app.is_some());
    assert_eq!(association.available_apps.len(), 2);
}

#[tokio::test]
async fn test_launch_result_structure() {
    let result = LaunchResult {
        success: true,
        app_name: "Test App".to_string(),
        process_id: Some(12345),
        message: Some("Launched successfully".to_string()),
    };

    assert!(result.success);
    assert_eq!(result.app_name, "Test App");
    assert_eq!(result.process_id, Some(12345));
    assert!(result.message.is_some());
}

#[tokio::test]
async fn test_installed_app_structure() {
    let app = InstalledApp {
        name: "Test Editor".to_string(),
        path: std::path::PathBuf::from("/usr/bin/test-editor"),
        icon_path: Some(std::path::PathBuf::from("/usr/share/icons/test.png")),
        supported_extensions: vec!["txt".to_string(), "md".to_string()],
        is_default: true,
    };

    assert_eq!(app.name, "Test Editor");
    assert!(app.icon_path.is_some());
    assert_eq!(app.supported_extensions.len(), 2);
    assert!(app.is_default);
}
