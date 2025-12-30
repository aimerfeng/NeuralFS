-- NeuralFS Migration: Add FileID for rename tracking
-- Version: 002
-- Description: Adds file_id column to files table for tracking files across renames

-- Add file_id column to files table
-- This stores the platform-specific file identifier (Windows: volume:high:low, Unix: device:inode)
ALTER TABLE files ADD COLUMN file_id TEXT;

-- Create index for efficient FileID lookups during reconciliation
CREATE INDEX IF NOT EXISTS idx_files_file_id ON files(file_id);

-- Update migration record
INSERT INTO schema_migrations (version, name, applied_at, checksum)
VALUES (2, '002_add_file_id', datetime('now'), 'add_file_id');
