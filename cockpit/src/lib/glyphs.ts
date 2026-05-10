import type { AsylumNode, NodeLiveness, UiState } from "../types";

// the daemon emits `liveness` (snake_case enum) which closely overlaps with
// the prototype's `state` (running / waiting / idle / errored / stopped).
// this collapse keeps colour, copy, and pill chrome consistent across screens.
export function uiStateForLiveness(liveness: NodeLiveness): UiState {
  switch (liveness) {
    case "running":
    case "starting":
      return "running";
    case "waiting_for_input":
      return "waiting";
    case "exited":
      return "idle";
    case "archived":
      return "archived";
    case "failed":
      return "errored";
    case "stopped":
      return "stopped";
  }
}

export function uiStateOf(node: AsylumNode): UiState {
  return uiStateForLiveness(node.liveness);
}

export function uiStateLabel(state: UiState): string {
  return state;
}

export const ROLE_GLYPH: Record<string, string> = {
  "command-center": "⌬",
  "command_center": "⌬",
  supervisor: "◆",
  worker: "◇",
  evaluator: "◯",
  assistant: "·",
  node: "·",
};

export function roleGlyph(role: string): string {
  return ROLE_GLYPH[role] ?? ROLE_GLYPH[role.toLowerCase()] ?? "·";
}

// the role_hint string for a node selected as command-center.  asylum lets
// the operator type any string, but `command-center` is the canonical hint.
export function isCommandCenterRole(role: string): boolean {
  const r = role.toLowerCase();
  return r === "command-center" || r === "command_center" || r === "cc";
}

export function isCommandCenter(node: AsylumNode): boolean {
  return Boolean(node.is_command_center) || isCommandCenterRole(node.role_hint);
}

// shorten a node id for display (the daemon assigns uuids; the prototype uses
// short generated ids).  fall back to the first 8 chars of a uuid.
export function shortNodeId(id: string): string {
  const slug = id.split("-").pop();
  if (id.length > 12 && slug && slug.length >= 4) return slug.slice(0, 8);
  return id;
}

export function nodeDisplayName(node: AsylumNode): string {
  if (node.description && node.description.length <= 28) return node.description;
  return shortNodeId(node.id);
}

export function harnessLabel(harness: string): string {
  if (harness === "claude_code" || harness === "claude-code") return "claude code";
  return harness;
}

// approximate uptime from created_at; the daemon does not yet expose duration.
export function uptimeLabel(node: AsylumNode): string {
  const created = Date.parse(node.created_at);
  if (!Number.isFinite(created)) return "—";
  const ms = Date.now() - created;
  const sec = Math.max(0, Math.floor(ms / 1000));
  if (sec < 60) return `${sec}s`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ${sec % 60}s`;
  const hr = Math.floor(min / 60);
  return `${hr}h ${min % 60}m`;
}

// telemetry projects daemon-side counters from the events table; see crates/asylum-daemon/src/storage.rs::hydrate_node_telemetry.
export interface NodeTelemetry {
  tokensIn: number;
  tokensOut: number;
  ctx: number;
  tools: number;
}

export function telemetryFor(node: AsylumNode): NodeTelemetry {
  return {
    tokensIn: node.tokens_in ?? 0,
    tokensOut: node.tokens_out ?? 0,
    ctx: node.ctx_pct ?? 0,
    tools: node.tool_calls ?? 0,
  };
}

export function previewFor(node: AsylumNode): string {
  if (node.output_preview && node.output_preview.trim()) return node.output_preview;
  switch (uiStateOf(node)) {
    case "running":
      return "> running";
    case "waiting":
      return "? waiting on input";
    case "errored":
      return "! errored";
    case "idle":
      return "— idle";
    case "stopped":
      return "— stopped";
    case "archived":
      return "— archived";
  }
}
