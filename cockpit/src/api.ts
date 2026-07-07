// asylum cockpit — daemon api client

import type {
  AsylumNode,
  AttachBrowserResponse,
  ChannelCreateRequest,
  ChannelDescriptor,
  ChannelMessageRecord,
  ChannelTestRequest,
  ChannelUpdateRequest,
  CreateNodeRequest,
  DecisionCreateRequest,
  DecisionListResponse,
  DecisionRecord,
  DecisionResolveRequest,
  ForkNodeRequest,
  GraphResponse,
  GraphRelationship,
  RelationshipCreateRequest,
  HarnessDescriptor,
  HealthResponse,
  HookCreateRequest,
  HookEventCatalogEntry,
  HookFiringRecord,
  HookRule,
  HookUpdateRequest,
  NotificationRecord,
  SubstrateDescriptor,
  TokenListResponse,

  TokenRotateResponse,
} from "./types";

const BASE = "/api";

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

// M9: keep owner token in module-level memory rather than localStorage.
// Lost on page reload — cockpit re-prompts (acceptable for single-user tool).
// This prevents XSS on same origin from leaking the token via localStorage.
let _ownerToken = "";

export function getStoredOwnerToken(): string {
  return _ownerToken;
}

export function setStoredOwnerToken(token: string): void {
  _ownerToken = token.trim();
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
  if (res.status === 204) return undefined as T;
  if (!res.ok) {
    const body = await res.text();
    throw new ApiError(res.status, res.statusText, body);
  }
  return (await res.json()) as T;
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
  if ("graph" in data) return data.graph;
  return data;
}

export async function fetchNodes(): Promise<AsylumNode[]> {
  const data = await request<{ nodes: AsylumNode[] } | AsylumNode[]>("/nodes");
  if (Array.isArray(data)) return data;
  return data.nodes ?? [];
}

export async function fetchNode(id: string): Promise<AsylumNode> {
  const data = await request<AsylumNode | { node: AsylumNode }>(`/nodes/${id}`);
  return "node" in (data as object) ? (data as { node: AsylumNode }).node : (data as AsylumNode);
}

