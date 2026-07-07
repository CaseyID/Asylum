import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { HooksScreen } from "./HooksScreen";
import type { HookFiringRecord, HookRule } from "../types";

const apiMocks = vi.hoisted(() => ({
  createHook: vi.fn(),
  deleteHook: vi.fn(),
  dryRunHook: vi.fn(),
  fetchHookEvents: vi.fn(),
  fetchHookFirings: vi.fn(),
  fetchHooks: vi.fn(),
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

describe("HooksScreen error handling", () => {
  beforeEach(() => {
    apiMocks.createHook.mockReset();
    apiMocks.deleteHook.mockReset();
    apiMocks.dryRunHook.mockReset();
    apiMocks.fetchHookEvents.mockReset();
    apiMocks.fetchHookFirings.mockReset();
    apiMocks.fetchHooks.mockReset();
    apiMocks.updateHook.mockReset();

    apiMocks.fetchHookEvents.mockResolvedValue([]);
    apiMocks.fetchHookFirings.mockResolvedValue([]);
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
});

