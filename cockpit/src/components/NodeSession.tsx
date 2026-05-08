// asylum cockpit — NodeSession: the one terminal/chat primitive.
// every "chat" surface in the app is a viewport onto this. modes only change
// the chrome (compact vs full); the harness-authentic TUI inside is the same.
//
// all input goes to the daemon via postNodeInput; all output arrives over the
// observe websocket and streams into the xterm.js terminal. no canned simulation.

import { useEffect, useRef, useState, useCallback, type ReactElement } from "react";
import "@xterm/xterm/css/xterm.css";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { Btn } from "../lib/ui";
import { ToolCall } from "../lib/ui";
import { Icon } from "../lib/icons";
import { isCommandCenter, shortNodeId, uptimeLabel } from "../lib/glyphs";
import { postNodeInput, openNodeObserveSocket } from "../api";
import type { AsylumNode } from "../types";

// ─── public types ────────────────────────────────────────────────────────────

export type SessionMode = "cockpit" | "fullscreen";

export interface NodeSessionProps {
  node: AsylumNode;
  mode?: SessionMode;
  onAttach?: (nodeId: string) => void;
  onNativeAttach?: (nodeId: string) => void;
  onInterrupt?: (nodeId: string) => void;
  onExpand?: () => void;
}

// ─── internal transcript entry types (structured view only) ─────────────────

type TranscriptEntry =
  | { kind: "user"; text: string }
  | { kind: "thought"; text: string }
  | { kind: "text"; text: string; id?: string }
  | { kind: "list"; items: string[] }
  | { kind: "tool"; name: string; args?: Record<string, unknown>; output?: string; state?: "ok" | "pending" | "error" }
  | { kind: "attach"; node: string; url: string }
  | { kind: "sys-line"; text: string }
  | { kind: "prompt" };

// ─── observe ws event shape ──────────────────────────────────────────────────
// matches NodeEvent on the daemon side (asylum-core::event).

interface NodeEvent {
  id?: string;
  node_id?: string;
  sequence?: number;
  kind?: string;
  body?: Record<string, unknown> | null;
  created_at?: string;
}

const WS_INIT_FRAME = "asylum.observe.ws.initialized";
const WS_LIVE_UNAVAILABLE = "asylum.observe.ws.live_stream_unavailable";

// ─── xterm theme matching cockpit dark palette ────────────────────────────────
const XTERM_THEME = {
  background: "#0d0d0d",
  foreground: "#e2e2e2",
  black: "#1a1a1a",
  red: "#e06c75",
  green: "#7dbb87",
  yellow: "#e5c07b",
  blue: "#61afef",
  magenta: "#c678dd",
  cyan: "#56b6c2",
  white: "#abb2bf",
  brightBlack: "#4b5263",
  brightRed: "#e06c75",
  brightGreen: "#98c379",
  brightYellow: "#e5c07b",
  brightBlue: "#61afef",
  brightMagenta: "#c678dd",
  brightCyan: "#56b6c2",
  brightWhite: "#ffffff",
  cursor: "#e2e2e2",
  cursorAccent: "#0d0d0d",
  selectionBackground: "#3e4451",
};

// ─── initial transcript (structured view) ────────────────────────────────────
function initialTranscript(node: AsylumNode): TranscriptEntry[] {
  const isCC = isCommandCenter(node);
  const harnessId = node.harness === "claude_code" ? "claude-code" : "codex";
  const role = isCC ? "command-center" : (node.role_hint || "worker");
  const sysLine = `attached to ${shortNodeId(node.id)} · ${harnessId} · ${node.substrate} · workspace ${node.workspace ?? "~/"} · ${role}`;
  return [
    { kind: "sys-line", text: sysLine },
    { kind: "prompt" },
  ];
}

