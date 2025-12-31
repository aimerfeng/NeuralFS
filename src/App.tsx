/**
 * NeuralFS Main Application
 */

import { createSignal, onMount, Show } from 'solid-js';
import { SearchBar } from './components/SearchBar';
import { FileGrid } from './components/FileGrid';
import { TagPanel, type TagFilter } from './components/TagPanel';
import { RelationGraph } from './components/RelationGraph';
import { OnboardingWizard } from './components/OnboardingWizard';
import { Settings } from './components/Settings';
import { initSessionToken, searchFiles, checkFirstLaunch } from './api/tauri';
import type { SearchResult, SearchIntent, Tag, FileRecord, FileRelation } from './types';
import './App.css';

function App() {
  const [searchResults, setSearchResults] = createSignal<SearchResult[]>([]);
  const [selectedFile, setSelectedFile] = createSignal<FileRecord | null>(null);
  const [selectedTags, setSelectedTags] = createSignal<Tag[]>([]);
  const [tagFilter, setTagFilter] = createSignal<TagFilter | null>(null);
  const [isSearching, setIsSearching] = createSignal(false);
  const [showRelationGraph, setShowRelationGraph] = createSignal(false);
  const [isInitialized, setIsInitialized] = createSignal(false);
  const [showOnboarding, setShowOnboarding] = createSignal(false);
  const [showSettings, setShowSettings] = createSignal(false);

  // Initialize session token and check for first launch
  onMount(async () => {
    try {
      // Check if this is the first launch
      const isFirstLaunch = await checkFirstLaunch();
      if (isFirstLaunch) {
        setShowOnboarding(true);
      }
      
      await initSessionToken();
      setIsInitialized(true);
    } catch (error) {
      console.error('Failed to initialize session:', error);
      // Still allow app to function in development
      setIsInitialized(true);
    }
  });

  // Handle onboarding completion
  const handleOnboardingComplete = () => {
    setShowOnboarding(false);
  };

  // Handle search
  const handleSearch = async (query: string, intent?: SearchIntent) => {
    setIsSearching(true);
    try {
      const filter = tagFilter();
      const response = await searchFiles({
        query,
        intent,
        filters: {
          tags: filter?.includeTags,
          exclude_tags: filter?.excludeTags,
          min_score: 0.3,
          exclude_private: false,
        },
        pagination: { offset: 0, limit: 50 },
        enable_cloud: true,
        request_id: crypto.randomUUID(),
        timestamp: new Date().toISOString(),
      });

      setSearchResults(response.results);
    } catch (error) {
      console.error('Search failed:', error);
      setSearchResults([]);
    } finally {
      setIsSearching(false);
    }
  };

  // Handle file selection
  const handleFileSelect = (file: FileRecord) => {
    setSelectedFile(file);
  };

  // Handle file open
  const handleFileOpen = (file: FileRecord) => {
    // Open file with system default application
    // This would use Tauri's shell.open API
    console.log('Opening file:', file.path);
  };

  // Handle tag selection
  const handleTagSelect = (tags: Tag[]) => {
    setSelectedTags(tags);
  };

  // Handle tag filter change
  const handleFilterChange = (filter: TagFilter) => {
    setTagFilter(filter);
  };

  // Handle relation click
  const handleRelationClick = (file: FileRecord, relation: FileRelation) => {
    setSelectedFile(file);
    setShowRelationGraph(true);
  };

  return (
    <div class="app">
      <Show when={!isInitialized()}>
        <div class="loading-screen">
          <span class="loading-icon">⏳</span>
          <span>正在初始化...</span>
        </div>
      </Show>

      {/* Onboarding Wizard */}
      <Show when={isInitialized() && showOnboarding()}>
        <OnboardingWizard onComplete={handleOnboardingComplete} />
      </Show>

      <Show when={isInitialized() && !showOnboarding()}>
        {/* Header with Search */}
        <header class="app-header">
          <div class="logo">
            <span class="logo-icon">🧠</span>
            <span class="logo-text">NeuralFS</span>
          </div>
          <div class="search-container">
            <SearchBar
              onSearch={handleSearch}
              placeholder="搜索文件或内容..."
            />
          </div>
          <div class="header-actions">
            <button
              class="settings-btn"
              onClick={() => setShowSettings(true)}
              title="设置"
            >
              ⚙️
            </button>
            <button
              class={`view-toggle ${showRelationGraph() ? 'active' : ''}`}
              onClick={() => setShowRelationGraph(!showRelationGraph())}
              disabled={!selectedFile()}
              title="显示关联图"
            >
              🔗
            </button>
          </div>
        </header>

        {/* Main Content */}
        <main class="app-main">
          {/* Sidebar - Tag Panel */}
          <aside class="sidebar">
            <TagPanel
              onTagSelect={handleTagSelect}
              onFilterChange={handleFilterChange}
              selectedTags={selectedTags()}
              allowMultiSelect={true}
              showCounts={true}
            />
          </aside>

          {/* Content Area */}
          <div class="content">
            <Show when={isSearching()}>
              <div class="search-loading">
                <span class="loading-spinner">⏳</span>
                <span>搜索中...</span>
              </div>
            </Show>

            <Show when={!isSearching()}>
              <FileGrid
                files={searchResults()}
                onFileSelect={handleFileSelect}
                onFileOpen={handleFileOpen}
                onRelationClick={handleRelationClick}
                viewMode="grid"
                showTags={true}
                showRelations={true}
              />
            </Show>
          </div>

          {/* Relation Graph Panel */}
          <Show when={showRelationGraph() && selectedFile()}>
            <aside class="relation-panel">
              <div class="panel-header">
                <h3>文件关联</h3>
                <button
                  class="close-btn"
                  onClick={() => setShowRelationGraph(false)}
                >
                  ✕
                </button>
              </div>
              <RelationGraph
                fileId={selectedFile()!.id}
                depth={2}
                onNodeClick={(node) => {
                  // Navigate to clicked file
                  console.log('Navigate to:', node.file_id);
                }}
                width={400}
                height={500}
              />
            </aside>
          </Show>
        </main>

        {/* Status Bar */}
        <footer class="app-footer">
          <div class="status-left">
            <Show when={searchResults().length > 0}>
              <span>{searchResults().length} 个结果</span>
            </Show>
            <Show when={selectedTags().length > 0}>
              <span>• {selectedTags().length} 个标签筛选</span>
            </Show>
          </div>
          <div class="status-right">
            <Show when={selectedFile()}>
              <span>{selectedFile()!.filename}</span>
            </Show>
          </div>
        </footer>

        {/* Settings Panel */}
        <Settings
          isOpen={showSettings()}
          onClose={() => setShowSettings(false)}
        />
      </Show>
    </div>
  );
}

export default App;
