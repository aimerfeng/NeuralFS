//! Shared Memory Module
//!
//! Cross-platform shared memory implementation for heartbeat communication
//! between the main NeuralFS process and the watchdog supervisor.
//!
//! - Windows: Uses named shared memory via CreateFileMappingW
//! - Non-Windows: Uses file-based mock for development/testing

use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Heartbeat interval in milliseconds
pub const HEARTBEAT_INTERVAL_MS: u64 = 1000;

/// Heartbeat timeout in milliseconds (3x interval for safety margin)
pub const HEARTBEAT_TIMEOUT_MS: u64 = 3000;

/// Shared memory name for NeuralFS heartbeat
pub const SHARED_MEMORY_NAME: &str = "NeuralFS_Heartbeat_v1";

/// Size of shared memory region in bytes
pub const SHARED_MEMORY_SIZE: usize = std::mem::size_of::<HeartbeatData>();

/// Heartbeat data structure stored in shared memory
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct HeartbeatData {
    /// Magic number for validation (0x4E465348 = "NFSH")
    pub magic: u32,
    /// Version of the heartbeat protocol
    pub version: u32,
    /// Process ID of the main application
    pub process_id: u32,
    /// Timestamp of last heartbeat (milliseconds since UNIX epoch)
    pub last_heartbeat_ms: u64,
    /// Application state flags
    pub state_flags: u32,
    /// Reserved for future use
    pub reserved: [u8; 44],
}

impl HeartbeatData {
    /// Magic number for validation
    pub const MAGIC: u32 = 0x4E465348; // "NFSH"
    
    /// Current protocol version
    pub const VERSION: u32 = 1;

    /// Create a new heartbeat data with current timestamp
    pub fn new(process_id: u32) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            process_id,
            last_heartbeat_ms: Self::current_timestamp_ms(),
            state_flags: 0,
            reserved: [0; 44],
        }
    }

    /// Update the heartbeat timestamp
    pub fn update(&mut self) {
        self.last_heartbeat_ms = Self::current_timestamp_ms();
    }

    /// Check if the heartbeat data is valid
    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version == Self::VERSION
    }

    /// Check if the heartbeat has timed out
    pub fn is_timed_out(&self, timeout_ms: u64) -> bool {
        let current = Self::current_timestamp_ms();
        current.saturating_sub(self.last_heartbeat_ms) > timeout_ms
    }

    /// Get current timestamp in milliseconds since UNIX epoch
    pub fn current_timestamp_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Errors that can occur during shared memory operations
#[derive(Error, Debug)]
pub enum SharedMemoryError {
    #[error("Failed to create shared memory: {0}")]
    CreateFailed(String),

    #[error("Failed to open shared memory: {0}")]
    OpenFailed(String),

    #[error("Failed to map shared memory: {0}")]
    MapFailed(String),

    #[error("Failed to write to shared memory: {0}")]
    WriteFailed(String),

    #[error("Failed to read from shared memory: {0}")]
    ReadFailed(String),

    #[error("Invalid heartbeat data")]
    InvalidData,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Trait for shared memory operations
pub trait SharedMemory: Send + Sync {
    /// Create or open shared memory for writing (main process)
    fn create(&mut self) -> Result<(), SharedMemoryError>;

    /// Open existing shared memory for reading (watchdog)
    fn open(&mut self) -> Result<(), SharedMemoryError>;

    /// Write heartbeat data to shared memory
    fn write(&self, data: &HeartbeatData) -> Result<(), SharedMemoryError>;

    /// Read heartbeat data from shared memory
    fn read(&self) -> Result<HeartbeatData, SharedMemoryError>;

    /// Close and cleanup shared memory
    fn close(&mut self);

