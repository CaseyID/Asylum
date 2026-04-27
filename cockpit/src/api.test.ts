import { beforeEach, describe, expect, it, vi } from "vitest";
import { fetchGraph, setStoredOwnerToken } from "./api";

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
