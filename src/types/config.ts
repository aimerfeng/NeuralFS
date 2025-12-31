/**
 * Configuration types for NeuralFS settings
 */

/** Cloud configuration */
export interface CloudConfig {
  enabled: boolean;
  endpoint: string | null;
  api_key_set: boolean;
  monthly_cost_limit: number;
  requests_per_minute: number;
  model: string;
  provider: string;
}

/** Performance configuration */
export interface PerformanceConfig {
  max_vram_mb: number;
  indexing_threads: number;
  embedding_batch_size: number;
  enable_cuda: boolean;
  fast_inference_mode: boolean;
}

/** Privacy configuration */
export interface PrivacyConfig {
  privacy_mode: boolean;
  excluded_directories: string[];
  excluded_patterns: string[];
  enable_telemetry: boolean;
}

/** UI configuration */
export interface UIConfig {
  theme: 'light' | 'dark' | 'system';
  language: string;
  enable_animations: boolean;
  show_extensions: boolean;
  default_view: 'grid' | 'list';
  thumbnail_size: 'small' | 'medium' | 'large';
}

/** Full application configuration */
export interface AppConfig {
  version: number;
  monitored_directories: string[];
  cloud: CloudConfig;
  performance: PerformanceConfig;
  privacy: PrivacyConfig;
  ui: UIConfig;
  last_modified: string;
}

/** Cloud status information */
export interface CloudStatus {
  enabled: boolean;
  connected: boolean;
  current_month_usage: number;
  monthly_limit: number;
  remaining_budget: number;
  requests_this_minute: number;
  requests_per_minute_limit: number;
  last_api_call: string | null;
  error: string | null;
}

/** Configuration update request */
export interface UpdateConfigRequest {
  monitored_directories?: string[];
  cloud?: Partial<CloudConfig> & { api_key?: string };
  performance?: PerformanceConfig;
  privacy?: PrivacyConfig;
  ui?: UIConfig;
}

/** Configuration operation result */
export interface ConfigOperationResult {
  success: boolean;
  message: string;
  config: AppConfig | null;
}
