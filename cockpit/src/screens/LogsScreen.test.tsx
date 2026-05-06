import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { LogsScreen } from "./LogsScreen";
import type { NotificationRecord } from "../types";

function fixture(overrides: Partial<NotificationRecord>): NotificationRecord {
  return {
    id: "notif-1",
    node_id: null,
    title: "default title",
    body: "default body",
    severity: "info",
    created_at: "2026-01-01T00:00:00Z",
    read: false,
    ...overrides,
  };
}

describe("LogsScreen notification workflow", () => {
  afterEach(() => {
    cleanup();
  });

  it("filters all/unread and shows counts", () => {
    const notifications: NotificationRecord[] = [
      fixture({ id: "n-1", read: false, title: "new event" }),
      fixture({ id: "n-2", read: true, title: "old event" }),
    ];

    const { getByRole, getAllByRole, container } = render(
      <LogsScreen notifications={notifications} onMarkRead={vi.fn().mockResolvedValue(undefined)} />,
    );

    expect(getByRole("button", { name: "all (2)" })).toBeDefined();
    expect(getByRole("button", { name: "unread (1)" })).toBeDefined();

    fireEvent.click(getAllByRole("button", { name: "unread (1)" })[0]);
    expect(container.textContent).toContain("new event");
    expect(container.textContent).not.toContain("old event");
  });

  it("calls mark-read when user accepts", () => {
    const onMarkRead = vi.fn().mockResolvedValue(undefined);
    const notifications = [fixture({ id: "n-1", read: false })];

    const { getAllByRole } = render(<LogsScreen notifications={notifications} onMarkRead={onMarkRead} />);

    const markReadButton = getAllByRole("button", { name: "mark read" })[0];
    fireEvent.click(markReadButton);
    expect(onMarkRead).toHaveBeenCalledWith("n-1");
  });

  it("opens notification-linked nodes", () => {
    const onOpenNode = vi.fn();
    const notifications = [fixture({ id: "n-1", node_id: "node-abc" })];
    const { getByRole } = render(
      <LogsScreen notifications={notifications} onMarkRead={vi.fn().mockResolvedValue(undefined)} onOpenNode={onOpenNode} />,
    );

    const openButton = getByRole("button", { name: "open" });
    fireEvent.click(openButton);
    expect(onOpenNode).toHaveBeenCalledWith("node-abc");
  });
});
