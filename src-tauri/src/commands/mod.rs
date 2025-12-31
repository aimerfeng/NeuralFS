//! Tauri IPC Commands for NeuralFS
//!
//! This module provides all Tauri commands for frontend-backend communication.
//! Commands are organized by functionality:
//! - Search commands (search_files, get_search_suggestions)
//! - Tag commands (get_tags, add_tag, remove_tag, confirm_tag, reject_tag)
//! - Relation commands (get_relations, confirm_relation, reject_relation, block_relation)
//! - Config commands (get_config, set_config, get_cloud_status, set_cloud_enabled)
//! - Status commands (get_index_status, get_system_status, get_dead_letter_tasks, retry_dead_letter)
//! - Protocol commands (get_session_token, build_thumbnail_url, build_preview_url, build_file_url)
//! - Onboarding commands (check_first_launch, get_suggested_directories, save_onboarding_config, etc.)

pub mod search;
pub mod tags;
pub mod relations;
pub mod config;
pub mod status;
pub mod protocol;
pub mod onboarding;

pub use search::*;
pub use tags::*;
pub use relations::*;
pub use config::*;
pub use status::*;
pub use protocol::*;
pub use onboarding::*;
