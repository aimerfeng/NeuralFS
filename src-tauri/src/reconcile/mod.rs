//! File System Reconciliation Module
//!
//! Provides file system reconciliation services for detecting changes
//! between the database state and the actual file system state.
//! Supports rename detection via platform-specific FileID tracking.

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::core::error::{NeuralFSError, Result};
use crate::core::types::{FileRecord, FileType, IndexStatus, PrivacyLevel};
use crate::watcher::{DirectoryFilter, FilterResult};

/// Platform-specific file identifier for tracking files across renames
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId {
    #[cfg(windows)]
    /// Volume serial number (Windows)
    pub volume_serial: u32,
    #[cfg(windows)]
    /// High part of file index (Windows)
    pub file_index_high: u32,
    #[cfg(windows)]
    /// Low part of file index (Windows)
    pub file_index_low: u32,

    #[cfg(unix)]
    /// Device ID (Unix)
    pub device: u64,
    #[cfg(unix)]
    /// Inode number (Unix)
    pub inode: u64,
}

impl FileId {
    /// Get the file ID for a given path
    #[cfg(windows)]
    pub fn from_path(path: &Path) -> Result<Self> {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::Storage::FileSystem::{
            CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            OPEN_EXISTING,
        };
        use windows::Win32::Security::SECURITY_ATTRIBUTES;
        use windows::core::PCWSTR;

        unsafe {
            // Convert path to wide string
            let wide_path: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let handle = CreateFileW(
                PCWSTR::from_raw(wide_path.as_ptr()),
                0, // We only need to read attributes
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None as Option<*const SECURITY_ATTRIBUTES>,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS, // Required for directories
                None as Option<HANDLE>,
            )
            .map_err(|e| {
                NeuralFSError::FileSystem(crate::core::error::FileSystemError::ReadFailed {
                    path: path.display().to_string(),
                    reason: format!("Failed to open file for ID: {}", e),
                })
            })?;

            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            let result = GetFileInformationByHandle(handle, &mut info);
            let _ = CloseHandle(handle);

            result.map_err(|e| {
                NeuralFSError::FileSystem(crate::core::error::FileSystemError::ReadFailed {
                    path: path.display().to_string(),
                    reason: format!("Failed to get file information: {}", e),
                })
            })?;

            Ok(FileId {
                volume_serial: info.dwVolumeSerialNumber,
                file_index_high: info.nFileIndexHigh,
                file_index_low: info.nFileIndexLow,
            })
        }
    }

