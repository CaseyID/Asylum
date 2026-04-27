import "@xyflow/react/dist/style.css";
import {
  type Edge,
  ReactFlow,
  type Node,
  type ReactFlowInstance,
  useEdgesState,
  useNodesState,
  MiniMap,
  Controls,
  Background,
} from "@xyflow/react";
import { type FC, type MouseEvent, useEffect, useMemo, useState } from "react";
import { type GraphFlow } from "../api";

export interface GraphViewProps {
  flow: GraphFlow;
  selectedNodeId?: string;
  onSelectNode: (id: string) => void;
}

export const GraphView: FC<GraphViewProps> = ({ flow, selectedNodeId, onSelectNode }) => {
  const [flowApi, setFlowApi] = useState<ReactFlowInstance | null>(null);
  const initialNodes: Node<Record<string, unknown>>[] = useMemo(
    () =>
      flow.nodes.map((node) => ({
        ...node,
        selected: node.id === selectedNodeId,
      })),
    [flow.nodes, selectedNodeId],
  );
  const initialEdges: Edge[] = useMemo(() => flow.edges, [flow.edges]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges] = useEdgesState(initialEdges);

  useEffect(() => {
    setNodes(initialNodes);
  }, [setNodes, initialNodes]);

  useEffect(() => {
    flowApi?.fitView({ duration: 150 });
  }, [flow.nodes.length, flow.edges.length]);

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
      <div className="graph-view-wrap">
        <ReactFlow
          onInit={setFlowApi}
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onNodeClick={(_event: MouseEvent, node) => onSelectNode(node.id)}
          fitView
          panOnDrag={true}
          zoomOnScroll={true}
          attributionPosition="bottom-left"
        >
          <MiniMap pannable zoomable />
          <Controls />
          <Background gap={18} color="#2b3040" />
        </ReactFlow>
      </div>
    </div>
  );
};
