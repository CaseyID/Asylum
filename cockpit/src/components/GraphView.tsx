import "@xyflow/react/dist/style.css";
import {
  type Edge,
  Handle,
  ReactFlow,
  type Node,
  type NodeProps,
  type ReactFlowInstance,
  useEdgesState,
  useNodesState,
  Controls,
  Background,
  Position,
} from "@xyflow/react";
import { memo, type FC, type MouseEvent, useEffect, useMemo, useRef, useState } from "react";
import { type GraphFlow } from "../api";

type AsylumNodeData = GraphFlow["nodes"][number]["data"];

const AsylumGraphNode = memo(({ data, selected }: NodeProps<Node<AsylumNodeData>>) => {
  const node = data.node;
  return (
    <div className={`asylum-flow-node ${selected ? "selected" : ""} state-${node.liveness}`}>
      <Handle type="target" position={Position.Left} />
      <div className="node-shell-row">
        <span className="status-dot" />
        <span className="node-role">{node.role_hint}</span>
        <span className="node-state">{node.liveness}</span>
      </div>
      <div className="node-meta">
        <span>{node.harness}</span>
        <span>{node.substrate}</span>
        <span>{node.id.slice(0, 8)}</span>
      </div>
      <p>{node.output_preview ?? node.description ?? "waiting for output"}</p>
      <Handle type="source" position={Position.Right} />
    </div>
  );
});

const nodeTypes = { asylum: AsylumGraphNode };

export interface GraphViewProps {
  flow: GraphFlow;
  selectedNodeId?: string;
  onSelectNode: (id: string) => void;
}

export const GraphView: FC<GraphViewProps> = ({ flow, selectedNodeId, onSelectNode }) => {
  const [flowApi, setFlowApi] = useState<ReactFlowInstance | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [wrapWidth, setWrapWidth] = useState(0);
  const initialNodes: Node<Record<string, unknown>>[] = useMemo(
    () => {
      const compact = wrapWidth > 0 && wrapWidth < 560;
      return flow.nodes.map((node, index) => ({
        ...node,
        position: compact ? { x: 0, y: index * 150 } : node.position,
        selected: node.id === selectedNodeId,
      }));
    },
    [flow.nodes, selectedNodeId, wrapWidth],
  );
  const initialEdges: Edge[] = useMemo(() => flow.edges, [flow.edges]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges] = useEdgesState(initialEdges);

  useEffect(() => {
    if (!wrapRef.current) return;
    const observer = new ResizeObserver(([entry]) => {
      setWrapWidth(entry.contentRect.width);
    });
    observer.observe(wrapRef.current);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    setNodes(initialNodes);
  }, [setNodes, initialNodes]);

  useEffect(() => {
    const handle = window.requestAnimationFrame(() => {
      flowApi?.fitView({ duration: 150, padding: wrapWidth < 560 ? 0.08 : 0.16 });
    });
    return () => window.cancelAnimationFrame(handle);
  }, [flowApi, flow.nodes.length, flow.edges.length, wrapWidth]);

  useEffect(() => {
    setEdges(
      initialEdges.map((edge) => ({
        ...edge,
        className: selectedNodeId && (edge.source === selectedNodeId || edge.target === selectedNodeId) ? "active-edge" : "",
      })),
    );
  }, [initialEdges, selectedNodeId, setEdges]);

  return (
    <div className="graph-card">
      <div className="graph-toolbar">
        <span className="graph-toolbar-title">Node Graph</span>
        <button
          type="button"
          className="ghost-btn"
          onClick={() => flowApi?.fitView({ padding: 0.2 })}
        >
          Reset zoom
        </button>
      </div>
      <div className="graph-view-wrap" ref={wrapRef}>
        <ReactFlow
          onInit={(instance) => {
            setFlowApi(instance);
            window.requestAnimationFrame(() => instance.fitView({ padding: 0.16 }));
          }}
          nodeTypes={nodeTypes}
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onNodeClick={(_event: MouseEvent, node) => onSelectNode(node.id)}
          fitView
          panOnDrag={true}
          zoomOnScroll={true}
          attributionPosition="bottom-left"
        >
          <Controls />
          <Background gap={18} color="#2b3040" />
        </ReactFlow>
      </div>
    </div>
  );
};
