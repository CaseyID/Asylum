import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NodeScreen } from "./NodeScreen";
import type { AsylumNode, CapabilitySnapshot } from "../types";

const apiMocks = vi.hoisted(() => ({
  createRelationship: vi.fn(),
  fetchHarnessDescriptors: vi.fn(),
  fetchNodeEvents: vi.fn(),
  openNodeObserveSocket: vi.fn(),
  postNodeInput: vi.fn(),
  removeRelationship: vi.fn(),
  requestBrowserAttach: vi.fn(),
  openAttachSocket: vi.fn(),
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
    id: "node-1",
    harness: "codex",
    substrate: "local",
    role_hint: "command-center",
    liveness: "running",
    workspace: "/tmp/asylum",
    description: "command center",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    external_id: null,
    capabilities: caps,
    tokens_in: 10,
    tokens_out: 5,
    tool_calls: 1,
    ctx_pct: 0.15,
    idle_seconds: 4,
    output_preview: "ready",
    ...overrides,
  };
}

describe("NodeScreen W5 decision + session surfacing", () => {
  beforeEach(() => {
    apiMocks.fetchHarnessDescriptors.mockReset();
    apiMocks.fetchHarnessDescriptors.mockResolvedValue([]);
    apiMocks.fetchNodeEvents.mockReset();
    apiMocks.fetchNodeEvents.mockResolvedValue([]);
  });

  afterEach(() => cleanup());

  it("shows a pending-decision affordance and routes it to the decisions screen", async () => {
    const onOpenDecisions = vi.fn();
    const props = {
      node: node(),
      nodes: [node()],
      relationships: [],
      onBack: vi.fn(),
      onOpen: vi.fn(),
      onAction: vi.fn(),
      onGraphRefresh: vi.fn(),
      hasPendingDecision: true,
      onOpenDecisions,
    };

    const { getByRole, container } = render(<NodeScreen {...props} />);

    await waitFor(() => expect(container.textContent).toContain("command-center"));
    const btn = getByRole("button", { name: "pending decision" });
    fireEvent.click(btn);
    expect(onOpenDecisions).toHaveBeenCalledTimes(1);
  });

  it("does not show a pending-decision affordance when there is none", () => {
    const props = {
      node: node(),
      nodes: [node()],
      relationships: [],
      onBack: vi.fn(),
      onOpen: vi.fn(),
      onAction: vi.fn(),
      onGraphRefresh: vi.fn(),
      hasPendingDecision: false,
    };

    const { queryByRole } = render(<NodeScreen {...props} />);
    expect(queryByRole("button", { name: "pending decision" })).toBeNull();
  });

  it("shows the harness_session_id when the daemon provides one", async () => {
    const props = {
      node: node({ harness_session_id: "sess-abc-123" }),
      nodes: [node({ harness_session_id: "sess-abc-123" })],
      relationships: [],
      onBack: vi.fn(),
      onOpen: vi.fn(),
      onAction: vi.fn(),
      onGraphRefresh: vi.fn(),
    };

    const { container } = render(<NodeScreen {...props} />);
    await waitFor(() => expect(container.textContent).toContain("sess-abc-123"));
  });

  it("falls back to an em dash when no harness_session_id is recorded", async () => {
    const props = {
      node: node({ harness_session_id: undefined }),
      nodes: [node({ harness_session_id: undefined })],
      relationships: [],
      onBack: vi.fn(),
      onOpen: vi.fn(),
      onAction: vi.fn(),
      onGraphRefresh: vi.fn(),
    };

    const { container } = render(<NodeScreen {...props} />);
    await waitFor(() => expect(container.textContent).toContain("command-center"));
    expect(container.textContent).toMatch(/session:\s*—/);
  });
});

describe("NodeScreen selection transition", () => {
  beforeEach(() => {
    apiMocks.requestBrowserAttach.mockReset();
    apiMocks.requestBrowserAttach.mockResolvedValue({
      attach_url: "ws://localhost/attach/raw-token",
      token: "raw-token",
      expires_in_seconds: 600,
      transport: "local_pty",
      note: null,
    });
    apiMocks.openAttachSocket.mockReset();
    apiMocks.openAttachSocket.mockReturnValue({
      readyState: 1,
      send: vi.fn(),
      close: vi.fn(),
      addEventListener: vi.fn(),
    } as unknown as WebSocket);
    apiMocks.openNodeObserveSocket.mockReset();
  });

  afterEach(() => cleanup());

  it("can transition from no selected node to a node detail without changing hook order", async () => {
    apiMocks.fetchHarnessDescriptors.mockResolvedValue([]);
    apiMocks.openNodeObserveSocket.mockReturnValue({ close: vi.fn() });

    const props = {
      nodes: [node()],
      relationships: [],
      onBack: vi.fn(),
      onOpen: vi.fn(),
      onAction: vi.fn(),
      onGraphRefresh: vi.fn(),
    };

    const { container, getByText, rerender } = render(<NodeScreen {...props} />);

    expect(getByText("no node selected")).toBeDefined();

    rerender(<NodeScreen {...props} node={node()} />);

    await waitFor(() => expect(container.textContent).toContain("command-center"));
  });

  it("shows success feedback only after the action resolves", async () => {
    let releaseStop: (() => void) | null = null;
    const stopAction = vi.fn(() => {
      return new Promise<void>((resolve) => {
        releaseStop = resolve;
      });
    });
    const props = {
      nodes: [node()],
      relationships: [],
      onBack: vi.fn(),
      onOpen: vi.fn(),
      onAction: stopAction,
      onGraphRefresh: vi.fn(),
    };

    const { container, getByRole, queryByText } = render(<NodeScreen {...props} node={node()} />);
    const stop = getByRole("button", { name: "stop" });

    fireEvent.click(stop);
    expect(queryByText("stop issued")).toBeNull();
    expect(container.textContent).not.toContain("stop issued");

    expect(releaseStop).not.toBeNull();
    releaseStop!();

    await waitFor(() => expect(container.textContent).toContain("stop issued"));
  });

  it("shows inline failure feedback when action rejects", async () => {
    const props = {
      nodes: [node()],
      relationships: [],
      onBack: vi.fn(),
      onOpen: vi.fn(),
      onAction: vi.fn().mockRejectedValue(new Error("daemon unavailable")),
      onGraphRefresh: vi.fn(),
    };

    const { getByRole, queryByText } = render(<NodeScreen {...props} node={node()} />);
    const archive = getByRole("button", { name: "archive" });

    fireEvent.click(archive);

    expect(queryByText("archived · transcript exported")).toBeNull();
    await waitFor(() => expect(queryByText("archive failed: daemon unavailable")).toBeDefined());
  });
});
