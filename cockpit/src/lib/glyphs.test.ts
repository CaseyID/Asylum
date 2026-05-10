import { describe, expect, it } from "vitest";
import { previewFor, uiStateForLiveness, uiStateLabel } from "./glyphs";
import type { AsylumNode, CapabilitySnapshot } from "../types";

const caps: CapabilitySnapshot = {
  browser_attach: true,
  native_attach: true,
  send_input: true,
  interrupt: true,
  stop: true,
  resume: false,
  structured_events: false,
  transcript_export: false,
};

function node(overrides: Partial<AsylumNode> = {}): AsylumNode {
  return {
    id: "archived-node",
    harness: "codex",
    substrate: "local",
    role_hint: "worker",
    liveness: "archived",
    workspace: "/tmp/asylum",
    description: "",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    external_id: null,
    capabilities: caps,
    tokens_in: 0,
    tokens_out: 0,
    tool_calls: 0,
    ctx_pct: 0,
    idle_seconds: 0,
    ...overrides,
  };
}

describe("node liveness display", () => {
  it("keeps archived distinct from idle/stopped nodes", () => {
    const state = uiStateForLiveness("archived");

    expect(state).toBe("archived");
    expect(uiStateLabel(state)).toBe("archived");
    expect(previewFor(node())).toBe("— archived");
  });
});
