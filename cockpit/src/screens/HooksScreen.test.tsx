import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { HooksScreen } from "./HooksScreen";
import type { AsylumNode, CapabilitySnapshot, HookAction, HookFiringRecord, HookRule } from "../types";

const apiMocks = vi.hoisted(() => ({
  createHook: vi.fn(),
  deleteHook: vi.fn(),
  dryRunHook: vi.fn(),
  fetchHookEvents: vi.fn(),
  fetchHookFirings: vi.fn(),
  fetchHooks: vi.fn(),
  fetchNodes: vi.fn(),
  updateHook: vi.fn(),

}));

vi.mock("../api", () => apiMocks);

function hook(overrides: Partial<HookRule> = {}): HookRule {
  return {
    id: "hook-1",
    name: "hook-1",
    enabled: true,
    event: "node.permission_requested",
    filter: "any",
    actions: [{ kind: "channel", target: "ntfy-default", template: "{node.id} started" }],
    future: false,
    created_at_epoch_secs: 0,
    updated_at_epoch_secs: 0,
    ...overrides,
  };
}

function firing(overrides: Partial<HookFiringRecord> = {}): HookFiringRecord {
  return {
    id: 10,
    hook_id: "hook-1",
    ts_epoch_secs: Math.floor(Date.now() / 1000),
    trigger: "node.permission_requested",
    outcome: "ok",
    ok: true,
    payload: {},
    ...overrides,
  };
}

const caps: CapabilitySnapshot = {
  browser_attach: false,
  native_attach: false,
  send_input: true,
  interrupt: true,
  stop: true,
  resume: false,
  structured_events: false,
  transcript_export: false,
};

function fakeNode(overrides: Partial<AsylumNode> = {}): AsylumNode {
  return {
    id: "11111111-1111-1111-1111-111111111111",
    harness: "claude_code",
    substrate: "local",
    role_hint: "worker",
    liveness: "running",
    workspace: "/tmp/asylum",
    description: "",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    external_id: null,
    capabilities: caps,
    tokens_in: 0,
    tokens_out: 0,
    tool_calls: 0,
    ctx_pct: 0,
    idle_seconds: 0,
    ...overrides,
  };
}

