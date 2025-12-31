/**
 * RelationGraph Component
 * 
 * Implements:
 * - Visual relation graph between files
 * - One-click relation removal (Human-in-the-Loop)
 * - Relation strength visualization
 * 
 * Requirements: 6.1, Human-in-the-Loop
 */

import { createSignal, createEffect, Show, For, onMount, onCleanup } from 'solid-js';
import type { 
  RelationGraph as RelationGraphData, 
  RelationNode, 
  RelationEdge,
  RelationType,
  UserFeedback 
} from '../../types';
import { getRelationGraph, confirmRelation, rejectRelation, blockRelation } from '../../api/tauri';
import './RelationGraph.css';

export interface RelationGraphProps {
  fileId: string;
  depth?: number;
  onNodeClick?: (node: RelationNode) => void;
  onRelationChange?: () => void;
  width?: number;
  height?: number;
}

interface GraphNode extends RelationNode {
  x: number;
  y: number;
  vx: number;
  vy: number;
  isCenter: boolean;
  isDragging: boolean;
}

interface GraphEdge extends RelationEdge {
  sourceNode: GraphNode;
  targetNode: GraphNode;
}

export function RelationGraph(props: RelationGraphProps) {
  const [graphData, setGraphData] = createSignal<RelationGraphData | null>(null);
  const [nodes, setNodes] = createSignal<GraphNode[]>([]);
  const [edges, setEdges] = createSignal<GraphEdge[]>([]);
  const [selectedEdge, setSelectedEdge] = createSignal<GraphEdge | null>(null);
  const [hoveredNode, setHoveredNode] = createSignal<GraphNode | null>(null);
  const [isLoading, setIsLoading] = createSignal(true);
  const [showContextMenu, setShowContextMenu] = createSignal(false);
  const [contextMenuPos, setContextMenuPos] = createSignal({ x: 0, y: 0 });
  const [draggedNode, setDraggedNode] = createSignal<GraphNode | null>(null);

  let svgRef: SVGSVGElement | undefined;
  let animationFrame: number | undefined;

  const width = () => props.width || 600;
  const height = () => props.height || 400;

  // Load graph data
  const loadGraph = async () => {
    setIsLoading(true);
    try {
      const data = await getRelationGraph(props.fileId, props.depth || 2);
      setGraphData(data);
      initializeGraph(data);
    } catch (error) {
      console.error('Failed to load relation graph:', error);
    } finally {
      setIsLoading(false);
    }
  };

  // Initialize graph layout
  const initializeGraph = (data: RelationGraphData) => {
    const centerX = width() / 2;
    const centerY = height() / 2;

    // Create graph nodes with positions
    const graphNodes: GraphNode[] = data.nodes.map((node, index) => {
      const isCenter = node.file_id === data.center_file_id;
      const angle = (2 * Math.PI * index) / data.nodes.length;
      const radius = isCenter ? 0 : 150;

      return {
        ...node,
        x: centerX + radius * Math.cos(angle),
        y: centerY + radius * Math.sin(angle),
        vx: 0,
        vy: 0,
        isCenter,
        isDragging: false,
      };
    });

    // Create graph edges with node references
    const nodeMap = new Map(graphNodes.map(n => [n.file_id, n]));
    const graphEdges: GraphEdge[] = data.edges
      .filter(edge => edge.user_feedback.type !== 'Rejected')
      .map(edge => ({
        ...edge,
        sourceNode: nodeMap.get(edge.source)!,
        targetNode: nodeMap.get(edge.target)!,
      }))
      .filter(edge => edge.sourceNode && edge.targetNode);

    setNodes(graphNodes);
    setEdges(graphEdges);

    // Start force simulation
    startSimulation();
  };

  // Force-directed layout simulation
  const startSimulation = () => {
    const simulate = () => {
      const currentNodes = nodes();
      const currentEdges = edges();

      if (currentNodes.length === 0) return;

      const centerX = width() / 2;
      const centerY = height() / 2;

      // Apply forces
      const newNodes = currentNodes.map(node => {
        if (node.isDragging) return node;

        let fx = 0;
        let fy = 0;

        // Center gravity for center node
        if (node.isCenter) {
          fx += (centerX - node.x) * 0.1;
          fy += (centerY - node.y) * 0.1;
        }

        // Repulsion between nodes
        for (const other of currentNodes) {
          if (other.file_id === node.file_id) continue;

          const dx = node.x - other.x;
          const dy = node.y - other.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 1;
          const force = 1000 / (dist * dist);

          fx += (dx / dist) * force;
          fy += (dy / dist) * force;
        }

        // Attraction along edges
        for (const edge of currentEdges) {
          let other: GraphNode | null = null;
          if (edge.sourceNode.file_id === node.file_id) {
            other = edge.targetNode;
          } else if (edge.targetNode.file_id === node.file_id) {
            other = edge.sourceNode;
          }

          if (other) {
            const dx = other.x - node.x;
            const dy = other.y - node.y;
            const dist = Math.sqrt(dx * dx + dy * dy) || 1;
            const targetDist = 120 + (1 - edge.strength) * 80;
            const force = (dist - targetDist) * 0.02;

            fx += (dx / dist) * force;
            fy += (dy / dist) * force;
          }
        }

        // Apply velocity with damping
        const vx = (node.vx + fx) * 0.8;
        const vy = (node.vy + fy) * 0.8;

        // Update position with bounds
        const padding = 40;
        const x = Math.max(padding, Math.min(width() - padding, node.x + vx));
        const y = Math.max(padding, Math.min(height() - padding, node.y + vy));

        return { ...node, x, y, vx, vy };
      });

      setNodes(newNodes);

      // Update edge references
      const nodeMap = new Map(newNodes.map(n => [n.file_id, n]));
      setEdges(currentEdges.map(edge => ({
        ...edge,
        sourceNode: nodeMap.get(edge.source)!,
        targetNode: nodeMap.get(edge.target)!,
      })));

      animationFrame = requestAnimationFrame(simulate);
    };

    simulate();
  };

  // Handle node drag
  const handleNodeMouseDown = (node: GraphNode, e: MouseEvent) => {
    e.preventDefault();
    setDraggedNode(node);
    setNodes(prev => prev.map(n => 
      n.file_id === node.file_id ? { ...n, isDragging: true } : n
    ));
  };

  const handleMouseMove = (e: MouseEvent) => {
    const dragged = draggedNode();
    if (!dragged || !svgRef) return;

    const rect = svgRef.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    setNodes(prev => prev.map(n =>
      n.file_id === dragged.file_id ? { ...n, x, y, vx: 0, vy: 0 } : n
    ));
  };

  const handleMouseUp = () => {
    const dragged = draggedNode();
    if (dragged) {
      setNodes(prev => prev.map(n =>
        n.file_id === dragged.file_id ? { ...n, isDragging: false } : n
      ));
      setDraggedNode(null);
    }
  };

  // Handle edge click (show context menu)
  const handleEdgeClick = (edge: GraphEdge, e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setSelectedEdge(edge);
    setContextMenuPos({ x: e.clientX, y: e.clientY });
    setShowContextMenu(true);
  };

  // Handle relation actions
  const handleConfirmRelation = async () => {
    const edge = selectedEdge();
    if (!edge) return;

    try {
      await confirmRelation(edge.id);
      setShowContextMenu(false);
      props.onRelationChange?.();
      await loadGraph();
    } catch (error) {
      console.error('Failed to confirm relation:', error);
    }
  };

  const handleRejectRelation = async (blockSimilar: boolean = false) => {
    const edge = selectedEdge();
    if (!edge) return;

    try {
      await rejectRelation(edge.id, undefined, blockSimilar);
      setShowContextMenu(false);
      props.onRelationChange?.();
      await loadGraph();
    } catch (error) {
      console.error('Failed to reject relation:', error);
    }
  };

  const handleBlockRelation = async () => {
    const edge = selectedEdge();
    if (!edge) return;

    try {
      await blockRelation(edge.id, { type: 'ThisPairOnly' });
      setShowContextMenu(false);
      props.onRelationChange?.();
      await loadGraph();
    } catch (error) {
      console.error('Failed to block relation:', error);
    }
  };

  // Get relation type label
  const getRelationLabel = (type: RelationType): string => {
    const labels: Record<RelationType, string> = {
      ContentSimilar: '内容相似',
      SameSession: '同会话',
      SameProject: '同项目',
      SameAuthor: '同作者',
      Reference: '引用',
      Derivative: '衍生',
      Workflow: '工作流',
      UserDefined: '用户定义',
    };
    return labels[type];
  };

  // Get edge color based on relation type
  const getEdgeColor = (type: RelationType): string => {
    const colors: Record<RelationType, string> = {
      ContentSimilar: '#4a90d9',
      SameSession: '#9c27b0',
      SameProject: '#4caf50',
      SameAuthor: '#ff9800',
      Reference: '#00bcd4',
      Derivative: '#e91e63',
      Workflow: '#795548',
      UserDefined: '#607d8b',
    };
    return colors[type];
  };

  // Get feedback indicator
  const getFeedbackIndicator = (feedback: UserFeedback): string => {
    switch (feedback.type) {
      case 'Confirmed': return '✓';
      case 'Adjusted': return '~';
      default: return '';
    }
  };

  // Close context menu on outside click
  const handleOutsideClick = () => {
    setShowContextMenu(false);
    setSelectedEdge(null);
  };

  // Lifecycle
  onMount(() => {
    loadGraph();
    document.addEventListener('click', handleOutsideClick);
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  });

  onCleanup(() => {
    if (animationFrame) {
      cancelAnimationFrame(animationFrame);
    }
    document.removeEventListener('click', handleOutsideClick);
    document.removeEventListener('mousemove', handleMouseMove);
    document.removeEventListener('mouseup', handleMouseUp);
  });

  // Reload when fileId changes
  createEffect(() => {
    props.fileId;
    loadGraph();
  });

  return (
    <div class="relation-graph-container">
      <Show when={isLoading()}>
        <div class="loading-overlay">
          <span class="loading-spinner">⏳</span>
          <span>加载关联图...</span>
        </div>
      </Show>

      <Show when={!isLoading() && nodes().length === 0}>
        <div class="empty-state">
          <span class="empty-icon">🔗</span>
          <p>暂无关联文件</p>
        </div>
      </Show>

      <Show when={!isLoading() && nodes().length > 0}>
        <svg
          ref={svgRef}
          class="relation-graph"
          width={width()}
          height={height()}
          viewBox={`0 0 ${width()} ${height()}`}
        >
          {/* Edges */}
          <g class="edges">
            <For each={edges()}>
              {(edge) => {
                const isSelected = selectedEdge()?.id === edge.id;
                const strokeWidth = 1 + edge.strength * 3;
                const color = getEdgeColor(edge.relation_type);

                return (
                  <g class={`edge ${isSelected ? 'selected' : ''}`}>
                    <line
                      x1={edge.sourceNode.x}
                      y1={edge.sourceNode.y}
                      x2={edge.targetNode.x}
                      y2={edge.targetNode.y}
                      stroke={color}
                      stroke-width={strokeWidth}
                      stroke-opacity={0.6}
                      class="edge-line"
                      onClick={(e) => handleEdgeClick(edge, e)}
                    />
                    {/* Edge label */}
                    <text
                      x={(edge.sourceNode.x + edge.targetNode.x) / 2}
                      y={(edge.sourceNode.y + edge.targetNode.y) / 2 - 8}
                      class="edge-label"
                      fill={color}
                    >
                      {getRelationLabel(edge.relation_type)}
                      {getFeedbackIndicator(edge.user_feedback)}
                    </text>
                    {/* Strength indicator */}
                    <text
                      x={(edge.sourceNode.x + edge.targetNode.x) / 2}
                      y={(edge.sourceNode.y + edge.targetNode.y) / 2 + 8}
                      class="edge-strength"
                    >
                      {(edge.strength * 100).toFixed(0)}%
                    </text>
                  </g>
                );
              }}
            </For>
          </g>

          {/* Nodes */}
          <g class="nodes">
            <For each={nodes()}>
              {(node) => {
                const isHovered = hoveredNode()?.file_id === node.file_id;
                const radius = node.isCenter ? 35 : 28;

                return (
                  <g
                    class={`node ${node.isCenter ? 'center' : ''} ${isHovered ? 'hovered' : ''}`}
                    transform={`translate(${node.x}, ${node.y})`}
                    onMouseDown={(e) => handleNodeMouseDown(node, e)}
                    onMouseEnter={() => setHoveredNode(node)}
                    onMouseLeave={() => setHoveredNode(null)}
                    onClick={() => props.onNodeClick?.(node)}
                  >
                    {/* Node circle */}
                    <circle
                      r={radius}
                      class="node-circle"
                    />
                    
                    {/* Thumbnail or icon */}
                    <Show
                      when={node.thumbnail_url}
                      fallback={
                        <text class="node-icon" dy="0.35em">
                          {node.file_type === 'Image' ? '🖼️' :
                           node.file_type === 'Document' ? '📄' :
                           node.file_type === 'Video' ? '🎬' :
                           node.file_type === 'Code' ? '💻' : '📁'}
                        </text>
                      }
                    >
                      <clipPath id={`clip-${node.file_id}`}>
                        <circle r={radius - 2} />
                      </clipPath>
                      <image
                        href={node.thumbnail_url}
                        x={-(radius - 2)}
                        y={-(radius - 2)}
                        width={(radius - 2) * 2}
                        height={(radius - 2) * 2}
                        clip-path={`url(#clip-${node.file_id})`}
                      />
                    </Show>

                    {/* Node label */}
                    <text
                      class="node-label"
                      y={radius + 14}
                    >
                      {node.filename.length > 15 
                        ? node.filename.slice(0, 12) + '...' 
                        : node.filename}
                    </text>

                    {/* Center indicator */}
                    <Show when={node.isCenter}>
                      <circle
                        r={radius + 4}
                        class="center-indicator"
                        fill="none"
                        stroke="var(--primary-color, #4a90d9)"
                        stroke-width="2"
                        stroke-dasharray="4 2"
                      />
                    </Show>
                  </g>
                );
              }}
            </For>
          </g>
        </svg>

        {/* Legend */}
        <div class="graph-legend">
          <span class="legend-title">关联类型:</span>
          <For each={['ContentSimilar', 'SameSession', 'SameProject', 'Reference'] as RelationType[]}>
            {(type) => (
              <span class="legend-item">
                <span
                  class="legend-color"
                  style={{ background: getEdgeColor(type) }}
                />
                {getRelationLabel(type)}
              </span>
            )}
          </For>
        </div>

        {/* Context Menu */}
        <Show when={showContextMenu() && selectedEdge()}>
          <div
            class="context-menu"
            style={{
              left: `${contextMenuPos().x}px`,
              top: `${contextMenuPos().y}px`,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div class="context-menu-header">
              {getRelationLabel(selectedEdge()!.relation_type)}
              <span class="strength-badge">
                {(selectedEdge()!.strength * 100).toFixed(0)}%
              </span>
            </div>
            
            <button class="menu-item confirm" onClick={handleConfirmRelation}>
              <span class="menu-icon">✓</span>
              确认关联
            </button>
            
            <button class="menu-item reject" onClick={() => handleRejectRelation(false)}>
              <span class="menu-icon">✕</span>
              解除关联
            </button>
            
            <button class="menu-item block" onClick={() => handleRejectRelation(true)}>
              <span class="menu-icon">🚫</span>
              解除并屏蔽类似
            </button>
            
            <button class="menu-item block-pair" onClick={handleBlockRelation}>
              <span class="menu-icon">⛔</span>
              永久屏蔽此对
            </button>
          </div>
        </Show>
      </Show>

      {/* Instructions */}
      <div class="graph-instructions">
        💡 点击连线管理关联，拖拽节点调整布局
      </div>
    </div>
  );
}

export default RelationGraph;
