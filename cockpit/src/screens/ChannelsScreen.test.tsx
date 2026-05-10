import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent } from "@testing-library/dom";
import { ChannelsScreen } from "./ChannelsScreen";
import type { ChannelDescriptor } from "../types";

const apiMocks = vi.hoisted(() => ({
  createChannel: vi.fn(),
  deleteChannel: vi.fn(),
  fetchChannelMessages: vi.fn(),
  fetchChannels: vi.fn(),
  testChannel: vi.fn(),
  updateChannel: vi.fn(),
}));

vi.mock("../api", () => apiMocks);

function channel(overrides: Partial<ChannelDescriptor> = {}): ChannelDescriptor {
  return {
    id: "ntfy-default",
    kind: "ntfy",
    name: "ntfy default",
    label: "ntfy default",
    direction: "duplex",
    status: "live",
    detail: "ntfy outbound + inbound",
    config: {},
    live: true,
    builtin: true,
    created_at_epoch_secs: 0,
    message_count_24h: 0,
    ...overrides,
  };
}

describe("ChannelsScreen", () => {
  beforeEach(() => {
    apiMocks.fetchChannelMessages.mockReset();
    apiMocks.fetchChannels.mockReset();
    apiMocks.createChannel.mockReset();
    apiMocks.deleteChannel.mockReset();
    apiMocks.testChannel.mockReset();
    apiMocks.updateChannel.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it("does not expose manual inbound recording for a live inbound channel", async () => {
    apiMocks.fetchChannels.mockResolvedValue([
      channel(),
      channel({
        id: "webhook-substrate",
        kind: "webhook",
        name: "webhook substrate",
        direction: "inbound",
      }),
    ]);
    apiMocks.fetchChannelMessages.mockResolvedValue([]);

    render(<ChannelsScreen />);

    expect((await screen.findAllByText("ntfy default")).length).toBeGreaterThan(0);
    expect(screen.queryByText("record inbound")).toBeNull();
  });

  it("new channel modal only exposes implemented channel kinds", async () => {
    apiMocks.fetchChannels.mockResolvedValue([channel()]);
    apiMocks.fetchChannelMessages.mockResolvedValue([]);

    render(<ChannelsScreen />);

    const createButton = await screen.findByText("new channel");
    fireEvent.click(createButton);

    const modal = await screen.findByRole("button", { name: "cancel" }).then(() =>
      document.querySelector(".modal"),
    );
    expect(modal).toBeTruthy();
    if (!modal) return;

    const selects = modal.querySelectorAll("select");
    const kindSelect = selects?.[0];
    expect(selects).toHaveLength(2);
    expect(kindSelect).not.toBeNull();
    if (!kindSelect) return;

    const optionValues = Array.from(kindSelect.options).map((o) => o.value);
    expect(optionValues).toEqual(expect.arrayContaining(["ntfy", "webhook"]));
    expect(optionValues).not.toEqual(expect.arrayContaining(["sms", "discord", "slack", "email"]));
  });

  it("defaults webhook channels to inbound direction and hides outbound", async () => {
    apiMocks.fetchChannels.mockResolvedValue([channel()]);
    apiMocks.fetchChannelMessages.mockResolvedValue([]);

    render(<ChannelsScreen />);

    const createButton = await screen.findByText("new channel");
    fireEvent.click(createButton);

    const modal = await screen.findByRole("button", { name: "cancel" }).then(() =>
      document.querySelector(".modal"),
    );
    expect(modal).toBeTruthy();
    if (!modal) return;

    const selects = modal.querySelectorAll("select");
    expect(selects).toHaveLength(2);

    const kindSelect = selects[0];
    const directionSelect = selects[1];
    fireEvent.change(kindSelect, { target: { value: "webhook" } });

    const dirOptions = Array.from(directionSelect.options).map((option) => option.value);
    expect(dirOptions).toEqual(["inbound"]);
    expect(directionSelect).toHaveProperty("value", "inbound");
  });

  it("shows adapter cleanup wording for inactive channels", async () => {
    apiMocks.fetchChannels.mockResolvedValue([
      channel({
        id: "disabled-ntfy",
        kind: "ntfy",
        name: "disabled ntfy channel",
        live: false,
      }),
    ]);
    apiMocks.fetchChannelMessages.mockResolvedValue([]);

    render(<ChannelsScreen />);

    expect(await screen.findByText("disabled ntfy channel")).toBeTruthy();
    expect(screen.getByText("inactive · adapters disabled")).toBeTruthy();
  });

  it("disables send test when a channel has no outbound adapter", async () => {
    apiMocks.fetchChannels.mockResolvedValue([
      channel({
        id: "webhook-substrate",
        kind: "webhook",
        name: "webhook substrate",
        direction: "inbound",
        detail: "inbound webhook",
      }),
    ]);
    apiMocks.fetchChannelMessages.mockResolvedValue([]);
    apiMocks.testChannel.mockResolvedValue({ sent: false });

    render(<ChannelsScreen />);

    const sendButton = await screen.findByRole("button", { name: "send test" });
    expect((sendButton as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(sendButton);
    expect(screen.getByText(/send-test unavailable/i)).toBeTruthy();
    expect(screen.queryByText(/recorded but not delivered/i)).toBeNull();
    expect(apiMocks.testChannel).not.toHaveBeenCalled();
  });

  it("shows an explicit error when channels cannot be loaded initially", async () => {
    apiMocks.fetchChannels.mockRejectedValue(new Error("channels endpoint unavailable"));
    apiMocks.fetchChannelMessages.mockResolvedValue([]);

    render(<ChannelsScreen />);

    expect(await screen.findByText("unable to load channels")).toBeTruthy();
    expect(screen.getByText("channels endpoint unavailable")).toBeTruthy();
    expect(screen.queryByText(/loading channels/i)).toBeNull();
  });

  it("surfaces message refresh failures and keeps channel metadata visible", async () => {
    apiMocks.fetchChannels.mockResolvedValue([
      channel({
        id: "webhook-substrate",
        kind: "webhook",
        name: "webhook substrate",
        detail: "local inbound",
        direction: "inbound",
      }),
    ]);
    apiMocks.fetchChannelMessages.mockRejectedValue(new Error("message endpoint unavailable"));

    render(<ChannelsScreen />);

    expect(await screen.findByText("webhook substrate")).toBeTruthy();
    expect(await screen.findByText(/local inbound/i)).toBeTruthy();
    expect(await screen.findByText(/message refresh failed/i)).toBeTruthy();
    expect(screen.getByText(/message endpoint unavailable/i)).toBeTruthy();
  });
});
