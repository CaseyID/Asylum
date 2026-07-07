import { afterEach, describe, it, expect, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { Inspector } from "./Inspector";
import { shortNodeId } from "../lib/glyphs";
import type { AsylumNode, GraphRelationship } from "../types";

afterEach(() => cleanup());

// Minimal node fixture — only fields Inspector reads.
function makeNode(id: string): AsylumNode {
  return {
    id,
    role_hint: "worker",
    harness: "codex",
    substrate: "local",
    liveness: "running",
    workspace: "/tmp",
    description: "",
    ctx_pct: 0.1,
    tokens_in: 0,
    tokens_out: 0,
    tool_calls: 0,
    idle_seconds: 0,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    external_id: null,
    capabilities: {
      browser_attach: false,
      native_attach: false,
      send_input: true,
      interrupt: true,
      stop: true,
      resume: false,
      structured_events: false,
      transcript_export: false,
    },
    is_command_center: false,
  };
}

/** Return the value cell text adjacent to the "parent" key in the KV grid. */
function getParentValue(container: HTMLElement): string {
  const keys = Array.from(container.querySelectorAll(".kv .k"));
  const parentKey = keys.find((el) => el.textContent === "parent");
  // The value span is the next sibling
  const valueSpan = parentKey?.nextElementSibling;
  return valueSpan?.textContent ?? "";
}

describe("Inspector parent display", () => {
  it("shows — when no relationships provided", () => {
    const node = makeNode("worker-abc123");
    const { container } = render(
      <Inspector
        node={node}
        onAction={vi.fn()}
        onOpen={vi.fn()}
      />
    );
    expect(getParentValue(container)).toBe("—");
  });

  it("resolves parent shortNodeId from relationships", () => {
    const child = makeNode("worker-abc123");
    const parentId = "cc-def456xyzabc";
    const relationships: GraphRelationship[] = [
      { id: "rel-1", source_node_id: parentId, target_node_id: child.id, kind: "spawned_for" },
    ];
    const { container } = render(
      <Inspector
        node={child}
        onAction={vi.fn()}
        onOpen={vi.fn()}
        relationships={relationships}
      />
    );
    const expectedLabel = shortNodeId(parentId);
    expect(getParentValue(container)).toBe(expectedLabel);
    expect(getParentValue(container)).not.toBe("—");
  });

  it("shows — when no relationship targets this node", () => {
    const node = makeNode("worker-abc123");
    const relationships: GraphRelationship[] = [
      // targets a different node
      { id: "rel-2", source_node_id: "other-parent", target_node_id: "other-child", kind: "spawned_for" },
    ];
    const { container } = render(
      <Inspector
        node={node}
        onAction={vi.fn()}
        onOpen={vi.fn()}
        relationships={relationships}
      />
    );
    expect(getParentValue(container)).toBe("—");
  });

  it("does not expose attach controls", () => {
    const node = makeNode("worker-abc123");
    const { queryByText } = render(
      <Inspector
        node={node}
        onAction={vi.fn()}
        onOpen={vi.fn()}
      />,
    );

    expect(queryByText(/^attach$/i)).toBeNull();
  });
});

describe("Inspector W5 decision + session surfacing", () => {
  it("shows a pending-decision affordance that opens the decisions screen", () => {
    const node = makeNode("worker-abc123");
    const onOpenDecisions = vi.fn();
    const { getByRole } = render(
      <Inspector
        node={node}
        onAction={vi.fn()}
        onOpen={vi.fn()}
        hasPendingDecision
        onOpenDecisions={onOpenDecisions}
      />,
    );

    const btn = getByRole("button", { name: "pending decision" });
    btn.click();
    expect(onOpenDecisions).toHaveBeenCalledTimes(1);
  });

  it("does not show a pending-decision affordance when there is none pending", () => {
    const node = makeNode("worker-abc123");
    const { queryByRole } = render(
      <Inspector node={node} onAction={vi.fn()} onOpen={vi.fn()} />,
    );

    expect(queryByRole("button", { name: "pending decision" })).toBeNull();
  });

  it("shows the harness session id in the overview when present", () => {
    const node = { ...makeNode("worker-abc123"), harness_session_id: "sess-xyz" };
    const { container } = render(
      <Inspector node={node} onAction={vi.fn()} onOpen={vi.fn()} />,
    );

    expect(container.textContent).toContain("sess-xyz");
  });
});
