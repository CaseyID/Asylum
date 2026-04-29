// asylum cockpit — NodeSession: the one terminal/chat primitive.
// every "chat" surface in the app is a viewport onto this. modes only change
// the chrome (compact vs full); the harness-authentic TUI inside is the same.
//
// all input goes to the daemon via postNodeInput; all output arrives over the
// observe websocket and streams into entries. no canned simulation.

import { useEffect, useRef, useState, type ReactElement } from "react";
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
  onExpand?: () => void;
}

// ─── internal transcript entry types ────────────────────────────────────────

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
// matches NodeEvent on the daemon side (asylum-core::node).

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

// ─── initial transcript ───────────────────────────────────────────────────────
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
  onExpand,
}: NodeSessionProps): ReactElement {
  const [entries, setEntries] = useState<TranscriptEntry[]>(() => initialTranscript(node));
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [view, setView] = useState<"tui" | "structured">("tui");
  const termRef = useRef<HTMLDivElement | null>(null);
  const phaseRef = useRef<"history" | "live">("history");
  const liveDisabledRef = useRef<boolean>(false);
  const liveEntryIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (termRef.current) termRef.current.scrollTop = termRef.current.scrollHeight;
  }, [entries]);

  const harnessId = node.harness === "claude_code" ? "claude-code" : "codex";

  async function submit(): Promise<void> {
    const v = input.trim();
    if (!v || streaming) return;
    setInput("");
    setEntries(p => [...p, { kind: "user", text: v }]);
    try {
      await postNodeInput(node.id, v);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setEntries(p => [...p, { kind: "sys-line", text: `send-input failed: ${msg}` }]);
    }
  }

  // observe ws — opens on mount, closes on unmount. history frames arrive as
  // NodeEvent JSON; then a literal init frame; then live raw output chunks.
  useEffect(() => {
    phaseRef.current = "history";
    liveDisabledRef.current = false;
    liveEntryIdRef.current = null;

    const ws = openNodeObserveSocket(node.id, {
      onMessage: (data) => {
        if (typeof data !== "string") return;
        if (data === WS_INIT_FRAME) {
          phaseRef.current = "live";
          return;
        }
        if (data === WS_LIVE_UNAVAILABLE) {
          liveDisabledRef.current = true;
          setEntries(p => [...p, { kind: "sys-line", text: "live streaming not supported by this substrate" }]);
          return;
        }
        if (phaseRef.current === "history") {
          handleHistoryFrame(data);
        } else {
          if (liveDisabledRef.current) return;
          handleLiveFrame(data);
        }
      },
    });

    return () => {
      try {
        ws.close();
      } catch {
        // ignore — already closed
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [node.id]);

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
      // unknown frame in history phase — render as raw text so it isn't lost
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
        setEntries(p => [...p, { kind: "text", text }]);
        return;
      }
      case "input_sent": {
        const text = typeof body.text === "string" ? body.text : "";
        setEntries(p => [...p, { kind: "user", text }]);
        return;
      }
      case "node_started": {
        setEntries(p => [...p, { kind: "sys-line", text: "node started" }]);
        return;
      }
      case "liveness_changed": {
        const next = typeof body.liveness === "string" ? body.liveness : (typeof body.next === "string" ? body.next : "?");
        setEntries(p => [...p, { kind: "sys-line", text: `liveness · ${next}` }]);
        return;
      }
      case "attach_issued": {
        const url = typeof body.url === "string" ? body.url : (typeof body.attach_url === "string" ? body.attach_url : "");
        const targetNode = typeof body.node_id === "string" ? body.node_id : (evt.node_id ?? node.id);
        setEntries(p => [...p, { kind: "attach", node: targetNode, url }]);
        if (onAttach) onAttach(targetNode);
        return;
      }
      case "tool_call": {
        const name = typeof body.name === "string" ? body.name : (typeof body.tool === "string" ? body.tool : "tool");
        const args = (body.args && typeof body.args === "object") ? body.args as Record<string, unknown> : undefined;
        const output = typeof body.output === "string" ? body.output : undefined;
        const stateRaw = typeof body.state === "string" ? body.state : "ok";
        const state: "ok" | "pending" | "error" = stateRaw === "pending" || stateRaw === "error" ? stateRaw : "ok";
        setEntries(p => [...p, { kind: "tool", name, args, output, state }]);
        return;
      }
      default: {
        setEntries(p => [...p, { kind: "sys-line", text: `event · ${evt.kind ?? "?"}` }]);
        return;
      }
    }
  }

  function handleLiveFrame(data: string): void {
    // append live raw output to a single rolling text entry; start a new entry
    // each time the previous chunk ended on a newline, so completed lines are
    // committed and styled by the caret-aware renderer.
    setEntries(prev => {
      const last = prev[prev.length - 1];
      const liveId = liveEntryIdRef.current;
      const lastIsLive = last && last.kind === "text" && "id" in last && last.id === liveId;
      if (lastIsLive && last.kind === "text") {
        const merged = (last.text ?? "") + data;
        const next = prev.slice(0, -1);
        next.push({ kind: "text", id: liveId ?? undefined, text: merged });
        if (data.includes("\n")) liveEntryIdRef.current = null;
        return next;
      }
      const newId = Math.random().toString(36).slice(2, 8);
      liveEntryIdRef.current = data.endsWith("\n") ? null : newId;
      return [...prev, { kind: "text", id: newId, text: data }];
    });
  }

  const last = entries[entries.length - 1];

  return (
    <div className={`session session-${mode} harness-${harnessId}`} data-screen-label={`session-${node.id}`}>
      <SessionHeader node={node} harnessId={harnessId} mode={mode} view={view} setView={setView} onExpand={onExpand} />
      <SessionBanner node={node} harnessId={harnessId} />
      <div className="session-body" ref={termRef}>
        {entries.map((e, i) => (
          <TermEntry
            key={i}
            e={e}
            harness={harnessId}
            view={view}
            streaming={streaming && i === entries.length - 1}
          />
        ))}
        {!streaming && last?.kind === "prompt" && <PromptLine harness={harnessId} />}
      </div>
      <SessionInput
        node={node}
        harnessId={harnessId}
        value={input}
        streaming={streaming}
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
  onExpand,
}: {
  node: AsylumNode;
  harnessId: string;
  mode: SessionMode;
  view: "tui" | "structured";
  setView: (v: "tui" | "structured") => void;
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
        <div className="view-toggle" role="tablist" title="transcript rendering">
          <button className={view === "tui" ? "on" : ""} onClick={() => setView("tui")} title="raw tui replay">tui</button>
          <button className={view === "structured" ? "on" : ""} onClick={() => setView("structured")} title="structured / semantic">struct</button>
        </div>
        <Btn kind="ghost" size="sm" icon="external-link" iconOnly title="attach in browser" />
        <Btn kind="ghost" size="sm" icon="terminal" iconOnly title="native attach" />
        <Btn kind="ghost" size="sm" icon="square" iconOnly title="interrupt (ctrl-c)" />
        {mode === "cockpit" && onExpand && (
          <Btn kind="ghost" size="sm" icon="maximize-2" iconOnly title="open in chat" onClick={onExpand} />
        )}
        <Btn kind="ghost" size="sm" icon="more-horizontal" iconOnly />
      </span>
    </div>
  );
}

