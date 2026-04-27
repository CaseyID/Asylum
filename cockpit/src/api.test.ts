import { beforeEach, describe, expect, it, vi } from "vitest";
import { deleteRelationship, fetchGraph, graphToFlow, setStoredOwnerToken } from "./api";

beforeEach(() => {
  window.localStorage.clear();
  vi.restoreAllMocks();
});

describe("graphToFlow", () => {
  it("keeps only explicit relationships as edges", () => {
    const graph = {
      nodes: [
        { id: "a", role_hint: "command-center" } as const,
        { id: "b", role_hint: "worker" } as const,
      ],
      relationships: [{ id: "r1", source_node_id: "a", target_node_id: "b", kind: "supervises" }] as const,
    };

    const flow = graphToFlow(graph as never);
    expect(flow.nodes).toHaveLength(2);
    expect(flow.edges).toHaveLength(1);
    expect(flow.edges[0]).toMatchObject({ id: "r1", source: "a", target: "b", label: "supervises" });
  });

  it("treats no-content responses as successful actions", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(deleteRelationship("rel-1")).resolves.toBeUndefined();
  });

  it("sends the stored owner token as a bearer header", async () => {
    setStoredOwnerToken("owner-secret");
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ graph: { nodes: [], relationships: [] } }), {
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
});
