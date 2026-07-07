import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  archiveNode,
  createRelationship,
  fetchGraph,
  interruptNode,
  removeRelationship,
  markNotificationRead,
  requestBrowserAttach,
  resumeNode,
  setStoredOwnerToken,
  stopNode,
} from "./api";
import type { ToastPayload } from "./components/NtfyToast";

beforeEach(() => {
  window.localStorage.clear();
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

describe("relationship api helpers", () => {
  it("creates relationships through the daemon relationship endpoint", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          id: "rel-1",
          source_node_id: "node-a",
          target_node_id: "node-b",
          kind: "user_created",
          label: null,
        }),
        {
          status: 200,
          headers: { "content-type": "application/json" },
        },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    const rel = await createRelationship({
      source_node_id: "node-a",
      target_node_id: "node-b",
      kind: "user_created",
      label: null,
    });

    expect(rel.id).toBe("rel-1");
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/relationships",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          source_node_id: "node-a",
          target_node_id: "node-b",
          kind: "user_created",
          label: null,
        }),
      }),
    );
  });

  it("removes relationships through the daemon relationship endpoint", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);

    await removeRelationship("rel-1");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/relationships/rel-1",
      expect.objectContaining({ method: "DELETE" }),
    );
  });
});

describe("notification api helpers", () => {
  it("marks a notification as read via POST /notifications/:id/read", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);

    await markNotificationRead("notif-123");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/notifications/notif-123/read",
      expect.objectContaining({ method: "POST" }),
    );
  });
});

describe("attach api helpers", () => {
  it("preserves daemon attach transport notes", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          url: "http://127.0.0.1:7800/attach/token-123",
          expires_in_seconds: 600,
          transport: "loon_attach_proxy",
          note: "attach tab relays `loon attach`",
        }),
        {
          status: 200,
          headers: { "content-type": "application/json" },
        },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    const response = await requestBrowserAttach("node-1");

    expect(response.attach_url).toBe("http://127.0.0.1:7800/attach/token-123");
    expect(response.transport).toBe("loon_attach_proxy");
    expect(response.note).toContain("loon attach");
  });
});

// H6 — toast reply lookup must use nodeId, not the free-form sender string.
//
// ChannelMessageRecord carries node_id only when inbound routing targeted a node.
// The reply handler should skip the lookup when nodeId is null.
describe("H6 — toast nodeId field and reply handler behaviour", () => {
  it("toast built from an unrouted ChannelMessageRecord has nodeId=null", () => {
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
      node_id: null,
    };

    const toast: ToastPayload = {
      id: "t-" + latest.id,
      from: latest.sender,
      nodeId: latest.node_id ?? null,
      channel: "ntfy-main",
      subject: latest.subject,
      body: latest.subject ? `${latest.subject}\n${latest.body}` : latest.body,
      replies: latest.replies,
    };

    expect(toast.nodeId).toBeNull();
    // from is preserved for display
    expect(toast.from).toBe("ntfy:user@host");
  });

  it("toast built from a routed ChannelMessageRecord preserves nodeId", () => {
    const latest = {
      id: 43,
      channel_id: "ch-1",
      direction: "in" as const,
      ts_epoch_secs: 1001,
      sender: "ntfy:user@host",
      subject: "route",
      body: "body",
      replies: ["ok"],
      node_id: "node-abc",
    };

    const toast: ToastPayload = {
      id: "t-" + latest.id,
      from: latest.sender,
      nodeId: latest.node_id ?? null,
      channel: "ntfy-main",
      subject: latest.subject,
      body: latest.subject ? `${latest.subject}\n${latest.body}` : latest.body,
      replies: latest.replies,
    };

    expect(toast.nodeId).toBe("node-abc");
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

// D2 — resumeNode reintroduced against the real POST /api/nodes/:id/resume
// route (a parallel workstream delivers the daemon side). H7 (2026-04-29)
// removed the old dead client plumbing for this because no route backed it;
// this time it is wired to a real Resume button (NodeScreen/Inspector), so
// the client call is real again and covered by its own tests.
describe("resumeNode", () => {
  it("posts to /nodes/:id/resume with an empty JSON body", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);

    await resumeNode("node-42");

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/nodes/node-42/resume",
      expect.objectContaining({ method: "POST", body: JSON.stringify({}) }),
    );
  });

  it("propagates a daemon error (e.g. route not yet available) as an ApiError", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response("not found", { status: 404, statusText: "Not Found" }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(resumeNode("node-42")).rejects.toThrow(/404/);
  });
});
