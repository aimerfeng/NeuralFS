/**
 * FileGrid Component
 * 
 * Displays files in a grid layout with:
 * - Tag display (confirmed vs suggested)
 * - Relation indicators
 * - File previews and thumbnails
 * 
 * Requirements: UI/UX Design
 */

import { createSignal, createEffect, Show, For, onMount } from 'solid-js';
import type { 
  SearchResult, 
  FileRecord, 
  Tag, 
  FileTagRelation,
  FileRelation 
} from '../../types';
import { getFileTags, getRelations, getAssetUrl, confirmTag, rejectTag } from '../../api/tauri';
import './FileGrid.css';

export interface FileGridProps {
  files: SearchResult[];
  onFileSelect?: (file: FileRecord) => void;
  onFileOpen?: (file: FileRecord) => void;
  onTagClick?: (tag: Tag) => void;
  onRelationClick?: (file: FileRecord, relation: FileRelation) => void;
  viewMode?: 'grid' | 'list';
  showTags?: boolean;
  showRelations?: boolean;
}

interface FileItemData {
  result: SearchResult;
  tags: FileTagRelation[];
  relations: FileRelation[];
  isLoadingTags: boolean;
  isLoadingRelations: boolean;
}

export function FileGrid(props: FileGridProps) {
  const [selectedFileId, setSelectedFileId] = createSignal<string | null>(null);
  const [fileData, setFileData] = createSignal<Map<string, FileItemData>>(new Map());
  const [hoveredFileId, setHoveredFileId] = createSignal<string | null>(null);

  // Initialize file data when files change
  createEffect(() => {
    const newData = new Map<string, FileItemData>();
    for (const result of props.files) {
      const existing = fileData().get(result.file.id);
      newData.set(result.file.id, existing || {
        result,
        tags: [],
        relations: [],
        isLoadingTags: false,
        isLoadingRelations: false,
      });
    }
    setFileData(newData);
  });

  // Load tags and relations for a file
  const loadFileDetails = async (fileId: string) => {
    const data = fileData().get(fileId);
    if (!data || data.isLoadingTags) return;

    // Update loading state
    setFileData(prev => {
      const newMap = new Map(prev);
      const item = newMap.get(fileId);
      if (item) {
        newMap.set(fileId, { ...item, isLoadingTags: true, isLoadingRelations: true });
      }
      return newMap;
    });

    try {
      const [tags, relations] = await Promise.all([
        props.showTags !== false ? getFileTags(fileId) : Promise.resolve([]),
        props.showRelations !== false ? getRelations(fileId) : Promise.resolve([]),
      ]);

      setFileData(prev => {
        const newMap = new Map(prev);
        const item = newMap.get(fileId);
        if (item) {
          newMap.set(fileId, {
            ...item,
            tags,
            relations,
            isLoadingTags: false,
            isLoadingRelations: false,
          });
        }
        return newMap;
      });
    } catch (error) {
      console.error('Failed to load file details:', error);
      setFileData(prev => {
        const newMap = new Map(prev);
        const item = newMap.get(fileId);
        if (item) {
          newMap.set(fileId, { ...item, isLoadingTags: false, isLoadingRelations: false });
        }
        return newMap;
      });
    }
  };

  // Handle file selection
  const handleFileSelect = (file: FileRecord) => {
    setSelectedFileId(file.id);
    loadFileDetails(file.id);
    props.onFileSelect?.(file);
  };

  // Handle file double-click (open)
  const handleFileOpen = (file: FileRecord) => {
    props.onFileOpen?.(file);
  };

  // Handle tag confirmation
  const handleConfirmTag = async (fileId: string, tagId: string, e: MouseEvent) => {
    e.stopPropagation();
    try {
      await confirmTag(fileId, tagId);
      // Reload tags
      loadFileDetails(fileId);
    } catch (error) {
      console.error('Failed to confirm tag:', error);
    }
  };

  // Handle tag rejection
  const handleRejectTag = async (fileId: string, tagId: string, e: MouseEvent) => {
    e.stopPropagation();
    try {
      await rejectTag(fileId, tagId);
      // Reload tags
      loadFileDetails(fileId);
    } catch (error) {
      console.error('Failed to reject tag:', error);
    }
  };

  // Get file type icon
  const getFileIcon = (fileType: string): string => {
    const icons: Record<string, string> = {
      Document: '📄',
      Image: '🖼️',
      Video: '🎬',
      Audio: '🎵',
      Code: '💻',
      Archive: '📦',
      Model3D: '🎮',
      Other: '📁',
    };
    return icons[fileType] || '📁';
  };

  // Get relation type label
  const getRelationLabel = (type: string): string => {
    const labels: Record<string, string> = {
      ContentSimilar: '内容相似',
      SameSession: '同会话',
      SameProject: '同项目',
      SameAuthor: '同作者',
      Reference: '引用',
      Derivative: '衍生',
      Workflow: '工作流',
      UserDefined: '用户定义',
    };
    return labels[type] || type;
  };

  // Format file size
  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  };

  // Format date
  const formatDate = (dateStr: string): string => {
    const date = new Date(dateStr);
    return date.toLocaleDateString('zh-CN', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  };

  return (
    <div class={`file-grid ${props.viewMode === 'list' ? 'list-view' : 'grid-view'}`}>
      <For each={props.files}>
        {(result) => {
          const data = () => fileData().get(result.file.id);
          const isSelected = () => selectedFileId() === result.file.id;
          const isHovered = () => hoveredFileId() === result.file.id;
          
          // Separate confirmed and suggested tags
          const confirmedTags = () => data()?.tags.filter(t => t.is_confirmed) || [];
          const suggestedTags = () => data()?.tags.filter(t => !t.is_confirmed && !t.is_rejected) || [];
          const relations = () => data()?.relations || [];

          return (
            <div
              class={`file-item ${isSelected() ? 'selected' : ''}`}
              onClick={() => handleFileSelect(result.file)}
              onDblClick={() => handleFileOpen(result.file)}
              onMouseEnter={() => {
                setHoveredFileId(result.file.id);
                loadFileDetails(result.file.id);
              }}
              onMouseLeave={() => setHoveredFileId(null)}
            >
              {/* Thumbnail / Icon */}
              <div class="file-thumbnail">
                <Show
                  when={result.file.file_type === 'Image'}
                  fallback={
                    <span class="file-icon">{getFileIcon(result.file.file_type)}</span>
                  }
                >
                  <img
                    src={getAssetUrl(`thumbnail/${result.file.id}`)}
                    alt={result.file.filename}
                    loading="lazy"
                    onError={(e) => {
                      (e.target as HTMLImageElement).style.display = 'none';
                      (e.target as HTMLImageElement).nextElementSibling?.classList.remove('hidden');
                    }}
                  />
                  <span class="file-icon hidden">{getFileIcon(result.file.file_type)}</span>
                </Show>
                
                {/* Score indicator */}
                <Show when={result.score > 0}>
                  <div class="score-indicator" title={`相关度: ${(result.score * 100).toFixed(0)}%`}>
                    <div class="score-bar" style={{ width: `${result.score * 100}%` }} />
                  </div>
                </Show>
              </div>

              {/* File Info */}
              <div class="file-info">
                <h4 class="file-name" title={result.file.filename}>
                  {result.file.filename}
                </h4>
                
                <div class="file-meta">
                  <span class="file-size">{formatFileSize(result.file.size_bytes)}</span>
                  <span class="file-date">{formatDate(result.file.modified_at)}</span>
                </div>

                {/* Preview snippet for content search */}
                <Show when={result.matched_chunk}>
                  <div class="content-preview">
                    <p class="preview-text">
                      {result.matched_chunk!.content.slice(0, 150)}
                      {result.matched_chunk!.content.length > 150 ? '...' : ''}
                    </p>
                  </div>
                </Show>

                {/* Tags Section */}
                <Show when={props.showTags !== false && (confirmedTags().length > 0 || suggestedTags().length > 0)}>
                  <div class="file-tags">
                    {/* Confirmed Tags */}
                    <For each={confirmedTags()}>
                      {(tagRelation) => (
                        <span
                          class="tag confirmed"
                          onClick={(e) => {
                            e.stopPropagation();
                            // Find the tag from result.tags
                            const tag = result.tags.find(t => t.id === tagRelation.tag_id);
                            if (tag) props.onTagClick?.(tag);
                          }}
                        >
                          {result.tags.find(t => t.id === tagRelation.tag_id)?.name || tagRelation.tag_id}
                        </span>
                      )}
                    </For>

                    {/* Suggested Tags (AI-generated, not confirmed) */}
                    <For each={suggestedTags()}>
                      {(tagRelation) => (
                        <span class="tag suggested">
                          <span class="tag-name">
                            {result.tags.find(t => t.id === tagRelation.tag_id)?.name || tagRelation.tag_id}
                          </span>
                          <Show when={tagRelation.confidence}>
                            <span class="tag-confidence">
                              {(tagRelation.confidence! * 100).toFixed(0)}%
                            </span>
                          </Show>
                          <span class="tag-actions">
                            <button
                              class="tag-action confirm"
                              onClick={(e) => handleConfirmTag(result.file.id, tagRelation.tag_id, e)}
                              title="确认标签"
                            >
                              ✓
                            </button>
                            <button
                              class="tag-action reject"
                              onClick={(e) => handleRejectTag(result.file.id, tagRelation.tag_id, e)}
                              title="拒绝标签"
                            >
                              ✕
                            </button>
                          </span>
                        </span>
                      )}
                    </For>
                  </div>
                </Show>

                {/* Relations Section */}
                <Show when={props.showRelations !== false && relations().length > 0}>
                  <div class="file-relations">
                    <span class="relations-label">关联:</span>
                    <div class="relations-list">
                      <For each={relations().slice(0, 3)}>
                        {(relation) => (
                          <span
                            class={`relation-badge ${relation.user_feedback.type.toLowerCase()}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              props.onRelationClick?.(result.file, relation);
                            }}
                            title={getRelationLabel(relation.relation_type)}
                          >
                            🔗 {getRelationLabel(relation.relation_type)}
                          </span>
                        )}
                      </For>
                      <Show when={relations().length > 3}>
                        <span class="relations-more">
                          +{relations().length - 3} 更多
                        </span>
                      </Show>
                    </div>
                  </div>
                </Show>
              </div>

              {/* Source indicator */}
              <Show when={result.source !== 'LocalVector'}>
                <div class="source-badge" title={`来源: ${result.source}`}>
                  {result.source === 'CloudEnhanced' ? '☁️' : '🏷️'}
                </div>
              </Show>
            </div>
          );
        }}
      </For>

      {/* Empty state */}
      <Show when={props.files.length === 0}>
        <div class="empty-state">
          <span class="empty-icon">📂</span>
          <p class="empty-text">没有找到文件</p>
        </div>
      </Show>
    </div>
  );
}

export default FileGrid;
