//! Highlight Navigator Implementation
//!
//! Handles file opening and navigation to specific content locations.
//! Supports text files (line navigation), PDFs (page navigation), and images (region indication).

use crate::core::types::chunk::ChunkLocation;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use uuid::Uuid;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Errors that can occur during navigation
#[derive(Error, Debug)]
pub enum NavigationError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Failed to open file: {reason}")]
    OpenFailed { reason: String },

    #[error("Unsupported file type for navigation: {extension}")]
    UnsupportedFileType { extension: String },

    #[error("Application not found: {app}")]
    AppNotFound { app: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Navigation to location failed: {reason}")]
    LocationNavigationFailed { reason: String },
}

/// Configuration for the highlight navigator
#[derive(Debug, Clone)]
pub struct NavigatorConfig {
    /// Whether to use system default applications
    pub use_system_default: bool,
    /// Custom application overrides by extension
    pub app_overrides: std::collections::HashMap<String, PathBuf>,
    /// Whether to track opened files for session logging
    pub track_sessions: bool,
    /// Timeout for application launch (milliseconds)
    pub launch_timeout_ms: u64,
}

impl Default for NavigatorConfig {
    fn default() -> Self {
        Self {
            use_system_default: true,
            app_overrides: std::collections::HashMap::new(),
            track_sessions: true,
            launch_timeout_ms: 5000,
        }
    }
}

/// Target for navigation - specifies where to navigate within a file
#[derive(Debug, Clone)]
pub struct NavigationTarget {
    /// File path to open
    pub path: PathBuf,
    /// File UUID (for tracking)
    pub file_id: Option<Uuid>,
    /// Location within the file to navigate to
    pub location: Option<ChunkLocation>,
    /// Search query that led to this navigation (for highlighting)
    pub query: Option<String>,
}

impl NavigationTarget {
    /// Create a new navigation target for a file
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            file_id: None,
            location: None,
            query: None,
        }
    }

    /// Set the file ID
    pub fn with_file_id(mut self, file_id: Uuid) -> Self {
        self.file_id = Some(file_id);
        self
    }

    /// Set the location to navigate to
    pub fn with_location(mut self, location: ChunkLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the search query for highlighting
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }
}

/// Result of a navigation operation
#[derive(Debug, Clone)]
pub struct NavigationResult {
    /// Whether the file was successfully opened
    pub success: bool,
    /// The application used to open the file
    pub app_used: Option<String>,
    /// Whether location navigation was attempted
    pub location_navigated: bool,
    /// Any warnings or notes
    pub message: Option<String>,
}

/// Mode for opening files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOpenMode {
    /// Open with system default application
    SystemDefault,
    /// Open with a specific application
    SpecificApp,
    /// Open in NeuralFS internal viewer (if supported)
    InternalViewer,
}

/// Highlight Navigator - handles file opening and location navigation
pub struct HighlightNavigator {
    config: NavigatorConfig,
}

impl HighlightNavigator {
    /// Create a new highlight navigator with default config
    pub fn new() -> Self {
        Self::with_config(NavigatorConfig::default())
    }

    /// Create a new highlight navigator with custom config
    pub fn with_config(config: NavigatorConfig) -> Self {
        Self { config }
    }

    /// Navigate to a file, optionally at a specific location
    ///
    /// This is the main entry point for opening files and navigating to content.
    /// It handles:
    /// - Opening the file with the appropriate application
    /// - Navigating to specific lines (text files)
    /// - Navigating to specific pages (PDFs)
    /// - Indicating regions (images)
    pub async fn navigate(&self, target: &NavigationTarget) -> Result<NavigationResult, NavigationError> {
        // Verify file exists
        if !target.path.exists() {
            return Err(NavigationError::FileNotFound {
                path: target.path.display().to_string(),
            });
        }

        let extension = target.path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Check for app override
        if let Some(app_path) = self.config.app_overrides.get(&extension) {
            return self.open_with_app(&target.path, app_path, &target.location).await;
        }

        // Use system default
        if self.config.use_system_default {
            return self.open_with_system_default(&target.path, &extension, &target.location).await;
        }

        Err(NavigationError::OpenFailed {
            reason: "No application configured for this file type".to_string(),
        })
    }

