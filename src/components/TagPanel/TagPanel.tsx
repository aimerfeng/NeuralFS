/**
 * TagPanel Component
 * 
 * Implements:
 * - Hierarchical tag navigation
 * - Multi-dimensional filtering
 * - Tag management (create, edit, delete)
 * 
 * Requirements: 5.2, 5.6
 */

import { createSignal, createEffect, Show, For, createMemo, onMount } from 'solid-js';
import type { Tag, TagType, TagHierarchy } from '../../types';
import { getTags, executeTagCommand } from '../../api/tauri';
import './TagPanel.css';

export interface TagPanelProps {
  onTagSelect?: (tags: Tag[]) => void;
  onFilterChange?: (filters: TagFilter) => void;
  selectedTags?: Tag[];
  allowMultiSelect?: boolean;
  showCounts?: boolean;
}

export interface TagFilter {
  includeTags: string[];
  excludeTags: string[];
  tagTypes?: TagType[];
}

interface TagNode {
  tag: Tag;
  children: TagNode[];
  isExpanded: boolean;
  fileCount?: number;
}

export function TagPanel(props: TagPanelProps) {
  const [tags, setTags] = createSignal<Tag[]>([]);
  const [selectedTagIds, setSelectedTagIds] = createSignal<Set<string>>(new Set());
  const [excludedTagIds, setExcludedTagIds] = createSignal<Set<string>>(new Set());
  const [expandedTagIds, setExpandedTagIds] = createSignal<Set<string>>(new Set());
  const [searchQuery, setSearchQuery] = createSignal('');
  const [isLoading, setIsLoading] = createSignal(true);
  const [activeTypeFilter, setActiveTypeFilter] = createSignal<TagType | null>(null);
  const [showCreateDialog, setShowCreateDialog] = createSignal(false);
  const [newTagName, setNewTagName] = createSignal('');
  const [newTagParentId, setNewTagParentId] = createSignal<string | null>(null);
  const [newTagType, setNewTagType] = createSignal<TagType>('Custom');

  // Load tags on mount
  onMount(async () => {
    await loadTags();
  });

  const loadTags = async () => {
    setIsLoading(true);
    try {
      const loadedTags = await getTags();
      setTags(loadedTags);
    } catch (error) {
      console.error('Failed to load tags:', error);
    } finally {
      setIsLoading(false);
    }
  };

  // Build tag hierarchy
  const tagHierarchy = createMemo(() => {
    const tagMap = new Map<string, TagNode>();
    const rootNodes: TagNode[] = [];

    // Create nodes for all tags
    for (const tag of tags()) {
      tagMap.set(tag.id, {
        tag,
        children: [],
        isExpanded: expandedTagIds().has(tag.id),
      });
    }

    // Build hierarchy
    for (const tag of tags()) {
      const node = tagMap.get(tag.id)!;
      if (tag.parent_id && tagMap.has(tag.parent_id)) {
        tagMap.get(tag.parent_id)!.children.push(node);
      } else {
        rootNodes.push(node);
      }
    }

    // Sort by usage count
    const sortNodes = (nodes: TagNode[]) => {
      nodes.sort((a, b) => b.tag.usage_count - a.tag.usage_count);
      for (const node of nodes) {
        sortNodes(node.children);
      }
    };
    sortNodes(rootNodes);

    return rootNodes;
  });

  // Filter tags by search query and type
  const filteredHierarchy = createMemo(() => {
    const query = searchQuery().toLowerCase();
    const typeFilter = activeTypeFilter();

    const filterNode = (node: TagNode): TagNode | null => {
      const matchesQuery = !query || 
        node.tag.name.toLowerCase().includes(query) ||
        Object.values(node.tag.display_name).some(n => n.toLowerCase().includes(query));
      
      const matchesType = !typeFilter || node.tag.tag_type === typeFilter;

      // Filter children recursively
      const filteredChildren = node.children
        .map(filterNode)
        .filter((n): n is TagNode => n !== null);

      // Include node if it matches or has matching children
      if ((matchesQuery && matchesType) || filteredChildren.length > 0) {
        return {
          ...node,
          children: filteredChildren,
          isExpanded: query ? true : node.isExpanded, // Auto-expand when searching
        };
      }

      return null;
    };

    return tagHierarchy()
      .map(filterNode)
      .filter((n): n is TagNode => n !== null);
  });

  // Handle tag selection
  const handleTagClick = (tag: Tag, e: MouseEvent) => {
    const isCtrlClick = e.ctrlKey || e.metaKey;
    const isShiftClick = e.shiftKey;

    setSelectedTagIds(prev => {
      const newSet = new Set(prev);
      
      if (isShiftClick) {
        // Shift+click: toggle exclude
        if (excludedTagIds().has(tag.id)) {
          setExcludedTagIds(ex => {
            const newEx = new Set(ex);
            newEx.delete(tag.id);
            return newEx;
          });
        } else {
          newSet.delete(tag.id);
          setExcludedTagIds(ex => new Set(ex).add(tag.id));
        }
      } else if (props.allowMultiSelect && isCtrlClick) {
        // Ctrl+click: toggle selection (multi-select mode)
        if (newSet.has(tag.id)) {
          newSet.delete(tag.id);
        } else {
          newSet.add(tag.id);
          // Remove from excluded if adding to selected
          setExcludedTagIds(ex => {
            const newEx = new Set(ex);
            newEx.delete(tag.id);
            return newEx;
          });
        }
      } else {
        // Normal click: single select
        newSet.clear();
        newSet.add(tag.id);
        setExcludedTagIds(new Set());
      }

      return newSet;
    });

    // Notify parent
    notifyFilterChange();
  };

  // Toggle tag expansion
  const toggleExpand = (tagId: string, e: MouseEvent) => {
    e.stopPropagation();
    setExpandedTagIds(prev => {
      const newSet = new Set(prev);
      if (newSet.has(tagId)) {
        newSet.delete(tagId);
      } else {
        newSet.add(tagId);
      }
      return newSet;
    });
  };

  // Notify parent of filter changes
  const notifyFilterChange = () => {
    const selectedTags = tags().filter(t => selectedTagIds().has(t.id));
    props.onTagSelect?.(selectedTags);

    props.onFilterChange?.({
      includeTags: Array.from(selectedTagIds()),
      excludeTags: Array.from(excludedTagIds()),
      tagTypes: activeTypeFilter() ? [activeTypeFilter()!] : undefined,
    });
  };

  // Create new tag
  const handleCreateTag = async () => {
    const name = newTagName().trim();
    if (!name) return;

    try {
      await executeTagCommand({
        type: 'CreateTag',
        name,
        parent_id: newTagParentId() || undefined,
        tag_type: newTagType(),
      });

      // Reload tags
      await loadTags();

      // Reset form
      setNewTagName('');
      setNewTagParentId(null);
      setShowCreateDialog(false);
    } catch (error) {
      console.error('Failed to create tag:', error);
    }
  };

  // Clear all filters
  const clearFilters = () => {
    setSelectedTagIds(new Set());
    setExcludedTagIds(new Set());
    setActiveTypeFilter(null);
    setSearchQuery('');
    notifyFilterChange();
  };

  // Get tag type label
  const getTagTypeLabel = (type: TagType): string => {
    const labels: Record<TagType, string> = {
      Category: '分类',
      FileType: '文件类型',
      Project: '项目',
      Status: '状态',
      Custom: '自定义',
      AutoGenerated: 'AI生成',
    };
    return labels[type];
  };

  // Get tag type icon
  const getTagTypeIcon = (type: TagType): string => {
    const icons: Record<TagType, string> = {
      Category: '📁',
      FileType: '📄',
      Project: '📋',
      Status: '🔄',
      Custom: '🏷️',
      AutoGenerated: '🤖',
    };
    return icons[type];
  };

  // Render tag node recursively
  const renderTagNode = (node: TagNode, depth: number = 0) => {
    const isSelected = selectedTagIds().has(node.tag.id);
    const isExcluded = excludedTagIds().has(node.tag.id);
    const hasChildren = node.children.length > 0;

    return (
      <div class="tag-node" style={{ '--depth': depth }}>
        <div
          class={`tag-item ${isSelected ? 'selected' : ''} ${isExcluded ? 'excluded' : ''}`}
          onClick={(e) => handleTagClick(node.tag, e)}
        >
          {/* Expand/Collapse button */}
          <Show when={hasChildren}>
            <button
              class={`expand-btn ${node.isExpanded ? 'expanded' : ''}`}
              onClick={(e) => toggleExpand(node.tag.id, e)}
            >
              ▶
            </button>
          </Show>
          <Show when={!hasChildren}>
            <span class="expand-placeholder" />
          </Show>

          {/* Tag icon and color */}
          <span
            class="tag-color"
            style={{ background: node.tag.color || '#999' }}
          />

          {/* Tag name */}
          <span class="tag-name">
            {node.tag.icon && <span class="tag-icon">{node.tag.icon}</span>}
            {node.tag.name}
          </span>

          {/* Tag type badge */}
          <span class="tag-type-badge" title={getTagTypeLabel(node.tag.tag_type)}>
            {getTagTypeIcon(node.tag.tag_type)}
          </span>

          {/* Usage count */}
          <Show when={props.showCounts !== false}>
            <span class="tag-count">{node.tag.usage_count}</span>
          </Show>

          {/* Selection indicator */}
          <Show when={isSelected}>
            <span class="selection-indicator">✓</span>
          </Show>
          <Show when={isExcluded}>
            <span class="exclusion-indicator">✕</span>
          </Show>
        </div>

        {/* Children */}
        <Show when={hasChildren && node.isExpanded}>
          <div class="tag-children">
            <For each={node.children}>
              {(child) => renderTagNode(child, depth + 1)}
            </For>
          </div>
        </Show>
      </div>
    );
  };

  return (
    <div class="tag-panel">
      {/* Header */}
      <div class="tag-panel-header">
        <h3 class="panel-title">标签</h3>
        <button
          class="create-tag-btn"
          onClick={() => setShowCreateDialog(true)}
          title="创建新标签"
        >
          +
        </button>
      </div>

      {/* Search */}
      <div class="tag-search">
        <input
          type="text"
          placeholder="搜索标签..."
          value={searchQuery()}
          onInput={(e) => setSearchQuery(e.currentTarget.value)}
        />
        <Show when={searchQuery()}>
          <button class="clear-search" onClick={() => setSearchQuery('')}>
            ✕
          </button>
        </Show>
      </div>

      {/* Type Filter */}
      <div class="type-filters">
        <button
          class={`type-filter ${activeTypeFilter() === null ? 'active' : ''}`}
          onClick={() => {
            setActiveTypeFilter(null);
            notifyFilterChange();
          }}
        >
          全部
        </button>
        <For each={['Category', 'Project', 'Status', 'Custom', 'AutoGenerated'] as TagType[]}>
          {(type) => (
            <button
              class={`type-filter ${activeTypeFilter() === type ? 'active' : ''}`}
              onClick={() => {
                setActiveTypeFilter(activeTypeFilter() === type ? null : type);
                notifyFilterChange();
              }}
              title={getTagTypeLabel(type)}
            >
              {getTagTypeIcon(type)}
            </button>
          )}
        </For>
      </div>

      {/* Active Filters Summary */}
      <Show when={selectedTagIds().size > 0 || excludedTagIds().size > 0}>
        <div class="active-filters">
          <span class="filter-summary">
            {selectedTagIds().size > 0 && `${selectedTagIds().size} 个已选`}
            {selectedTagIds().size > 0 && excludedTagIds().size > 0 && ', '}
            {excludedTagIds().size > 0 && `${excludedTagIds().size} 个已排除`}
          </span>
          <button class="clear-filters" onClick={clearFilters}>
            清除筛选
          </button>
        </div>
      </Show>

      {/* Tag List */}
      <div class="tag-list">
        <Show when={isLoading()}>
          <div class="loading-state">加载中...</div>
        </Show>

        <Show when={!isLoading() && filteredHierarchy().length === 0}>
          <div class="empty-state">
            <Show when={searchQuery()}>
              <p>没有找到匹配的标签</p>
            </Show>
            <Show when={!searchQuery()}>
              <p>暂无标签</p>
              <button onClick={() => setShowCreateDialog(true)}>
                创建第一个标签
              </button>
            </Show>
          </div>
        </Show>

        <Show when={!isLoading()}>
          <For each={filteredHierarchy()}>
            {(node) => renderTagNode(node)}
          </For>
        </Show>
      </div>

      {/* Multi-select hint */}
      <Show when={props.allowMultiSelect}>
        <div class="hint-text">
          💡 Ctrl+点击多选，Shift+点击排除
        </div>
      </Show>

      {/* Create Tag Dialog */}
      <Show when={showCreateDialog()}>
        <div class="dialog-overlay" onClick={() => setShowCreateDialog(false)}>
          <div class="create-tag-dialog" onClick={(e) => e.stopPropagation()}>
            <h4>创建新标签</h4>
            
            <div class="form-group">
              <label>标签名称</label>
              <input
                type="text"
                value={newTagName()}
                onInput={(e) => setNewTagName(e.currentTarget.value)}
                placeholder="输入标签名称"
                autofocus
              />
            </div>

            <div class="form-group">
              <label>标签类型</label>
              <select
                value={newTagType()}
                onChange={(e) => setNewTagType(e.currentTarget.value as TagType)}
              >
                <option value="Custom">自定义</option>
                <option value="Category">分类</option>
                <option value="Project">项目</option>
                <option value="Status">状态</option>
              </select>
            </div>

            <div class="form-group">
              <label>父标签 (可选)</label>
              <select
                value={newTagParentId() || ''}
                onChange={(e) => setNewTagParentId(e.currentTarget.value || null)}
              >
                <option value="">无</option>
                <For each={tags().filter(t => !t.parent_id)}>
                  {(tag) => (
                    <option value={tag.id}>{tag.name}</option>
                  )}
                </For>
              </select>
            </div>

            <div class="dialog-actions">
              <button class="cancel-btn" onClick={() => setShowCreateDialog(false)}>
                取消
              </button>
              <button
                class="create-btn"
                onClick={handleCreateTag}
                disabled={!newTagName().trim()}
              >
                创建
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

export default TagPanel;
