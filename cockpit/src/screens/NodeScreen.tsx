// asylum cockpit — node detail screen.
// ports NodeSession / EventsView / ActivityView / CapsView / RelView from the prototype.

import { Fragment, useEffect, useRef, useState, type FormEvent, type JSX } from "react";
import { Btn, Empty, KV, Pill, Tag } from "../lib/ui";
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
import { createRelationship, fetchHarnessDescriptors, fetchNodeEvents, removeRelationship } from "../api";
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
  | "native-attach"
  | "send"
  | "interrupt"
  | "fork"
  | "stop"
  | "terminate"
  | "archive";

export interface NodeScreenProps {
  node?: AsylumNode;
  nodes: AsylumNode[];
  relationships: GraphRelationship[];
  onBack: () => void;
  onOpen: (node: AsylumNode) => void;
  onAction: (action: NodeScreenAction, payload?: string) => Promise<void>;
  onGraphRefresh: () => void;
}

type Tab = "session" | "events" | "activity" | "capabilities" | "relationships";

type ActionFlashStatus = "ok" | "error";

type ActionFlash = {
  id: number;
  label: string;
  status: ActionFlashStatus;
};

export function NodeScreen({
  node,
  nodes,
  relationships,
  onBack,
  onOpen,
  onAction,
  onGraphRefresh,
}: NodeScreenProps): JSX.Element {
  const [tab, setTab] = useState<Tab>("session");
  const [flash, setFlash] = useState<ActionFlash | null>(null);
  const [harnesses, setHarnesses] = useState<HarnessDescriptor[]>([]);
  const flashTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  // Clear flash timer on unmount (L13).
  useEffect(() => () => { if (flashTimerRef.current !== null) clearTimeout(flashTimerRef.current); }, []);

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

  async function fire(action: NodeScreenAction, label: string) {
    if (action === "send") setTab("session");
    const id = Date.now();
    try {
      await onAction(action);
      reportFlash("ok", label, id);
    } catch (err) {
      reportFlash("error", `${action} failed: ${String(err instanceof Error ? err.message : err)}`, id);
    }
  }

  function reportFlash(status: ActionFlashStatus, message: string, id: number) {
    // Clear any previous timer before starting a new one to avoid leaking on unmount (L13).
    if (flashTimerRef.current !== null) clearTimeout(flashTimerRef.current);
    setFlash({ id, status, label: message });
    flashTimerRef.current = setTimeout(() => {
      flashTimerRef.current = null;
      setFlash((f) => (f && f.id === id ? null : f));
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
              <Btn size="sm" icon="external-link" onClick={() => fire("attach", "attach link issued")}>
                open attach tab
              </Btn>
              <Btn size="sm" icon="terminal" onClick={() => fire("native-attach", "terminal attach prepared")}>
                open in terminal
              </Btn>
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
              ctx est.: <b>{Math.round(tel.ctx * 100)}%</b>
            </span>
          </div>
          <div className="node-tabs">
            {(["session", "events", "activity", "capabilities", "relationships"] as Tab[]).map((t) => (
              <div key={t} className={`tab ${tab === t ? "active" : ""}`} onClick={() => setTab(t)}>
                {t}
              </div>
            ))}
          </div>
        </div>

        {tab === "session" && (
          <NodeSession
            key={node.id}
            node={node}
            mode="fullscreen"
            onAttach={() => fire("attach", "attach link issued")}
            onNativeAttach={() => fire("native-attach", "terminal attach prepared")}
            onInterrupt={() => fire("interrupt", "sigint sent · paused")}
          />
        )}
        {tab === "events" && <EventsView node={node} />}
        {tab === "activity" && <ActivityView node={node} />}
        {tab === "capabilities" && <CapsView node={node} harnesses={harnesses} />}
        {tab === "relationships" && (
          <RelView
            node={node}
            nodes={nodes}
            relationships={relationships}
            parent={parent}
            children={children}
            onOpen={onOpen}
            onGraphRefresh={onGraphRefresh}
          />
        )}
      </div>

      <div className="node-side">
        <div className="sect">
          <div className="h">telemetry estimates</div>
          <KV
            items={[
              ["tokens in est.", tel.tokensIn.toLocaleString()],
              ["tokens out est.", tel.tokensOut.toLocaleString()],
              ["ctx est.", `${Math.round(tel.ctx * 100)}%`],
              ["tool calls est.", tel.tools],
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
            <Btn size="sm" icon="stop-circle" onClick={() => fire("stop", "stop issued")}>
              stop
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
            <div className={`action-flash ${flash.status}`} key={flash.id}>
              <span className={flash.status === "error" ? "fail-cross" : "ok-tick"}>
                {flash.status === "error" ? "×" : "✓"}
              </span>{" "}
              {flash.label}
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
    if (typeof obj.reason === "string") return `reason: ${obj.reason}`.slice(0, 240);
    if (typeof obj.command === "string") return `command: ${obj.command}`.slice(0, 240);
    if (typeof obj.title === "string" && typeof obj.body === "string") {
      return `${obj.title}: ${obj.body}`.slice(0, 240);
    }
    if (typeof obj.error === "string") return `error: ${obj.error}`.slice(0, 240);
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

function ActivityView({ node }: { node: AsylumNode }): JSX.Element {
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
          timer = setTimeout(tick, 3000);
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
        <Empty glyph="[!]" lead="failed to load activity" sub={error} />
      </div>
    );
  }

  const relevantEvents = events.filter((ev) => String(ev.kind).includes("tool")).sort((a, b) => b.sequence - a.sequence);
  if (relevantEvents.length === 0) {
    return (
      <div className="log">
        <Empty
          glyph="[ ]"
          lead="no dedicated tool-events yet"
          sub="activity panel is event-backed and currently only shows explicit tool-like events."
        />
      </div>
    );
  }
  return (
    <div className="log" style={{ overflow: "auto", padding: "8px 12px", fontFamily: "var(--font-mono, monospace)" }}>
      {relevantEvents.map((ev) => (
        <div key={ev.id} style={{ padding: "4px 0", borderBottom: "1px solid var(--border)" }}>
          <span style={{ color: "var(--text-muted)" }}>#{ev.sequence}</span>{" "}
          <span style={{ color: "var(--text-muted)" }}>{ev.created_at}</span>{" "}
          <Tag>{ev.kind}</Tag>{" "}
          <span>{summarizeEventBody(ev.body)}</span>
        </div>
      ))}
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
  nodes,
  relationships,
  parent,
  children,
  onOpen,
  onGraphRefresh,
}: {
  node: AsylumNode;
  nodes: AsylumNode[];
  relationships: GraphRelationship[];
  parent: AsylumNode | undefined;
  children: AsylumNode[];
  onOpen: (node: AsylumNode) => void;
  onGraphRefresh: () => void;
}): JSX.Element {
  const [direction, setDirection] = useState<"out" | "in">("out");
  const [targetId, setTargetId] = useState("");
  const [kind, setKind] = useState("user_created");
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const related = relationships.filter((r) => r.source_node_id === node.id || r.target_node_id === node.id);
  const candidates = nodes.filter((n) => n.id !== node.id);
  const chosenTargetId = targetId || candidates[0]?.id || "";

  async function createEdge(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!chosenTargetId) return;
    setBusy(true);
    setError(null);
    try {
      await createRelationship({
        source_node_id: direction === "out" ? node.id : chosenTargetId,
        target_node_id: direction === "out" ? chosenTargetId : node.id,
        kind,
        label: label.trim() || null,
      });
      setLabel("");
      setTargetId("");
      onGraphRefresh();
    } catch (err) {
      setError(`create failed: ${String(err instanceof Error ? err.message : err)}`);
    } finally {
      setBusy(false);
    }
  }

  async function deleteEdge(id: string) {
    setBusy(true);
    setError(null);
    try {
      await removeRelationship(id);
      onGraphRefresh();
    } catch (err) {
      setError(`remove failed: ${String(err instanceof Error ? err.message : err)}`);
    } finally {
      setBusy(false);
    }
  }

  function nodeLink(id: string): JSX.Element {
    const relatedNode = nodes.find((n) => n.id === id);
    if (!relatedNode) return <span className="muted">{shortNodeId(id)}</span>;
    return (
      <a style={{ color: "var(--fg)", cursor: "pointer" }} onClick={() => onOpen(relatedNode)}>
        {shortNodeId(id)}
      </a>
    );
  }

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
          <a style={{ color: "var(--fg)", cursor: "pointer" }} onClick={() => onOpen(parent)}>
            {shortNodeId(parent.id)}
          </a>
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
      {related.length > 0 && (
        <>
          <div className="hr" />
          <div className="muted" style={{ marginBottom: 8 }}>
            edge records:
          </div>
          <div className="rel-table-wrap">
            <table className="table" style={{ fontFamily: "var(--font-mono)", fontSize: 12 }}>
              <thead>
                <tr>
                  <th>source</th>
                  <th>kind</th>
                  <th>target</th>
                  <th>label</th>
                  <th className="right">action</th>
                </tr>
              </thead>
              <tbody>
                {related.map((rel) => (
                  <tr key={rel.id}>
                    <td className="mono">{nodeLink(rel.source_node_id)}</td>
                    <td className="mono">{rel.kind}</td>
                    <td className="mono">{nodeLink(rel.target_node_id)}</td>
                    <td className="mono muted">{rel.label || "—"}</td>
                    <td className="right">
                      <Btn
                        size="sm"
                        kind="danger"
                        icon="trash"
                        disabled={busy}
                        onClick={() => void deleteEdge(rel.id)}
                      >
                        remove
                      </Btn>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      {related.length === 0 && <div className="muted">no explicit relationships</div>}
      <div className="hr" />
      <form onSubmit={(e) => void createEdge(e)} style={{ display: "grid", gap: 10, maxWidth: 720 }}>
        <div className="muted" style={{ fontSize: 11, marginBottom: 2 }}>
          create explicit edge
        </div>
        <div className="rel-create-grid">
          <select className="input mono" value={direction} onChange={(e) => setDirection(e.target.value as "out" | "in")}>
            <option value="out">from node</option>
            <option value="in">to node</option>
          </select>
          <select
            className="input mono"
            value={chosenTargetId}
            onChange={(e) => setTargetId(e.target.value)}
            disabled={candidates.length === 0}
          >
            {candidates.map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                {shortNodeId(candidate.id)} · {candidate.role_hint}
              </option>
            ))}
          </select>
          <select className="input mono" value={kind} onChange={(e) => setKind(e.target.value)}>
            <option value="user_created">user_created</option>
            <option value="supervises">supervises</option>
            <option value="spawned_for">spawned_for</option>
            <option value="platform_responsibility">platform_responsibility</option>
          </select>
          <input
            className="input mono"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="label"
          />
          <Btn kind="primary" icon="plus" disabled={busy || !chosenTargetId} type="submit">
            create
          </Btn>
        </div>
        {error && <div style={{ color: "var(--status-errored)", fontFamily: "var(--font-mono)", fontSize: 11 }}>{error}</div>}
      </form>
      <div className="hr" />
      <div className="muted" style={{ fontSize: 11, marginBottom: 8 }}>
        correlations (not edges)
      </div>
      <div style={{ color: "var(--fg)" }}>workspace {node.workspace ?? "—"}</div>
      <div style={{ color: "var(--fg)" }}>substrate {node.substrate}</div>
    </div>
  );
}
