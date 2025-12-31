//! Application Launcher Module
//!
//! Provides functionality for:
//! - Opening files with system default applications
//! - Discovering installed applications
//! - "Open with" menu support
//! - File type associations
//!
//! Requirements: 14.1, 14.3

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Errors that can occur during application launching
#[derive(Error, Debug)]
pub enum LaunchError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Application not found: {app}")]
    AppNotFound { app: String },

    #[error("Failed to launch application: {reason}")]
    LaunchFailed { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Registry error: {reason}")]
    RegistryError { reason: String },

    #[error("Unsupported platform")]
    UnsupportedPlatform,
}

/// Information about an installed application
#[derive(Debug, Clone)]
pub struct InstalledApp {
    /// Application name
    pub name: String,
    /// Path to the executable
    pub path: PathBuf,
    /// Application icon path (if available)
    pub icon_path: Option<PathBuf>,
    /// Supported file extensions
    pub supported_extensions: Vec<String>,
    /// Whether this is a system default for any extension
    pub is_default: bool,
}

/// Application information for display
#[derive(Debug, Clone)]
pub struct AppInfo {
    /// Application name
    pub name: String,
    /// Path to the executable
    pub path: PathBuf,
    /// Whether this is the default app for the file type
    pub is_default: bool,
}

/// File type association
#[derive(Debug, Clone)]
pub struct FileTypeAssociation {
    /// File extension (without dot)
    pub extension: String,
    /// MIME type (if known)
    pub mime_type: Option<String>,
    /// Default application for this type
    pub default_app: Option<AppInfo>,
    /// All applications that can open this type
    pub available_apps: Vec<AppInfo>,
}

/// Result of launching an application
#[derive(Debug, Clone)]
pub struct LaunchResult {
    /// Whether the launch was successful
    pub success: bool,
    /// The application that was launched
    pub app_name: String,
    /// Process ID (if available)
    pub process_id: Option<u32>,
    /// Any message or warning
    pub message: Option<String>,
}

/// Application launcher - handles opening files with applications
pub struct AppLauncher {
    /// Cache of discovered applications
    app_cache: HashMap<String, Vec<InstalledApp>>,
    /// Whether the cache has been populated
    cache_populated: bool,
}

impl AppLauncher {
    /// Create a new application launcher
    pub fn new() -> Self {
        Self {
            app_cache: HashMap::new(),
            cache_populated: false,
        }
    }

