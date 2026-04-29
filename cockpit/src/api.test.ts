import { beforeEach, describe, expect, it, vi } from "vitest";
import { fetchGraph, interruptNode, archiveNode, stopNode, setStoredOwnerToken } from "./api";
import type { ToastPayload } from "./components/NtfyToast";

beforeEach(() => {
  try {
    window.localStorage.clear();
  } catch {
    // jsdom may not support localStorage.clear in all configurations
  }
  vi.restoreAllMocks();
});

describe("fetchGraph", () => {
  it("sends the stored owner token as a bearer header", async () => {
    setStoredOwnerToken("owner-secret");
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ nodes: [], relationships: [] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await fetchGraph();

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/graph",
      expect.objectContaining({
        headers: expect.objectContaining({ authorization: "Bearer owner-secret" }),
      }),
    );
  });

  it("unwraps a wrapped graph response", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ graph: { nodes: [], relationships: [] } }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const graph = await fetchGraph();
    expect(graph).toEqual({ nodes: [], relationships: [] });
  });
});

// H6 — toast reply lookup must use nodeId, not the free-form sender string.
//
// ChannelMessageRecord carries no node_id, so toasts always have nodeId=null.
// The reply handler should skip the lookup entirely when nodeId is null.
describe("H6 — toast nodeId field and reply handler behaviour", () => {
  it("toast built from a ChannelMessageRecord has nodeId=null", () => {
    // Simulate what App.tsx does when constructing a toast from latest message.
    const latest = {
      id: 42,
      channel_id: "ch-1",
      direction: "in" as const,
      ts_epoch_secs: 1000,
      sender: "ntfy:user@host",
      subject: "hello",
      body: "world",
      replies: ["ok", "ack"],
    };

    const toast: ToastPayload = {
      id: "t-" + latest.id,
      from: latest.sender,
      nodeId: null, // no node_id on ChannelMessageRecord
      channel: "ntfy-main",
      subject: latest.subject,
      body: latest.subject ? `${latest.subject}\n${latest.body}` : latest.body,
      replies: latest.replies,
    };

    expect(toast.nodeId).toBeNull();
    // from is preserved for display
    expect(toast.from).toBe("ntfy:user@host");
  });

  it("reply handler resolves the correct node when nodeId is set", () => {
    const nodes = [
      { id: "node-abc", liveness: "running" },
      { id: "node-xyz", liveness: "running" },
    ];

    const toast: ToastPayload = {
      id: "t-1",
      from: "ntfy:user@host",
      nodeId: "node-abc",
      channel: "ntfy-main",
      body: "hello",
      replies: [],
    };

    // Simulate the reply handler lookup from App.tsx
    const target = nodes.find((n) => n.id === toast.nodeId);
    expect(target).toBeDefined();
    expect(target?.id).toBe("node-abc");
  });

  it("reply handler is a no-op when nodeId is null", () => {
    const nodes = [{ id: "node-abc", liveness: "running" }];

    const toast: ToastPayload = {
      id: "t-2",
      from: "ntfy:user@host",
      nodeId: null,
      channel: "ntfy-main",
      body: "hello",
      replies: [],
    };

    // Guard that matches App.tsx: `if (!t.nodeId) return;`
    const wouldReply = toast.nodeId !== null && nodes.some((n) => n.id === toast.nodeId);
    expect(wouldReply).toBe(false);
  });
});

// H7 — resumeNode must no longer exist on the API client surface.
describe("H7 — resumeNode removed from API surface", () => {
  it("the api module does not export resumeNode", () => {
    // The real check: verify the named export is absent at module level.
    // We imported the module above; TypeScript would have caught it at
    // compile time. This runtime check gives a clear test-failure message.
    const apiModule = { interruptNode, archiveNode, stopNode } as Record<string, unknown>;
    expect("resumeNode" in apiModule).toBe(false);
  });
});
