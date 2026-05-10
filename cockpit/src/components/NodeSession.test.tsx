import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NodeSession } from "./NodeSession";
import type { AsylumNode, CapabilitySnapshot } from "../types";

const apiMocks = vi.hoisted(() => ({
  postNodeInput: vi.fn(),
  openNodeObserveSocket: vi.fn(),
  requestBrowserAttach: vi.fn(),
  openAttachSocket: vi.fn(),
}));

vi.mock("../api", () => apiMocks);

const xtermMocks = vi.hoisted(() => ({
  terminals: [] as Array<{
    clear: ReturnType<typeof vi.fn>;
    dispose: ReturnType<typeof vi.fn>;
    loadAddon: ReturnType<typeof vi.fn>;
    onData: ReturnType<typeof vi.fn>;
    open: ReturnType<typeof vi.fn>;
    write: ReturnType<typeof vi.fn>;
    emitData: (data: string) => void;
  }>,
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: vi.fn().mockImplementation(function Terminal() {
    let dataHandler: ((data: string) => void) | undefined;
    const terminal = {
      clear: vi.fn(),
      dispose: vi.fn(),
      loadAddon: vi.fn(),
      onData: vi.fn((handler: (data: string) => void) => {
        dataHandler = handler;
        return { dispose: vi.fn() };
      }),
      open: vi.fn(),
      write: vi.fn(),
      emitData: (data: string) => dataHandler?.(data),
    };
    xtermMocks.terminals.push(terminal);
    return terminal;
  }),
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: vi.fn().mockImplementation(function FitAddon() {
    return { fit: vi.fn() };
  }),
}));

const caps: CapabilitySnapshot = {
  browser_attach: true,
  native_attach: true,
  send_input: true,
  interrupt: true,
  stop: true,
  resume: false,
  structured_events: true,
  transcript_export: true,
};

function node(overrides: Partial<AsylumNode> = {}): AsylumNode {
  return {
    id: "node-session-loop",
    harness: "codex",
    substrate: "local",
    role_hint: "worker",
    liveness: "running",
    workspace: "/tmp/asylum",
    description: "worker",
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

class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  static instances: MockWebSocket[] = [];

  readonly CONNECTING = MockWebSocket.CONNECTING;
  readonly OPEN = MockWebSocket.OPEN;
  readonly CLOSING = MockWebSocket.CLOSING;
  readonly CLOSED = MockWebSocket.CLOSED;
  readonly sent: unknown[] = [];
  throwOnNextSend: ((data: unknown) => void) | null = null;
  readyState = MockWebSocket.CONNECTING;
  binaryType: BinaryType = "blob";
  private listeners = new Map<string, Array<(event: Event | MessageEvent) => void>>();

  constructor(readonly url: string) {
    MockWebSocket.instances.push(this);
  }

  addEventListener(type: string, listener: (event: Event | MessageEvent) => void): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  send(data: unknown): void {
    if (this.readyState !== MockWebSocket.OPEN) {
      throw new Error("WebSocket is not open");
    }
    if (this.throwOnNextSend) {
      const doThrow = this.throwOnNextSend;
      this.throwOnNextSend = null;
      doThrow(data);
      return;
    }
    this.sent.push(data);
  }

  close(): void {
    this.readyState = MockWebSocket.CLOSED;
    this.dispatch("close", new Event("close"));
  }

  open(): void {
    this.readyState = MockWebSocket.OPEN;
    this.dispatch("open", new Event("open"));
  }

  message(data: string): void {
    this.dispatch("message", new MessageEvent("message", { data }));
  }

  private dispatch(type: string, event: Event | MessageEvent): void {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(event);
    }
  }
}

