import { describe, expect, it } from "vitest";
import { selectCommandCenter } from "./state";

describe("selectCommandCenter", () => {
  it("selects a running command-center node before workers", () => {
    const selected = selectCommandCenter([
      { id: "worker", role_hint: "worker", liveness: "running" },
      { id: "cc", role_hint: "command-center", liveness: "running" },
    ]);

    expect(selected?.id).toBe("cc");
  });
});
