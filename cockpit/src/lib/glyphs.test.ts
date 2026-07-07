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

  it("renders waiting_for_input as a distinct 'waiting' chip (W5 liveness truth)", () => {
    const state = uiStateForLiveness("waiting_for_input");

    expect(state).toBe("waiting");
    expect(uiStateLabel(state)).toBe("waiting");
    expect(previewFor(node({ liveness: "waiting_for_input" }))).toBe("? waiting on input");
    // distinct from a plain running node
    expect(state).not.toBe(uiStateForLiveness("running"));
  });

  it("keeps failed nodes reading as errored, not a generic stopped state", () => {
    const state = uiStateForLiveness("failed");

    expect(state).toBe("errored");
    expect(previewFor(node({ liveness: "failed" }))).toBe("! errored");
    expect(state).not.toBe(uiStateForLiveness("stopped"));
  });
});
