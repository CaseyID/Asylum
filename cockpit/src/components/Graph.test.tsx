import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { Graph, type GraphNode } from "./Graph";
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
    id: "worker-aaa11111",
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

function graphNode(overrides: Partial<AsylumNode> = {}): GraphNode {
  return { node: node(overrides), parentId: null, edgeKind: "spawned_for" };
}

describe("Graph node-card decision badge", () => {
  it("renders a pending-decision badge on the matching node card", () => {
    const gn = graphNode({ id: "worker-with-pending" });
    const { getByTitle } = render(
      <Graph
        nodes={[gn]}
        layout="tree"
        onSelect={vi.fn()}
        substrates={[]}
        pendingDecisionNodeIds={new Set(["worker-with-pending"])}
      />,
    );

    expect(getByTitle("pending decision")).toBeDefined();
  });

  it("does not render a badge for nodes with no pending decision", () => {
    const gn = graphNode({ id: "worker-without-pending" });
    const { queryByTitle } = render(
      <Graph nodes={[gn]} layout="tree" onSelect={vi.fn()} substrates={[]} />,
    );

    expect(queryByTitle("pending decision")).toBeNull();
  });
});