export async function createRelationship(req: RelationshipCreateRequest): Promise<GraphRelationship> {
  return request<GraphRelationship>("/relationships", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function removeRelationship(id: string): Promise<void> {
  await request<void>(`/relationships/${id}`, {
    method: "DELETE",
  });
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

export async function fetchHarnessDescriptors(): Promise<HarnessDescriptor[]> {
  const data = await request<{ harnesses: HarnessDescriptor[] }>("/harness-descriptors");
  return data.harnesses;
}

export async function fetchSubstrateDescriptors(): Promise<SubstrateDescriptor[]> {
  const data = await request<{ substrates: SubstrateDescriptor[] }>("/substrate-descriptors");
  return data.substrates;
}

export async function markNotificationRead(id: string): Promise<void> {
  await request<void>(`/notifications/${id}/read`, { method: "POST" });
}

export async function createNode(payload: CreateNodeRequest): Promise<AsylumNode> {
  const body = { ...payload, role_hint: payload.role_hint.trim() || "worker" };
  const created = await request<AsylumNode | { node_id: string }>("/nodes", {
    method: "POST",
    body: JSON.stringify(body),
  });
  if ("node_id" in (created as object)) {
    return fetchNode((created as { node_id: string }).node_id);
  }
  return created as AsylumNode;
}

async function postNodeAction(nodeId: string, action: "interrupt" | "stop" | "archive" | "resume"): Promise<void> {
  await request<void>(`/nodes/${nodeId}/${action}`, { method: "POST", body: JSON.stringify({}) });
}

export async function postNodeInput(nodeId: string, input: string): Promise<void> {
  return request<void>(`/nodes/${nodeId}/input`, {
    method: "POST",
    body: JSON.stringify({ text: input }),
  });
}

export const interruptNode = (id: string) => postNodeAction(id, "interrupt");
export const stopNode = (id: string) => postNodeAction(id, "stop");
export const archiveNode = (id: string) => postNodeAction(id, "archive");
// D2: resume a stopped-but-resumable node (has harness_session_id) via the
// daemon's POST /api/nodes/:id/resume. The route is delivered by a parallel
// workstream; this client call is shaped to match the existing node-action
// endpoints (empty JSON body, same auth/error handling as stop/archive).
export const resumeNode = (id: string) => postNodeAction(id, "resume");

export async function requestBrowserAttach(nodeId: string): Promise<AttachBrowserResponse> {
  const data = await request<
    AttachBrowserResponse | { url: string; expires_in_seconds: number; transport?: string | null; note?: string | null }
  >(
    `/nodes/${nodeId}/attach/browser`,
    { method: "POST" },
  );
  if ("url" in data) {
    const token = data.url.split("/attach/")[1]?.split(/[/?#]/)[0];
    return {
      attach_url: data.url,
      token,
      expires_in_seconds: data.expires_in_seconds,
      transport: data.transport ?? null,
      note: data.note ?? null,
    };
  }
  return data;
}

export interface AttachSocketOptions {
  onMessage?: (data: string) => void;
  onError?: (e: Event) => void;
  onClose?: () => void;
  onOpen?: () => void;
}

export function openAttachSocket(token: string, options: AttachSocketOptions = {}): WebSocket {
  const proto = typeof window !== "undefined" && window.location.protocol === "https:" ? "wss" : "ws";
  const host = typeof window !== "undefined" ? window.location.host : "";
  const url = `${proto}://${host}/api/attach/${encodeURIComponent(token)}/ws`;
  const ws = new WebSocket(url);
  if (options.onOpen) ws.addEventListener("open", () => options.onOpen!());
  if (options.onMessage) {
    ws.addEventListener("message", (event: MessageEvent) => {
      options.onMessage!(event.data as string);
    });
  }
  if (options.onError) ws.addEventListener("error", (e: Event) => options.onError!(e));
  if (options.onClose) ws.addEventListener("close", () => options.onClose!());
  return ws;
}

export async function fetchNodeEvents(nodeId: string): Promise<unknown[]> {
  const data = await request<{ events: unknown[] } | unknown[]>(`/nodes/${nodeId}/events`);
  return Array.isArray(data) ? data : data.events;
}

// — channels —

export async function fetchChannels(): Promise<ChannelDescriptor[]> {
  const data = await request<{ channels: ChannelDescriptor[] } | ChannelDescriptor[]>("/channels");
  return Array.isArray(data) ? data : data.channels ?? [];
}

export async function createChannel(req: ChannelCreateRequest): Promise<ChannelDescriptor> {
  return request<ChannelDescriptor>("/channels", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateChannel(id: string, req: ChannelUpdateRequest): Promise<ChannelDescriptor> {
  return request<ChannelDescriptor>(`/channels/${id}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteChannel(id: string): Promise<void> {
  await request<void>(`/channels/${id}`, { method: "DELETE" });
}

export async function fetchChannelMessages(id: string, limit = 200): Promise<ChannelMessageRecord[]> {
  const params = new URLSearchParams({ limit: String(limit) });
  const data = await request<{ messages: ChannelMessageRecord[] } | ChannelMessageRecord[]>(
    `/channels/${id}/messages?${params.toString()}`,
  );
  return Array.isArray(data) ? data : data.messages ?? [];
}

export async function testChannel(id: string, body: ChannelTestRequest): Promise<{ sent: boolean }> {
  return request<{ sent: boolean }>(`/channels/${id}/test`, {
    method: "POST",
    body: JSON.stringify(body),
  });
}

// — hooks —

export async function fetchHooks(): Promise<HookRule[]> {
  const data = await request<{ hooks: HookRule[] } | HookRule[]>("/hooks");
  return Array.isArray(data) ? data : data.hooks ?? [];
}

export async function createHook(req: HookCreateRequest): Promise<HookRule> {
  return request<HookRule>("/hooks", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function updateHook(id: string, req: HookUpdateRequest): Promise<HookRule> {
  return request<HookRule>(`/hooks/${id}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteHook(id: string): Promise<void> {
  await request<void>(`/hooks/${id}`, { method: "DELETE" });
}

export async function fetchHookFirings(limit = 200): Promise<HookFiringRecord[]> {
  const params = new URLSearchParams({ limit: String(limit) });
  const data = await request<{ firings: HookFiringRecord[] } | HookFiringRecord[]>(
    `/hooks/firings?${params.toString()}`,
  );
  return Array.isArray(data) ? data : data.firings ?? [];
}

export async function fetchHookEvents(): Promise<HookEventCatalogEntry[]> {
  const data = await request<{ events: HookEventCatalogEntry[] } | HookEventCatalogEntry[]>("/hooks/events");
  return Array.isArray(data) ? data : data.events ?? [];
}

export async function dryRunHook(id: string): Promise<HookFiringRecord> {
  const data = await request<{ firing: HookFiringRecord } | HookFiringRecord>(`/hooks/${id}/test`, {
    method: "POST",
    body: JSON.stringify({}),
  });
  return "firing" in (data as object)
    ? (data as { firing: HookFiringRecord }).firing
    : (data as HookFiringRecord);
}

// — decisions —

export async function fetchDecisions(): Promise<DecisionRecord[]> {
  const data = await request<DecisionListResponse | DecisionRecord[]>("/decisions");
  return Array.isArray(data) ? data : data.decisions ?? [];
}

export async function createDecision(req: DecisionCreateRequest): Promise<DecisionRecord> {
  return request<DecisionRecord>("/decisions", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function resolveDecision(id: string, req: DecisionResolveRequest): Promise<DecisionRecord> {
  return request<DecisionRecord>(`/decisions/${id}/resolve`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

// — fork —


export async function forkNode(id: string, req: ForkNodeRequest = {}): Promise<AsylumNode> {
  const data = await request<AsylumNode | { node: AsylumNode }>(`/nodes/${id}/fork`, {
    method: "POST",
    body: JSON.stringify(req),
  });
  return "node" in (data as object) ? (data as { node: AsylumNode }).node : (data as AsylumNode);
}

// — health & settings —

export async function fetchHealth(): Promise<HealthResponse> {
  return request<HealthResponse>("/health");
}

export async function fetchTokens(): Promise<TokenListResponse> {
  return request<TokenListResponse>("/tokens");
}

export async function rotateToken(id: string): Promise<TokenRotateResponse> {
  return request<TokenRotateResponse>(`/tokens/${id}/rotate`, { method: "POST" });
}

export interface RemoteCommandResponse {
  kind: string;
  status: string;
  node_id: string | null;
  result: unknown;
}

export async function sendRemoteCommand(command: string): Promise<RemoteCommandResponse> {
  return request<RemoteCommandResponse>("/remote-commands", {
    method: "POST",
    body: JSON.stringify({ command }),
  });
}

// — observe websocket —

export interface ObserveSocketOptions {
  onMessage?: (data: string) => void;
  onError?: (e: Event) => void;
  onClose?: () => void;
  onOpen?: () => void;
}

export function openNodeObserveSocket(nodeId: string, options: ObserveSocketOptions = {}): WebSocket {
  // the daemon's auth middleware reads bearer headers; since the browser WS API cannot set headers, we pass the token via ?token= for ws upgrades
  const proto = typeof window !== "undefined" && window.location.protocol === "https:" ? "wss" : "ws";
  const host = typeof window !== "undefined" ? window.location.host : "";
  const token = getStoredOwnerToken();
  const url = `${proto}://${host}/api/nodes/${nodeId}/observe/ws?token=${encodeURIComponent(token)}`;
  const ws = new WebSocket(url);
  if (options.onOpen) ws.addEventListener("open", () => options.onOpen!());
  if (options.onMessage) {
    ws.addEventListener("message", (event: MessageEvent) => {
      options.onMessage!(event.data as string);
    });
  }
  if (options.onError) ws.addEventListener("error", (e: Event) => options.onError!(e));
  if (options.onClose) ws.addEventListener("close", () => options.onClose!());
  return ws;
}
