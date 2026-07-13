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

// the cockpit ui states ("running", "waiting", "idle", "errored", "stopped", "archived")
export type UiState = "running" | "waiting" | "idle" | "errored" | "stopped" | "archived";

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
  // Harness-native session identity recorded from the harness-event bridge
  // (claude `session_id`, codex `thread-id`). Resume key for Phase C. Present
  // on the daemon's NodeRecord as of W1/W3; optional here so fixtures/tests
  // that predate it still type-check.
  harness_session_id?: string | null;
  // Launch-profile model/effort the node was actually launched with (recorded
  // at launch, not an Asylum-owned catalog). `null`/absent means the harness
  // default was used. Present on the daemon's NodeRecord as of the
  // launch-profile workstream.
  model?: string | null;
  effort?: string | null;
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

export interface RelationshipCreateRequest {
  source_node_id: string;
  target_node_id: string;
  kind: string;
  label?: string | null;
}

export interface RelationshipListResponse {
  relationships: GraphRelationship[];
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
  // Optional launch-profile overrides, passed verbatim to the harness. Only
  // included in the request when the operator typed a non-empty value; the
  // daemon treats an omitted field as "harness default".
  model?: string;
  effort?: string;
}

export interface AttachBrowserResponse {
  token?: string;
  attach_url: string;
  expires_in_seconds?: number;
  transport?: string | null;
  note?: string | null;
}

export interface NativeTargetResponse {
  command: string;
  args: string[];
  env: Record<string, string>;
  label?: string;
}

// ─── channels ─────────────────────────────────────────────────────

export type ChannelKind = "ntfy" | "webhook";

export interface ChannelDescriptor {
  id: string;
  kind: ChannelKind;
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
  node_id?: string | null;
  correlation_token?: string | null;
}

export interface ChannelCreateRequest {
  kind: ChannelKind;
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

// ─── hooks ────────────────────────────────────────────────────────

export interface HookAction {
  kind: "channel" | "send_input" | "spawn" | "tool" | "pause_node" | "archive" | string;
  target: string;
  template?: string;
  args?: Record<string, unknown>;
}

// ─── hook action arg shapes (W4 contract; args stays a loose Record on the
// wire, these are the shapes the cockpit forms read/write) ────────────────
export interface SendInputActionArgs {
  text?: string;
}

export interface SpawnActionArgs {
  harness?: HarnessKind;
  substrate?: SubstrateKind;
  role?: string;
  workspace?: string;
  description?: string;
  prompt?: string;
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

// ─── decisions ────────────────────────────────────────────────────


export interface DecisionRecord {
  id: string;
  node_id: string | null;
  text: string;
  status: string;
  created_at_epoch_secs: number;
  decided_at_epoch_secs: number | null;
}

export interface DecisionListResponse {
  decisions: DecisionRecord[];
}

export interface DecisionCreateRequest {
  node_id?: string;
  text: string;
}

export interface DecisionResolveRequest {
  status: "approved" | "denied";
  // Optional free-text answer injected verbatim into the node's PTY,
  // overriding the status-derived affirmative/negative feedback (W4).
  answer?: string;
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
  // Whether this harness accepts a per-launch model / reasoning-effort
  // override. Derived from the adapter, not an Asylum-owned catalog — the
  // cockpit only shows the corresponding control when the flag is true.
  supports_model?: boolean;
  supports_effort?: boolean;
}

export interface SubstrateDescriptor {
  id: string;
  name: string;
  host: string;
  status?: string;
  healthy: boolean;
  capacity: number;
  nodes: number;
}

// ─── health / settings ────────────────────────────────────────────

export interface HealthResponse {
  status: string;
  daemon_version: string;
  bind_addr: string;
  database_path: string;
  database_size_bytes: number;
  transcripts_dir: string;
  // D2: daemon-provided uptime (was previously derived client-side from
  // per-node created_at only; the daemon itself did not expose its own
  // start time). Optional so fixtures/tests that predate the field still
  // type-check.
  daemon_started_at_epoch_secs?: number;
  uptime_seconds?: number;
}

export interface TokenSummary {
  id: string;
  label: string;
  created_at_epoch_secs: number;
  expires_at_epoch_secs: number;
  revoked: boolean;
}

export interface TokenListResponse {
  tokens: TokenSummary[];
}

export interface TokenIssueResponse {
  id: string;
  raw_token: string;
  scope: string[];
  expires_at_epoch_secs: number;
}

export interface TokenRotateResponse {
  old_id: string;
  new_token: TokenIssueResponse;
}

// ─── derived view types ───────────────────────────────────────────

export type ScreenId =
  | "cockpit"
  | "fleet"
  | "node"
  | "create"
  | "decisions"
  | "channels"
  | "hooks"
  | "logs"
  | "settings"
  | "chat";
