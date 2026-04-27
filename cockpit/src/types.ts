// asylum cockpit — types shared across the app
//
// the wire types come from the daemon (see crates/asylum-core/src/api.rs and node.rs).
// the cockpit augments them with view-only fields (preview, telemetry estimates, etc.)
// when no upstream value is available.

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

// the cockpit ui states ("running", "waiting", "idle", "errored", "stopped")
export type UiState = "running" | "waiting" | "idle" | "errored" | "stopped";

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
  tokens_in: number;
  tokens_out: number;
  tool_calls: number;
  ctx_pct: number;
  idle_seconds: number;
  // augments
  output_preview?: string;
  is_command_center?: boolean;
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

export interface AttachBrowserResponse {
  token?: string;
  attach_url: string;
  expires_in_seconds?: number;
}

export interface NativeTargetResponse {
  command: string;
  args: string[];
  env: Record<string, string>;
  label?: string;
}

// ─── channels ─────────────────────────────────────────────────────

export interface ChannelDescriptor {
  id: string;
  kind: string;
  name: string;
  label: string;
  direction: "inbound" | "outbound" | "duplex" | string;
  status: string;
  detail: string;
  config: Record<string, unknown>;
  live: boolean;
  builtin: boolean;
  created_at_epoch_secs: number;
  message_count_24h: number;
}

export interface ChannelMessageRecord {
  id: number;
  channel_id: string;
  direction: "in" | "out";
  ts_epoch_secs: number;
  sender: string;
  subject: string;
  body: string;
  replies: string[];
}

export interface ChannelCreateRequest {
  kind: string;
  name: string;
  label?: string;
  direction: string;
  detail?: string;
  config?: Record<string, unknown>;
  live?: boolean;
}

export interface ChannelUpdateRequest {
  name?: string;
  label?: string;
  detail?: string;
  direction?: string;
  status?: string;
  config?: Record<string, unknown>;
  live?: boolean;
}

export interface ChannelTestRequest {
  title: string;
  body: string;
}

export interface ChannelInboundRequest {
  sender: string;
  subject: string;
  body: string;
  replies?: string[];
}

// ─── hooks ────────────────────────────────────────────────────────

export interface HookAction {
  kind: "channel" | "spawn" | "tool" | "pause_node" | "archive" | string;
  target: string;
  template?: string;
  args?: Record<string, unknown>;
}

export interface HookRule {
  id: string;
  name: string;
  enabled: boolean;
  event: string;
  filter: string;
  actions: HookAction[];
  future: boolean;
  created_at_epoch_secs: number;
  updated_at_epoch_secs: number;
}

export interface HookCreateRequest {
  name: string;
  enabled?: boolean;
  event: string;
  filter?: string;
  actions?: HookAction[];
  future?: boolean;
}

export interface HookUpdateRequest {
  name?: string;
  enabled?: boolean;
  event?: string;
  filter?: string;
  actions?: HookAction[];
  future?: boolean;
}

export interface HookFiringRecord {
  id: number;
  hook_id: string;
  ts_epoch_secs: number;
  trigger: string;
  outcome: string;
  ok: boolean;
  payload: Record<string, unknown>;
}

export interface HookEventCatalogEntry {
  id: string;
  label: string;
}

// ─── recipes ──────────────────────────────────────────────────────

export interface RecipeDescriptor {
  id: string;
  title: string;
  prompt_template: string;
  kind: "single" | "fanout" | string;
}

export interface RecipeSpawnRequest {
  harness: HarnessKind;
  substrate: SubstrateKind;
  workspace?: string;
  description?: string;
  role_hint?: string;
}

// ─── fork ─────────────────────────────────────────────────────────

export interface ForkNodeRequest {
  role_hint?: string;
  workspace?: string;
  description?: string;
}

// ─── harness / substrate descriptors ──────────────────────────────

export interface HarnessDescriptor {
  id: string;
  name: string;
  kind: string;
  available: boolean;
  command: string;
  caps: string[];
}

export interface SubstrateDescriptor {
  id: string;
  name: string;
  host: string;
  healthy: boolean;
  capacity: number;
  nodes: number;
}

// ─── derived view types ───────────────────────────────────────────

export type ScreenId =
  | "cockpit"
  | "fleet"
  | "node"
  | "create"
  | "channels"
  | "hooks"
  | "logs"
  | "settings"
  | "chat";