describe("HooksScreen error handling", () => {
  beforeEach(() => {
    apiMocks.createHook.mockReset();
    apiMocks.deleteHook.mockReset();
    apiMocks.dryRunHook.mockReset();
    apiMocks.fetchHookEvents.mockReset();
    apiMocks.fetchHookFirings.mockReset();
    apiMocks.fetchHooks.mockReset();
    apiMocks.fetchNodes.mockReset();
    apiMocks.updateHook.mockReset();

    apiMocks.fetchHookEvents.mockResolvedValue([]);
    apiMocks.fetchHookFirings.mockResolvedValue([]);
    apiMocks.fetchNodes.mockResolvedValue([]);
  });


  afterEach(() => {
    cleanup();
  });

  it("displays a reload error instead of staying silent", async () => {
    apiMocks.fetchHooks.mockRejectedValue(new Error("hooks unavailable"));
    apiMocks.fetchHookEvents.mockResolvedValue([{ id: "node.permission_requested", label: "when permission requested" }]);

    const { getByText } = render(<HooksScreen />);

    await waitFor(() => expect(apiMocks.fetchHooks).toHaveBeenCalled());
    expect(getByText(/failed to reload hooks/i)).toBeDefined();
  });

  it("shows toggle failure without pretending the hook was updated", async () => {
    apiMocks.fetchHooks.mockResolvedValue([hook()]);
    apiMocks.updateHook.mockRejectedValue(new Error("toggle forbidden"));
    apiMocks.fetchHookEvents.mockResolvedValue([]);

    const { getByTitle, getByText } = render(<HooksScreen />);

    await waitFor(() => expect(getByText("hook-1")).toBeDefined());

    fireEvent.click(getByTitle("disable"));
    await waitFor(() => expect(getByText(/toggle failed for hook-1/i)).toBeDefined());
    expect(getByText("hook-1")).toBeDefined();
  });

  it("shows dry-run failure message and does not hide state", async () => {
    apiMocks.fetchHooks.mockResolvedValue([hook()]);
    apiMocks.dryRunHook.mockRejectedValue(new Error("dry-run failed"));

    const { getByTitle, getByText } = render(<HooksScreen />);

    await waitFor(() => expect(getByText("hook-1")).toBeDefined());

    fireEvent.click(getByTitle("dry-run"));
    await waitFor(() => expect(getByText(/dry-run failed for hook-1/i)).toBeDefined());
    expect(apiMocks.fetchHookFirings).toHaveBeenCalledTimes(0);
  });

  it("offers spawn and send_input as always-available actions", async () => {
    apiMocks.fetchHooks.mockResolvedValue([hook()]);
    apiMocks.fetchHookEvents.mockResolvedValue([]);

    const { getAllByRole, findByText } = render(<HooksScreen />);
    await findByText("hook-1");

    const createButton = getAllByRole("button", { name: "new hook" })[0];
    createButton.click();

    const editor = await findByText("new hook");
    expect(editor).toBeTruthy();
    const actionSelect = document.querySelectorAll("select")[1];
    const options = Array.from(actionSelect?.querySelectorAll("option") ?? []).map(
      (o) => o.textContent ?? "",
    );
    expect(options).toContain("spawn");
    expect(options).toContain("send_input");
  });

  it("serializes a send_input action's node picker + text into target/template", async () => {
    const target = fakeNode({ id: "22222222-2222-2222-2222-222222222222", role_hint: "supervisor" });
    apiMocks.fetchHooks.mockResolvedValue([]);
    apiMocks.fetchHookEvents.mockResolvedValue([]);
    apiMocks.fetchNodes.mockResolvedValue([target]);
    apiMocks.createHook.mockResolvedValue(hook());

    const { getAllByRole, findByText, getByLabelText, getByPlaceholderText } = render(<HooksScreen />);
    fireEvent.click(getAllByRole("button", { name: "new hook" })[0]);
    await waitFor(() => expect(document.querySelector(".modal")).toBeTruthy());

    const kindSelect = document.querySelectorAll("select")[1];
    fireEvent.change(kindSelect, { target: { value: "send_input" } });

    const targetSelect = await waitFor(() => getByLabelText("send_input target node"));
    fireEvent.change(targetSelect, { target: { value: target.id } });

    const textInput = getByPlaceholderText("text — e.g. continue: {event}");
    fireEvent.change(textInput, { target: { value: "continue: reply done" } });

    fireEvent.click(getAllByRole("button", { name: "create hook" })[0]);

    await waitFor(() => expect(apiMocks.createHook).toHaveBeenCalled());
    const actions = apiMocks.createHook.mock.calls[0][0].actions as HookAction[];
    expect(actions[0]).toEqual({
      kind: "send_input",
      target: target.id,
      template: "continue: reply done",
      args: {},
    });
  });

  it("defaults a send_input action's target to the event's node when left blank", async () => {
    apiMocks.fetchHooks.mockResolvedValue([]);
    apiMocks.fetchHookEvents.mockResolvedValue([]);
    apiMocks.fetchNodes.mockResolvedValue([]);
    apiMocks.createHook.mockResolvedValue(hook());

    const { getAllByRole, findByText, getByPlaceholderText } = render(<HooksScreen />);
    fireEvent.click(getAllByRole("button", { name: "new hook" })[0]);
    await waitFor(() => expect(document.querySelector(".modal")).toBeTruthy());

    const kindSelect = document.querySelectorAll("select")[1];
    fireEvent.change(kindSelect, { target: { value: "send_input" } });

    const textInput = getByPlaceholderText("text — e.g. continue: {event}");
    fireEvent.change(textInput, { target: { value: "hello" } });

    fireEvent.click(getAllByRole("button", { name: "create hook" })[0]);

    await waitFor(() => expect(apiMocks.createHook).toHaveBeenCalled());
    const actions = apiMocks.createHook.mock.calls[0][0].actions as HookAction[];
    expect(actions[0].target).toBe("");
  });

  it("serializes a spawn action's harness/substrate/role/workspace/description/prompt fields", async () => {
    apiMocks.fetchHooks.mockResolvedValue([]);
    apiMocks.fetchHookEvents.mockResolvedValue([]);
    apiMocks.fetchNodes.mockResolvedValue([]);
    apiMocks.createHook.mockResolvedValue(hook());

    const { getAllByRole, findByText, getByLabelText, getByPlaceholderText } = render(<HooksScreen />);
    fireEvent.click(getAllByRole("button", { name: "new hook" })[0]);
    await waitFor(() => expect(document.querySelector(".modal")).toBeTruthy());

    const kindSelect = document.querySelectorAll("select")[1];
    fireEvent.change(kindSelect, { target: { value: "spawn" } });

    fireEvent.change(await waitFor(() => getByLabelText("spawn harness")), {
      target: { value: "codex" },
    });
    fireEvent.change(getByLabelText("spawn substrate"), { target: { value: "loon" } });
    fireEvent.change(getByPlaceholderText("role (default worker)"), {
      target: { value: "auditor" },
    });
    fireEvent.change(getByPlaceholderText("workspace (optional path)"), {
      target: { value: "/tmp/audit" },
    });
    fireEvent.change(getByPlaceholderText("description (optional)"), {
      target: { value: "night audit run" },
    });
    fireEvent.change(
      getByPlaceholderText("prompt — first instruction (optional, template-rendered)"),
      { target: { value: "audit {node.id}" } },
    );

    fireEvent.click(getAllByRole("button", { name: "create hook" })[0]);

    await waitFor(() => expect(apiMocks.createHook).toHaveBeenCalled());
    const actions = apiMocks.createHook.mock.calls[0][0].actions as HookAction[];
    expect(actions[0]).toEqual({
      kind: "spawn",
      target: "",
      template: undefined,
      args: {
        harness: "codex",
        substrate: "loon",
        role: "auditor",
        workspace: "/tmp/audit",
        description: "night audit run",
        prompt: "audit {node.id}",
      },
    });
  });

  it("spawn action defaults harness/substrate and omits blank optional fields", async () => {
    apiMocks.fetchHooks.mockResolvedValue([]);
    apiMocks.fetchHookEvents.mockResolvedValue([]);
    apiMocks.fetchNodes.mockResolvedValue([]);
    apiMocks.createHook.mockResolvedValue(hook());

    const { getAllByRole, findByText } = render(<HooksScreen />);
    fireEvent.click(getAllByRole("button", { name: "new hook" })[0]);
    await waitFor(() => expect(document.querySelector(".modal")).toBeTruthy());

    const kindSelect = document.querySelectorAll("select")[1];
    fireEvent.change(kindSelect, { target: { value: "spawn" } });

    fireEvent.click(getAllByRole("button", { name: "create hook" })[0]);

    await waitFor(() => expect(apiMocks.createHook).toHaveBeenCalled());
    const actions = apiMocks.createHook.mock.calls[0][0].actions as HookAction[];
    expect(actions[0]).toEqual({
      kind: "spawn",
      target: "",
      template: undefined,
      args: { harness: "claude_code", substrate: "local", role: "worker" },
    });
  });
});
