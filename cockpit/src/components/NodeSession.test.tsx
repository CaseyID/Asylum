import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { NodeSession } from "./NodeSession";
import type { AsylumNode, CapabilitySnapshot } from "../types";

const apiMocks = vi.hoisted(() => ({
  postNodeInput: vi.fn(),
  openNodeObserveSocket: vi.fn(),
}));

vi.mock("../api", () => apiMocks);

const caps: CapabilitySnapshot = {
  browser_attach: true,
  native_attach: true,
  send_input: true,
  interrupt: true,
  stop: true,
  resume: false,
  structured_events: true,
  transcript_export: true,
};

function node(overrides: Partial<AsylumNode> = {}): AsylumNode {
  return {
    id: "node-attach-loop",
    harness: "codex",
    substrate: "local",
    role_hint: "worker",
    liveness: "running",
    workspace: "/tmp/asylum",
    description: "worker",
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

describe("NodeSession attach events", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders attach-issued history without firing the attach action again", async () => {
    let onMessage: ((data: string) => void) | undefined;
    apiMocks.openNodeObserveSocket.mockImplementation((_nodeId: string, options: { onMessage?: (data: string) => void }) => {
      onMessage = options.onMessage;
      return { close: vi.fn() };
    });
    const onAttach = vi.fn();

    const { getByText } = render(<NodeSession node={node()} onAttach={onAttach} />);

    onMessage?.(JSON.stringify({
      kind: "attach_issued",
      node_id: "node-attach-loop",
      body: {
        url: "http://127.0.0.1:7717/attach/token",
        node_id: "node-attach-loop",
      },
    }));

    await waitFor(() => expect(getByText("open a time-limited Cockpit attach view for this node")).toBeDefined());
    expect(onAttach).not.toHaveBeenCalled();
  });

  it("fires the attach action only from the explicit toolbar button", () => {
    apiMocks.openNodeObserveSocket.mockReturnValue({ close: vi.fn() });
    const onAttach = vi.fn();

    const { getByTitle } = render(<NodeSession node={node()} onAttach={onAttach} />);

    fireEvent.click(getByTitle("open attach tab"));

    expect(onAttach).toHaveBeenCalledWith("node-attach-loop");
  });
});
