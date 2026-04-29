// asylum cockpit — node detail screen.
// ports NodeScreen / EventsView / ToolsView / CapsView / RelView from the prototype.

import { Fragment, useEffect, useState, type JSX } from "react";
import { Btn, Empty, KV, Pill, Tag } from "../lib/ui";
import { Icon } from "../lib/icons";
import { NodeSession } from "../components/NodeSession";
import {
  ROLE_GLYPH,
  harnessLabel,
  isCommandCenter,
  shortNodeId,
  telemetryFor,
  uiStateLabel,
  uiStateOf,
  uptimeLabel,
} from "../lib/glyphs";
import { fetchHarnessDescriptors, fetchNodeEvents } from "../api";
import type { AsylumNode, GraphRelationship, HarnessDescriptor } from "../types";

interface NodeEventRecord {
  id: number;
  sequence: number;
  kind: string;
  body: unknown;
  created_at: string;
}

export type NodeScreenAction =
  | "attach"
  | "send"
  | "interrupt"
  | "fork"
  | "restart"
  | "terminate"
  | "archive";

export interface NodeScreenProps {
  node?: AsylumNode;
  nodes: AsylumNode[];
  relationships: GraphRelationship[];
  onBack: () => void;
  onOpen: (node: AsylumNode) => void;
  onAction: (action: NodeScreenAction, payload?: string) => void;
}

type Tab = "session" | "events" | "tools" | "capabilities" | "relationships";

