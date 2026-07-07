// Decisions screen: bounded list/create/resolve flow for operator decisions.

import { useCallback, useEffect, useMemo, useState, type JSX } from "react";
import { Btn, Empty, Field, Panel } from "../lib/ui";
import type { DecisionCreateRequest, DecisionRecord, DecisionResolveRequest } from "../types";
import { createDecision, fetchDecisions, resolveDecision } from "../api";

type DecisionAction = DecisionResolveRequest["status"];
type FetchState = "idle" | "loading" | "ready" | "error";

function fmtEpoch(sec: number | null): string {
  if (!sec) return "-";
  const d = new Date(sec * 1000);
  if (Number.isNaN(d.getTime())) return "-";
  return d.toLocaleString();
}

function statusPillClass(status: string): string {
  const s = status.toLowerCase();
  if (s === "pending") return "pill pill-waiting";
  if (s === "approved") return "pill pill-running";
  return "pill pill-errored";
}

function statusLabel(status: string): string {
  const s = status.toLowerCase();
  if (s === "pending") return "pending";
  if (s === "approved") return "approved";
  return "denied";
}

function DecisionRow({
  decision,
  showActions,
  onResolve,
  resolving,
  answer,
  onAnswerChange,
}: {
  decision: DecisionRecord;
  showActions: boolean;
  onResolve: (id: string, status: DecisionAction) => void;
  resolving: string | null;
  answer: string;
  onAnswerChange: (id: string, value: string) => void;
}) {
  const normalized = decision.status.toLowerCase();

  return (
    <tr>
      <td>
        <span className={statusPillClass(normalized)}>{statusLabel(normalized)}</span>
      </td>
      <td className="mono">{decision.id}</td>
      <td className="mono muted">{decision.node_id ?? "-"}</td>
      <td className="mono muted">{fmtEpoch(decision.created_at_epoch_secs)}</td>
      <td style={{ maxWidth: 600, whiteSpace: "normal", wordBreak: "break-word" }}>{decision.text}</td>
      <td className="mono muted">{fmtEpoch(decision.decided_at_epoch_secs)}</td>
      <td style={{ width: 220 }}>
        {showActions ? (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <input
              className="input mono"
              aria-label={`answer for decision ${decision.id}`}
              placeholder="free-text answer (optional)"
              value={answer}
              onChange={(e) => onAnswerChange(decision.id, e.target.value)}
              style={{ fontSize: 11 }}
            />
            <div style={{ display: "flex", gap: 6 }}>
              <Btn
                kind="secondary"
                size="sm"
                icon="thumbs-up"
                disabled={Boolean(resolving)}
                onClick={() => onResolve(decision.id, "approved")}
              >
                approve
              </Btn>
              <Btn
                kind="danger"
                size="sm"
                disabled={Boolean(resolving)}
                onClick={() => onResolve(decision.id, "denied")}
              >
                deny
              </Btn>
            </div>
          </div>
        ) : (
          <span className="muted">resolved</span>
        )}
      </td>
    </tr>
  );
}

function EmptyState({ text, sub }: { text: string; sub: string }) {
  return (
    <div style={{ margin: "16px 0" }}>
      <Empty lead={text} sub={sub} />
    </div>
  );
}

