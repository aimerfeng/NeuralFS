/**
 * SearchBar Component
 * 
 * Implements:
 * - Intent hints (file-level vs content-level search)
 * - Clarification options for ambiguous queries
 * - Search suggestions
 * 
 * Requirements: 2.6, 10.1
 */

import { createSignal, createEffect, Show, For, onCleanup } from 'solid-js';
import type { SearchIntent, Clarification, ClarificationOption } from '../../types';
import { searchFiles, getSearchSuggestions } from '../../api/tauri';
import './SearchBar.css';

export interface SearchBarProps {
  onSearch: (query: string, intent?: SearchIntent) => void;
  onClarificationSelect?: (option: ClarificationOption) => void;
  placeholder?: string;
  initialQuery?: string;
}

export function SearchBar(props: SearchBarProps) {
  const [query, setQuery] = createSignal(props.initialQuery || '');
  const [suggestions, setSuggestions] = createSignal<string[]>([]);
  const [showSuggestions, setShowSuggestions] = createSignal(false);
  const [clarifications, setClarifications] = createSignal<Clarification[] | null>(null);
  const [intentHint, setIntentHint] = createSignal<'file' | 'content' | null>(null);
  const [isLoading, setIsLoading] = createSignal(false);
  const [selectedSuggestionIndex, setSelectedSuggestionIndex] = createSignal(-1);

  let inputRef: HTMLInputElement | undefined;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  // Cleanup debounce timer
  onCleanup(() => {
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  // Analyze query to provide intent hints
  const analyzeIntent = (q: string): 'file' | 'content' | null => {
    const trimmed = q.trim().toLowerCase();
    
    // File-level indicators
    const fileIndicators = [
      /^find\s+(file|document|image|video)/i,
      /\.(pdf|docx?|xlsx?|pptx?|txt|md|png|jpe?g|mp4|mov)$/i,
      /^(where|which)\s+(is|are)/i,
      /文件|文档|图片|视频/,
    ];
    
    // Content-level indicators
    const contentIndicators = [
      /^(search|find)\s+(for|in|within)/i,
      /contains?|mentions?|talks?\s+about/i,
      /paragraph|section|chapter|page/i,
      /内容|段落|提到|包含/,
      /"[^"]+"/,  // Quoted text suggests content search
    ];

    for (const pattern of fileIndicators) {
      if (pattern.test(trimmed)) return 'file';
    }
    
    for (const pattern of contentIndicators) {
      if (pattern.test(trimmed)) return 'content';
    }
    
    return null;
  };

  // Fetch suggestions with debounce
  const fetchSuggestions = async (q: string) => {
    if (q.length < 2) {
      setSuggestions([]);
      return;
    }

    try {
      const results = await getSearchSuggestions(q);
      setSuggestions(results);
    } catch (error) {
      console.error('Failed to fetch suggestions:', error);
      setSuggestions([]);
    }
  };

  // Handle input change
  const handleInput = (e: InputEvent) => {
    const target = e.target as HTMLInputElement;
    const value = target.value;
    setQuery(value);
    setSelectedSuggestionIndex(-1);
    
    // Analyze intent
    setIntentHint(analyzeIntent(value));
    
    // Clear previous clarifications
    setClarifications(null);
    
    // Debounce suggestions fetch
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => fetchSuggestions(value), 200);
    
    setShowSuggestions(value.length >= 2);
  };

  // Handle search submission
  const handleSearch = async () => {
    const q = query().trim();
    if (!q) return;

    setIsLoading(true);
    setShowSuggestions(false);

    try {
      // Build search intent based on hint
      let intent: SearchIntent | undefined;
      const hint = intentHint();
      
      if (hint === 'file') {
        intent = { type: 'FindFile' };
      } else if (hint === 'content') {
        intent = { type: 'FindContent', need_location: true };
      }

      // Perform search to check for clarifications
      const response = await searchFiles({
        query: q,
        intent,
        filters: {
          min_score: 0.5,
          exclude_private: false,
        },
        pagination: { offset: 0, limit: 20 },
        enable_cloud: true,
        request_id: crypto.randomUUID(),
        timestamp: new Date().toISOString(),
      });

      // Check if clarification is needed
      if (response.status === 'NeedsClarity' && response.clarifications) {
        setClarifications(response.clarifications);
      } else {
        props.onSearch(q, intent);
      }
    } catch (error) {
      console.error('Search failed:', error);
      // Still trigger search callback for error handling
      props.onSearch(q);
    } finally {
      setIsLoading(false);
    }
  };

  // Handle clarification option selection
  const handleClarificationSelect = (option: ClarificationOption) => {
    setClarifications(null);
    props.onClarificationSelect?.(option);
    props.onSearch(query(), option.intent);
  };

  // Handle keyboard navigation
  const handleKeyDown = (e: KeyboardEvent) => {
    const suggestionList = suggestions();
    
    switch (e.key) {
      case 'Enter':
        e.preventDefault();
        if (selectedSuggestionIndex() >= 0 && suggestionList[selectedSuggestionIndex()]) {
          setQuery(suggestionList[selectedSuggestionIndex()]);
          setShowSuggestions(false);
        }
        handleSearch();
        break;
        
      case 'ArrowDown':
        e.preventDefault();
        if (showSuggestions() && suggestionList.length > 0) {
          setSelectedSuggestionIndex(i => 
            Math.min(i + 1, suggestionList.length - 1)
          );
        }
        break;
        
      case 'ArrowUp':
        e.preventDefault();
        if (showSuggestions()) {
          setSelectedSuggestionIndex(i => Math.max(i - 1, -1));
        }
        break;
        
      case 'Escape':
        setShowSuggestions(false);
        setClarifications(null);
        break;
    }
  };

  // Handle suggestion click
  const handleSuggestionClick = (suggestion: string) => {
    setQuery(suggestion);
    setShowSuggestions(false);
    setIntentHint(analyzeIntent(suggestion));
    inputRef?.focus();
  };

  // Get intent hint display text
  const getIntentHintText = () => {
    const hint = intentHint();
    if (hint === 'file') return '🔍 搜索文件';
    if (hint === 'content') return '📄 搜索内容片段';
    return null;
  };

  return (
    <div class="search-bar-container">
      <div class="search-bar">
        <div class="search-input-wrapper">
          <span class="search-icon">🔍</span>
          <input
            ref={inputRef}
            type="text"
            class="search-input"
            placeholder={props.placeholder || '搜索文件或内容...'}
            value={query()}
            onInput={handleInput}
            onKeyDown={handleKeyDown}
            onFocus={() => query().length >= 2 && setShowSuggestions(true)}
            onBlur={() => setTimeout(() => setShowSuggestions(false), 200)}
          />
          <Show when={isLoading()}>
            <span class="search-loading">⏳</span>
          </Show>
          <Show when={!isLoading() && query()}>
            <button
              class="search-clear"
              onClick={() => {
                setQuery('');
                setIntentHint(null);
                setClarifications(null);
                inputRef?.focus();
              }}
            >
              ✕
            </button>
          </Show>
        </div>
        
        <button
          class="search-button"
          onClick={handleSearch}
          disabled={isLoading() || !query().trim()}
        >
          搜索
        </button>
      </div>

      {/* Intent Hint */}
      <Show when={getIntentHintText()}>
        <div class="intent-hint">
          <span class="intent-hint-text">{getIntentHintText()}</span>
          <button
            class="intent-hint-toggle"
            onClick={() => {
              const current = intentHint();
              setIntentHint(current === 'file' ? 'content' : 'file');
            }}
          >
            切换为{intentHint() === 'file' ? '内容搜索' : '文件搜索'}
          </button>
        </div>
      </Show>

      {/* Suggestions Dropdown */}
      <Show when={showSuggestions() && suggestions().length > 0}>
        <ul class="search-suggestions">
          <For each={suggestions()}>
            {(suggestion, index) => (
              <li
                class={`suggestion-item ${index() === selectedSuggestionIndex() ? 'selected' : ''}`}
                onClick={() => handleSuggestionClick(suggestion)}
                onMouseEnter={() => setSelectedSuggestionIndex(index())}
              >
                <span class="suggestion-icon">🔎</span>
                <span class="suggestion-text">{suggestion}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>

      {/* Clarification Panel */}
      <Show when={clarifications()}>
        <div class="clarification-panel">
          <For each={clarifications()}>
            {(clarification) => (
              <div class="clarification-group">
                <p class="clarification-question">{clarification.question}</p>
                <div class="clarification-options">
                  <For each={clarification.options}>
                    {(option) => (
                      <button
                        class="clarification-option"
                        onClick={() => handleClarificationSelect(option)}
                      >
                        <span class="option-text">{option.text}</span>
                        <Show when={option.estimated_count !== undefined}>
                          <span class="option-count">
                            约 {option.estimated_count} 个结果
                          </span>
                        </Show>
                      </button>
                    )}
                  </For>
                </div>
              </div>
            )}
          </For>
          <button
            class="clarification-dismiss"
            onClick={() => setClarifications(null)}
          >
            跳过，直接搜索
          </button>
        </div>
      </Show>
    </div>
  );
}

export default SearchBar;