    /// Get the file ID for a given path (Unix implementation)
    #[cfg(unix)]
    pub fn from_path(path: &Path) -> Result<Self> {
        use std::os::unix::fs::MetadataExt;

        let metadata = std::fs::metadata(path).map_err(|e| {
            NeuralFSError::FileSystem(crate::core::error::FileSystemError::ReadFailed {
                path: path.display().to_string(),
                reason: format!("Failed to get metadata: {}", e),
            })
        })?;

        Ok(FileId {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    /// Convert to a string representation for database storage
    pub fn to_string_repr(&self) -> String {
        #[cfg(windows)]
        {
            format!(
                "{}:{}:{}",
                self.volume_serial, self.file_index_high, self.file_index_low
            )
        }
        #[cfg(unix)]
        {
            format!("{}:{}", self.device, self.inode)
        }
    }

    /// Parse from string representation
    pub fn from_string_repr(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();

        #[cfg(windows)]
        {
            if parts.len() != 3 {
                return None;
            }
            Some(FileId {
                volume_serial: parts[0].parse().ok()?,
                file_index_high: parts[1].parse().ok()?,
                file_index_low: parts[2].parse().ok()?,
            })
        }

        #[cfg(unix)]
        {
            if parts.len() != 2 {
                return None;
            }
            Some(FileId {
                device: parts[0].parse().ok()?,
                inode: parts[1].parse().ok()?,
            })
        }
    }
}


/// File system file information (from scanning)
#[derive(Debug, Clone)]
pub struct FsFileInfo {
    /// File path
    pub path: PathBuf,
    /// Platform-specific file ID
    pub file_id: FileId,
    /// File size in bytes
    pub size_bytes: u64,
    /// Last modification time
    pub modified_at: DateTime<Utc>,
    /// File extension
    pub extension: String,
}

/// Rename event detected during reconciliation
#[derive(Debug, Clone)]
pub struct RenameEvent {
    /// Old file path (from database)
    pub old_path: PathBuf,
    /// New file path (from filesystem)
    pub new_path: PathBuf,
    /// File ID that links the two
    pub file_id: FileId,
}

/// Result of reconciliation operation
#[derive(Debug, Default)]
pub struct ReconcileResult {
    /// Files that were added (new files in filesystem)
    pub added: Vec<PathBuf>,
    /// Files that were deleted (missing from filesystem)
    pub deleted: Vec<PathBuf>,
    /// Files that were modified (content changed)
    pub modified: Vec<PathBuf>,
    /// Files that were renamed (detected via FileID)
    pub renamed: Vec<RenameEvent>,
    /// Errors encountered during reconciliation
    pub errors: Vec<(PathBuf, String)>,
}

impl ReconcileResult {
    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty()
            || !self.deleted.is_empty()
            || !self.modified.is_empty()
            || !self.renamed.is_empty()
    }

    /// Get total number of changes
    pub fn total_changes(&self) -> usize {
        self.added.len() + self.deleted.len() + self.modified.len() + self.renamed.len()
    }
}

/// Configuration for reconciliation
#[derive(Debug, Clone)]
pub struct ReconcileConfig {
    /// Maximum parallel scans
    pub max_parallel_scans: usize,
    /// Batch size for database operations
    pub batch_size: usize,
    /// Use fast mode (only check mtime and size)
    pub fast_mode: bool,
    /// Verify content hash for modified files
    pub verify_hash: bool,
}

impl Default for ReconcileConfig {
    fn default() -> Self {
        Self {
            max_parallel_scans: 4,
            batch_size: 1000,
            fast_mode: true,
            verify_hash: false,
        }
    }
}

/// Database file record with FileID for reconciliation
#[derive(Debug, Clone)]
struct DbFileRecord {
    id: Uuid,
    path: PathBuf,
    file_id: Option<FileId>,
    size_bytes: u64,
    modified_at: DateTime<Utc>,
    content_hash: String,
}

/// File system reconciliation service
pub struct ReconciliationService {
    /// Database connection pool
    db: SqlitePool,
    /// Directory filter for excluding paths
    filter: Option<DirectoryFilter>,
    /// Configuration
    config: ReconcileConfig,
    /// Cache of file IDs (path -> FileId)
    file_id_cache: RwLock<HashMap<PathBuf, FileId>>,
}

impl ReconciliationService {
    /// Create a new reconciliation service
    pub fn new(db: SqlitePool) -> Self {
        Self {
            db,
            filter: None,
            config: ReconcileConfig::default(),
            file_id_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Create with custom configuration
    pub fn with_config(db: SqlitePool, config: ReconcileConfig) -> Self {
        Self {
            db,
            filter: None,
            config,
            file_id_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Set the directory filter
    pub fn with_filter(mut self, filter: DirectoryFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Execute reconciliation on startup
    pub async fn reconcile_on_startup(
        &self,
        monitored_paths: &[PathBuf],
    ) -> Result<ReconcileResult> {
        let mut result = ReconcileResult::default();

        // 1. Load all known files from database
        let db_files = self.load_db_files().await?;
        let mut db_file_map: HashMap<PathBuf, DbFileRecord> = db_files
            .into_iter()
            .map(|f| (f.path.clone(), f))
            .collect();

        // Build FileID -> DbFileRecord map for rename detection
        let mut file_id_to_db: HashMap<String, DbFileRecord> = HashMap::new();
        for record in db_file_map.values() {
            if let Some(ref file_id) = record.file_id {
                file_id_to_db.insert(file_id.to_string_repr(), record.clone());
            }
        }

        // 2. Scan filesystem
        let fs_files = self.scan_filesystem(monitored_paths).await?;
        let fs_file_map: HashMap<PathBuf, FsFileInfo> = fs_files
            .into_iter()
            .map(|f| (f.path.clone(), f))
            .collect();

        // 3. Calculate differences

        // 3.1 Check for new files and renames
        for (path, fs_info) in &fs_file_map {
            if !db_file_map.contains_key(path) {
                // File not in DB at this path - check if it's a rename
                let file_id_str = fs_info.file_id.to_string_repr();
                if let Some(old_record) = file_id_to_db.get(&file_id_str) {
                    // This is a rename - same FileID, different path
                    result.renamed.push(RenameEvent {
                        old_path: old_record.path.clone(),
                        new_path: path.clone(),
                        file_id: fs_info.file_id,
                    });
                    // Remove from db_file_map so it's not marked as deleted
                    db_file_map.remove(&old_record.path);
                } else {
                    // Truly new file
                    result.added.push(path.clone());
                }
            }
        }

        // 3.2 Check for deleted files
        for (path, _) in &db_file_map {
            if !fs_file_map.contains_key(path) {
                // Check if this was already handled as a rename
                let is_renamed = result.renamed.iter().any(|r| r.old_path == *path);
                if !is_renamed {
                    result.deleted.push(path.clone());
                }
            }
        }

        // 3.3 Check for modified files
        for (path, fs_info) in &fs_file_map {
            if let Some(db_record) = db_file_map.get(path) {
                // File exists in both - check if modified
                if self.is_file_modified(db_record, fs_info) {
                    result.modified.push(path.clone());
                }
            }
        }

        // 4. Apply changes to database
        self.apply_reconcile_result(&result).await?;

        Ok(result)
    }

    /// Check if a file has been modified
    fn is_file_modified(&self, db_record: &DbFileRecord, fs_info: &FsFileInfo) -> bool {
        // Fast mode: only check mtime and size
        if self.config.fast_mode {
            return fs_info.modified_at > db_record.modified_at
                || fs_info.size_bytes != db_record.size_bytes;
        }

        // Full mode: also verify hash (not implemented yet)
        fs_info.modified_at > db_record.modified_at || fs_info.size_bytes != db_record.size_bytes
    }

    /// Load all files from database
    async fn load_db_files(&self) -> Result<Vec<DbFileRecord>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, path, file_id, size_bytes, modified_at, content_hash
            FROM files
            WHERE is_excluded = 0
            "#
        )
        .fetch_all(&self.db)
        .await
        .map_err(NeuralFSError::Database)?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let file_id = row.file_id.as_ref().and_then(|s| FileId::from_string_repr(s));
            let modified_at = DateTime::parse_from_rfc3339(&row.modified_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            records.push(DbFileRecord {
                id: Uuid::parse_str(&row.id).unwrap_or_else(|_| Uuid::now_v7()),
                path: PathBuf::from(&row.path),
                file_id,
                size_bytes: row.size_bytes as u64,
                modified_at,
                content_hash: row.content_hash,
            });
        }

        Ok(records)
    }


    /// Scan filesystem for files
    async fn scan_filesystem(&self, paths: &[PathBuf]) -> Result<Vec<FsFileInfo>> {
        let mut all_files = Vec::new();

        for base_path in paths {
            if !base_path.exists() {
                continue;
            }

            let files = self.scan_directory(base_path).await?;
            all_files.extend(files);
        }

        Ok(all_files)
    }

    /// Scan a single directory recursively
    async fn scan_directory(&self, path: &Path) -> Result<Vec<FsFileInfo>> {
        let mut files = Vec::new();
        let mut stack = vec![path.to_path_buf()];

        while let Some(current_path) = stack.pop() {
            // Check filter
            if let Some(ref filter) = self.filter {
                if let FilterResult::Exclude(_) = filter.should_filter(&current_path) {
                    continue;
                }
            }

            let entries = match tokio::fs::read_dir(&current_path).await {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!("Failed to read directory {:?}: {}", current_path, e);
                    continue;
                }
            };

            let mut entries = entries;
            while let Ok(Some(entry)) = entries.next_entry().await {
                let entry_path = entry.path();

                // Check filter for each entry
                if let Some(ref filter) = self.filter {
                    if let FilterResult::Exclude(_) = filter.should_filter(&entry_path) {
                        continue;
                    }
                }

                let metadata = match entry.metadata().await {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!("Failed to get metadata for {:?}: {}", entry_path, e);
                        continue;
                    }
                };

                if metadata.is_dir() {
                    stack.push(entry_path);
                } else if metadata.is_file() {
                    // Get file info
                    match self.get_file_info(&entry_path, &metadata).await {
                        Ok(info) => files.push(info),
                        Err(e) => {
                            tracing::warn!("Failed to get file info for {:?}: {}", entry_path, e);
                        }
                    }
                }
            }
        }

        Ok(files)
    }

    /// Get file information including FileID
    async fn get_file_info(
        &self,
        path: &Path,
        metadata: &std::fs::Metadata,
    ) -> Result<FsFileInfo> {
        let file_id = FileId::from_path(path)?;

        let modified_at = metadata
            .modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        Ok(FsFileInfo {
            path: path.to_path_buf(),
            file_id,
            size_bytes: metadata.len(),
            modified_at,
            extension,
        })
    }

    /// Apply reconciliation result to database
    async fn apply_reconcile_result(&self, result: &ReconcileResult) -> Result<()> {
        // Handle renames - update path while preserving all other data
        for rename in &result.renamed {
            self.handle_rename(&rename.old_path, &rename.new_path, &rename.file_id)
                .await?;
        }

        // Handle deletions - mark files as deleted or remove from index
        for path in &result.deleted {
            self.handle_deletion(path).await?;
        }

        // Handle modifications - mark for re-indexing
        for path in &result.modified {
            self.handle_modification(path).await?;
        }

        // Handle additions - create new file records
        for path in &result.added {
            self.handle_addition(path).await?;
        }

        Ok(())
    }

    /// Handle a file rename
    async fn handle_rename(
        &self,
        old_path: &Path,
        new_path: &Path,
        file_id: &FileId,
    ) -> Result<()> {
        let new_filename = new_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let new_extension = new_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let old_path_str = old_path.to_string_lossy().to_string();
        let new_path_str = new_path.to_string_lossy().to_string();
        let file_id_str = file_id.to_string_repr();

        sqlx::query!(
            r#"
            UPDATE files
            SET path = ?, filename = ?, extension = ?, file_id = ?, indexed_at = datetime('now')
            WHERE path = ?
            "#,
            new_path_str,
            new_filename,
            new_extension,
            file_id_str,
            old_path_str
        )
        .execute(&self.db)
        .await
        .map_err(NeuralFSError::Database)?;

        tracing::info!("Renamed file: {:?} -> {:?}", old_path, new_path);
        Ok(())
    }

    /// Handle a file deletion
    async fn handle_deletion(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();

        // Delete the file record (cascades to chunks, tags, relations)
        sqlx::query!(
            r#"
            DELETE FROM files WHERE path = ?
            "#,
            path_str
        )
        .execute(&self.db)
        .await
        .map_err(NeuralFSError::Database)?;

        tracing::info!("Deleted file from index: {:?}", path);
        Ok(())
    }

    /// Handle a file modification
    async fn handle_modification(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();

        // Mark file for re-indexing
        sqlx::query!(
            r#"
            UPDATE files
            SET index_status = 'Pending', indexed_at = datetime('now')
            WHERE path = ?
            "#,
            path_str
        )
        .execute(&self.db)
        .await
        .map_err(NeuralFSError::Database)?;

        tracing::debug!("Marked file for re-indexing: {:?}", path);
        Ok(())
    }

    /// Handle a new file addition
    async fn handle_addition(&self, path: &Path) -> Result<()> {
        // Get file metadata
        let metadata = tokio::fs::metadata(path).await.map_err(|e| {
            NeuralFSError::FileSystem(crate::core::error::FileSystemError::ReadFailed {
                path: path.display().to_string(),
                reason: e.to_string(),
            })
        })?;

        let file_id = FileId::from_path(path)?;

        let id = Uuid::now_v7().to_string();
        let path_str = path.to_string_lossy().to_string();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let file_type = FileType::from_extension(&extension);
        let file_type_str = format!("{:?}", file_type);
        let size_bytes = metadata.len() as i64;
        let file_id_str = file_id.to_string_repr();

        let modified_at = metadata
            .modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now())
            .to_rfc3339();

        let created_at = metadata
            .created()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now())
            .to_rfc3339();

        // Placeholder hash - will be computed during indexing
        let content_hash = "pending".to_string();

        sqlx::query!(
            r#"
            INSERT INTO files (
                id, path, filename, extension, file_type, size_bytes,
                content_hash, created_at, modified_at, indexed_at,
                index_status, privacy_level, is_excluded, file_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), 'Pending', 'Normal', 0, ?)
            ON CONFLICT(path) DO UPDATE SET
                size_bytes = excluded.size_bytes,
                modified_at = excluded.modified_at,
                index_status = 'Pending',
                file_id = excluded.file_id
            "#,
            id,
            path_str,
            filename,
            extension,
            file_type_str,
            size_bytes,
            content_hash,
            created_at,
            modified_at,
            file_id_str
        )
        .execute(&self.db)
        .await
        .map_err(NeuralFSError::Database)?;

        tracing::debug!("Added new file to index: {:?}", path);
        Ok(())
    }