export function DecisionsScreen(): JSX.Element {
  const [decisions, setDecisions] = useState<DecisionRecord[]>([]);
  const [state, setState] = useState<FetchState>("loading");
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [resolving, setResolving] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [nodeId, setNodeId] = useState("");
  const [answers, setAnswers] = useState<Record<string, string>>({});

  const loadDecisions = useCallback(async () => {
    setState("loading");
    try {
      const all = await fetchDecisions();
      setDecisions(all);
      setError(null);
      setState("ready");
    } catch (err) {
      setError(String(err instanceof Error ? err.message : err));
      setState("error");
    }
  }, []);

  useEffect(() => {
    void loadDecisions();
    const timer = setInterval(() => {
      void loadDecisions();
    }, 6000);
    return () => clearInterval(timer);
  }, [loadDecisions]);

  const pending = useMemo(
    () => decisions.filter((d) => d.status.toLowerCase() === "pending"),
    [decisions],
  );
  const resolved = useMemo(
    () => decisions.filter((d) => d.status.toLowerCase() !== "pending"),
    [decisions],
  );

  const isBusy = creating || resolving !== null;

  async function submitCreate() {
    const payload: DecisionCreateRequest = {
      text: text.trim(),
      ...(nodeId.trim() ? { node_id: nodeId.trim() } : {}),
    };
    if (!payload.text) return;

    setCreating(true);
    setError(null);
    try {
      await createDecision(payload);
      setText("");
      setNodeId("");
      await loadDecisions();
    } catch (err) {
      setError(String(err instanceof Error ? err.message : err));
    } finally {
      setCreating(false);
    }
  }

  function setAnswer(id: string, value: string) {
    setAnswers((cur) => ({ ...cur, [id]: value }));
  }

  async function handleResolve(id: string, status: DecisionAction) {
    const answer = answers[id]?.trim();
    setResolving(id);
    setError(null);
    try {
      await resolveDecision(id, { status, ...(answer ? { answer } : {}) });
      setAnswers((cur) => {
        const next = { ...cur };
        delete next[id];
        return next;
      });
      await loadDecisions();
    } catch (err) {
      setError(String(err instanceof Error ? err.message : err));
    } finally {
      setResolving(null);
    }
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1 className="page-title">decisions</h1>
          <div className="page-sub">
            pending {pending.length} / resolved {resolved.length}
          </div>
        </div>
        <div className="page-actions">
          <Btn kind="primary" icon="plus" onClick={() => void loadDecisions()} disabled={isBusy}>
            refresh
          </Btn>
        </div>
      </div>

      <Panel eyebrow="new decision">
        <div style={{ display: "grid", gap: 10 }}>
          <Field label="text" hint="operator-visible question or instruction for a node">
            <input
              className="input"
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="decision text"
            />
          </Field>
          <Field label="node id" hint="optional; tie decision to a node">
            <input
              className="input mono"
              value={nodeId}
              onChange={(e) => setNodeId(e.target.value)}
              placeholder="00000000-0000-0000-0000-000000000000"
            />
          </Field>
          <div>
            <Btn kind="primary" icon="git-pull-request" onClick={() => void submitCreate()} disabled={!text.trim() || isBusy}>
              {creating ? "creating..." : "create"}
            </Btn>
          </div>
        </div>
      </Panel>

      {error && (
        <div style={{ margin: "14px 0", color: "var(--status-errored)", fontSize: 12 }}>{error}</div>
      )}

      <div style={{ marginTop: 18 }}>
        {state === "loading" && decisions.length === 0 ? (
          <EmptyState text="loading decisions..." sub="waiting for daemon response" />
        ) : state === "error" && decisions.length === 0 ? (
          <EmptyState text="failed to load decisions" sub="check daemon connectivity and owner token" />
        ) : decisions.length === 0 ? (
          <EmptyState text="no decisions yet" sub="decisions will appear here after creation or harness events" />
        ) : (
          <Panel title="pending decisions" eyebrow="pending">
            {pending.length === 0 ? (
              <div className="muted" style={{ padding: "10px 0", fontFamily: "var(--font-mono)" }}>
                no pending decisions
              </div>
            ) : (
              <table className="table" style={{ borderTop: "none", marginBottom: 18 }}>
                <thead>
                  <tr>
                    <th style={{ width: 90 }}>status</th>
                    <th style={{ width: 140 }}>id</th>
                    <th style={{ width: 190 }}>node</th>
                    <th style={{ width: 190 }}>created</th>
                    <th>text</th>
                    <th style={{ width: 170 }}>decided</th>
                    <th style={{ width: 170 }}></th>
                  </tr>
                </thead>
                <tbody>
                  {pending.map((d) => (
                    <DecisionRow
                      key={d.id}
                      decision={d}
                      showActions
                      onResolve={handleResolve}
                      resolving={resolving === d.id ? d.id : null}
                      answer={answers[d.id] ?? ""}
                      onAnswerChange={setAnswer}
                    />
                  ))}
                </tbody>
              </table>
            )}
          </Panel>
        )}

        {decisions.length > 0 && (
          <div style={{ marginTop: 18 }}>
            <Panel title="resolved decisions" eyebrow="resolved">
              <table className="table" style={{ borderTop: "none" }}>
                <thead>
                  <tr>
                    <th style={{ width: 90 }}>status</th>
                    <th style={{ width: 140 }}>id</th>
                    <th style={{ width: 190 }}>node</th>
                    <th style={{ width: 190 }}>created</th>
                    <th>text</th>
                    <th style={{ width: 170 }}>decided</th>
                    <th style={{ width: 120 }}></th>
                  </tr>
                </thead>
                <tbody>
                  {resolved.length === 0 ? (
                    <tr>
                      <td colSpan={7} style={{ color: "var(--fg-muted)", padding: 22 }}>
                        no resolved decisions
                      </td>
                    </tr>
                  ) : (
                    resolved.map((d) => (
                      <DecisionRow
                        key={d.id}
                        decision={d}
                        showActions={false}
                        onResolve={handleResolve}
                        resolving={resolving}
                        answer=""
                        onAnswerChange={setAnswer}
                      />
                    ))
                  )}
                </tbody>
              </table>
            </Panel>
          </div>
        )}
      </div>
    </div>
  );
}