// ─── NodeSession ──────────────────────────────────────────────────────────────
export function NodeSession({
  node,
  mode = "cockpit",
  onAttach,
  onNativeAttach,
  onInterrupt,
  onExpand,
}: NodeSessionProps): ReactElement {
  // structured view fallback transcript (used when view === "structured")
  const [entries, setEntries] = useState<TranscriptEntry[]>(() => initialTranscript(node));
  const [input, setInput] = useState("");
  const [view, setView] = useState<"tui" | "structured">("tui");

  // xterm refs
  const termContainerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  // structured-view scroll container
  const structBodyRef = useRef<HTMLDivElement | null>(null);

  const phaseRef = useRef<"history" | "live">("history");
  const liveDisabledRef = useRef<boolean>(false);

  // track recently-sent input texts for structured-view dedupe (M3)
  // keyed by text; value is expiry timestamp
  const sentSetRef = useRef<Map<string, number>>(new Map());

  const harnessId = node.harness === "claude_code" ? "claude-code" : "codex";

  // ─── xterm setup / teardown ───────────────────────────────────────────────

  useEffect(() => {
    if (!termContainerRef.current) return;

    const term = new Terminal({
      theme: XTERM_THEME,
      fontFamily: "var(--font-mono, 'JetBrains Mono', 'Fira Mono', 'Menlo', monospace)",
      fontSize: 13,
      lineHeight: 1.45,
      cursorBlink: true,
      scrollback: 5000,
      convertEol: false,
      allowProposedApi: false,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(termContainerRef.current);
    fitAddon.fit();

    termRef.current = term;
    fitAddonRef.current = fitAddon;

    // raw keystrokes from the terminal go directly to the harness
    const dataDispose = term.onData((data: string) => {
      void postNodeInput(node.id, data).catch(() => {
        // swallow — the harness may not be live; errors surface via ws events
      });
    });

    // resize observer — refit when the container changes size
    const ro = new ResizeObserver(() => {
      try {
        fitAddon.fit();
      } catch {
        // ignore — can race with dispose
      }
    });
    ro.observe(termContainerRef.current);

    return () => {
      dataDispose.dispose();
      ro.disconnect();
      term.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
    };
  // node.id intentionally excluded — terminal lifecycle is per mount, ws handles node changes
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // refit when view switches to tui (container may have been hidden)
  useEffect(() => {
    if (view === "tui") {
      requestAnimationFrame(() => {
        try {
          fitAddonRef.current?.fit();
        } catch {
          // ignore
        }
      });
    }
  }, [view]);

  // scroll structured view on new entries
  useEffect(() => {
    if (structBodyRef.current) {
      structBodyRef.current.scrollTop = structBodyRef.current.scrollHeight;
    }
  }, [entries]);

  // ─── submit (textarea input) ──────────────────────────────────────────────

  const submit = useCallback(async (): Promise<void> => {
    const v = input.trimEnd();
    if (!v) return;
    setInput("");

    // optimistic echo into both surfaces
    const withNewline = v.endsWith("\n") ? v : v + "\n";
    // write to terminal (local echo for the tui surface)
    termRef.current?.write(withNewline);
    // optimistic push for structured view — track it so input_sent doesn't duplicate
    setEntries(p => [...p, { kind: "user", text: v }]);
    sentSetRef.current.set(v, Date.now() + 4000);

    try {
      await postNodeInput(node.id, v);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      termRef.current?.write(`\r\nsend-input failed: ${msg}\r\n`);
      setEntries(p => [...p, { kind: "sys-line", text: `send-input failed: ${msg}` }]);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [input, node.id]);

  // ─── observe websocket ────────────────────────────────────────────────────

  useEffect(() => {
    phaseRef.current = "history";
    liveDisabledRef.current = false;

    // clear terminal and structured transcript on new node
    termRef.current?.clear();
    setEntries(initialTranscript(node));

    const ws = openNodeObserveSocket(node.id, {
      onMessage: (data) => {
        if (typeof data !== "string") return;

        if (data === WS_INIT_FRAME) {
          phaseRef.current = "live";
          return;
        }

        if (data === WS_LIVE_UNAVAILABLE) {
          liveDisabledRef.current = true;
          const message = node.substrate === "loon"
            ? "Loon nodes do not stream local PTY-style live observe output; use attach for an interactive session"
            : "live streaming not supported by this substrate";
          termRef.current?.write(`\r\n· ${message}\r\n`);
          setEntries(p => [...p, { kind: "sys-line", text: message }]);
          return;
        }

        if (phaseRef.current === "history") {
          handleHistoryFrame(data);
        } else {
          if (!liveDisabledRef.current) {
            handleLiveFrame(data);
          }
        }
      },
    });

    return () => {
      try { ws.close(); } catch { /* already closed */ }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [node.id]);

  // ─── frame handlers ───────────────────────────────────────────────────────

  function handleHistoryFrame(data: string): void {
    let evt: NodeEvent | null = null;
    try {
      const parsed = JSON.parse(data) as unknown;
      if (parsed && typeof parsed === "object" && "kind" in (parsed as object)) {
        evt = parsed as NodeEvent;
      }
    } catch {
      evt = null;
    }
    if (!evt) {
      // unknown frame — write raw to terminal
      termRef.current?.write(data);
      setEntries(p => [...p, { kind: "text", text: data }]);
      return;
    }
    appendNodeEvent(evt);
  }

  function appendNodeEvent(evt: NodeEvent): void {
    const body = (evt.body ?? {}) as Record<string, unknown>;
    switch (evt.kind) {
      case "output_chunk": {
        const text = typeof body.text === "string" ? body.text : "";
        if (!text) return;
        // write raw bytes to terminal — xterm decodes ANSI
        termRef.current?.write(text);
        // also track for structured view
        setEntries(p => [...p, { kind: "text", text }]);
        return;
      }
      case "input_sent": {
        const text = typeof body.text === "string" ? body.text : "";
        // M3: skip rendering if this client sent it recently (optimistic echo already shown)
        const expiry = sentSetRef.current.get(text);
        if (expiry && Date.now() < expiry) {
          sentSetRef.current.delete(text);
          return;
        }
        setEntries(p => [...p, { kind: "user", text }]);
        return;
      }
      case "node_started": {
        termRef.current?.write("\r\n· node started\r\n");
        setEntries(p => [...p, { kind: "sys-line", text: "node started" }]);
        return;
      }
      case "liveness_changed": {
        const next = typeof body.liveness === "string" ? body.liveness : (typeof body.next === "string" ? body.next : "?");
        termRef.current?.write(`\r\n· liveness · ${next}\r\n`);
        setEntries(p => [...p, { kind: "sys-line", text: `liveness · ${next}` }]);
        return;
      }
      case "harness_failure": {
        const msg = summarizeEventText("harness_failure", body);
        termRef.current?.write(`\r\n· ${msg}\r\n`);
        setEntries(p => [...p, { kind: "sys-line", text: msg }]);
        return;
      }
      case "substrate_failure": {
        const msg = summarizeEventText("substrate_failure", body);
        termRef.current?.write(`\r\n· ${msg}\r\n`);
        setEntries(p => [...p, { kind: "sys-line", text: msg }]);
        return;
      }
      case "human_input_requested": {
        const msg = summarizeEventText("human_input_requested", body);
        termRef.current?.write(`\r\n· ${msg}\r\n`);
        setEntries(p => [...p, { kind: "sys-line", text: msg }]);
        return;
      }
      case "notification_sent": {
        const msg = summarizeEventText("notification_sent", body);
        setEntries(p => [...p, { kind: "sys-line", text: msg }]);
        return;
      }
      case "remote_command_received": {
        const msg = summarizeEventText("remote_command_received", body);
        setEntries(p => [...p, { kind: "sys-line", text: msg }]);
        return;
      }
      case "attach_issued": {
        const url = typeof body.url === "string" ? body.url : (typeof body.attach_url === "string" ? body.attach_url : "");
        const targetNode = typeof body.node_id === "string" ? body.node_id : (evt.node_id ?? node.id);
        setEntries(p => [...p, { kind: "attach", node: targetNode, url }]);
        return;
      }
      default: {
        const msg = `event · ${evt.kind ?? "?"}`;
        setEntries(p => [...p, { kind: "sys-line", text: msg }]);
        return;
      }
    }
  }

  function handleLiveFrame(data: string): void {
    // live frames are raw PTY bytes — write directly to terminal
    termRef.current?.write(data);
    // also append to structured view transcript
    setEntries(prev => {
      const last = prev[prev.length - 1];
      if (last && last.kind === "text") {
        const merged = last.text + data;
        return [...prev.slice(0, -1), { kind: "text", text: merged }];
      }
      return [...prev, { kind: "text", text: data }];
    });
  }

  // ─── render ───────────────────────────────────────────────────────────────

  return (
    <div className={`session session-${mode} harness-${harnessId}`} data-screen-label={`session-${node.id}`}>
      <SessionHeader
        node={node}
        harnessId={harnessId}
        mode={mode}
        view={view}
        setView={setView}
        onAttach={onAttach}
        onNativeAttach={onNativeAttach}
        onInterrupt={onInterrupt}
        onExpand={onExpand}
      />
      <SessionBanner node={node} harnessId={harnessId} />

      {/* tui surface: xterm.js terminal */}
      <div
        ref={termContainerRef}
        className="session-terminal"
        style={{ display: view === "tui" ? "flex" : "none" }}
      />

      {/* structured surface: semantic transcript */}
      {view === "structured" && (
        <div className="session-body" ref={structBodyRef}>
          {entries.map((e, i) => (
            <TermEntry key={i} e={e} harness={harnessId} view={view} />
          ))}
          {entries[entries.length - 1]?.kind === "prompt" && <PromptLine harness={harnessId} />}
        </div>
      )}

      <SessionInput
        node={node}
        harnessId={harnessId}
        value={input}
        onChange={setInput}
        onSubmit={() => { void submit(); }}
      />
    </div>
  );
}

// ─── SessionHeader ────────────────────────────────────────────────────────────
function SessionHeader({
  node,
  harnessId,
  mode,
  view,
  setView,
  onAttach,
  onNativeAttach,
  onInterrupt,
  onExpand,
}: {
  node: AsylumNode;
  harnessId: string;
  mode: SessionMode;
  view: "tui" | "structured";
  setView: (v: "tui" | "structured") => void;
  onAttach?: (nodeId: string) => void;
  onNativeAttach?: (nodeId: string) => void;
  onInterrupt?: (nodeId: string) => void;
  onExpand?: () => void;
}): ReactElement {
  const isCC = isCommandCenter(node);
  const hLabel = harnessId === "claude-code" ? "claude code" : (node.harness || "codex");
  const nodeState = node.liveness ?? "running";
  return (
    <div className="session-head">
      {/* ◆ for claude-code, ›_ for codex — design glyph, not decorative */}
      <span className="hglyph">{harnessId === "claude-code" ? "◆" : "›_"}</span>
      <span className="hid">{node.id}</span>
      <span className="hsep">·</span>
      <span className="hharn">{hLabel}</span>
      <span className="hsep">·</span>
      <span className="hsubs">{node.substrate}</span>
      {node.role_hint && <span className="hsep">·</span>}
      {node.role_hint && <span className="hrole">{node.role_hint}</span>}
      {isCC && <span className="hbadge" title="this node is the command center">cc</span>}
      <span className={`pill pill-${nodeState}`}>
        <span className="dot" />
        {nodeState}
      </span>

      <span className="hright">
        {onAttach && (
          <Btn
            kind="ghost"
            size="sm"
            icon="external-link"
            iconOnly
            title={node.substrate === "loon" ? "open attach tab via loon attach" : "open attach tab"}
            onClick={() => onAttach(node.id)}
          />
        )}
        {onNativeAttach && (
          <Btn kind="ghost" size="sm" icon="terminal" iconOnly title="open in terminal" onClick={() => onNativeAttach(node.id)} />
        )}
        {onInterrupt && (
          <Btn kind="ghost" size="sm" icon="square" iconOnly title="interrupt" onClick={() => onInterrupt(node.id)} />
        )}
        <div className="view-toggle" role="tablist" title="transcript rendering">
          <button className={view === "tui" ? "on" : ""} onClick={() => setView("tui")} title="raw tui — xterm.js terminal">tui</button>
          <button className={view === "structured" ? "on" : ""} onClick={() => setView("structured")} title="structured / semantic">struct</button>
        </div>
        {mode === "cockpit" && onExpand && (
          <Btn kind="ghost" size="sm" icon="maximize-2" iconOnly title="open in chat" onClick={onExpand} />
        )}
      </span>
    </div>
  );
}

// ─── SessionBanner ────────────────────────────────────────────────────────────
// codex: minimal hr rule; claude-code: ASCII box (design typography)
function SessionBanner({ node, harnessId }: { node: AsylumNode; harnessId: string }): ReactElement {
  const ctxPct = Math.round((node.ctx_pct ?? 0) * 100);
  const uptime = uptimeLabel(node);
  const subline = `${node.workspace ?? "~/"} · ctx est. ${ctxPct}% · uptime ${uptime}`;
  const dashes = "─".repeat(46);

  if (harnessId === "claude-code") {
    return (
      <div className="banner cc-claude">
        <div className="row top">{`╭${dashes}╮`}</div>
        <div className="row mid"><span>│ </span><b>Claude Code</b><span> · session </span><span className="m">{node.id}</span></div>
        <div className="row mid"><span>│ </span><span className="m">{subline}</span></div>
        <div className="row bot">{`╰${dashes}╯`}</div>
      </div>
    );
  }
  return (
    <div className="banner cc-codex">
      <div className="hr" />
      <div className="row">
        <span className="b">codex</span>
        <span className="sep">·</span>
        <span className="m">connected to {node.id}</span>
        <span className="m" style={{ marginLeft: "auto" }}>{subline}</span>
      </div>
      <div className="hr" />
    </div>
  );
}

// ─── TermEntry ────────────────────────────────────────────────────────────────
function TermEntry({
  e,
  harness,
  view,
}: {
  e: TranscriptEntry;
  harness: string;
  view: "tui" | "structured";
}): ReactElement | null {
  if (e.kind === "user") {
    return (
      <div className="line line-user">
        <span className="g">{harness === "claude-code" ? ">" : "$"}</span>
        <span className="t">{e.text}</span>
      </div>
    );
  }
  if (e.kind === "thought") {
    if (harness === "claude-code") {
      // ✻ is the claude-code spinner glyph (design typography)
      return <div className="line line-thought claude">{"✳"} <i>{e.text}</i></div>;
    }
    return <div className="line line-thought codex">{"·"} <i>{e.text}</i></div>;
  }
  if (e.kind === "text") {
    return <div className="line line-text">{e.text}</div>;
  }
  if (e.kind === "list") {
    return (
      <ul className="line line-list">
        {e.items.map((it, i) => (
          <li key={i}>
            {/* ⏺ is the claude-code list bullet (design glyph); codex uses · */}
            <span className="bul">{harness === "claude-code" ? "⏺" : "·"}</span>
            <span>{it}</span>
          </li>
        ))}
      </ul>
    );
  }
  if (e.kind === "tool") {
    if (view === "tui") {
      return <ToolCallTUI name={e.name} args={e.args} output={e.output} state={e.state} harness={harness} />;
    }
    return <ToolCall name={e.name} args={e.args} output={e.output} state={e.state} collapsed />;
  }
  if (e.kind === "attach") {
    return (
      <div className="attach-preview">
        <div className="h">
          <Icon name="external-link" size={11} />
          <span>attach tab: <span style={{ color: "var(--fg)" }}>{e.node}</span></span>
          <span className="right" style={{ marginLeft: "auto", color: "var(--fg-subtle)" }}>time-limited url</span>
        </div>
        <div className="body">
          <span className="muted">{">"}</span> <span className="b">open a time-limited Cockpit attach view for this node</span>{"\n"}
          <span className="muted">url</span> {e.url}
        </div>
        <div className="foot">
          <Btn size="sm" kind="primary" onClick={() => window.open(e.url, "_blank", "noopener,noreferrer")}>open</Btn>
          <Btn size="sm" kind="secondary" icon="copy" onClick={() => void navigator.clipboard.writeText(e.url)}>copy url</Btn>
          <span className="muted mono" style={{ fontSize: 10, marginLeft: "auto" }}>{e.url}</span>
        </div>
      </div>
    );
  }
  if (e.kind === "sys-line") return <div className="line line-sys">{"·"} {e.text}</div>;
  if (e.kind === "prompt") return null;
  return null;
}

// ─── ToolCallTUI ──────────────────────────────────────────────────────────────
// renders a tool call in harness-authentic TUI style (not the structured ToolCall card)
function ToolCallTUI({
  name,
  args,
  output,
  state,
  harness,
}: {
  name: string;
  args?: Record<string, unknown>;
  output?: string;
  state?: "ok" | "pending" | "error";
  harness: string;
}): ReactElement {
  const argStr = args
    ? Object.entries(args).map(([k, v]) => `${k}=${typeof v === "string" ? v : JSON.stringify(v)}`).join(" ")
    : "";
  const lines = (output ?? "").split("\n").slice(0, 8);

  if (harness === "claude-code") {
    return (
      <div className="tui-tool claude">
        {/* ⏺ is the claude-code tool-call glyph (design typography) */}
        <div className="hd">{"⏺"} <b>{name}</b>{argStr ? <span className="m"> ({argStr})</span> : null}</div>
        {lines.map((l, i) => (
          <div key={i} className="ln">{"  "}{"⎿"}{"  "}<span className="m">{l}</span></div>
        ))}
      </div>
    );
  }
  return (
    <div className="tui-tool codex">
      <div className="hd"><span className="g">tool</span> {name} <span className="m">{argStr}</span> <span className={`st ${state ?? "ok"}`}>{"·"} {state ?? "ok"}</span></div>
      {lines.map((l, i) => <div key={i} className="ln">{"  "}{l}</div>)}
    </div>
  );
}

// ─── PromptLine ───────────────────────────────────────────────────────────────
function PromptLine({ harness }: { harness: string }): ReactElement {
  if (harness === "claude-code") {
    return <div className="prompt-line claude"><span className="g">{">"}</span> <span className="caret" /></div>;
  }
  // › is the codex prompt glyph (design typography)
  return <div className="prompt-line codex"><span className="g">{"›"}</span><span className="caret" /></div>;
}

// ─── SessionInput ─────────────────────────────────────────────────────────────
function SessionInput({
  node,
  harnessId,
  value,
  onChange,
  onSubmit,
}: {
  node: AsylumNode;
  harnessId: string;
  value: string;
  onChange: (v: string) => void;
  onSubmit: () => void;
}): ReactElement {
  const isCC = isCommandCenter(node);
  const placeholder = isCC
      ? `send to ${node.id} · try: spawn 2 workers, status, attach to w-9a4f1`
      : `send input to ${node.id} · this writes directly to its harness stdin`;

  return (
    <div className={`session-input harness-${harnessId}`}>
      <span className="g">{harnessId === "claude-code" ? ">" : "›"}</span>
      <textarea
        placeholder={placeholder}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={e => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            onSubmit();
          }
        }}
        rows={3}
      />
      <span className="r">
        <span className="kbd">{"⏎"} send</span>
        <span className="kbd">shift+{"⏎"} newline</span>
      </span>
    </div>
  );
}

function summarizeEventText(kind: string, body: Record<string, unknown>): string {
  const normalizedReason = pickTextBody(body, [
    "reason",
    "message",
    "text",
    "error",
    "payload",
    "title",
  ]);
  if (kind === "notification_sent") {
    const title = typeof body.title === "string" ? body.title : "notification";
    const message = typeof body.body === "string" ? body.body : normalizedReason;
    return `${title}: ${message}`.trim();
  }
  if (kind === "human_input_requested") {
    return normalizedReason
      ? `human input requested · ${normalizedReason}`
      : "human input requested";
  }
  if (kind === "remote_command_received") {
    const command = typeof body.command === "string" ? body.command : "command";
    const error = typeof body.error === "string" ? ` · ${body.error}` : "";
    return `remote command ${command}${error}`;
  }
  const pretty = kind.replace(/_/g, " ");
  return `${pretty} · ${normalizedReason || "details pending"}`;
}

function pickTextBody(body: Record<string, unknown>, keys: string[]): string {
  for (const key of keys) {
    const value = body[key];
    if (typeof value === "string") return value;
  }
  const fallback = body["arguments"];
  if (typeof fallback === "string") return fallback;
  return "";
}