    /// Open a file with the system default application
    ///
    /// Requirement 14.1: WHEN a user double-clicks a file, THE NeuralFS_Shell 
    /// SHALL open it with the system default application
    pub async fn open_with_default(&self, path: &Path) -> Result<LaunchResult, LaunchError> {
        if !path.exists() {
            return Err(LaunchError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        #[cfg(windows)]
        {
            self.windows_shell_execute(path).await
        }

        #[cfg(target_os = "macos")]
        {
            self.macos_open(path).await
        }

        #[cfg(target_os = "linux")]
        {
            self.linux_xdg_open(path).await
        }

        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            Err(LaunchError::UnsupportedPlatform)
        }
    }

    /// Open a file with a specific application
    pub async fn open_with_app(&self, path: &Path, app: &AppInfo) -> Result<LaunchResult, LaunchError> {
        if !path.exists() {
            return Err(LaunchError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        if !app.path.exists() {
            return Err(LaunchError::AppNotFound {
                app: app.name.clone(),
            });
        }

        let mut cmd = Command::new(&app.path);
        cmd.arg(path);

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd.spawn().map_err(|e| LaunchError::LaunchFailed {
            reason: e.to_string(),
        })?;

        Ok(LaunchResult {
            success: true,
            app_name: app.name.clone(),
            process_id: Some(child.id()),
            message: None,
        })
    }

    /// Get available applications for a file type
    ///
    /// Requirement 14.3: WHEN displaying file context menu, THE NeuralFS_Shell 
    /// SHALL show "Open with" options for compatible applications
    pub async fn get_apps_for_file(&mut self, path: &Path) -> Result<FileTypeAssociation, LaunchError> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        self.get_apps_for_extension(&extension).await
    }

    /// Get available applications for a file extension
    pub async fn get_apps_for_extension(&mut self, extension: &str) -> Result<FileTypeAssociation, LaunchError> {
        // Populate cache if needed
        if !self.cache_populated {
            self.discover_applications().await?;
        }

        let extension_lower = extension.to_lowercase();
        let mime_type = self.get_mime_type(&extension_lower);

        // Get default app
        let default_app = self.get_default_app(&extension_lower).await.ok();

        // Get all available apps
        let available_apps = self.get_available_apps(&extension_lower).await;

        Ok(FileTypeAssociation {
            extension: extension_lower,
            mime_type,
            default_app,
            available_apps,
        })
    }

    /// Discover installed applications
    async fn discover_applications(&mut self) -> Result<(), LaunchError> {
        #[cfg(windows)]
        {
            self.discover_windows_apps().await?;
        }

        #[cfg(target_os = "macos")]
        {
            self.discover_macos_apps().await?;
        }

        #[cfg(target_os = "linux")]
        {
            self.discover_linux_apps().await?;
        }

        self.cache_populated = true;
        Ok(())
    }

    /// Get the default application for an extension
    async fn get_default_app(&self, extension: &str) -> Result<AppInfo, LaunchError> {
        #[cfg(windows)]
        {
            self.get_windows_default_app(extension).await
        }

        #[cfg(target_os = "macos")]
        {
            self.get_macos_default_app(extension).await
        }

        #[cfg(target_os = "linux")]
        {
            self.get_linux_default_app(extension).await
        }

        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            Err(LaunchError::UnsupportedPlatform)
        }
    }

    /// Get all available apps for an extension
    async fn get_available_apps(&self, extension: &str) -> Vec<AppInfo> {
        let mut apps = Vec::new();

        // Add apps from cache that support this extension
        if let Some(cached_apps) = self.app_cache.get(extension) {
            for app in cached_apps {
                apps.push(AppInfo {
                    name: app.name.clone(),
                    path: app.path.clone(),
                    is_default: app.is_default,
                });
            }
        }

        // Add common apps based on extension type
        apps.extend(self.get_common_apps_for_extension(extension));

        // Remove duplicates
        apps.dedup_by(|a, b| a.path == b.path);

        apps
    }

    /// Get common applications for an extension
    fn get_common_apps_for_extension(&self, extension: &str) -> Vec<AppInfo> {
        let mut apps = Vec::new();

        match extension {
            // Text/code files
            "txt" | "md" | "rs" | "py" | "js" | "ts" | "json" | "yaml" | "yml" 
            | "toml" | "xml" | "html" | "css" | "java" | "c" | "cpp" | "h" | "go" => {
                // VS Code
                #[cfg(windows)]
                {
                    let vscode_paths = [
                        r"C:\Program Files\Microsoft VS Code\Code.exe",
                        r"C:\Users\%USERNAME%\AppData\Local\Programs\Microsoft VS Code\Code.exe",
                    ];
                    for path in &vscode_paths {
                        let expanded = self.expand_env_vars(path);
                        if Path::new(&expanded).exists() {
                            apps.push(AppInfo {
                                name: "Visual Studio Code".to_string(),
                                path: PathBuf::from(expanded),
                                is_default: false,
                            });
                            break;
                        }
                    }

                    // Notepad++
                    let notepadpp_path = r"C:\Program Files\Notepad++\notepad++.exe";
                    if Path::new(notepadpp_path).exists() {
                        apps.push(AppInfo {
                            name: "Notepad++".to_string(),
                            path: PathBuf::from(notepadpp_path),
                            is_default: false,
                        });
                    }

                    // Notepad (always available)
                    apps.push(AppInfo {
                        name: "Notepad".to_string(),
                        path: PathBuf::from(r"C:\Windows\System32\notepad.exe"),
                        is_default: false,
                    });
                }

                #[cfg(target_os = "macos")]
                {
                    apps.push(AppInfo {
                        name: "TextEdit".to_string(),
                        path: PathBuf::from("/Applications/TextEdit.app"),
                        is_default: false,
                    });
                }

                #[cfg(target_os = "linux")]
                {
                    // Check for common editors using command -v
                    for (name, cmd) in &[("gedit", "gedit"), ("kate", "kate"), ("nano", "nano")] {
                        let check = std::process::Command::new("sh")
                            .args(["-c", &format!("command -v {}", cmd)])
                            .output();
                        if let Ok(output) = check {
                            if output.status.success() {
                                apps.push(AppInfo {
                                    name: name.to_string(),
                                    path: PathBuf::from(cmd),
                                    is_default: false,
                                });
                            }
                        }
                    }
                }
            }

            // Image files
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => {
                #[cfg(windows)]
                {
                    // Windows Photo Viewer / Photos app
                    apps.push(AppInfo {
                        name: "Photos".to_string(),
                        path: PathBuf::from("ms-photos:"),
                        is_default: false,
                    });

                    // Paint
                    apps.push(AppInfo {
                        name: "Paint".to_string(),
                        path: PathBuf::from(r"C:\Windows\System32\mspaint.exe"),
                        is_default: false,
                    });
                }

                #[cfg(target_os = "macos")]
                {
                    apps.push(AppInfo {
                        name: "Preview".to_string(),
                        path: PathBuf::from("/Applications/Preview.app"),
                        is_default: false,
                    });
                }
            }

            // PDF files
            "pdf" => {
                #[cfg(windows)]
                {
                    let acrobat_paths = [
                        r"C:\Program Files\Adobe\Acrobat DC\Acrobat\Acrobat.exe",
                        r"C:\Program Files (x86)\Adobe\Acrobat Reader DC\Reader\AcroRd32.exe",
                    ];
                    for path in &acrobat_paths {
                        if Path::new(path).exists() {
                            apps.push(AppInfo {
                                name: "Adobe Acrobat".to_string(),
                                path: PathBuf::from(path),
                                is_default: false,
                            });
                            break;
                        }
                    }

                    // Edge (can open PDFs)
                    let edge_path = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe";
                    if Path::new(edge_path).exists() {
                        apps.push(AppInfo {
                            name: "Microsoft Edge".to_string(),
                            path: PathBuf::from(edge_path),
                            is_default: false,
                        });
                    }
                }

                #[cfg(target_os = "macos")]
                {
                    apps.push(AppInfo {
                        name: "Preview".to_string(),
                        path: PathBuf::from("/Applications/Preview.app"),
                        is_default: false,
                    });
                }

                #[cfg(target_os = "linux")]
                {
                    for (name, cmd) in &[("Evince", "evince"), ("Okular", "okular")] {
                        let check = std::process::Command::new("sh")
                            .args(["-c", &format!("command -v {}", cmd)])
                            .output();
                        if let Ok(output) = check {
                            if output.status.success() {
                                apps.push(AppInfo {
                                    name: name.to_string(),
                                    path: PathBuf::from(cmd),
                                    is_default: false,
                                });
                            }
                        }
                    }
                }
            }

            _ => {}
        }

        apps
    }

    /// Get MIME type for an extension
    fn get_mime_type(&self, extension: &str) -> Option<String> {
        match extension {
            "txt" => Some("text/plain".to_string()),
            "html" | "htm" => Some("text/html".to_string()),
            "css" => Some("text/css".to_string()),
            "js" => Some("application/javascript".to_string()),
            "json" => Some("application/json".to_string()),
            "xml" => Some("application/xml".to_string()),
            "pdf" => Some("application/pdf".to_string()),
            "png" => Some("image/png".to_string()),
            "jpg" | "jpeg" => Some("image/jpeg".to_string()),
            "gif" => Some("image/gif".to_string()),
            "webp" => Some("image/webp".to_string()),
            "svg" => Some("image/svg+xml".to_string()),
            "mp3" => Some("audio/mpeg".to_string()),
            "mp4" => Some("video/mp4".to_string()),
            "zip" => Some("application/zip".to_string()),
            _ => None,
        }
    }

    /// Expand environment variables in a path
    #[cfg(windows)]
    fn expand_env_vars(&self, path: &str) -> String {
        let mut result = path.to_string();
        if let Ok(username) = std::env::var("USERNAME") {
            result = result.replace("%USERNAME%", &username);
        }
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            result = result.replace("%USERPROFILE%", &userprofile);
        }
        result
    }

