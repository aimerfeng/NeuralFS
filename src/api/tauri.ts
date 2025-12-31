/**
 * Tauri IPC API wrapper
 */

import { invoke } from '@tauri-apps/api/tauri';
import type {
  SearchRequest,
  SearchResponse,
  Tag,
  FileTagRelation,
  TagSuggestion,
  TagCommand,
  FileRelation,
  RelationCommand,
  RelationBlockRule,
  RelationGraph,
} from '../types';

// Session token for secure asset requests
let sessionToken: string | null = null;

/**
 * Initialize session token for secure asset streaming
 */
export async function initSessionToken(): Promise<string> {
  if (!sessionToken) {
    sessionToken = await invoke<string>('get_session_token');
  }
  return sessionToken;
}

/**
 * Get asset URL with session token
 */
export function getAssetUrl(path: string): string {
  const token = sessionToken || '';
  return `nfs://localhost/${path}?token=${encodeURIComponent(token)}`;
}

// Search API
export async function searchFiles(request: SearchRequest): Promise<SearchResponse> {
  return invoke<SearchResponse>('search_files', { request });
}

export async function getSearchSuggestions(query: string): Promise<string[]> {
  return invoke<string[]>('get_search_suggestions', { query });
}

// Tag API
export async function getTags(): Promise<Tag[]> {
  return invoke<Tag[]>('get_tags');
}

export async function getFileTags(fileId: string): Promise<FileTagRelation[]> {
  return invoke<FileTagRelation[]>('get_file_tags', { fileId });
}

export async function suggestTags(fileId: string): Promise<TagSuggestion[]> {
  return invoke<TagSuggestion[]>('suggest_tags', { fileId });
}

export async function executeTagCommand(command: TagCommand): Promise<void> {
  return invoke<void>('execute_tag_command', { command });
}

export async function confirmTag(fileId: string, tagId: string): Promise<void> {
  return invoke<void>('confirm_tag', { fileId, tagId });
}

export async function rejectTag(fileId: string, tagId: string): Promise<void> {
  return invoke<void>('reject_tag', { fileId, tagId });
}

export async function addTag(fileId: string, tagId: string): Promise<void> {
  return invoke<void>('add_tag', { fileId, tagId });
}

export async function removeTag(fileId: string, tagId: string): Promise<void> {
  return invoke<void>('remove_tag', { fileId, tagId });
}

// Relation API
export async function getRelations(fileId: string): Promise<FileRelation[]> {
  return invoke<FileRelation[]>('get_relations', { fileId });
}

export async function getRelationGraph(fileId: string, depth?: number): Promise<RelationGraph> {
  return invoke<RelationGraph>('get_relation_graph', { fileId, depth: depth ?? 2 });
}

export async function executeRelationCommand(command: RelationCommand): Promise<void> {
  return invoke<void>('execute_relation_command', { command });
}

export async function confirmRelation(relationId: string): Promise<void> {
  return invoke<void>('confirm_relation', { relationId });
}

export async function rejectRelation(
  relationId: string,
  reason?: string,
  blockSimilar?: boolean
): Promise<void> {
  return invoke<void>('reject_relation', { relationId, reason, blockSimilar: blockSimilar ?? false });
}

export async function blockRelation(
  relationId: string,
  blockScope?: { type: string; [key: string]: unknown }
): Promise<void> {
  return invoke<void>('block_relation', { relationId, blockScope });
}

export async function getBlockRules(fileId?: string): Promise<RelationBlockRule[]> {
  return invoke<RelationBlockRule[]>('get_block_rules', { fileId });
}

export async function removeBlockRule(ruleId: string): Promise<void> {
  return invoke<void>('remove_block_rule', { ruleId });
}


// Onboarding API
import type {
  OnboardingState,
  CloudSetupConfig,
  ScanProgress,
  DirectorySuggestion,
  OnboardingResult,
} from '../types/onboarding';

/**
 * Check if this is the first launch (onboarding needed)
 */
export async function checkFirstLaunch(): Promise<boolean> {
  return invoke<boolean>('check_first_launch');
}

/**
 * Get suggested directories for monitoring
 */
export async function getSuggestedDirectories(): Promise<DirectorySuggestion[]> {
  return invoke<DirectorySuggestion[]>('get_suggested_directories');
}

/**
 * Browse for a directory using native file picker
 */
export async function browseDirectory(): Promise<string | null> {
  return invoke<string | null>('browse_directory');
}

/**
 * Save onboarding configuration
 */
export async function saveOnboardingConfig(
  directories: string[],
  cloudConfig: CloudSetupConfig
): Promise<OnboardingResult> {
  return invoke<OnboardingResult>('save_onboarding_config', {
    directories,
    cloudConfig,
  });
}

/**
 * Start initial directory scan
 */
export async function startInitialScan(directories: string[]): Promise<void> {
  return invoke<void>('start_initial_scan', { directories });
}

/**
 * Get current scan progress
 */
export async function getScanProgress(): Promise<ScanProgress> {
  return invoke<ScanProgress>('get_scan_progress');
}

/**
 * Complete onboarding
 */
export async function completeOnboarding(): Promise<void> {
  return invoke<void>('complete_onboarding');
}


// Config API
import type {
  AppConfig,
  CloudStatus,
  UpdateConfigRequest,
  ConfigOperationResult,
} from '../types/config';

/**
 * Initialize configuration store
 */
export async function initConfig(): Promise<void> {
  return invoke<void>('init_config');
}

/**
 * Get current application configuration
 */
export async function getConfig(): Promise<AppConfig> {
  return invoke<AppConfig>('get_config');
}

/**
 * Update application configuration
 */
export async function setConfig(request: UpdateConfigRequest): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('set_config', { request });
}

/**
 * Get cloud service status
 */
export async function getCloudStatus(): Promise<CloudStatus> {
  return invoke<CloudStatus>('get_cloud_status');
}

/**
 * Enable or disable cloud features
 */
export async function setCloudEnabled(enabled: boolean): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('set_cloud_enabled', { enabled });
}

/**
 * Add a monitored directory
 */
export async function addMonitoredDirectory(path: string): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('add_monitored_directory', { path });
}

/**
 * Remove a monitored directory
 */
export async function removeMonitoredDirectory(path: string): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('remove_monitored_directory', { path });
}

/**
 * Set UI theme
 */
export async function setTheme(theme: string): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('set_theme', { theme });
}

/**
 * Set language
 */
export async function setLanguage(language: string): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('set_language', { language });
}

/**
 * Export configuration to file
 */
export async function exportConfig(path: string): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('export_config', { path });
}

/**
 * Import configuration from file
 */
export async function importConfig(path: string): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('import_config', { path });
}

/**
 * Reset configuration to defaults
 */
export async function resetConfig(): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('reset_config');
}

/**
 * List available configuration backups
 */
export async function listConfigBackups(): Promise<string[]> {
  return invoke<string[]>('list_config_backups');
}

/**
 * Restore configuration from backup
 */
export async function restoreConfigBackup(backupPath: string): Promise<ConfigOperationResult> {
  return invoke<ConfigOperationResult>('restore_config_backup', { backupPath });
}
