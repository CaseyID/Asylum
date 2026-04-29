import { describe, it, expect, vi } from "vitest";
import { render } from "@testing-library/react";
import { Inspector } from "./Inspector";
import { shortNodeId } from "../lib/glyphs";
import type { AsylumNode, GraphRelationship } from "../types";

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
});

