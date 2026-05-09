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
    id: "node-session-loop",
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

describe("NodeSession session semantics", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("does not expose attach controls in the session header", () => {
    apiMocks.openNodeObserveSocket.mockReturnValue({ close: vi.fn() });

    const { queryByTitle } = render(<NodeSession node={node()} />);

    expect(queryByTitle("open attach tab")).toBeNull();
    expect(queryByTitle("open attach tab via loon attach")).toBeNull();
    expect(queryByTitle("open in terminal")).toBeNull();
  });

  it("ignores attach-issued history as an internal transport event", async () => {
    let onMessage: ((data: string) => void) | undefined;
    apiMocks.openNodeObserveSocket.mockImplementation((_nodeId: string, options: { onMessage?: (data: string) => void }) => {
      onMessage = options.onMessage;
      return { close: vi.fn() };
    });

    const { container, queryByText, queryByTitle } = render(<NodeSession node={node()} />);

    onMessage?.(JSON.stringify({
      kind: "attach_issued",
      node_id: "node-session-loop",
      body: {
        url: "http://127.0.0.1:7717/attach/token",
        node_id: "node-session-loop",
      },
    }));

    await waitFor(() => expect(apiMocks.openNodeObserveSocket).toHaveBeenCalled());
    expect(queryByText("open a time-limited Cockpit attach view for this node")).toBeNull();
    expect(queryByTitle("open attach tab")).toBeNull();
    expect(container.textContent ?? "").not.toContain("attach tab");
  });

  it("describes Loon live-stream limitations in session language", async () => {
    let onMessage: ((data: string) => void) | undefined;
    apiMocks.openNodeObserveSocket.mockImplementation((_nodeId: string, options: { onMessage?: (data: string) => void }) => {
      onMessage = options.onMessage;
      return { close: vi.fn() };
    });

    const { queryByText, getByText, container } = render(<NodeSession node={node({ substrate: "loon" })} />);

    onMessage?.("asylum.observe.ws.initialized");
    onMessage?.("asylum.observe.ws.live_stream_unavailable");
    fireEvent.click(getByText("struct"));

    await waitFor(() => {
      expect(container.textContent).toContain("Loon nodes do not stream local PTY-style live observe output; open the node session for an interactive terminal");
    });
    expect(queryByText(/use attach/i)).toBeNull();
  });

});
