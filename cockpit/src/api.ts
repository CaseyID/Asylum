import { Edge, type Node } from "@xyflow/react";

export type HarnessKind = "codex" | "claude_code";
export type SubstrateKind = "local" | "loon";
export type NodeLiveness =
  | "starting"
  | "running"
  | "waiting_for_input"
  | "exited"
  | "stopped"
  | "failed"
  | "archived";

export interface CapabilitySnapshot {
  browser_attach: boolean;
  native_attach: boolean;
  send_input: boolean;
  interrupt: boolean;
  stop: boolean;
  resume: boolean;
  structured_events: boolean;
  transcript_export: boolean;
}

export interface AsylumNode {
  [key: string]: unknown;
  id: string;
  harness: HarnessKind;
  substrate: SubstrateKind;
  role_hint: string;
  liveness: NodeLiveness;
  workspace: string | null;
  description: string;
  created_at: string;
  updated_at: string;
  external_id: string | null;
  capabilities: CapabilitySnapshot;
  output_preview?: string;
}

export interface GraphRelationship {
  id: string;
  source_node_id: string;
  target_node_id: string;
  kind: string;
  label?: string | null;
}

export interface GraphResponse {
  nodes: AsylumNode[];
  relationships: GraphRelationship[];
}

export interface NodeListResponse {
  nodes: AsylumNode[];
}

export interface NotificationRecord {
  id: string;
  node_id: string | null;
  title: string;
  body: string;
  severity: "info" | "warn" | "error" | string;
  created_at: string;
  read: boolean;
}

export interface CreateNodeRequest {
  harness: HarnessKind;
  substrate: SubstrateKind;
  role_hint: string;
  workspace?: string;
  description?: string;
}

export interface RelationshipCreateRequest {
  source_node_id: string;
  target_node_id: string;
  kind: string;
}

export interface RelationshipResponse {
  id: string;
  source_node_id: string;
  target_node_id: string;
  kind: string;
}

export interface AttachBrowserResponse {
  token: string;
  attach_url: string;
}

export interface NativeTargetResponse {
  command: string;
  args: string[];
  env: Record<string, string>;
}

export interface GraphFlow {
  nodes: Node<{ node: AsylumNode; label: string; status: NodeLiveness } & Record<string, unknown>>[];
  edges: Edge[];
}

const BASE = "/api";

type Jsonish = Record<string, unknown>;

async function parseResponseBody<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  const data = (await res.json()) as T;
  return data;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: {
      "content-type": "application/json",
      accept: "application/json",
      ...(init.headers ?? {}),
    },
    ...init,
  });
  return parseResponseBody<T>(res);
}

export async function fetchGraph(): Promise<GraphResponse> {
  return request<GraphResponse>("/graph");
}

export async function fetchNotifications(): Promise<NotificationRecord[]> {
  return request<NotificationRecord[]>("/notifications");
}

export async function markNotificationRead(id: string): Promise<void> {
  await request<void>(`/notifications/${id}/read`, { method: "POST" });
}

export async function fetchNode(id: string): Promise<AsylumNode> {
  return request<AsylumNode>(`/nodes/${id}`);
}

export async function fetchNodes(): Promise<AsylumNode[]> {
  const data = await request<GraphResponse | NodeListResponse | AsylumNode[]>("/nodes");
  if ("nodes" in (data as object)) {
    return (data as NodeListResponse).nodes ?? [];
  }
  return (data as AsylumNode[]) || [];
}

export async function createNode(requestBody: CreateNodeRequest): Promise<AsylumNode> {
  const payload = {
    ...requestBody,
    role_hint: requestBody.role_hint.trim() || "worker",
  };
  return request<AsylumNode>("/nodes", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

async function fallbackPostNodeAction(
  nodeId: string,
  action: "interrupt" | "stop" | "archive" | "resume",
  body?: Jsonish,
): Promise<void> {
  const primaryPath = `/nodes/${nodeId}/${action}`;
  try {
    await request<void>(primaryPath, { method: "POST", body: JSON.stringify(body ?? {}) });
    return;
  } catch (err) {
    const message = String((err as Error).message);
    if (!message.startsWith("404")) {
      throw err;
    }
  }
  return request<void>(`/${action}`, {
    method: "POST",
    body: JSON.stringify({ node_id: nodeId, ...(body ?? {}) }),
  });
}

export async function postNodeInput(nodeId: string, input: string): Promise<void> {
  return request<void>(`/nodes/${nodeId}/input`, {
    method: "POST",
    body: JSON.stringify({ input }),
  });
}

export async function interruptNode(nodeId: string): Promise<void> {
  return fallbackPostNodeAction(nodeId, "interrupt");
}

export async function stopNode(nodeId: string): Promise<void> {
  return fallbackPostNodeAction(nodeId, "stop");
}

export async function archiveNode(nodeId: string): Promise<void> {
  return fallbackPostNodeAction(nodeId, "archive");
}

export async function resumeNode(nodeId: string): Promise<void> {
  return fallbackPostNodeAction(nodeId, "resume");
}

export async function requestBrowserAttach(nodeId: string): Promise<AttachBrowserResponse> {
  return request<AttachBrowserResponse>(`/nodes/${nodeId}/attach/browser`, {
    method: "POST",
  });
}

export async function requestNativeTarget(nodeId: string): Promise<NativeTargetResponse> {
  return request<NativeTargetResponse>(`/nodes/${nodeId}/attach/native-target`, {
    method: "POST",
  });
}

export async function createRelationship(requestBody: RelationshipCreateRequest): Promise<RelationshipResponse> {
  return request<RelationshipResponse>("/relationships", {
    method: "POST",
    body: JSON.stringify(requestBody),
  });
}

export async function deleteRelationship(id: string): Promise<void> {
  return request<void>(`/relationships/${id}`, { method: "DELETE" });
}

export function graphToFlow(graph: GraphResponse): GraphFlow {
  const rows = 160;
  const cols = Math.max(1, Math.min(5, Math.ceil(Math.sqrt(graph.nodes.length || 1))));
  const byId = new Map(graph.nodes.map((node) => [node.id, node]));

  const flowNodes: Node<{ node: AsylumNode; label: string; status: NodeLiveness } & Record<string, unknown>>[] = graph.nodes.map(
    (node, index) => {
    const x = (index % cols) * 260;
    const y = Math.floor(index / cols) * rows;
    return {
      id: node.id,
      type: "default",
      position: { x, y },
      data: {
        node,
        label: `${node.role_hint} (${node.harness}/${node.substrate})`,
        status: node.liveness,
      },
      selectable: true,
    };
  });

  const edges: Edge[] = graph.relationships
    .filter((edge) => byId.has(edge.source_node_id) && byId.has(edge.target_node_id))
    .map((edge) => ({
      id: edge.id,
      source: edge.source_node_id,
      target: edge.target_node_id,
      label: edge.kind,
    }));

  return { nodes: flowNodes, edges };
}