    /// Check if shared memory is open
    fn is_open(&self) -> bool;
}


// ============================================================================
// Windows Implementation
// ============================================================================

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::ptr;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
        FILE_MAP_ALL_ACCESS, FILE_MAP_READ, PAGE_READWRITE,
    };

    /// Windows shared memory implementation using named file mapping
    pub struct WindowsSharedMemory {
        handle: Option<HANDLE>,
        view: Option<*mut HeartbeatData>,
        is_creator: bool,
    }

    // SAFETY: The raw pointer is only accessed through synchronized methods
    unsafe impl Send for WindowsSharedMemory {}
    unsafe impl Sync for WindowsSharedMemory {}

    impl WindowsSharedMemory {
        pub fn new() -> Self {
            Self {
                handle: None,
                view: None,
                is_creator: false,
            }
        }

        fn get_wide_name() -> Vec<u16> {
            SHARED_MEMORY_NAME
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect()
        }
    }

    impl Default for WindowsSharedMemory {
        fn default() -> Self {
            Self::new()
        }
    }

    impl SharedMemory for WindowsSharedMemory {
        fn create(&mut self) -> Result<(), SharedMemoryError> {
            if self.is_open() {
                return Ok(());
            }

            let name = Self::get_wide_name();
            
            // Create file mapping
            let handle = unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    None,
                    PAGE_READWRITE,
                    0,
                    SHARED_MEMORY_SIZE as u32,
                    PCWSTR(name.as_ptr()),
                )
            }.map_err(|e| SharedMemoryError::CreateFailed(e.to_string()))?;

            // Map view of file
            let view = unsafe {
                MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, SHARED_MEMORY_SIZE)
            };

            if view.Value.is_null() {
                unsafe { CloseHandle(handle).ok() };
                return Err(SharedMemoryError::MapFailed("MapViewOfFile returned null".to_string()));
            }

            self.handle = Some(handle);
            self.view = Some(view.Value as *mut HeartbeatData);
            self.is_creator = true;

            Ok(())
        }

        fn open(&mut self) -> Result<(), SharedMemoryError> {
            if self.is_open() {
                return Ok(());
            }

            let name = Self::get_wide_name();

            // Open existing file mapping
            let handle = unsafe {
                OpenFileMappingW(FILE_MAP_READ.0, false, PCWSTR(name.as_ptr()))
            }.map_err(|e| SharedMemoryError::OpenFailed(e.to_string()))?;

            // Map view of file (read-only)
            let view = unsafe {
                MapViewOfFile(handle, FILE_MAP_READ, 0, 0, SHARED_MEMORY_SIZE)
            };

            if view.Value.is_null() {
                unsafe { CloseHandle(handle).ok() };
                return Err(SharedMemoryError::MapFailed("MapViewOfFile returned null".to_string()));
            }

            self.handle = Some(handle);
            self.view = Some(view.Value as *mut HeartbeatData);
            self.is_creator = false;

            Ok(())
        }

        fn write(&self, data: &HeartbeatData) -> Result<(), SharedMemoryError> {
            let view = self.view.ok_or(SharedMemoryError::WriteFailed(
                "Shared memory not open".to_string(),
            ))?;

            unsafe {
                ptr::write_volatile(view, *data);
            }

            Ok(())
        }

        fn read(&self) -> Result<HeartbeatData, SharedMemoryError> {
            let view = self.view.ok_or(SharedMemoryError::ReadFailed(
                "Shared memory not open".to_string(),
            ))?;

            let data = unsafe { ptr::read_volatile(view) };

            if !data.is_valid() {
                return Err(SharedMemoryError::InvalidData);
            }

            Ok(data)
        }

        fn close(&mut self) {
            if let Some(view) = self.view.take() {
                unsafe {
                    let _ = UnmapViewOfFile(windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                        Value: view as *mut _,
                    });
                }
            }

            if let Some(handle) = self.handle.take() {
                unsafe {
                    let _ = CloseHandle(handle);
                }
            }

            self.is_creator = false;
        }

        fn is_open(&self) -> bool {
            self.handle.is_some() && self.view.is_some()
        }
    }

    impl Drop for WindowsSharedMemory {
        fn drop(&mut self) {
            self.close();
        }
    }
}

#[cfg(windows)]
pub use windows_impl::WindowsSharedMemory;


// ============================================================================
// Non-Windows Implementation (File-based Mock)
// ============================================================================

#[cfg(not(windows))]
mod file_impl {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// File-based shared memory mock for non-Windows platforms
    /// This allows development and testing on macOS/Linux
    pub struct FileSharedMemory {
        file: Option<Mutex<File>>,
        path: PathBuf,
        is_creator: bool,
    }

    impl FileSharedMemory {
        pub fn new() -> Self {
            let path = std::env::temp_dir().join("neuralfs_heartbeat.bin");
            Self {
                file: None,
                path,
                is_creator: false,
            }
        }

        pub fn with_path(path: PathBuf) -> Self {
            Self {
                file: None,
                path,
                is_creator: false,
            }
        }
    }

    impl Default for FileSharedMemory {
        fn default() -> Self {
            Self::new()
        }
    }

    impl SharedMemory for FileSharedMemory {
        fn create(&mut self) -> Result<(), SharedMemoryError> {
            if self.is_open() {
                return Ok(());
            }

            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&self.path)
                .map_err(|e| SharedMemoryError::CreateFailed(e.to_string()))?;

            // Pre-allocate the file
            file.set_len(SHARED_MEMORY_SIZE as u64)
                .map_err(|e| SharedMemoryError::CreateFailed(e.to_string()))?;

            self.file = Some(Mutex::new(file));
            self.is_creator = true;

            Ok(())
        }

