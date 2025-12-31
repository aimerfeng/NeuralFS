/**
 * OnboardingWizard Component
 * 
 * First-launch onboarding wizard that guides users through:
 * 1. Welcome screen
 * 2. Directory selection
 * 3. Cloud configuration
 * 4. Initial scan progress
 * 5. Completion
 * 
 * **Validates: Requirements 17.1, 17.2, 17.3, 17.4, 17.5**
 */

import { createSignal, createEffect, onCleanup, Show, For } from 'solid-js';
import type {
  OnboardingStep,
  CloudSetupConfig,
  ScanProgress,
  DirectorySuggestion,
} from '../../types/onboarding';
import {
  getSuggestedDirectories,
  browseDirectory,
  saveOnboardingConfig,
  startInitialScan,
  getScanProgress,
  completeOnboarding,
} from '../../api/tauri';
import './OnboardingWizard.css';

export interface OnboardingWizardProps {
  /** Callback when onboarding is complete */
  onComplete: () => void;
}

export function OnboardingWizard(props: OnboardingWizardProps) {
  // State
  const [currentStep, setCurrentStep] = createSignal<OnboardingStep>('welcome');
  const [selectedDirectories, setSelectedDirectories] = createSignal<string[]>([]);
  const [suggestedDirectories, setSuggestedDirectories] = createSignal<DirectorySuggestion[]>([]);
  const [cloudConfig, setCloudConfig] = createSignal<CloudSetupConfig>({
    enabled: false,
    model: 'gpt-4o-mini',
    monthlyCostLimit: 10,
  });
  const [scanProgress, setScanProgress] = createSignal<ScanProgress>({
    isScanning: false,
    totalFiles: 0,
    processedFiles: 0,
    isComplete: false,
  });
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Load suggested directories on mount
  createEffect(async () => {
    if (currentStep() === 'directories' && suggestedDirectories().length === 0) {
      try {
        const suggestions = await getSuggestedDirectories();
        setSuggestedDirectories(suggestions);
        // Pre-select recommended directories
        const recommended = suggestions
          .filter(s => s.recommended)
          .map(s => s.path);
        setSelectedDirectories(recommended);
      } catch (err) {
        console.error('Failed to load suggested directories:', err);
        // Provide fallback suggestions
        setSuggestedDirectories([
          {
            path: getDefaultDownloadsPath(),
            name: '下载',
            description: '浏览器下载和应用导出的文件',
            recommended: true,
            icon: '📥',
          },
          {
            path: getDefaultDesktopPath(),
            name: '桌面',
            description: '桌面上的文件和快捷方式',
            recommended: true,
            icon: '🖥️',
          },
          {
            path: getDefaultDocumentsPath(),
            name: '文档',
            description: '个人文档和工作文件',
            recommended: false,
            icon: '📄',
          },
        ]);
      }
    }
  });

  // Poll scan progress when scanning
  let progressInterval: number | undefined;
  
  createEffect(() => {
    if (currentStep() === 'scanning' && scanProgress().isScanning) {
      progressInterval = window.setInterval(async () => {
        try {
          const progress = await getScanProgress();
          setScanProgress(progress);
          
          if (progress.isComplete) {
            window.clearInterval(progressInterval);
            // Auto-advance after a short delay
            setTimeout(() => setCurrentStep('complete'), 1000);
          }
        } catch (err) {
          console.error('Failed to get scan progress:', err);
        }
      }, 500);
    }
  });

  onCleanup(() => {
    if (progressInterval) {
      window.clearInterval(progressInterval);
    }
  });

  // Navigation handlers
  const goToStep = (step: OnboardingStep) => {
    setError(null);
    setCurrentStep(step);
  };

  const handleNext = async () => {
    setError(null);
    
    switch (currentStep()) {
      case 'welcome':
        goToStep('directories');
        break;
        
      case 'directories':
        if (selectedDirectories().length === 0) {
          setError('请至少选择一个目录进行监控');
          return;
        }
        goToStep('cloud');
        break;
        
      case 'cloud':
        setIsLoading(true);
        try {
          // Save configuration
          const result = await saveOnboardingConfig(
            selectedDirectories(),
            cloudConfig()
          );
          
          if (!result.success) {
            setError(result.error || '保存配置失败');
            return;
          }
          
          // Start initial scan
          await startInitialScan(selectedDirectories());
          setScanProgress({
            isScanning: true,
            totalFiles: 0,
            processedFiles: 0,
            isComplete: false,
          });
          goToStep('scanning');
        } catch (err) {
          setError('启动扫描失败: ' + String(err));
        } finally {
          setIsLoading(false);
        }
        break;
        
      case 'scanning':
        // Skip waiting for scan
        goToStep('complete');
        break;
        
      case 'complete':
        await completeOnboarding();
        props.onComplete();
        break;
    }
  };

  const handleBack = () => {
    switch (currentStep()) {
      case 'directories':
        goToStep('welcome');
        break;
      case 'cloud':
        goToStep('directories');
        break;
      case 'scanning':
        // Can't go back during scanning
        break;
      case 'complete':
        // Can't go back after completion
        break;
    }
  };

  // Directory selection handlers
  const toggleDirectory = (path: string) => {
    const current = selectedDirectories();
    if (current.includes(path)) {
      setSelectedDirectories(current.filter(p => p !== path));
    } else {
      setSelectedDirectories([...current, path]);
    }
  };

  const handleBrowseDirectory = async () => {
    try {
      const path = await browseDirectory();
      if (path && !selectedDirectories().includes(path)) {
        setSelectedDirectories([...selectedDirectories(), path]);
      }
    } catch (err) {
      console.error('Failed to browse directory:', err);
    }
  };

  // Cloud config handlers
  const updateCloudConfig = (updates: Partial<CloudSetupConfig>) => {
    setCloudConfig({ ...cloudConfig(), ...updates });
  };

  // Progress calculation
  const progressPercent = () => {
    const progress = scanProgress();
    if (progress.totalFiles === 0) return 0;
    return Math.round((progress.processedFiles / progress.totalFiles) * 100);
  };

  return (
    <div class="onboarding-wizard">
      <div class="wizard-container">
        {/* Progress indicator */}
        <div class="wizard-progress">
          <div class="progress-steps">
            <For each={['welcome', 'directories', 'cloud', 'scanning', 'complete'] as OnboardingStep[]}>
              {(step, index) => (
                <div
                  class={`progress-step ${currentStep() === step ? 'active' : ''} ${
                    getStepIndex(currentStep()) > index() ? 'completed' : ''
                  }`}
                >
                  <div class="step-dot">{index() + 1}</div>
                  <span class="step-label">{getStepLabel(step)}</span>
                </div>
              )}
            </For>
          </div>
        </div>

        {/* Step content */}
        <div class="wizard-content">
          {/* Welcome Step */}
          <Show when={currentStep() === 'welcome'}>
            <div class="step-welcome">
              <div class="welcome-icon">🧠</div>
              <h1>欢迎使用 NeuralFS</h1>
              <p class="welcome-subtitle">
                AI 驱动的智能文件管理系统
              </p>
              <div class="welcome-features">
                <div class="feature-item">
                  <span class="feature-icon">🔍</span>
                  <div class="feature-text">
                    <strong>语义搜索</strong>
                    <span>用自然语言描述找到任何文件</span>
                  </div>
                </div>
                <div class="feature-item">
                  <span class="feature-icon">🏷️</span>
                  <div class="feature-text">
                    <strong>智能标签</strong>
                    <span>AI 自动分类和组织您的文件</span>
                  </div>
                </div>
                <div class="feature-item">
                  <span class="feature-icon">🔗</span>
                  <div class="feature-text">
                    <strong>逻辑链条</strong>
                    <span>发现文件之间的隐藏关联</span>
                  </div>
                </div>
              </div>
            </div>
          </Show>

          {/* Directory Selection Step */}
          <Show when={currentStep() === 'directories'}>
            <div class="step-directories">
              <h2>选择监控目录</h2>
              <p class="step-description">
                选择您希望 NeuralFS 监控和索引的目录。您可以随时在设置中修改。
              </p>
              
              <div class="directory-list">
                <For each={suggestedDirectories()}>
                  {(dir) => (
                    <div
                      class={`directory-item ${selectedDirectories().includes(dir.path) ? 'selected' : ''}`}
                      onClick={() => toggleDirectory(dir.path)}
                    >
                      <div class="directory-checkbox">
                        <Show when={selectedDirectories().includes(dir.path)}>
                          ✓
                        </Show>
                      </div>
                      <span class="directory-icon">{dir.icon}</span>
                      <div class="directory-info">
                        <div class="directory-name">
                          {dir.name}
                          <Show when={dir.recommended}>
                            <span class="recommended-badge">推荐</span>
                          </Show>
                        </div>
                        <div class="directory-path">{dir.path}</div>
                        <div class="directory-description">{dir.description}</div>
                      </div>
                    </div>
                  )}
                </For>
              </div>

              <button class="browse-button" onClick={handleBrowseDirectory}>
                <span>📁</span> 浏览其他目录...
              </button>

              <Show when={selectedDirectories().length > 0}>
                <div class="selected-summary">
                  已选择 {selectedDirectories().length} 个目录
                </div>
              </Show>
            </div>
          </Show>

          {/* Cloud Configuration Step */}
          <Show when={currentStep() === 'cloud'}>
            <div class="step-cloud">
              <h2>云端 AI 配置</h2>
              <p class="step-description">
                启用云端 AI 可以获得更精准的搜索结果和智能建议。您的文件内容不会上传，仅发送匿名化的查询。
              </p>

              <div class="cloud-toggle">
                <label class="toggle-label">
                  <input
                    type="checkbox"
                    checked={cloudConfig().enabled}
                    onChange={(e) => updateCloudConfig({ enabled: e.currentTarget.checked })}
                  />
                  <span class="toggle-switch"></span>
                  <span class="toggle-text">启用云端 AI 增强</span>
                </label>
              </div>

              <Show when={cloudConfig().enabled}>
                <div class="cloud-options">
                  <div class="option-group">
                    <label>AI 模型</label>
                    <select
                      value={cloudConfig().model}
                      onChange={(e) => updateCloudConfig({ model: e.currentTarget.value })}
                    >
                      <option value="gpt-4o-mini">GPT-4o Mini (推荐)</option>
                      <option value="claude-haiku">Claude Haiku</option>
                    </select>
                    <span class="option-hint">GPT-4o Mini 提供最佳性价比</span>
                  </div>

                  <div class="option-group">
                    <label>API 密钥 (可选)</label>
                    <input
                      type="password"
                      placeholder="sk-..."
                      value={cloudConfig().apiKey || ''}
                      onInput={(e) => updateCloudConfig({ apiKey: e.currentTarget.value })}
                    />
                    <span class="option-hint">留空将使用内置配额</span>
                  </div>

                  <div class="option-group">
                    <label>每月费用限制</label>
                    <div class="cost-slider">
                      <input
                        type="range"
                        min="1"
                        max="50"
                        value={cloudConfig().monthlyCostLimit}
                        onInput={(e) => updateCloudConfig({ monthlyCostLimit: Number(e.currentTarget.value) })}
                      />
                      <span class="cost-value">${cloudConfig().monthlyCostLimit}</span>
                    </div>
                    <span class="option-hint">达到限制后将自动切换到本地模式</span>
                  </div>
                </div>
              </Show>

              <div class="privacy-notice">
                <span class="privacy-icon">🔒</span>
                <div class="privacy-text">
                  <strong>隐私保护</strong>
                  <p>
                    NeuralFS 不会上传您的文件内容。云端 AI 仅接收匿名化的搜索查询和元数据摘要。
                    您可以随时在设置中禁用云端功能。
                  </p>
                </div>
              </div>
            </div>
          </Show>

          {/* Scanning Step */}
          <Show when={currentStep() === 'scanning'}>
            <div class="step-scanning">
              <h2>正在扫描文件</h2>
              <p class="step-description">
                NeuralFS 正在扫描您选择的目录并建立索引。这可能需要几分钟时间。
              </p>

              <div class="scan-progress">
                <div class="progress-bar">
                  <div
                    class="progress-fill"
                    style={{ width: `${progressPercent()}%` }}
                  ></div>
                </div>
                <div class="progress-stats">
                  <span>{scanProgress().processedFiles.toLocaleString()} / {scanProgress().totalFiles.toLocaleString()} 文件</span>
                  <span>{progressPercent()}%</span>
                </div>
              </div>

              <Show when={scanProgress().currentFile}>
                <div class="current-file">
                  <span class="file-icon">📄</span>
                  <span class="file-path" title={scanProgress().currentFile}>
                    {truncatePath(scanProgress().currentFile!, 60)}
                  </span>
                </div>
              </Show>

              <Show when={scanProgress().estimatedTimeRemaining && scanProgress().estimatedTimeRemaining! > 0}>
                <div class="time-remaining">
                  预计剩余时间: {formatTime(scanProgress().estimatedTimeRemaining!)}
                </div>
              </Show>

              <Show when={scanProgress().isComplete}>
                <div class="scan-complete-notice">
                  <span class="complete-icon">✅</span>
                  <span>扫描完成！</span>
                </div>
              </Show>

              <Show when={!scanProgress().isComplete}>
                <p class="scan-hint">
                  您可以跳过等待，开始使用 NeuralFS。扫描将在后台继续进行。
                </p>
              </Show>
            </div>
          </Show>

          {/* Complete Step */}
          <Show when={currentStep() === 'complete'}>
            <div class="step-complete">
              <div class="complete-icon">🎉</div>
              <h2>设置完成！</h2>
              <p class="step-description">
                NeuralFS 已准备就绪。开始探索您的文件吧！
              </p>

              <div class="complete-summary">
                <div class="summary-item">
                  <span class="summary-icon">📁</span>
                  <span>{selectedDirectories().length} 个监控目录</span>
                </div>
                <div class="summary-item">
                  <span class="summary-icon">{cloudConfig().enabled ? '☁️' : '💻'}</span>
                  <span>{cloudConfig().enabled ? '云端 AI 已启用' : '仅本地模式'}</span>
                </div>
                <Show when={scanProgress().totalFiles > 0}>
                  <div class="summary-item">
                    <span class="summary-icon">📊</span>
                    <span>已索引 {scanProgress().processedFiles} 个文件</span>
                  </div>
                </Show>
              </div>

              <div class="tips">
                <h3>快速提示</h3>
                <ul>
                  <li>使用自然语言搜索，如 "上周的会议记录"</li>
                  <li>点击标签可以快速筛选文件</li>
                  <li>查看文件关联发现相关内容</li>
                </ul>
              </div>
            </div>
          </Show>
        </div>

        {/* Error message */}
        <Show when={error()}>
          <div class="wizard-error">
            <span class="error-icon">⚠️</span>
            <span>{error()}</span>
          </div>
        </Show>

        {/* Navigation buttons */}
        <div class="wizard-actions">
          <Show when={currentStep() !== 'welcome' && currentStep() !== 'scanning' && currentStep() !== 'complete'}>
            <button class="btn-secondary" onClick={handleBack}>
              返回
            </button>
          </Show>
          
          <button
            class="btn-primary"
            onClick={handleNext}
            disabled={isLoading()}
          >
            <Show when={isLoading()}>
              <span class="loading-spinner">⏳</span>
            </Show>
            {getNextButtonText(currentStep(), scanProgress().isScanning)}
          </button>
        </div>
      </div>
    </div>
  );
}

