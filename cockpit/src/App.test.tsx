import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "./App";
import { useCockpitStore } from "./state";
import type { AsylumNode, CapabilitySnapshot, GraphRelationship } from "./types";

const apiMocks = vi.hoisted(() => {
  class MockApiError extends Error {
    status: number;

    constructor(status: number, message: string) {
      super(message);
      this.status = status;
    }
  }

  return {
    ApiError: MockApiError,
    archiveNode: vi.fn(),
    fetchChannelMessages: vi.fn(),
    fetchChannels: vi.fn(),
    fetchDecisions: vi.fn(),
    fetchGraph: vi.fn(),
    fetchHarnessDescriptors: vi.fn(),
    fetchHealth: vi.fn(),
    fetchHooks: vi.fn(),
    fetchNotifications: vi.fn(),
    fetchSubstrateDescriptors: vi.fn(),
    forkNode: vi.fn(),
    hydrateOwnerTokenFromLocation: vi.fn(),
    interruptNode: vi.fn(),
    markNotificationRead: vi.fn(),
    openAttachSocket: vi.fn(),
    openNodeObserveSocket: vi.fn(),
    postNodeInput: vi.fn(),
    requestBrowserAttach: vi.fn(),
    sendRemoteCommand: vi.fn(),
    setStoredOwnerToken: vi.fn(),
    stopNode: vi.fn(),
  };
});

vi.mock("./api", () => apiMocks);

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

