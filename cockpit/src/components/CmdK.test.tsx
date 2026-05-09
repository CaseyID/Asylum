import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { CmdK } from "./CmdK";
import type { AsylumNode } from "../types";

function nodes(): AsylumNode[] {
  return [
    {
      id: "node-1",
      role_hint: "worker",
      harness: "codex",
      substrate: "local",
      liveness: "running",
      workspace: "/tmp/asylum",
      description: "worker",
      created_at: "2026-05-09T00:00:00Z",
      updated_at: "2026-05-09T00:00:00Z",
      external_id: null,
      capabilities: {
        browser_attach: false,
        native_attach: false,
        send_input: true,
        interrupt: true,
        stop: true,
        resume: false,
        structured_events: false,
        transcript_export: false,
      },
      tokens_in: 0,
      tokens_out: 0,
      tool_calls: 0,
      ctx_pct: 0,
      idle_seconds: 0,
      is_command_center: false,
    },
  ];
}

describe("CmdK command palette", () => {
  it("does not expose attach actions", () => {
    const { container, queryByText } = render(
      <CmdK
        onClose={() => undefined}
        onPick={() => undefined}
        onLaunch={() => undefined}
        onPickNode={() => undefined}
        onSendRemoteCommand={() => undefined}
        nodes={nodes()}
      />,
    );

    expect(queryByText(/open attach tab/i)).toBeNull();
    expect(queryByText(/browser/)).toBeNull();
    expect(queryByText(/terminal/i)).toBeNull();
    expect(queryByText(/send remote command/i)).toBeDefined();
    expect(container.textContent).toContain("launch new node…");
  });
});