    #[cfg(not(windows))]
    fn expand_env_vars(&self, path: &str) -> String {
        path.to_string()
    }

    // Platform-specific implementations

    #[cfg(windows)]
    async fn windows_shell_execute(&self, path: &Path) -> Result<LaunchResult, LaunchError> {
        use std::ptr;
        
        // Use ShellExecuteW for proper Windows shell integration
        let path_str = path.display().to_string();
        
        let child = Command::new("cmd")
            .args(["/C", "start", "", &path_str])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| LaunchError::LaunchFailed {
                reason: e.to_string(),
            })?;

        Ok(LaunchResult {
            success: true,
            app_name: "System Default".to_string(),
            process_id: Some(child.id()),
            message: None,
        })
    }

    #[cfg(target_os = "macos")]
    async fn macos_open(&self, path: &Path) -> Result<LaunchResult, LaunchError> {
        let child = Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| LaunchError::LaunchFailed {
                reason: e.to_string(),
            })?;

        Ok(LaunchResult {
            success: true,
            app_name: "System Default".to_string(),
            process_id: Some(child.id()),
            message: None,
        })
    }

    #[cfg(target_os = "linux")]
    async fn linux_xdg_open(&self, path: &Path) -> Result<LaunchResult, LaunchError> {
        let child = Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| LaunchError::LaunchFailed {
                reason: e.to_string(),
            })?;

        Ok(LaunchResult {
            success: true,
            app_name: "System Default".to_string(),
            process_id: Some(child.id()),
            message: None,
        })
    }

    #[cfg(windows)]
    async fn discover_windows_apps(&mut self) -> Result<(), LaunchError> {
        // Basic discovery - in production, would query Windows Registry
        // HKEY_CLASSES_ROOT for file associations
        // HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths
        
        // For now, add common applications
        let common_apps = vec![
            InstalledApp {
                name: "Notepad".to_string(),
                path: PathBuf::from(r"C:\Windows\System32\notepad.exe"),
                icon_path: None,
                supported_extensions: vec!["txt".to_string(), "log".to_string(), "ini".to_string()],
                is_default: false,
            },
            InstalledApp {
                name: "Paint".to_string(),
                path: PathBuf::from(r"C:\Windows\System32\mspaint.exe"),
                icon_path: None,
                supported_extensions: vec!["png".to_string(), "jpg".to_string(), "bmp".to_string()],
                is_default: false,
            },
        ];

        for app in common_apps {
            if app.path.exists() {
                for ext in &app.supported_extensions {
                    self.app_cache
                        .entry(ext.clone())
                        .or_insert_with(Vec::new)
                        .push(app.clone());
                }
            }
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    async fn discover_macos_apps(&mut self) -> Result<(), LaunchError> {
        // Would use Launch Services API in production
        // For now, add common applications
        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn discover_linux_apps(&mut self) -> Result<(), LaunchError> {
        // Would parse .desktop files in /usr/share/applications
        // For now, basic discovery
        Ok(())
    }

    #[cfg(windows)]
    async fn get_windows_default_app(&self, extension: &str) -> Result<AppInfo, LaunchError> {
        // In production, would query HKEY_CLASSES_ROOT\.{extension}
        // and follow the ProgID to get the default application
        
        // For now, return a placeholder
        Err(LaunchError::AppNotFound {
            app: format!("Default app for .{}", extension),
        })
    }

    #[cfg(target_os = "macos")]
    async fn get_macos_default_app(&self, extension: &str) -> Result<AppInfo, LaunchError> {
        // Would use Launch Services API
        Err(LaunchError::AppNotFound {
            app: format!("Default app for .{}", extension),
        })
    }

    #[cfg(target_os = "linux")]
    async fn get_linux_default_app(&self, extension: &str) -> Result<AppInfo, LaunchError> {
        // Would use xdg-mime query
        Err(LaunchError::AppNotFound {
            app: format!("Default app for .{}", extension),
        })
    }
}

impl Default for AppLauncher {
    fn default() -> Self {
        Self::new()
    }
}
