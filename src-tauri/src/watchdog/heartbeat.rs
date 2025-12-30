//! Heartbeat Sender Module
//!
//! Provides functionality for the main NeuralFS process to send heartbeats
//! to the watchdog via shared memory.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::shared_memory::{
    create_shared_memory, HeartbeatData, SharedMemory, SharedMemoryError,
    HEARTBEAT_INTERVAL_MS,
};

/// Heartbeat sender for the main process
pub struct HeartbeatSender {
    shared_memory: Box<dyn SharedMemory>,
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
    process_id: u32,
}

impl HeartbeatSender {
    /// Create a new heartbeat sender
    pub fn new() -> Self {
        Self {
            shared_memory: create_shared_memory(),
            running: Arc::new(AtomicBool::new(false)),
            thread_handle: None,
            process_id: std::process::id(),
        }
    }

    /// Start sending heartbeats in a background thread
    pub fn start(&mut self) -> Result<(), SharedMemoryError> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(()); // Already running
        }

        // Create shared memory
        self.shared_memory.create()?;

        // Write initial heartbeat
        let data = HeartbeatData::new(self.process_id);
        self.shared_memory.write(&data)?;

        self.running.store(true, Ordering::SeqCst);

        // Clone for the thread
        let running = Arc::clone(&self.running);
        let mut shared_memory = create_shared_memory();
        shared_memory.create()?;
        let process_id = self.process_id;

        // Spawn heartbeat thread
        let handle = thread::Builder::new()
            .name("heartbeat-sender".to_string())
            .spawn(move || {
                let mut data = HeartbeatData::new(process_id);
                
                while running.load(Ordering::SeqCst) {
                    // Update heartbeat
                    data.update();
                    
                    if let Err(e) = shared_memory.write(&data) {
                        tracing::warn!("Failed to write heartbeat: {}", e);
                    }

                    thread::sleep(Duration::from_millis(HEARTBEAT_INTERVAL_MS));
                }

                // Cleanup
                shared_memory.close();
                tracing::debug!("Heartbeat sender stopped");
            })
            .map_err(|e| SharedMemoryError::CreateFailed(e.to_string()))?;

        self.thread_handle = Some(handle);
        tracing::info!("Heartbeat sender started (PID: {})", self.process_id);

        Ok(())
    }

    /// Stop sending heartbeats
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        self.shared_memory.close();
        tracing::info!("Heartbeat sender stopped");
    }

    /// Check if the heartbeat sender is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the process ID
    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    /// Manually send a heartbeat (useful for testing)
    pub fn send_heartbeat(&self) -> Result<(), SharedMemoryError> {
        if !self.shared_memory.is_open() {
            return Err(SharedMemoryError::WriteFailed(
                "Shared memory not open".to_string(),
            ));
        }

        let mut data = HeartbeatData::new(self.process_id);
        data.update();
        self.shared_memory.write(&data)
    }
}

impl Default for HeartbeatSender {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for HeartbeatSender {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_sender_creation() {
        let sender = HeartbeatSender::new();
        assert!(!sender.is_running());
        assert!(sender.process_id() > 0);
    }

    #[test]
    fn test_heartbeat_sender_start_stop() {
        let mut sender = HeartbeatSender::new();
        
        // Start should succeed
        assert!(sender.start().is_ok());
        assert!(sender.is_running());
        
        // Starting again should be idempotent
        assert!(sender.start().is_ok());
        
        // Stop
        sender.stop();
        assert!(!sender.is_running());
    }
}
