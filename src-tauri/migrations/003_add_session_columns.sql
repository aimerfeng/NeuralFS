-- NeuralFS Session Enhancement Migration
-- Version: 003
-- Description: Adds missing columns to sessions table and creates session_events table

-- Add missing columns to sessions table
ALTER TABLE sessions ADD COLUMN last_activity_at TEXT;
ALTER TABLE sessions ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1;

-- Update existing sessions to have last_activity_at
UPDATE sessions SET last_activity_at = COALESCE(ended_at, started_at) WHERE last_activity_at IS NULL;

-- Create session_events table for detailed event tracking
CREATE TABLE IF NOT EXISTS session_events (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    file_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Indexes for session_events table
CREATE INDEX IF NOT EXISTS idx_session_events_session ON session_events(session_id);
CREATE INDEX IF NOT EXISTS idx_session_events_file ON session_events(file_id);
CREATE INDEX IF NOT EXISTS idx_session_events_timestamp ON session_events(timestamp);

-- Insert migration record
INSERT OR IGNORE INTO schema_migrations (version, name, applied_at, checksum)
VALUES (3, '003_add_session_columns', datetime('now'), 'session_enhancement');