function node(overrides: Partial<AsylumNode>): AsylumNode {
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

describe("App populated daemon state", () => {
  beforeEach(() => {
    window.localStorage.clear();
    vi.restoreAllMocks();
    useCockpitStore.setState({
      graph: { nodes: [], relationships: [] },
      selectedNodeId: undefined,
      commandCenterNodeId: undefined,
      loading: true,
    });

    apiMocks.hydrateOwnerTokenFromLocation.mockReturnValue("");
    apiMocks.fetchNotifications.mockResolvedValue([]);
    apiMocks.fetchDecisions.mockResolvedValue([]);
    apiMocks.fetchHealth.mockResolvedValue({
      status: "ok",
      daemon_version: "0.1.6",
      bind_addr: "127.0.0.1:7800",
      database_path: "/tmp/asylum.sqlite",
      database_size_bytes: 4096,
      transcripts_dir: "/tmp/asylum/transcripts",
    });
    apiMocks.fetchChannels.mockResolvedValue([
      {
        id: "webhook-substrate",
        kind: "webhook",
        name: "webhook-substrate",
        label: "webhook",
        direction: "inbound",
        status: "configured",
        detail: "local inbound",
        config: {},
        live: true,
        builtin: true,
        created_at_epoch_secs: 1,
        message_count_24h: 0,
      },
    ]);
    apiMocks.fetchHooks.mockResolvedValue([{ id: "hook-1", name: "hook", enabled: true }]);
    apiMocks.fetchSubstrateDescriptors.mockResolvedValue([
      { id: "local", name: "local", host: "localhost", healthy: true, capacity: 4, nodes: 2 },
    ]);
    apiMocks.fetchHarnessDescriptors.mockResolvedValue([
      { id: "codex", name: "Codex", kind: "codex", available: true, command: "codex", caps: [] },
      { id: "claude_code", name: "Claude Code", kind: "claude_code", available: false, command: "claude", caps: [] },
    ]);
    apiMocks.fetchChannelMessages.mockResolvedValue([]);
    apiMocks.openNodeObserveSocket.mockReturnValue({
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      close: vi.fn(),
    });
    apiMocks.requestBrowserAttach.mockResolvedValue({
      attach_url: "http://localhost/attach/session",
      token: "session-token",
      expires_in_seconds: 3600,
      transport: "local_pty",
      note: null,
    });
    apiMocks.openAttachSocket.mockReturnValue({
      readyState: 1,
      send: vi.fn(),
      close: vi.fn(),
      addEventListener: vi.fn(),
    } as unknown as WebSocket);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("renders daemon-derived nav counts and footer for a populated graph", async () => {
    const nodes = [
      node({ id: "cc-node", role_hint: "command-center", liveness: "running" }),
      node({ id: "worker-node", role_hint: "worker", liveness: "stopped", description: "worker" }),
    ];
    const relationships: GraphRelationship[] = [
      { id: "rel-1", source_node_id: "cc-node", target_node_id: "worker-node", kind: "spawned_for", label: null },
    ];
    apiMocks.fetchGraph.mockResolvedValue({ nodes, relationships });

    const { container, getByText } = render(<App />);

    await waitFor(() => expect(getByText("asylum 0.1.6")).toBeDefined());

    expect(container.textContent).toContain("127.0.0.1:7800");
    expect(container.textContent).toContain("2");
    expect(container.textContent).toContain("1 running");
    expect(container.textContent).not.toContain("start a command center");
  });

  it("shows a pending-decision badge on the fleet screen for a node awaiting one", async () => {
    apiMocks.fetchGraph.mockResolvedValue({
      nodes: [
        node({ id: "cc-node", role_hint: "command-center", liveness: "running" }),
        node({ id: "worker-node", role_hint: "worker", liveness: "waiting_for_input", description: "needs input" }),
      ],
      relationships: [],
    });
    apiMocks.fetchDecisions.mockResolvedValue([
      {
        id: "dec-1",
        node_id: "worker-node",
        text: "should I proceed?",
        status: "pending",
        created_at_epoch_secs: 0,
        decided_at_epoch_secs: null,
      },
    ]);

    const { getByText, getByTitle } = render(<App />);

    await waitFor(() => expect(getByText("asylum 0.1.6")).toBeDefined());
    fireEvent.click(getByText("nodes"));

    await waitFor(() => expect(getByTitle("pending decision")).toBeDefined());
  });

  it("keeps the fleet screen backed by the populated graph snapshot", async () => {
    apiMocks.fetchGraph.mockResolvedValue({
      nodes: [
        node({ id: "cc-node", role_hint: "command-center", liveness: "running" }),
        node({ id: "worker-node", role_hint: "worker", liveness: "waiting_for_input", description: "needs input" }),
      ],
      relationships: [],
    });

    const { container, getByText } = render(<App />);

    await waitFor(() => expect(getByText("asylum 0.1.6")).toBeDefined());
    fireEvent.click(getByText("nodes"));

    await waitFor(() => expect(getByText("2 total · 1 running · 1 waiting · 0 errored")).toBeDefined());
    expect(container.textContent).toContain("command-center");
    expect(container.textContent).toContain("worker");
    expect(container.textContent).toContain("local");
  });

  it("marks per-resource refresh failures instead of silently keeping stale state", async () => {
    const nodes = [node({ id: "cc-node", role_hint: "command-center", liveness: "running" })];
    apiMocks.fetchGraph.mockResolvedValue({ nodes, relationships: [] });
    apiMocks.fetchChannels.mockRejectedValue(new Error("channel endpoint unavailable"));

    render(<App />);

    await waitFor(() => expect(apiMocks.fetchChannels).toHaveBeenCalled());
    expect(screen.getByText(/channels: channels refresh failed: channel endpoint unavailable/i)).toBeDefined();
  });

  it("logs ntfy toast polling failures instead of swallowing them", async () => {
    const channels = [
      {
        id: "ntfy-main",
        kind: "ntfy",
        name: "ntfy",
        label: "ntfy",
        direction: "duplex",
        status: "live",
        detail: "ntfy",
        config: {},
        live: true,
        builtin: true,
        created_at_epoch_secs: 0,
        message_count_24h: 0,
      },
    ];
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    apiMocks.fetchGraph.mockResolvedValue({ nodes: [node({ id: "cc-node", role_hint: "command-center", liveness: "running" })], relationships: [] });
    apiMocks.fetchChannels.mockResolvedValue(channels);
    apiMocks.fetchChannelMessages.mockRejectedValue(new Error("ntfy timeout"));

    render(<App />);
    await waitFor(() => expect(apiMocks.fetchChannels).toHaveBeenCalled());

    await new Promise((resolve) => setTimeout(resolve, 6500));
    await waitFor(() =>
      expect(consoleSpy).toHaveBeenCalledWith(
        "ntfy toast poll failed",
        expect.objectContaining({ channelId: "ntfy-main", reason: "ntfy timeout" }),
      ),
    );

    consoleSpy.mockRestore();
  }, 12000);
});
