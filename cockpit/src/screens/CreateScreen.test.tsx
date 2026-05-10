import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CreateScreen } from "./CreateScreen";
import type { HarnessDescriptor, SubstrateDescriptor } from "../types";

const apiMocks = vi.hoisted(() => ({
  createNode: vi.fn(),
  fetchHarnessDescriptors: vi.fn(),
  fetchSubstrateDescriptors: vi.fn(),
}));

vi.mock("../api", () => apiMocks);

const harnesses: HarnessDescriptor[] = [
  { id: "codex", name: "Codex", kind: "codex", available: true, command: "codex", caps: [] },
  { id: "claude_code", name: "Claude Code", kind: "claude_code", available: false, command: "claude", caps: [] },
];

const substrates: SubstrateDescriptor[] = [
  { id: "local", name: "local", host: "127.0.0.1", healthy: true, capacity: 0.75, nodes: 0 },
  { id: "loon", name: "loon", host: "127.0.0.1", healthy: true, capacity: 0.75, nodes: 0 },
];

describe("CreateScreen", () => {
  beforeEach(() => {
    apiMocks.createNode.mockReset();
    apiMocks.fetchHarnessDescriptors.mockReset();
    apiMocks.fetchSubstrateDescriptors.mockReset();
    apiMocks.fetchHarnessDescriptors.mockResolvedValue(harnesses);
    apiMocks.fetchSubstrateDescriptors.mockResolvedValue(substrates);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("does not claim repo URL support for workspace", async () => {
    render(<CreateScreen onCreated={vi.fn()} onCancel={vi.fn()} />);

    expect(await screen.findByRole("button", { name: /launch/i })).toBeTruthy();
    expect(screen.queryByText(/repo/i)).toBeNull();

    const workspace = screen.getByRole("textbox", { name: /workspace/i }) as HTMLInputElement;
    expect(workspace.value).toBe("");
    expect(workspace.getAttribute("placeholder")).toBe("/abs/path/to/workspace");
  });

  it("forwards the typed workspace value when launching", async () => {
    const onCreated = vi.fn();
    apiMocks.createNode.mockResolvedValue({
      id: "n-1",
    });

    render(<CreateScreen onCreated={onCreated} onCancel={vi.fn()} />);
    const workspace = await screen.findByRole("textbox", { name: /workspace/i });

    fireEvent.change(workspace, { target: { value: "/tmp/asylum-workspace" } });
    fireEvent.click(screen.getByRole("button", { name: /launch/i }));

    await waitFor(() => expect(apiMocks.createNode).toHaveBeenCalled());

    expect(apiMocks.createNode).toHaveBeenCalledWith(
      expect.objectContaining({
        workspace: "/tmp/asylum-workspace",
      }),
    );
    expect(onCreated).toHaveBeenCalledWith("n-1");
  });
});