    /// Open a file with the system default application
    async fn open_with_system_default(
        &self,
        path: &Path,
        extension: &str,
        location: &Option<ChunkLocation>,
    ) -> Result<NavigationResult, NavigationError> {
        let location_navigated = location.is_some();
        
        // Build command based on file type and location
        let result = match extension {
            // Text/code files - try to open at specific line
            "txt" | "md" | "rs" | "py" | "js" | "ts" | "java" | "c" | "cpp" 
            | "h" | "hpp" | "go" | "rb" | "json" | "yaml" | "yml" | "toml" 
            | "xml" | "html" | "css" | "sh" | "ps1" => {
                self.open_text_file(path, location).await
            }
            // PDF files - try to open at specific page
            "pdf" => {
                self.open_pdf_file(path, location).await
            }
            // Image files - open normally (region indication handled by preview)
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tiff" | "svg" => {
                self.open_image_file(path, location).await
            }
            // Other files - open with system default
            _ => {
                self.open_generic_file(path).await
            }
        };

        result.map(|app_used| NavigationResult {
            success: true,
            app_used,
            location_navigated,
            message: None,
        })
    }

    /// Open a text file, optionally at a specific line
    async fn open_text_file(
        &self,
        path: &Path,
        location: &Option<ChunkLocation>,
    ) -> Result<Option<String>, NavigationError> {
        let line = location.as_ref().and_then(|loc| loc.start_line);

        #[cfg(windows)]
        {
            // Try VS Code first (supports line navigation)
            if let Some(line_num) = line {
                if self.try_open_with_vscode(path, Some(line_num)).await.is_ok() {
                    return Ok(Some("Visual Studio Code".to_string()));
                }
            }

            // Fall back to system default
            self.shell_open(path).await?;
            Ok(Some("System Default".to_string()))
        }

        #[cfg(target_os = "macos")]
        {
            // Try VS Code first
            if let Some(line_num) = line {
                if self.try_open_with_vscode(path, Some(line_num)).await.is_ok() {
                    return Ok(Some("Visual Studio Code".to_string()));
                }
            }

            // Fall back to open command
            Command::new("open")
                .arg(path)
                .spawn()
                .map_err(|e| NavigationError::OpenFailed {
                    reason: e.to_string(),
                })?;
            Ok(Some("System Default".to_string()))
        }

        #[cfg(target_os = "linux")]
        {
            // Try VS Code first
            if let Some(line_num) = line {
                if self.try_open_with_vscode(path, Some(line_num)).await.is_ok() {
                    return Ok(Some("Visual Studio Code".to_string()));
                }
            }

            // Fall back to xdg-open
            Command::new("xdg-open")
                .arg(path)
                .spawn()
                .map_err(|e| NavigationError::OpenFailed {
                    reason: e.to_string(),
                })?;
            Ok(Some("System Default".to_string()))
        }

        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            Err(NavigationError::OpenFailed {
                reason: "Unsupported platform".to_string(),
            })
        }
    }

    /// Try to open a file with VS Code at a specific line
    async fn try_open_with_vscode(&self, path: &Path, line: Option<u32>) -> Result<(), NavigationError> {
        let mut cmd = Command::new("code");
        
        if let Some(line_num) = line {
            // VS Code format: code --goto file:line
            cmd.arg("--goto")
                .arg(format!("{}:{}", path.display(), line_num));
        } else {
            cmd.arg(path);
        }

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.spawn().map_err(|e| NavigationError::AppNotFound {
            app: format!("VS Code: {}", e),
        })?;

        Ok(())
    }

    /// Open a PDF file, optionally at a specific page
    async fn open_pdf_file(
        &self,
        path: &Path,
        location: &Option<ChunkLocation>,
    ) -> Result<Option<String>, NavigationError> {
        let page = location.as_ref().and_then(|loc| loc.page_number);

        #[cfg(windows)]
        {
            // Try Adobe Acrobat with page parameter
            if let Some(page_num) = page {
                // Adobe Acrobat command line: AcroRd32.exe /A "page=N" file.pdf
                let acrobat_paths = [
                    r"C:\Program Files\Adobe\Acrobat DC\Acrobat\Acrobat.exe",
                    r"C:\Program Files (x86)\Adobe\Acrobat Reader DC\Reader\AcroRd32.exe",
                    r"C:\Program Files\Adobe\Reader 11.0\Reader\AcroRd32.exe",
                ];

                for acrobat_path in &acrobat_paths {
                    if Path::new(acrobat_path).exists() {
                        let result = Command::new(acrobat_path)
                            .arg("/A")
                            .arg(format!("page={}", page_num))
                            .arg(path)
                            .spawn();

                        if result.is_ok() {
                            return Ok(Some("Adobe Acrobat".to_string()));
                        }
                    }
                }
            }

            // Fall back to system default
            self.shell_open(path).await?;
            Ok(Some("System Default".to_string()))
        }

        #[cfg(target_os = "macos")]
        {
            // macOS Preview supports page navigation via AppleScript
            // For simplicity, just open with default
            Command::new("open")
                .arg(path)
                .spawn()
                .map_err(|e| NavigationError::OpenFailed {
                    reason: e.to_string(),
                })?;
            Ok(Some("Preview".to_string()))
        }

        #[cfg(target_os = "linux")]
        {
            // Try evince with page parameter
            if let Some(page_num) = page {
                let result = Command::new("evince")
                    .arg("--page-index")
                    .arg(page_num.to_string())
                    .arg(path)
                    .spawn();

                if result.is_ok() {
                    return Ok(Some("Evince".to_string()));
                }
            }

            // Fall back to xdg-open
            Command::new("xdg-open")
                .arg(path)
                .spawn()
                .map_err(|e| NavigationError::OpenFailed {
                    reason: e.to_string(),
                })?;
            Ok(Some("System Default".to_string()))
        }

        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            Err(NavigationError::OpenFailed {
                reason: "Unsupported platform".to_string(),
            })
        }
    }

    /// Open an image file
    async fn open_image_file(
        &self,
        path: &Path,
        _location: &Option<ChunkLocation>,
    ) -> Result<Option<String>, NavigationError> {
        // Images don't support direct region navigation in external apps
        // Region indication is handled by the preview system
        self.open_generic_file(path).await
    }

    /// Open a file with the system default application
    async fn open_generic_file(&self, path: &Path) -> Result<Option<String>, NavigationError> {
        self.shell_open(path).await?;
        Ok(Some("System Default".to_string()))
    }

    /// Open a file using the system shell
    #[cfg(windows)]
    async fn shell_open(&self, path: &Path) -> Result<(), NavigationError> {
        Command::new("cmd")
            .args(["/C", "start", "", &path.display().to_string()])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| NavigationError::OpenFailed {
                reason: e.to_string(),
            })?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    async fn shell_open(&self, path: &Path) -> Result<(), NavigationError> {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| NavigationError::OpenFailed {
                reason: e.to_string(),
            })?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn shell_open(&self, path: &Path) -> Result<(), NavigationError> {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| NavigationError::OpenFailed {
                reason: e.to_string(),
            })?;
        Ok(())
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    async fn shell_open(&self, _path: &Path) -> Result<(), NavigationError> {
        Err(NavigationError::OpenFailed {
            reason: "Unsupported platform".to_string(),
        })
    }

    /// Open a file with a specific application
    async fn open_with_app(
        &self,
        path: &Path,
        app_path: &Path,
        location: &Option<ChunkLocation>,
    ) -> Result<NavigationResult, NavigationError> {
        if !app_path.exists() {
            return Err(NavigationError::AppNotFound {
                app: app_path.display().to_string(),
            });
        }

        let mut cmd = Command::new(app_path);
        cmd.arg(path);

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.spawn().map_err(|e| NavigationError::OpenFailed {
            reason: e.to_string(),
        })?;

        Ok(NavigationResult {
            success: true,
            app_used: Some(app_path.display().to_string()),
            location_navigated: location.is_some(),
            message: Some("Opened with custom application".to_string()),
        })
    }

    /// Get the configuration
    pub fn config(&self) -> &NavigatorConfig {
        &self.config
    }

    /// Update the configuration
    pub fn set_config(&mut self, config: NavigatorConfig) {
        self.config = config;
    }

    /// Add an application override for a file extension
    pub fn add_app_override(&mut self, extension: impl Into<String>, app_path: impl Into<PathBuf>) {
        self.config.app_overrides.insert(extension.into(), app_path.into());
    }

    /// Remove an application override
    pub fn remove_app_override(&mut self, extension: &str) -> Option<PathBuf> {
        self.config.app_overrides.remove(extension)
    }
}

impl Default for HighlightNavigator {
    fn default() -> Self {
        Self::new()
    }
}
