/**
 * Settings Component for NeuralFS
 * 
 * Provides configuration interface for:
 * - Monitored directories
 * - Cloud API settings
 * - Theme and UI preferences
 * 
 * **Validates: Requirements 15.1, 15.2, 15.3**
 */

import { createSignal, createEffect, onMount, Show, For } from 'solid-js';
import type { AppConfig, CloudStatus, UIConfig } from '../../types/config';
import {
  getConfig,
  setConfig,
  getCloudStatus,
  setCloudEnabled,
  addMonitoredDirectory,
  removeMonitoredDirectory,
  setTheme,
  browseDirectory,
  resetConfig,
} from '../../api/tauri';
import './Settings.css';

export interface SettingsProps {
  /** Whether the settings panel is open */
  isOpen: boolean;
  /** Callback when settings panel is closed */
  onClose: () => void;
  /** Callback when settings are saved */
  onSave?: (config: AppConfig) => void;
}

type SettingsTab = 'directories' | 'cloud' | 'appearance' | 'privacy' | 'advanced';

export function Settings(props: SettingsProps) {
  const [config, setConfigState] = createSignal<AppConfig | null>(null);
  const [cloudStatus, setCloudStatus] = createSignal<CloudStatus | null>(null);
  const [activeTab, setActiveTab] = createSignal<SettingsTab>('directories');
  const [isLoading, setIsLoading] = createSignal(true);
  const [isSaving, setIsSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [successMessage, setSuccessMessage] = createSignal<string | null>(null);

  // Form state
  const [newDirectory, setNewDirectory] = createSignal('');
  const [apiKey, setApiKey] = createSignal('');
  const [apiEndpoint, setApiEndpoint] = createSignal('');
  const [monthlyLimit, setMonthlyLimit] = createSignal(10);
  const [selectedTheme, setSelectedTheme] = createSignal<string>('dark');
  const [selectedLanguage, setSelectedLanguage] = createSignal('zh-CN');
  const [enableAnimations, setEnableAnimations] = createSignal(true);

  // Load configuration on mount
  onMount(async () => {
    await loadConfig();
  });

  // Reload when panel opens
  createEffect(() => {
    if (props.isOpen) {
      loadConfig();
    }
  });

  const loadConfig = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [configData, statusData] = await Promise.all([
        getConfig(),
        getCloudStatus(),
      ]);
      setConfigState(configData);
      setCloudStatus(statusData);
      
      // Initialize form state from config
      setSelectedTheme(configData.ui.theme);
      setSelectedLanguage(configData.ui.language);
      setEnableAnimations(configData.ui.enable_animations);
      setMonthlyLimit(configData.cloud.monthly_cost_limit);
      if (configData.cloud.endpoint) {
        setApiEndpoint(configData.cloud.endpoint);
      }
    } catch (err) {
      setError(`Failed to load configuration: ${err}`);
    } finally {
      setIsLoading(false);
    }
  };

  const showSuccess = (message: string) => {
    setSuccessMessage(message);
    setTimeout(() => setSuccessMessage(null), 3000);
  };

  // Directory management
  const handleAddDirectory = async () => {
    const dir = newDirectory().trim();
    if (!dir) return;

    setIsSaving(true);
    try {
      const result = await addMonitoredDirectory(dir);
      if (result.success && result.config) {
        setConfigState(result.config);
        setNewDirectory('');
        showSuccess('Directory added');
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(`Failed to add directory: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  const handleBrowseDirectory = async () => {
    try {
      const dir = await browseDirectory();
      if (dir) {
        setNewDirectory(dir);
      }
    } catch (err) {
      setError(`Failed to browse: ${err}`);
    }
  };

  const handleRemoveDirectory = async (dir: string) => {
    setIsSaving(true);
    try {
      const result = await removeMonitoredDirectory(dir);
      if (result.success && result.config) {
        setConfigState(result.config);
        showSuccess('Directory removed');
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(`Failed to remove directory: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  // Cloud settings
  const handleToggleCloud = async () => {
    const currentConfig = config();
    if (!currentConfig) return;

    setIsSaving(true);
    try {
      const result = await setCloudEnabled(!currentConfig.cloud.enabled);
      if (result.success && result.config) {
        setConfigState(result.config);
        showSuccess(result.message);
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(`Failed to toggle cloud: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  const handleSaveCloudSettings = async () => {
    setIsSaving(true);
    try {
      const result = await setConfig({
        cloud: {
          endpoint: apiEndpoint() || undefined,
          api_key: apiKey() || undefined,
          monthly_cost_limit: monthlyLimit(),
        },
      });
      if (result.success && result.config) {
        setConfigState(result.config);
        setApiKey(''); // Clear API key field after save
        showSuccess('Cloud settings saved');
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(`Failed to save cloud settings: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  // Theme settings
  const handleThemeChange = async (theme: string) => {
    setSelectedTheme(theme);
    setIsSaving(true);
    try {
      const result = await setTheme(theme);
      if (result.success && result.config) {
        setConfigState(result.config);
        showSuccess(`Theme changed to ${theme}`);
        // Apply theme to document
        document.documentElement.setAttribute('data-theme', theme);
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(`Failed to change theme: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  const handleSaveUISettings = async () => {
    setIsSaving(true);
    try {
      const result = await setConfig({
        ui: {
          theme: selectedTheme() as 'light' | 'dark' | 'system',
          language: selectedLanguage(),
          enable_animations: enableAnimations(),
          show_extensions: config()?.ui.show_extensions ?? true,
          default_view: config()?.ui.default_view ?? 'grid',
          thumbnail_size: config()?.ui.thumbnail_size ?? 'medium',
        },
      });
      if (result.success && result.config) {
        setConfigState(result.config);
        showSuccess('UI settings saved');
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(`Failed to save UI settings: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  // Reset settings
  const handleReset = async () => {
    if (!confirm('Are you sure you want to reset all settings to defaults?')) {
      return;
    }

    setIsSaving(true);
    try {
      const result = await resetConfig();
      if (result.success && result.config) {
        setConfigState(result.config);
        showSuccess('Settings reset to defaults');
        await loadConfig(); // Reload to update form state
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(`Failed to reset settings: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  if (!props.isOpen) return null;

  return (
    <div class="settings-overlay" onClick={() => props.onClose()}>
      <div class="settings-panel" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <div class="settings-header">
          <h2>⚙️ 设置</h2>
          <button class="close-btn" onClick={() => props.onClose()}>✕</button>
        </div>

        {/* Messages */}
        <Show when={error()}>
          <div class="settings-error">{error()}</div>
        </Show>
        <Show when={successMessage()}>
          <div class="settings-success">{successMessage()}</div>
        </Show>

        {/* Loading state */}
        <Show when={isLoading()}>
          <div class="settings-loading">
            <span class="loading-spinner">⏳</span>
            <span>加载配置中...</span>
          </div>
        </Show>

        <Show when={!isLoading() && config()}>
          {/* Tabs */}
          <div class="settings-tabs">
            <button
              class={`tab ${activeTab() === 'directories' ? 'active' : ''}`}
              onClick={() => setActiveTab('directories')}
            >
              📁 监控目录
            </button>
            <button
              class={`tab ${activeTab() === 'cloud' ? 'active' : ''}`}
              onClick={() => setActiveTab('cloud')}
            >
              ☁️ 云端 API
            </button>
            <button
              class={`tab ${activeTab() === 'appearance' ? 'active' : ''}`}
              onClick={() => setActiveTab('appearance')}
            >
              🎨 外观
            </button>
            <button
              class={`tab ${activeTab() === 'advanced' ? 'active' : ''}`}
              onClick={() => setActiveTab('advanced')}
            >
              🔧 高级
            </button>
          </div>

          {/* Tab Content */}
          <div class="settings-content">
            {/* Directories Tab */}
            <Show when={activeTab() === 'directories'}>
              <div class="settings-section">
                <h3>监控目录</h3>
                <p class="section-description">
                  选择要监控的目录，NeuralFS 将自动索引这些目录中的文件。
                </p>

                {/* Add directory */}
                <div class="add-directory">
                  <input
                    type="text"
                    value={newDirectory()}
                    onInput={(e) => setNewDirectory(e.currentTarget.value)}
                    placeholder="输入目录路径..."
                    class="directory-input"
                  />
                  <button
                    class="browse-btn"
                    onClick={handleBrowseDirectory}
                    disabled={isSaving()}
                  >
                    浏览...
                  </button>
                  <button
                    class="add-btn"
                    onClick={handleAddDirectory}
                    disabled={isSaving() || !newDirectory().trim()}
                  >
                    添加
                  </button>
                </div>

                {/* Directory list */}
                <div class="directory-list">
                  <For each={config()?.monitored_directories ?? []}>
                    {(dir) => (
                      <div class="directory-item">
                        <span class="directory-path">📁 {dir}</span>
                        <button
                          class="remove-btn"
                          onClick={() => handleRemoveDirectory(dir)}
                          disabled={isSaving()}
                        >
                          ✕
                        </button>
                      </div>
                    )}
                  </For>
                  <Show when={(config()?.monitored_directories ?? []).length === 0}>
                    <div class="empty-state">
                      暂无监控目录，请添加目录开始使用。
                    </div>
                  </Show>
                </div>
              </div>
            </Show>

            {/* Cloud Tab */}
            <Show when={activeTab() === 'cloud'}>
              <div class="settings-section">
                <h3>云端 API 配置</h3>
                <p class="section-description">
                  配置云端 AI 服务以获得更精确的搜索结果。
                </p>

                {/* Cloud toggle */}
                <div class="setting-row">
                  <label>启用云端功能</label>
                  <button
                    class={`toggle-btn ${config()?.cloud.enabled ? 'active' : ''}`}
                    onClick={handleToggleCloud}
                    disabled={isSaving()}
                  >
                    {config()?.cloud.enabled ? '已启用' : '已禁用'}
                  </button>
                </div>

                {/* Cloud status */}
                <Show when={cloudStatus()}>
                  <div class="cloud-status">
                    <div class="status-item">
                      <span class="status-label">连接状态:</span>
                      <span class={`status-value ${cloudStatus()?.connected ? 'connected' : 'disconnected'}`}>
                        {cloudStatus()?.connected ? '✓ 已连接' : '✗ 未连接'}
                      </span>
                    </div>
                    <div class="status-item">
                      <span class="status-label">本月用量:</span>
                      <span class="status-value">
                        ${cloudStatus()?.current_month_usage.toFixed(2)} / ${cloudStatus()?.monthly_limit.toFixed(2)}
                      </span>
                    </div>
                  </div>
                </Show>

                {/* API settings */}
                <Show when={config()?.cloud.enabled}>
                  <div class="setting-row">
                    <label>API 端点</label>
                    <input
                      type="text"
                      value={apiEndpoint()}
                      onInput={(e) => setApiEndpoint(e.currentTarget.value)}
                      placeholder="https://api.openai.com/v1"
                      class="setting-input"
                    />
                  </div>

                  <div class="setting-row">
                    <label>API 密钥</label>
                    <input
                      type="password"
                      value={apiKey()}
                      onInput={(e) => setApiKey(e.currentTarget.value)}
                      placeholder={config()?.cloud.api_key_set ? '••••••••' : '输入 API 密钥'}
                      class="setting-input"
                    />
                  </div>

                  <div class="setting-row">
                    <label>月度费用限制 (USD)</label>
                    <input
                      type="number"
                      value={monthlyLimit()}
                      onInput={(e) => setMonthlyLimit(parseFloat(e.currentTarget.value) || 0)}
                      min="0"
                      step="1"
                      class="setting-input small"
                    />
                  </div>

                  <button
                    class="save-btn"
                    onClick={handleSaveCloudSettings}
                    disabled={isSaving()}
                  >
                    {isSaving() ? '保存中...' : '保存云端设置'}
                  </button>
                </Show>
              </div>
            </Show>

            {/* Appearance Tab */}
            <Show when={activeTab() === 'appearance'}>
              <div class="settings-section">
                <h3>外观设置</h3>
                <p class="section-description">
                  自定义 NeuralFS 的外观和显示方式。
                </p>

                {/* Theme */}
                <div class="setting-row">
                  <label>主题</label>
                  <div class="theme-options">
                    <button
                      class={`theme-btn ${selectedTheme() === 'light' ? 'active' : ''}`}
                      onClick={() => handleThemeChange('light')}
                    >
                      ☀️ 浅色
                    </button>
                    <button
                      class={`theme-btn ${selectedTheme() === 'dark' ? 'active' : ''}`}
                      onClick={() => handleThemeChange('dark')}
                    >
                      🌙 深色
                    </button>
                    <button
                      class={`theme-btn ${selectedTheme() === 'system' ? 'active' : ''}`}
                      onClick={() => handleThemeChange('system')}
                    >
                      💻 跟随系统
                    </button>
                  </div>
                </div>

                {/* Language */}
                <div class="setting-row">
                  <label>语言</label>
                  <select
                    value={selectedLanguage()}
                    onChange={(e) => setSelectedLanguage(e.currentTarget.value)}
                    class="setting-select"
                  >
                    <option value="zh-CN">简体中文</option>
                    <option value="en-US">English</option>
                    <option value="ja-JP">日本語</option>
                  </select>
                </div>

                {/* Animations */}
                <div class="setting-row">
                  <label>启用动画</label>
                  <button
                    class={`toggle-btn ${enableAnimations() ? 'active' : ''}`}
                    onClick={() => setEnableAnimations(!enableAnimations())}
                  >
                    {enableAnimations() ? '已启用' : '已禁用'}
                  </button>
                </div>

                <button
                  class="save-btn"
                  onClick={handleSaveUISettings}
                  disabled={isSaving()}
                >
                  {isSaving() ? '保存中...' : '保存外观设置'}
                </button>
              </div>
            </Show>

            {/* Advanced Tab */}
            <Show when={activeTab() === 'advanced'}>
              <div class="settings-section">
                <h3>高级设置</h3>
                <p class="section-description">
                  高级配置选项，请谨慎修改。
                </p>

                {/* Config info */}
                <div class="config-info">
                  <div class="info-item">
                    <span class="info-label">配置版本:</span>
                    <span class="info-value">v{config()?.version}</span>
                  </div>
                  <div class="info-item">
                    <span class="info-label">最后修改:</span>
                    <span class="info-value">
                      {new Date(config()?.last_modified ?? '').toLocaleString()}
                    </span>
                  </div>
                </div>

                {/* Reset button */}
                <div class="danger-zone">
                  <h4>危险操作</h4>
                  <button
                    class="reset-btn"
                    onClick={handleReset}
                    disabled={isSaving()}
                  >
                    🔄 重置所有设置
                  </button>
                </div>
              </div>
            </Show>
          </div>
        </Show>
      </div>
    </div>
  );
}

export default Settings;