export function NodeScreen({ node, nodes, relationships, onBack, onOpen, onAction }: NodeScreenProps): JSX.Element {
  const [tab, setTab] = useState<Tab>("session");
  const [flash, setFlash] = useState<{ action: string; label: string; t: number } | null>(null);
  const [harnesses, setHarnesses] = useState<HarnessDescriptor[]>([]);

  useEffect(() => {
    let cancelled = false;
    fetchHarnessDescriptors()
      .then((items) => {
        if (!cancelled) setHarnesses(items);
      })
      .catch(() => {
        /* leave empty; downstream renders handle missing harness */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!node) {
    return (
      <Empty
        lead="no node selected"
        sub="go back to nodes and pick one"
        action={
          <Btn icon="arrow-left" onClick={onBack}>
            back to fleet
          </Btn>
        }
      />
    );
  }

  const harness = harnesses.find((h) => h.id === node.harness);
  const tel = telemetryFor(node);
  const state = uiStateOf(node);
  const cc = isCommandCenter(node);

  const childRel = relationships.filter((r) => r.source_node_id === node.id);
  const parentRel = relationships.find((r) => r.target_node_id === node.id);
  const children = childRel
    .map((r) => nodes.find((n) => n.id === r.target_node_id))
    .filter((n): n is AsylumNode => Boolean(n));
  const parent = parentRel ? nodes.find((n) => n.id === parentRel.source_node_id) : undefined;

  function fire(action: NodeScreenAction, label: string) {
    onAction(action);
    const t = Date.now();
    setFlash({ action, label, t });
    setTimeout(() => {
      setFlash((f) => (f && f.action === action ? null : f));
    }, 2200);
  }

  return (
    <div className="node-page">
      <div className="node-main">
        <div className="node-header">
          <div className="top">
            <Btn kind="ghost" size="sm" icon="arrow-left" iconOnly onClick={onBack} />
            <span style={{ fontSize: 18, opacity: 0.5 }}>{ROLE_GLYPH[node.role_hint] ?? "·"}</span>
            <span className="id">{shortNodeId(node.id)}</span>
            <Pill status={state}>{uiStateLabel(state)}</Pill>
            {cc && <Tag kind="role">command-center</Tag>}
            <span className="right">
              <Btn size="sm" icon="external-link" onClick={() => fire("attach", "attach url issued")}>
                attach in browser
              </Btn>
              <Btn size="sm" icon="terminal" onClick={() => fire("attach", "native attach prepared")}>
                native attach
              </Btn>
              <Btn size="sm" kind="ghost" icon="more-horizontal" iconOnly />
            </span>
          </div>
          <div className="meta">
            <span>
              <b>{harness?.name ?? harnessLabel(node.harness)}</b> · {node.role_hint}
            </span>
            <span>
              substrate: <b>{node.substrate}</b>
            </span>
            <span>
              workspace: <b>{node.workspace ?? "—"}</b>
            </span>
            <span>
              uptime: <b>{uptimeLabel(node)}</b>
            </span>
            <span>
              ctx: <b>{Math.round(tel.ctx * 100)}%</b>
            </span>
          </div>
          <div className="node-tabs">
            {(["session", "events", "tools", "capabilities", "relationships"] as Tab[]).map((t) => (
              <div key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
                {t}
              </div>
            ))}
          </div>
        </div>

        {tab === "session" && (
          <NodeSession key={node.id} node={node} mode="fullscreen" />
        )}
        {tab === "events" && <EventsView node={node} />}
        {tab === "tools" && <ToolsView />}
        {tab === "capabilities" && <CapsView node={node} harnesses={harnesses} />}
        {tab === "relationships" && <RelView node={node} parent={parent} children={children} />}
      </div>

      <div className="node-side">
        <div className="sect">
          <div className="h">telemetry</div>
          <KV
            items={[
              ["tokens in", tel.tokensIn.toLocaleString()],
              ["tokens out", tel.tokensOut.toLocaleString()],
              ["ctx", `${Math.round(tel.ctx * 100)}%`],
              ["tool calls", tel.tools],
              ["uptime", uptimeLabel(node)],
            ]}
          />
        </div>

        <div className="sect">
          <div className="h">relationships</div>
          {parent ? (
            <div style={{ fontFamily: "var(--font-mono)", fontSize: 12, marginBottom: 8 }}>
              <span className="muted">parent: </span>
              <a style={{ color: "var(--fg)", cursor: "pointer" }} onClick={() => onOpen(parent)}>
                {shortNodeId(parent.id)}
              </a>
              <span className="muted" style={{ marginLeft: 8, fontSize: 10 }}>
                ({parentRel?.kind ?? "spawned_for"})
              </span>
            </div>
          ) : (
            <div className="mono muted">no parent</div>
          )}
          {children.length > 0 && (
            <div style={{ fontFamily: "var(--font-mono)", fontSize: 12 }}>
              <span className="muted">children:</span>
              {children.map((c) => (
                <div key={c.id} style={{ paddingLeft: 14, marginTop: 4 }}>
                  <span className="muted">└ </span>
                  <a style={{ color: "var(--fg)", cursor: "pointer" }} onClick={() => onOpen(c)}>
                    {shortNodeId(c.id)}
                  </a>
                  <span className="muted" style={{ marginLeft: 6 }}>
                    · {c.role_hint}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="sect">
          <div className="h">controls</div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6 }}>
            <Btn size="sm" icon="message-square" onClick={() => fire("send", "opened input prompt")}>
              send input
            </Btn>
            <Btn size="sm" icon="square" onClick={() => fire("interrupt", "sigint sent · paused")}>
              interrupt
            </Btn>
            <Btn size="sm" icon="rotate-ccw" onClick={() => fire("restart", "restart issued · ctx reset")}>
              restart
            </Btn>
            <Btn size="sm" icon="git-branch" onClick={() => fire("fork", "forked → see graph")}>
              fork
            </Btn>
            <Btn size="sm" icon="archive" onClick={() => fire("archive", "archived · transcript exported")}>
              archive
            </Btn>
            <Btn size="sm" kind="danger" icon="x" onClick={() => fire("terminate", "terminated · resources released")}>
              terminate
            </Btn>
          </div>
          {flash && (
            <div className="action-flash" key={flash.t}>
              <span className="ok-tick">✓</span> {flash.label}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function EventsView({ node }: { node: AsylumNode }): JSX.Element {
  const [events, setEvents] = useState<NodeEventRecord[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const tick = () => {
      fetchNodeEvents(node.id)
        .then((items) => {
          if (cancelled) return;
          setEvents(items as NodeEventRecord[]);
          setError(null);
          timer = setTimeout(tick, 2000);
        })
        .catch((err: unknown) => {
          if (cancelled) return;
          setError(err instanceof Error ? err.message : String(err));
          timer = setTimeout(tick, 5000);
        });
    };
    tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [node.id]);

  if (error && events.length === 0) {
    return (
      <div className="log">
        <Empty glyph="[!]" lead="failed to load events" sub={error} />
      </div>
    );
  }
  if (events.length === 0) {
    return (
      <div className="log">
        <Empty glyph="[ ]" lead="no events yet" sub="harness output and lifecycle events appear here as they happen" />
      </div>
    );
  }
  const ordered = [...events].sort((a, b) => b.sequence - a.sequence);
  return (
    <div className="log" style={{ overflow: "auto", padding: "8px 12px", fontFamily: "var(--font-mono, monospace)" }}>
      {ordered.map((ev) => (
        <div key={ev.id} style={{ padding: "4px 0", borderBottom: "1px solid var(--border)" }}>
          <span style={{ color: "var(--text-muted)" }}>#{ev.sequence}</span>
          {" "}
          <span style={{ color: "var(--text-muted)" }}>{ev.created_at}</span>
          {" "}
          <Tag>{ev.kind}</Tag>
          {" "}
          <span>{summarizeEventBody(ev.body)}</span>
        </div>
      ))}
    </div>
  );
}

function summarizeEventBody(body: unknown): string {
  if (body == null) return "";
  if (typeof body === "string") return body;
  if (typeof body === "object") {
    const obj = body as Record<string, unknown>;
    if (typeof obj.text === "string") return obj.text.slice(0, 240);
    if (typeof obj.message === "string") return obj.message.slice(0, 240);
    try {
      return JSON.stringify(body).slice(0, 240);
    } catch {
      return String(body);
    }
  }
  return String(body);
}

function ToolsView(): JSX.Element {
  return (
    <div style={{ padding: 24, overflow: "auto", borderTop: "1px solid var(--border)" }}>
      <Empty glyph="[ ]" lead="no recent tool calls" sub="tool-call telemetry surfaces here as the harness streams output" />
    </div>
  );
}

function CapsView({ node, harnesses }: { node: AsylumNode; harnesses: HarnessDescriptor[] }): JSX.Element {
  const harness = harnesses.find((h) => h.id === node.harness);
  const caps = node.capabilities;
  const rows: [string, boolean][] = [
    ["browser_attach", caps.browser_attach],
    ["native_attach", caps.native_attach],
    ["send_input", caps.send_input],
    ["interrupt", caps.interrupt],
    ["stop", caps.stop],
    ["resume", caps.resume],
    ["structured_events", caps.structured_events],
    ["transcript_export", caps.transcript_export],
  ];
  return (
    <div style={{ padding: 24, overflow: "auto", borderTop: "1px solid var(--border)" }}>
      <div className="muted mono" style={{ fontSize: 11, marginBottom: 16, letterSpacing: 0.06 }}>
        capability matrix · {harness?.name ?? harnessLabel(node.harness)}
      </div>
      <div className="capgrid" style={{ maxWidth: 480 }}>
        {rows.map(([cap, has]) => (
          <Fragment key={cap}>
            <span className="cap">{cap}</span>
            <span className={has ? "ok" : "no"}>{has ? "✓ supported" : "— not advertised"}</span>
          </Fragment>
        ))}
      </div>
    </div>
  );
}

function RelView({
  node,
  parent,
  children,
}: {
  node: AsylumNode;
  parent: AsylumNode | undefined;
  children: AsylumNode[];
}): JSX.Element {
  return (
    <div
      style={{
        padding: 24,
        overflow: "auto",
        borderTop: "1px solid var(--border)",
        fontFamily: "var(--font-mono)",
        fontSize: 12,
      }}
    >
      <div className="muted" style={{ fontSize: 11, marginBottom: 16, letterSpacing: 0.06 }}>
        explicit graph relationships
      </div>
      {parent && (
        <div style={{ marginBottom: 14 }}>
          <span className="muted">parent · </span>
          <span style={{ color: "var(--fg)" }}>{shortNodeId(parent.id)}</span>
        </div>
      )}
      {children.length > 0 && (
        <>
          <div className="muted" style={{ marginBottom: 8 }}>
            children:
          </div>
          {children.map((c) => (
            <div key={c.id} style={{ paddingLeft: 14 }}>
              └ <span style={{ color: "var(--fg)" }}>{shortNodeId(c.id)}</span>{" "}
              <span className="muted">· {c.role_hint}</span>
            </div>
          ))}
        </>
      )}
      {!parent && children.length === 0 && <div className="muted">no explicit relationships</div>}
      <div className="hr" />
      <div className="muted" style={{ fontSize: 11, marginBottom: 8 }}>
        correlations (not edges)
      </div>
      <div style={{ color: "var(--fg)" }}>workspace {node.workspace ?? "—"}</div>
      <div style={{ color: "var(--fg)" }}>substrate {node.substrate}</div>
    </div>
  );
}
