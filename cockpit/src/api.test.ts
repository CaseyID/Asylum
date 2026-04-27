import { describe, expect, it } from "vitest";
import { graphToFlow } from "./api";

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
});