    /// Update FileID for an existing file record
    pub async fn update_file_id(&self, path: &Path) -> Result<FileId> {
        let file_id = FileId::from_path(path)?;
        let path_str = path.to_string_lossy().to_string();
        let file_id_str = file_id.to_string_repr();

        sqlx::query!(
            r#"
            UPDATE files SET file_id = ? WHERE path = ?
            "#,
            file_id_str,
            path_str
        )
        .execute(&self.db)
        .await
        .map_err(NeuralFSError::Database)?;

        // Update cache
        let mut cache = self.file_id_cache.write().await;
        cache.insert(path.to_path_buf(), file_id);

        Ok(file_id)
    }

    /// Get cached FileID or fetch from filesystem
    pub async fn get_file_id(&self, path: &Path) -> Result<FileId> {
        // Check cache first
        {
            let cache = self.file_id_cache.read().await;
            if let Some(file_id) = cache.get(path) {
                return Ok(*file_id);
            }
        }

        // Fetch from filesystem
        let file_id = FileId::from_path(path)?;

        // Update cache
        let mut cache = self.file_id_cache.write().await;
        cache.insert(path.to_path_buf(), file_id);

        Ok(file_id)
    }

    /// Clear the FileID cache
    pub async fn clear_cache(&self) {
        let mut cache = self.file_id_cache.write().await;
        cache.clear();
    }

    /// Get statistics about the reconciliation service
    pub async fn get_stats(&self) -> Result<ReconcileStats> {
        let total_files: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM files")
            .fetch_one(&self.db)
            .await
            .map_err(NeuralFSError::Database)?;

        let files_with_id: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM files WHERE file_id IS NOT NULL")
                .fetch_one(&self.db)
                .await
                .map_err(NeuralFSError::Database)?;

        let cache_size = self.file_id_cache.read().await.len();

        Ok(ReconcileStats {
            total_files: total_files.0 as u64,
            files_with_file_id: files_with_id.0 as u64,
            cache_size,
        })
    }
}

/// Statistics about the reconciliation service
#[derive(Debug, Clone)]
pub struct ReconcileStats {
    /// Total number of files in database
    pub total_files: u64,
    /// Number of files with FileID tracked
    pub files_with_file_id: u64,
    /// Size of FileID cache
    pub cache_size: usize,
}
