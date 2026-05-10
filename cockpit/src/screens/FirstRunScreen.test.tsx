import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { FirstRunScreen } from "./FirstRunScreen";

describe("FirstRunScreen", () => {
  it("does not advertise old overclaims", () => {
    render(
      <FirstRunScreen
        onLaunch={() => undefined}
        onOpenCli={() => undefined}
        onReadSpec={() => undefined}
        harnessCount={0}
        substrateCount={0}
        nodeCount={0}
      />,
    );

    expect(screen.getByText(/tokened replies require command payloads/)).toBeDefined();
    expect(screen.queryByText(/tokened commands include approve \/ attach/)).toBeNull();
    expect(screen.queryByText(/reply with `approve`, `attach`/)).toBeNull();
    expect(screen.queryByText(/retry/i)).toBeNull();
    expect(screen.queryByText(/loon-us-west/i)).toBeNull();
    expect(screen.queryByText(/same capability surface/i)).toBeNull();
    expect(screen.queryByText(/spawn workers/i)).toBeNull();
  });
});
