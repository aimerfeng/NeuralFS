/**
 * Onboarding Types for NeuralFS
 */

/**
 * Onboarding step identifiers
 */
export type OnboardingStep = 'welcome' | 'directories' | 'cloud' | 'scanning' | 'complete';

/**
 * Onboarding state
 */
export interface OnboardingState {
  /** Current step */
  currentStep: OnboardingStep;
  /** Selected directories to monitor */
  selectedDirectories: string[];
  /** Cloud configuration */
  cloudConfig: CloudSetupConfig;
  /** Initial scan progress */
  scanProgress: ScanProgress;
  /** Whether onboarding is complete */
  isComplete: boolean;
}

/**
 * Cloud setup configuration
 */
export interface CloudSetupConfig {
  /** Whether to enable cloud features */
  enabled: boolean;
  /** API key (if provided) */
  apiKey?: string;
  /** Selected cloud model */
  model: string;
  /** Monthly cost limit */
  monthlyCostLimit: number;
}

/**
 * Initial scan progress
 */
export interface ScanProgress {
  /** Whether scanning is in progress */
  isScanning: boolean;
  /** Total files discovered */
  totalFiles: number;
  /** Files processed */
  processedFiles: number;
  /** Current file being processed */
  currentFile?: string;
  /** Estimated time remaining in seconds */
  estimatedTimeRemaining?: number;
  /** Whether scan is complete */
  isComplete: boolean;
}

/**
 * Directory suggestion for onboarding
 */
export interface DirectorySuggestion {
  /** Directory path */
  path: string;
  /** Display name */
  name: string;
  /** Description */
  description: string;
  /** Whether it's recommended */
  recommended: boolean;
  /** Icon emoji */
  icon: string;
}

/**
 * Onboarding completion result
 */
export interface OnboardingResult {
  /** Whether onboarding was successful */
  success: boolean;
  /** Error message if any */
  error?: string;
  /** Configuration that was applied */
  config?: {
    monitoredDirectories: string[];
    cloudEnabled: boolean;
  };
}