// Helper functions
function getStepIndex(step: OnboardingStep): number {
  const steps: OnboardingStep[] = ['welcome', 'directories', 'cloud', 'scanning', 'complete'];
  return steps.indexOf(step);
}

function getStepLabel(step: OnboardingStep): string {
  const labels: Record<OnboardingStep, string> = {
    welcome: '欢迎',
    directories: '目录',
    cloud: '云端',
    scanning: '扫描',
    complete: '完成',
  };
  return labels[step];
}

function getNextButtonText(step: OnboardingStep, isScanning: boolean): string {
  switch (step) {
    case 'welcome':
      return '开始设置';
    case 'directories':
      return '下一步';
    case 'cloud':
      return '开始扫描';
    case 'scanning':
      return isScanning ? '跳过等待' : '继续';
    case 'complete':
      return '开始使用';
    default:
      return '下一步';
  }
}

function formatTime(seconds: number): string {
  if (seconds < 60) {
    return `${seconds} 秒`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  if (minutes < 60) {
    return `${minutes} 分 ${remainingSeconds} 秒`;
  }
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return `${hours} 小时 ${remainingMinutes} 分`;
}

function truncatePath(path: string, maxLength: number): string {
  if (path.length <= maxLength) {
    return path;
  }
  
  // Try to keep the filename visible
  const parts = path.split(/[/\\]/);
  const filename = parts[parts.length - 1];
  
  if (filename.length >= maxLength - 3) {
    return '...' + filename.slice(-(maxLength - 3));
  }
  
  const availableForPath = maxLength - filename.length - 4; // 4 for ".../"
  const pathPart = parts.slice(0, -1).join('/');
  
  if (pathPart.length <= availableForPath) {
    return path;
  }
  
  return '...' + pathPart.slice(-availableForPath) + '/' + filename;
}

function getDefaultDownloadsPath(): string {
  // Platform-specific default paths
  if (typeof window !== 'undefined') {
    return 'C:\\Users\\User\\Downloads';
  }
  return '~/Downloads';
}

function getDefaultDesktopPath(): string {
  if (typeof window !== 'undefined') {
    return 'C:\\Users\\User\\Desktop';
  }
  return '~/Desktop';
}

function getDefaultDocumentsPath(): string {
  if (typeof window !== 'undefined') {
    return 'C:\\Users\\User\\Documents';
  }
  return '~/Documents';
}

export default OnboardingWizard;
