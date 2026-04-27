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

export interface OptionListResponse<T extends string = string> {
  items: T[];
}

export interface AttachBrowserResponse {
  token?: string;
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
const AUTH_TOKEN_KEY = "asylum.ownerToken";

type Jsonish = Record<string, unknown>;

export class ApiError extends Error {
  status: number;
  statusText: string;
  body: string;

  constructor(status: number, statusText: string, body: string) {
    super(`${status} ${statusText}: ${body}`);
    this.name = "ApiError";
    this.status = status;
    this.statusText = statusText;
    this.body = body;
  }
}

export function getStoredOwnerToken(): string {
  if (typeof window === "undefined") return "";
  return window.localStorage.getItem(AUTH_TOKEN_KEY) ?? "";
}

export function setStoredOwnerToken(token: string): void {
  if (typeof window === "undefined") return;
  const trimmed = token.trim();
  if (trimmed) {
    window.localStorage.setItem(AUTH_TOKEN_KEY, trimmed);
  } else {
    window.localStorage.removeItem(AUTH_TOKEN_KEY);
  }
}

export function hydrateOwnerTokenFromLocation(): string {
  if (typeof window === "undefined") return "";
  const url = new URL(window.location.href);
  const token = url.searchParams.get("token") ?? "";
  if (token.trim()) {
    setStoredOwnerToken(token);
    url.searchParams.delete("token");
    window.history.replaceState({}, "", `${url.pathname}${url.search}${url.hash}`);
    return token.trim();
  }
  return getStoredOwnerToken();
}

async function parseResponseBody<T>(res: Response): Promise<T> {
  if (res.status === 204) {
    return undefined as T;
  }
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, res.statusText, body);
  }
  const data = (await res.json()) as T;
  return data;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = getStoredOwnerToken();
  const res = await fetch(`${BASE}${path}`, {
    headers: {
      "content-type": "application/json",
      accept: "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(init.headers ?? {}),
    },
    ...init,
  });
  return parseResponseBody<T>(res);
}

export async function fetchGraph(): Promise<GraphResponse> {
  const data = await request<GraphResponse | { graph: GraphResponse }>("/graph");
  if ("graph" in data) {
    return data.graph;
  }
  return data;
}

export async function fetchNotifications(): Promise<NotificationRecord[]> {
  const data = await request<NotificationRecord[] | { notifications: unknown[] }>("/notifications");
  const records = Array.isArray(data) ? data : data.notifications;
  return records.map((item) => {
    const raw = item as Record<string, unknown>;
    return {
      id: String(raw.id),
      node_id: raw.node_id ? String(raw.node_id) : null,
      title: String(raw.title ?? "System event"),
      body: String(raw.body ?? ""),
      severity: String(raw.severity ?? raw.kind ?? "info"),
      created_at: raw.created_at
        ? String(raw.created_at)
        : new Date(Number(raw.created_at_epoch_secs ?? 0) * 1000).toISOString(),
      read: Boolean(raw.read ?? raw.read_at_epoch_secs),
    };
  });
}

export async function fetchHarnesses(): Promise<HarnessKind[]> {
  const data = await request<OptionListResponse<HarnessKind> | HarnessKind[]>("/harnesses");
  return Array.isArray(data) ? data : data.items;
}

export async function fetchSubstrates(): Promise<SubstrateKind[]> {
  const data = await request<OptionListResponse<SubstrateKind> | SubstrateKind[]>("/substrates");
  return Array.isArray(data) ? data : data.items;
}

export async function markNotificationRead(id: string): Promise<void> {
  await request<void>(`/notifications/${id}/read`, { method: "POST" });
}

export async function fetchNode(id: string): Promise<AsylumNode> {
  const data = await request<AsylumNode | { node: AsylumNode }>(`/nodes/${id}`);
  const maybeWrapped = data as { node?: AsylumNode };
  if (maybeWrapped.node) {
    return maybeWrapped.node;
  }
  return data as AsylumNode;
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
  const created = await request<AsylumNode | { node_id: string }>("/nodes", {
    method: "POST",
    body: JSON.stringify(payload),
  });
  const maybeCreated = created as { node_id?: string };
  if (maybeCreated.node_id) {
    return fetchNode(maybeCreated.node_id);
  }
  return created as AsylumNode;
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
    body: JSON.stringify({ text: input }),
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
  const data = await request<AttachBrowserResponse | { url: string; expires_in_seconds: number }>(`/nodes/${nodeId}/attach/browser`, {
    method: "POST",
  });
  if ("url" in data) {
    const token = data.url.split("/attach/")[1]?.split(/[/?#]/)[0];
    return { attach_url: data.url, token };
  }
  return data;
}

export async function requestNativeTarget(nodeId: string): Promise<NativeTargetResponse> {
  const data = await request<NativeTargetResponse | { command: string; args: string[]; environment: Record<string, string> }>(`/nodes/${nodeId}/attach/native-target`, {
    method: "POST",
  });
  if ("environment" in data) {
    return { command: data.command, args: data.args, env: data.environment };
  }
  return data;
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
      type: "asylum",
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
      labelBgPadding: [6, 3] as [number, number],
      labelBgBorderRadius: 4,
      labelBgStyle: { fill: "#05080c", fillOpacity: 0.95 },
      labelStyle: { fill: "#c8d5e3", fontSize: 11, fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace" },
      style: { stroke: "#4d667e", strokeWidth: 1.4 },
    }));

  return { nodes: flowNodes, edges };
}