// ─── SessionBanner ────────────────────────────────────────────────────────────
// codex: minimal hr rule; claude-code: ASCII box (design typography)
function SessionBanner({ node, harnessId }: { node: AsylumNode; harnessId: string }): ReactElement {
  const ctxPct = Math.round((node.ctx_pct ?? 0) * 100);
  const uptime = uptimeLabel(node);
  const subline = `${node.workspace ?? "~/"} · ctx ${ctxPct}% · uptime ${uptime}`;
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
  streaming,
}: {
  e: TranscriptEntry;
  harness: string;
  view: "tui" | "structured";
  streaming: boolean;
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
    return (
      <div className="line line-text">
        {e.text}
        {streaming && <span className="caret" />}
      </div>
    );
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
          <span>browser attach: <span style={{ color: "var(--fg)" }}>{e.node}</span></span>
          <span className="right" style={{ marginLeft: "auto", color: "var(--fg-subtle)" }}>token ttl 3600s</span>
        </div>
        <div className="body">
          <span className="muted">{">"}</span> <span className="b">refactor:router</span> $ <span className="x">npm test</span>{"\n"}
          <span className="muted">PASS</span> src/router/match.test.ts (4 tests, 12ms){"\n"}
          <span className="muted">PASS</span> src/router/parse.test.ts (8 tests, 22ms){"\n"}
          <span className="muted">$</span> <span className="b">applying patch</span>: src/router/match.ts +114 -0{"\n"}
          <span className="muted">$</span> <span className="b">streaming output</span>… 412 tokens at ctx 41%{"\n"}
        </div>
        <div className="foot">
          <Btn size="sm" kind="primary">open ↗</Btn>
          <Btn size="sm" kind="secondary" icon="copy">copy url</Btn>
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
  streaming,
  onChange,
  onSubmit,
}: {
  node: AsylumNode;
  harnessId: string;
  value: string;
  streaming: boolean;
  onChange: (v: string) => void;
  onSubmit: () => void;
}): ReactElement {
  const isCC = isCommandCenter(node);
  const placeholder = streaming
    ? "streaming… (esc to interrupt)"
    : isCC
      ? `send to ${node.id} · try: spawn 2 workers, status, attach to w-9a4f1`
      : `send input to ${node.id} · this writes directly to its harness stdin`;

  return (
    <div className={`session-input harness-${harnessId}`}>
      <span className="g">{harnessId === "claude-code" ? ">" : "›"}</span>
      <input
        placeholder={placeholder}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={e => { if (e.key === "Enter") onSubmit(); }}
        disabled={streaming}
      />
      <span className="r">
        {/* ⏎ send label is design typography */}
        <span className="kbd">{"⏎"} send</span>
        <Btn kind="ghost" size="sm" icon="paperclip" iconOnly title="attach context" />
      </span>
    </div>
  );
}