        fn open(&mut self) -> Result<(), SharedMemoryError> {
            if self.is_open() {
                return Ok(());
            }

            let file = OpenOptions::new()
                .read(true)
                .write(false)
                .open(&self.path)
                .map_err(|e| SharedMemoryError::OpenFailed(e.to_string()))?;

            self.file = Some(Mutex::new(file));
            self.is_creator = false;

            Ok(())
        }

        fn write(&self, data: &HeartbeatData) -> Result<(), SharedMemoryError> {
            let file_mutex = self.file.as_ref().ok_or(SharedMemoryError::WriteFailed(
                "Shared memory not open".to_string(),
            ))?;

            let mut file = file_mutex
                .lock()
                .map_err(|e| SharedMemoryError::WriteFailed(e.to_string()))?;

            file.seek(SeekFrom::Start(0))
                .map_err(|e| SharedMemoryError::WriteFailed(e.to_string()))?;

            // Convert HeartbeatData to bytes
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    data as *const HeartbeatData as *const u8,
                    SHARED_MEMORY_SIZE,
                )
            };

            file.write_all(bytes)
                .map_err(|e| SharedMemoryError::WriteFailed(e.to_string()))?;

            file.sync_all()
                .map_err(|e| SharedMemoryError::WriteFailed(e.to_string()))?;

            Ok(())
        }

        fn read(&self) -> Result<HeartbeatData, SharedMemoryError> {
            let file_mutex = self.file.as_ref().ok_or(SharedMemoryError::ReadFailed(
                "Shared memory not open".to_string(),
            ))?;

            let mut file = file_mutex
                .lock()
                .map_err(|e| SharedMemoryError::ReadFailed(e.to_string()))?;

            file.seek(SeekFrom::Start(0))
                .map_err(|e| SharedMemoryError::ReadFailed(e.to_string()))?;

            let mut bytes = vec![0u8; SHARED_MEMORY_SIZE];
            file.read_exact(&mut bytes)
                .map_err(|e| SharedMemoryError::ReadFailed(e.to_string()))?;

            // Convert bytes to HeartbeatData
            let data = unsafe { std::ptr::read(bytes.as_ptr() as *const HeartbeatData) };

            if !data.is_valid() {
                return Err(SharedMemoryError::InvalidData);
            }

            Ok(data)
        }

        fn close(&mut self) {
            self.file = None;
            
            // Only delete the file if we created it
            if self.is_creator {
                let _ = std::fs::remove_file(&self.path);
            }
            
            self.is_creator = false;
        }

        fn is_open(&self) -> bool {
            self.file.is_some()
        }
    }

    impl Drop for FileSharedMemory {
        fn drop(&mut self) {
            self.close();
        }
    }
}

#[cfg(not(windows))]
pub use file_impl::FileSharedMemory;

// ============================================================================
// Platform-agnostic factory function
// ============================================================================

/// Create a platform-appropriate shared memory instance
#[cfg(windows)]
pub fn create_shared_memory() -> Box<dyn SharedMemory> {
    Box::new(WindowsSharedMemory::new())
}

#[cfg(not(windows))]
pub fn create_shared_memory() -> Box<dyn SharedMemory> {
    Box::new(FileSharedMemory::new())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_data_creation() {
        let data = HeartbeatData::new(12345);
        assert_eq!(data.magic, HeartbeatData::MAGIC);
        assert_eq!(data.version, HeartbeatData::VERSION);
        assert_eq!(data.process_id, 12345);
        assert!(data.is_valid());
    }

    #[test]
    fn test_heartbeat_data_update() {
        let mut data = HeartbeatData::new(12345);
        let old_timestamp = data.last_heartbeat_ms;
        
        std::thread::sleep(std::time::Duration::from_millis(10));
        data.update();
        
        assert!(data.last_heartbeat_ms >= old_timestamp);
    }

    #[test]
    fn test_heartbeat_timeout_detection() {
        let mut data = HeartbeatData::new(12345);
        
        // Fresh heartbeat should not be timed out
        assert!(!data.is_timed_out(HEARTBEAT_TIMEOUT_MS));
        
        // Simulate old heartbeat
        data.last_heartbeat_ms = HeartbeatData::current_timestamp_ms() - HEARTBEAT_TIMEOUT_MS - 1000;
        assert!(data.is_timed_out(HEARTBEAT_TIMEOUT_MS));
    }

    #[test]
    fn test_heartbeat_data_validation() {
        let valid_data = HeartbeatData::new(12345);
        assert!(valid_data.is_valid());

        let invalid_data = HeartbeatData {
            magic: 0x12345678, // Wrong magic
            ..HeartbeatData::new(12345)
        };
        assert!(!invalid_data.is_valid());
    }

    #[test]
    fn test_heartbeat_data_size() {
        // Ensure the struct is exactly 64 bytes for consistent memory layout
        assert_eq!(SHARED_MEMORY_SIZE, 64);
    }
}
