-- NeuralFS Initial Database Schema
-- Version: 001
-- Description: Creates all core tables for file indexing, tags, relations, and sessions

-- Enable foreign keys
PRAGMA foreign_keys = ON;

-- ============================================================================
-- Files Table
-- Stores metadata for all indexed files
-- ============================================================================
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    extension TEXT,
    file_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    indexed_at TEXT NOT NULL,
    last_accessed_at TEXT,
    index_status TEXT NOT NULL DEFAULT 'Pending',
    privacy_level TEXT NOT NULL DEFAULT 'Normal',
    is_excluded INTEGER NOT NULL DEFAULT 0
);

-- Indexes for files table
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
CREATE INDEX IF NOT EXISTS idx_files_filename ON files(filename);
CREATE INDEX IF NOT EXISTS idx_files_file_type ON files(file_type);
CREATE INDEX IF NOT EXISTS idx_files_index_status ON files(index_status);
CREATE INDEX IF NOT EXISTS idx_files_modified_at ON files(modified_at);
CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files(content_hash);

-- ============================================================================
-- Content Chunks Table
-- Stores semantic segments of documents after content splitting
-- ============================================================================
CREATE TABLE IF NOT EXISTS content_chunks (
    id TEXT PRIMARY KEY NOT NULL,
    file_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    chunk_type TEXT NOT NULL,
    content TEXT NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    start_line INTEGER,
    end_line INTEGER,
    page_number INTEGER,
    bounding_box TEXT, -- JSON: [x, y, width, height] normalized to 0-1
    vector_id INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Indexes for content_chunks table
CREATE INDEX IF NOT EXISTS idx_chunks_file_id ON content_chunks(file_id);
CREATE INDEX IF NOT EXISTS idx_chunks_vector_id ON content_chunks(vector_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_chunks_file_chunk ON content_chunks(file_id, chunk_index);

-- ============================================================================
-- Tags Table
-- Stores tag definitions with hierarchy support
-- ============================================================================
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT, -- JSON: {"zh": "工作", "en": "Work"}
    parent_id TEXT,
    tag_type TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#808080',
    icon TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    usage_count INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (parent_id) REFERENCES tags(id) ON DELETE SET NULL
);

-- Indexes for tags table
CREATE INDEX IF NOT EXISTS idx_tags_parent_id ON tags(parent_id);
CREATE INDEX IF NOT EXISTS idx_tags_tag_type ON tags(tag_type);
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);

-- ============================================================================
-- File Tags Table
-- Many-to-many relationship between files and tags
-- ============================================================================
CREATE TABLE IF NOT EXISTS file_tags (
    id TEXT PRIMARY KEY NOT NULL,
    file_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence REAL,
    is_confirmed INTEGER NOT NULL DEFAULT 0,
    is_rejected INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    user_action_at TEXT,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

-- Indexes for file_tags table
CREATE UNIQUE INDEX IF NOT EXISTS idx_file_tags_unique ON file_tags(file_id, tag_id);
CREATE INDEX IF NOT EXISTS idx_file_tags_file_id ON file_tags(file_id);
CREATE INDEX IF NOT EXISTS idx_file_tags_tag_id ON file_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_file_tags_source ON file_tags(source);

-- ============================================================================
-- File Relations Table
-- Stores relationships between files (logic chain associations)
-- ============================================================================
CREATE TABLE IF NOT EXISTS file_relations (
    id TEXT PRIMARY KEY NOT NULL,
    source_file_id TEXT NOT NULL,
    target_file_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    strength REAL NOT NULL,
    source TEXT NOT NULL,
    user_feedback TEXT NOT NULL DEFAULT 'None', -- JSON for complex feedback states
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    user_action_at TEXT,
    FOREIGN KEY (source_file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (target_file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Indexes for file_relations table
CREATE UNIQUE INDEX IF NOT EXISTS idx_relations_unique ON file_relations(source_file_id, target_file_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_relations_source ON file_relations(source_file_id);
CREATE INDEX IF NOT EXISTS idx_relations_target ON file_relations(target_file_id);
CREATE INDEX IF NOT EXISTS idx_relations_type ON file_relations(relation_type);

-- ============================================================================
-- Relation Block Rules Table
-- Prevents AI from regenerating rejected relations
-- ============================================================================
CREATE TABLE IF NOT EXISTS relation_block_rules (
    id TEXT PRIMARY KEY NOT NULL,
    rule_type TEXT NOT NULL,
    rule_detail TEXT NOT NULL, -- JSON containing rule specifics
    created_at TEXT NOT NULL,
    expires_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1
);

-- Indexes for relation_block_rules table
CREATE INDEX IF NOT EXISTS idx_block_rules_type ON relation_block_rules(rule_type);
CREATE INDEX IF NOT EXISTS idx_block_rules_active ON relation_block_rules(is_active);

-- ============================================================================
-- Cloud Usage Table
-- Tracks cloud API usage for cost management
-- ============================================================================
CREATE TABLE IF NOT EXISTS cloud_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    tokens INTEGER NOT NULL,
    cost REAL NOT NULL,
    model TEXT,
    request_type TEXT
);

-- Indexes for cloud_usage table
CREATE INDEX IF NOT EXISTS idx_cloud_usage_timestamp ON cloud_usage(timestamp);
CREATE INDEX IF NOT EXISTS idx_cloud_usage_model ON cloud_usage(model);

-- ============================================================================
-- Sessions Table
-- Tracks user sessions for logic chain associations
-- ============================================================================
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT
);

-- Indexes for sessions table
CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);

-- ============================================================================
-- Session File Access Table
-- Records file access within sessions
-- ============================================================================
CREATE TABLE IF NOT EXISTS session_file_access (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    file_id TEXT NOT NULL,
    accessed_at TEXT NOT NULL,
    access_type TEXT NOT NULL, -- 'open', 'preview', 'search_result'
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Indexes for session_file_access table
CREATE INDEX IF NOT EXISTS idx_session_access_session ON session_file_access(session_id);
CREATE INDEX IF NOT EXISTS idx_session_access_file ON session_file_access(file_id);
CREATE INDEX IF NOT EXISTS idx_session_access_time ON session_file_access(accessed_at);

-- ============================================================================
-- Schema Migrations Table
-- Tracks applied migrations for version control
-- ============================================================================
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    checksum TEXT NOT NULL
);

-- Insert initial migration record
INSERT OR IGNORE INTO schema_migrations (version, name, applied_at, checksum)
VALUES (1, '001_initial_schema', datetime('now'), 'initial');
