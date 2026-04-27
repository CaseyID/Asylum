import { create } from "zustand";
import { type AsylumNode, type GraphResponse } from "./api";

export interface CockpitState {
  graph: GraphResponse;
  selectedNodeId?: string;
  commandCenterNodeId?: string;
  loading: boolean;
  initializeGraph: (graph: GraphResponse) => void;
  setSelectedNode: (nodeId?: string) => void;
  setCommandCenterSelection: (nodeId?: string) => void;
}

export const useCockpitStore = create<CockpitState>((set, get) => ({
  graph: { nodes: [], relationships: [] },
  loading: true,
  initializeGraph(graph) {
    const commandCenter = selectCommandCenter(graph.nodes);
    const existing = get().selectedNodeId;
    const selectedNodeId = graph.nodes.some((node) => node.id === existing) ? existing : commandCenter?.id ?? graph.nodes[0]?.id;
    set({
      graph,
      loading: false,
      selectedNodeId,
      commandCenterNodeId: commandCenter?.id,
    });
  },
  setSelectedNode(nodeId) {
    set({ selectedNodeId: nodeId });
  },
  setCommandCenterSelection(nodeId) {
    set({ commandCenterNodeId: nodeId });
  },
}));

export interface NodeSelectorItem {
  id: string;
  role_hint: AsylumNode["role_hint"];
  liveness: AsylumNode["liveness"];
}

export function selectCommandCenter(nodes: readonly NodeSelectorItem[]): NodeSelectorItem | undefined {
  return (
    nodes.find(
      (node) => node.role_hint === "command-center" && node.liveness === "running",
    ) ??
    nodes.find((node) => node.role_hint === "command-center") ??
    nodes.find((node) => node.liveness === "running")
  );
}

export const isOperational = (node: AsylumNode): boolean =>
  node.liveness === "running" || node.liveness === "waiting_for_input";
