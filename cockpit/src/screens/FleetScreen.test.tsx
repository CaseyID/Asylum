import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { FleetScreen } from "./FleetScreen";
import type { AsylumNode, CapabilitySnapshot } from "../types";

afterEach(() => cleanup());

const caps: CapabilitySnapshot = {
  browser_attach: false,
  native_attach: false,
  send_input: true,
  interrupt: true,
  stop: true,
  resume: false,
  structured_events: false,
  transcript_export: false,
};

function node(overrides: Partial<AsylumNode> = {}): AsylumNode {
  return {
    id: "node-1",
    harness: "codex",
    substrate: "local",
    role_hint: "worker",
    liveness: "running",
    workspace: "/tmp",
    description: "",
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
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

describe("FleetScreen decision surfacing", () => {
  it("shows a decision badge for a node with a pending decision", () => {
    const n = node({ id: "worker-aaa11111" });
    const { getByTitle } = render(
      <FleetScreen
        nodes={[n]}
        onLaunch={vi.fn()}
        onOpen={vi.fn()}
        pendingDecisionNodeIds={new Set(["worker-aaa11111"])}
      />,
    );

    expect(getByTitle("pending decision")).toBeDefined();
  });

  it("does not show a decision badge for a node with no pending decision", () => {
    const n = node({ id: "worker-bbb22222" });
    const { queryByTitle } = render(
      <FleetScreen
        nodes={[n]}
        onLaunch={vi.fn()}
        onOpen={vi.fn()}
        pendingDecisionNodeIds={new Set()}
      />,
    );

    expect(queryByTitle("pending decision")).toBeNull();
  });

  it("renders distinct state pills for waiting/errored/running liveness", () => {
    const nodes = [
      node({ id: "n-running", liveness: "running" }),
      node({ id: "n-waiting", liveness: "waiting_for_input" }),
      node({ id: "n-failed", liveness: "failed" }),
    ];
    const { container } = render(
      <FleetScreen nodes={nodes} onLaunch={vi.fn()} onOpen={vi.fn()} />,
    );

    expect(container.querySelectorAll(".pill-running").length).toBeGreaterThan(0);
    expect(container.querySelectorAll(".pill-waiting").length).toBeGreaterThan(0);
    expect(container.querySelectorAll(".pill-errored").length).toBeGreaterThan(0);
  });
});
