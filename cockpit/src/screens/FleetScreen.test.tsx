import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { FleetScreen } from "./FleetScreen";
import type { AsylumNode, CapabilitySnapshot, GraphRelationship } from "../types";

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

// D2 — spawn_peer lineage in the Fleet table (Graph already showed this via
// dashed edges; the table view had no lineage information at all).
describe("FleetScreen spawn_peer lineage (D2)", () => {
  it("shows the spawning parent's short id for a node with a spawned_for relationship", () => {
    const parent = node({ id: "supervisor-aaa11111", role_hint: "supervisor" });
    const child = node({ id: "worker-bbb22222", role_hint: "worker" });
    const relationships: GraphRelationship[] = [
      { id: "rel-1", source_node_id: parent.id, target_node_id: child.id, kind: "spawned_for" },
    ];

    const { container } = render(
      <FleetScreen nodes={[parent, child]} onLaunch={vi.fn()} onOpen={vi.fn()} relationships={relationships} />,
    );

    expect(container.textContent).toContain("aaa11111");
  });

  it("shows an em dash for a node with no parent relationship", () => {
    const solo = node({ id: "worker-ccc33333" });
    const { container } = render(
      <FleetScreen nodes={[solo]} onLaunch={vi.fn()} onOpen={vi.fn()} relationships={[]} />,
    );

    expect(container.querySelector('a[title="spawned_for"]')).toBeNull();
    expect(container.textContent).toContain("—");
  });

  it("opens the parent node (not the child row's own node) when the lineage link is clicked", () => {
    const parent = node({ id: "supervisor-aaa11111", role_hint: "supervisor" });
    const child = node({ id: "worker-bbb22222", role_hint: "worker" });
    const relationships: GraphRelationship[] = [
      { id: "rel-1", source_node_id: parent.id, target_node_id: child.id, kind: "spawned_for" },
    ];
    const onOpen = vi.fn();

    const { container } = render(
      <FleetScreen nodes={[parent, child]} onLaunch={vi.fn()} onOpen={onOpen} relationships={relationships} />,
    );

    // Scope to the lineage cell's anchor specifically — the parent's own
    // node-id cell also renders its short id, so a plain text query would
    // match both.
    const lineageLink = container.querySelector('a[title="spawned_for"]');
    expect(lineageLink).not.toBeNull();
    fireEvent.click(lineageLink!);
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen).toHaveBeenCalledWith(parent);
  });
});