describe("NodeSession session semantics", () => {
  beforeEach(() => {
    apiMocks.postNodeInput.mockReset();
    apiMocks.openNodeObserveSocket.mockReset();
    apiMocks.openNodeObserveSocket.mockReturnValue({ close: vi.fn() });
    apiMocks.requestBrowserAttach.mockReset();
    apiMocks.requestBrowserAttach.mockResolvedValue({
      attach_url: "http://localhost/attach/raw-token",
      token: "raw-token",
      expires_in_seconds: 600,
      transport: "local_pty",
      note: null,
    });
    apiMocks.openAttachSocket.mockReset();
    apiMocks.openAttachSocket.mockImplementation((token: string, options: { onOpen?: () => void; onError?: () => void; onClose?: () => void; onMessage?: (data: string) => void }) => {
      const ws = new MockWebSocket(`ws://localhost/api/attach/${token}/ws`);
      if (options?.onOpen) ws.addEventListener("open", options.onOpen);
      if (options?.onError) ws.addEventListener("error", options.onError as (event: Event | MessageEvent) => void);
      if (options?.onClose) ws.addEventListener("close", options.onClose);
      return ws;
    });
    xtermMocks.terminals.length = 0;
    MockWebSocket.instances = [];
    vi.stubGlobal("WebSocket", MockWebSocket);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("does not expose attach controls in the session header", () => {
    apiMocks.openNodeObserveSocket.mockReturnValue({ close: vi.fn() });

    const { queryByTitle } = render(<NodeSession node={node()} />);

    expect(queryByTitle("open attach tab")).toBeNull();
    expect(queryByTitle("open attach tab via loon attach")).toBeNull();
    expect(queryByTitle("open in terminal")).toBeNull();
  });

  it("ignores attach-issued history as an internal transport event", async () => {
    let onMessage: ((data: string) => void) | undefined;
    apiMocks.openNodeObserveSocket.mockImplementation((_nodeId: string, options: { onMessage?: (data: string) => void }) => {
      onMessage = options.onMessage;
      return { close: vi.fn() };
    });

    const { container, queryByText, queryByTitle } = render(<NodeSession node={node()} />);

    await waitFor(() => {
      expect(onMessage).toBeTypeOf("function");
    });

    onMessage?.(JSON.stringify({
      kind: "attach_issued",
      node_id: "node-session-loop",
      body: {
        url: "http://127.0.0.1:7717/attach/token",
        node_id: "node-session-loop",
      },
    }));

    await waitFor(() => expect(apiMocks.openNodeObserveSocket).toHaveBeenCalled());
    expect(queryByText("open a time-limited Cockpit attach view for this node")).toBeNull();
    expect(queryByTitle("open attach tab")).toBeNull();
    expect(container.textContent ?? "").not.toContain("attach tab");
  });

  it("describes Loon live-stream limitations in session language", async () => {
    let onMessage: ((data: string) => void) | undefined;
    apiMocks.openNodeObserveSocket.mockImplementation((_nodeId: string, options: { onMessage?: (data: string) => void }) => {
      onMessage = options.onMessage;
      return { close: vi.fn() };
    });

    const { queryByText, getByText, container } = render(<NodeSession node={node({ substrate: "loon" })} />);

    onMessage?.("asylum.observe.ws.initialized");
    onMessage?.("asylum.observe.ws.live_stream_unavailable");
    fireEvent.click(getByText("struct"));

    await waitFor(() => {
      expect(container.textContent).toContain("Loon nodes do not stream local PTY-style live observe output; open the node session for an interactive terminal");
    });
    expect(queryByText(/use attach/i)).toBeNull();
  });

  it("renders missing workspace as none, not a home-directory default", () => {
    const { container } = render(<NodeSession node={node({ workspace: null })} />);

    expect(screen.getByText(/workspace none · ctx est\. 0%/)).toBeDefined();
    fireEvent.click(screen.getByText("struct"));
    expect(screen.getByText(/workspace none · worker/)).toBeDefined();
    expect(container.textContent).not.toContain("~/");
  });

  it("sends xterm keystrokes over the raw attach WebSocket instead of the line input endpoint", async () => {
    render(<NodeSession node={node()} />);

    await waitFor(() => expect(MockWebSocket.instances).toHaveLength(1));
    const ws = MockWebSocket.instances[0];
    ws.open();

    xtermMocks.terminals[0].emitData("a");
    xtermMocks.terminals[0].emitData("\r");

    expect(ws.url).toBe("ws://localhost/api/attach/raw-token/ws");
    expect(ws.sent).toEqual(["a", "\r"]);
    expect(apiMocks.postNodeInput).not.toHaveBeenCalled();
  });

  it("sends prompt textarea submissions over the raw attach WebSocket when available", async () => {
    render(<NodeSession node={node()} />);

    await waitFor(() => expect(MockWebSocket.instances).toHaveLength(1));
    const ws = MockWebSocket.instances[0];
    ws.open();

    const prompt = screen.getByPlaceholderText(/send input to node-session-loop/i);
    fireEvent.change(prompt, { target: { value: "Reply with ASYLUM_VALIDATION_ACK" } });
    fireEvent.keyDown(prompt, { key: "Enter", code: "Enter" });

    await waitFor(() => expect(ws.sent).toEqual(["Reply with ASYLUM_VALIDATION_ACK", "\r"]));
    expect(apiMocks.postNodeInput).not.toHaveBeenCalled();
    expect((prompt as HTMLTextAreaElement).value).toBe("");
  });

  it("queues terminal keystrokes until the attach websocket is OPEN, then flushes them", async () => {
    render(<NodeSession node={node()} />);

    await waitFor(() => expect(MockWebSocket.instances).toHaveLength(1));
    const ws = MockWebSocket.instances[0];

    xtermMocks.terminals[0].emitData("a");
    xtermMocks.terminals[0].emitData("\r");

    expect(ws.sent).toEqual([]);
    expect(apiMocks.postNodeInput).not.toHaveBeenCalled();

    ws.open();
    await waitFor(() => expect(ws.sent).toEqual(["a", "\r"]));
    expect(apiMocks.postNodeInput).not.toHaveBeenCalled();
  });

  it("falls back to /nodes/:id/input only after attach closes, not while connecting", async () => {
    render(<NodeSession node={node()} />);

    await waitFor(() => expect(MockWebSocket.instances).toHaveLength(1));
    const ws = MockWebSocket.instances[0];

    xtermMocks.terminals[0].emitData("a");
    expect(ws.sent).toEqual([]);
    expect(apiMocks.postNodeInput).not.toHaveBeenCalled();

    ws.close();
    xtermMocks.terminals[0].emitData("b");

    expect(apiMocks.postNodeInput).toHaveBeenCalledWith("node-session-loop", "a");
    expect(apiMocks.postNodeInput).toHaveBeenCalledWith("node-session-loop", "b");
  });

  it("falls back to line input when attach token cannot be extracted", async () => {
    apiMocks.requestBrowserAttach.mockResolvedValue({
      attach_url: "http://localhost/no-token-here",
      token: undefined,
      expires_in_seconds: 600,
      transport: "local_pty",
      note: null,
    });

    render(<NodeSession node={node()} />);

    await waitFor(() => expect(apiMocks.openAttachSocket).not.toHaveBeenCalled());
    await waitFor(() => expect(xtermMocks.terminals).toHaveLength(1));

    xtermMocks.terminals[0].emitData("q");

    await waitFor(() => expect(apiMocks.postNodeInput).toHaveBeenCalledWith("node-session-loop", "q"));
    expect(MockWebSocket.instances).toHaveLength(0);
    expect(apiMocks.postNodeInput).toHaveBeenCalledTimes(1);
  });

  it("degrades queued input to line input when flush send throws after attach open", async () => {
    apiMocks.openAttachSocket.mockImplementation((token: string, options: { onOpen?: () => void; onError?: (e: Event) => void; onClose?: () => void; onMessage?: (data: string) => void }) => {
      const ws = new MockWebSocket(`ws://localhost/api/attach/${token}/ws`);
      if (options?.onOpen) ws.addEventListener("open", options.onOpen);
      if (options?.onError) ws.addEventListener("error", options.onError as (event: Event | MessageEvent) => void);
      if (options?.onClose) ws.addEventListener("close", options.onClose);
      ws.throwOnNextSend = () => {
        throw new Error("forced attach send failure");
      };
      return ws;
    });

    render(<NodeSession node={node()} />);

    await waitFor(() => expect(MockWebSocket.instances).toHaveLength(1));
    const ws = MockWebSocket.instances[0];

    xtermMocks.terminals[0].emitData("a");
    xtermMocks.terminals[0].emitData("\r");

    ws.open();

    await waitFor(() => expect(apiMocks.postNodeInput).toHaveBeenCalledWith("node-session-loop", "a"));
    expect(apiMocks.postNodeInput).toHaveBeenCalledWith("node-session-loop", "\r");
    expect(ws.sent).toEqual([]);
    expect(apiMocks.postNodeInput).toHaveBeenCalledTimes(2);
  });

  it("shows a terminal/system-line error when fallback postNodeInput fails", async () => {
    apiMocks.requestBrowserAttach.mockResolvedValue({
      attach_url: "http://localhost/no-token-here",
      token: undefined,
      expires_in_seconds: 600,
      transport: "local_pty",
      note: null,
    });
    apiMocks.postNodeInput.mockRejectedValue(new Error("input endpoint offline"));

    render(<NodeSession node={node()} />);

    await waitFor(() => expect(apiMocks.openAttachSocket).not.toHaveBeenCalled());
    await waitFor(() => expect(xtermMocks.terminals).toHaveLength(1));

    xtermMocks.terminals[0].emitData("q");
    xtermMocks.terminals[0].emitData("\r");

    fireEvent.click(screen.getByText("struct"));
    await waitFor(() => {
      const matching = screen.getAllByText(/send-input fallback failed: input endpoint offline/i);
      expect(matching).toHaveLength(2);
    });
    expect(apiMocks.postNodeInput).toHaveBeenCalledWith("node-session-loop", "q");
    expect(apiMocks.postNodeInput).toHaveBeenCalledWith("node-session-loop", "\r");
    expect(apiMocks.postNodeInput).toHaveBeenCalledTimes(2);
  });
});
