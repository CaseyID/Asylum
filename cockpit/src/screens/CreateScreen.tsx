// ports prototype CreateScreen
import { Fragment, useEffect, useState, type JSX } from "react";
import { Btn, Field, Panel, Pill } from "../lib/ui";
import {
  createNode,
  fetchHarnessDescriptors,
  fetchSubstrateDescriptors,
} from "../api";
import type {
  HarnessDescriptor,
  HarnessKind,
  SubstrateDescriptor,
  SubstrateKind,
} from "../types";

function formatSubstrateStatus(status: string | undefined, healthy: boolean): string {
  const normalized = (status ?? "").trim().toLowerCase();
  if (healthy) {
    return "healthy";
  }
  return normalized === "ok" || normalized === "" ? "unavailable" : normalized;
}

function substrateMetricsHint(status: string, host: string, capacity: number): string {
  if (status === "healthy") {
    return `${host} · cap ${Math.round(capacity * 100)}%`;
  }
  return `${host} · metrics unavailable`;
}

export interface CreateScreenProps {
  onCreated: (nodeId: string) => void;
  onCancel: () => void;
}

export function CreateScreen({ onCreated, onCancel }: CreateScreenProps): JSX.Element {
  const [harness, setHarness] = useState<string>("codex");
  const [substrate, setSubstrate] = useState<string>("local");
  const [role, setRole] = useState<string>("command-center");
  const [workspace, setWorkspace] = useState<string>("");
  const [harnesses, setHarnesses] = useState<HarnessDescriptor[]>([]);
  const [substrates, setSubstrates] = useState<SubstrateDescriptor[]>([]);
  const [prompt, setPrompt] = useState<string>(
    "inspect the asylum context, summarize active nodes, and ask me what to spawn next.",
  );
  // Advanced launch-profile overrides. Free text, no Asylum-owned catalog —
  // rendered only when the selected harness descriptor advertises support.
  const [model, setModel] = useState<string>("");
  const [effort, setEffort] = useState<string>("");
  const [launching, setLaunching] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchHarnessDescriptors()
      .then((items) => {
        if (!cancelled) setHarnesses(items);
      })
      .catch((err) => {
        if (!cancelled) {
          setError((prev) => prev ?? (err instanceof Error ? err.message : String(err)));
        }
      });
    fetchSubstrateDescriptors()
      .then((items) => {
        if (!cancelled) setSubstrates(items);
      })
      .catch((err) => {
        if (!cancelled) {
          setError((prev) => prev ?? (err instanceof Error ? err.message : String(err)));
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const selectedHarness = harnesses.find((h) => h.id === harness);

  async function handleLaunch() {
    setLaunching(true);
    setError(null);
    try {
      const trimmedModel = model.trim();
      const trimmedEffort = effort.trim();
      const node = await createNode({
        harness: harness as HarnessKind,
        substrate: substrate as SubstrateKind,
        role_hint: role,
        workspace,
        description: prompt,
        ...(selectedHarness?.supports_model && trimmedModel ? { model: trimmedModel } : {}),
        ...(selectedHarness?.supports_effort && trimmedEffort ? { effort: trimmedEffort } : {}),
      });
      onCreated(node.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLaunching(false);
    }
  }

  return (
    <div className="page" style={{ maxWidth: 880 }}>
          <div className="page-head">
        <div>
          <h1 className="page-title">launch node</h1>
          <div className="page-sub">creates a real harness session. capabilities advertised at launch.</div>
        </div>
        <div className="page-actions">
          <Btn onClick={onCancel}>cancel</Btn>
          <Btn kind="primary" icon="play" onClick={handleLaunch} disabled={launching}>
            {launching ? "launching…" : "launch"}
          </Btn>
        </div>
      </div>

      {error && (
        <div
          style={{
            margin: "0 0 16px",
            padding: "10px 14px",
            border: "1px solid var(--status-errored)",
            background: "var(--status-errored-bg)",
            fontFamily: "var(--font-mono)",
            fontSize: 12,
            color: "var(--status-errored)",
          }}
        >
          {error}
        </div>
      )}

      <div style={{ display: "grid", gridTemplateColumns: "1fr 320px", gap: 32 }}>
        <div className="col" style={{ gap: 18 }}>
          <Field
            label="harness"
            hint="claude code advertises subagents and native resume; codex advertises tool-call telemetry"
          >
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
              {harnesses.map((h) => (
                <button
                  key={h.id}
                  disabled={!h.available}
                  className={`btn ${harness === h.id ? "btn-primary" : "btn-secondary"}`}
                  style={{
                    justifyContent: "flex-start",
                    padding: "10px 12px",
                    flexDirection: "column",
                    alignItems: "flex-start",
                    gap: 4,
                    opacity: h.available ? 1 : 0.45,
                    cursor: h.available ? "pointer" : "not-allowed",
                  }}
                  onClick={() => h.available && setHarness(h.id)}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: 8, width: "100%" }}>
                    <span>{h.name}</span>
                    <span style={{ marginLeft: "auto", fontFamily: "var(--font-mono)", fontSize: 10, opacity: 0.6 }}>
                      {h.kind}
                    </span>
                  </div>
                  <span style={{ fontFamily: "var(--font-mono)", fontSize: 10, opacity: 0.7 }}>
                    {h.available
                      ? `${h.caps.length} caps advertised`
                      : `binary \`${h.command}\` not found on daemon PATH — run \`asylum doctor\``}
                  </span>
                </button>
              ))}
            </div>
          </Field>

          <Field label="substrate" hint="loon vms boot in <2s. local nodes share your machine's resources.">
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
              {substrates.map((s) => (
                <button
                  key={s.id}
                  disabled={!s.healthy}
                  className={`btn ${substrate === s.id ? "btn-primary" : "btn-secondary"}`}
                  style={{
                    justifyContent: "flex-start",
                    padding: "10px 12px",
                    flexDirection: "column",
                    alignItems: "flex-start",
                    gap: 4,
                    opacity: s.healthy ? 1 : 0.5,
                  }}
                  onClick={() => s.healthy && setSubstrate(s.id)}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: 8, width: "100%" }}>
                    <span>{s.name}</span>
                  <Pill status={s.healthy ? "running" : "errored"}>
                    {formatSubstrateStatus(s.status, s.healthy)}
                  </Pill>
                  </div>
                  <span style={{ fontFamily: "var(--font-mono)", fontSize: 10, opacity: 0.7 }}>
                    {substrateMetricsHint(
                      formatSubstrateStatus(s.status, s.healthy),
                      s.host,
                      s.capacity,
                    )}
                  </span>
                </button>
              ))}
            </div>
          </Field>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
            <Field label="role hint">
              <select className="input mono" value={role} onChange={(e) => setRole(e.target.value)}>
                <option value="command-center">command-center</option>
                <option value="supervisor">supervisor</option>
                <option value="worker">worker</option>
                <option value="evaluator">evaluator</option>
                <option value="assistant">assistant</option>
                <option value="custom">custom…</option>
              </select>
            </Field>
            <Field label="workspace" hint="absolute path to a local workspace directory">
              <input
                aria-label="workspace"
                className="input mono"
                placeholder="/abs/path/to/workspace"
                value={workspace}
                onChange={(e) => setWorkspace(e.target.value)}
              />
            </Field>
          </div>

          {(selectedHarness?.supports_model || selectedHarness?.supports_effort) && (
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
              {selectedHarness?.supports_model && (
                <Field label="model" hint="passed verbatim to the harness. blank uses the harness default.">
                  <input
                    aria-label="model"
                    className="input mono"
                    placeholder="harness default"
                    value={model}
                    onChange={(e) => setModel(e.target.value)}
                  />
                </Field>
              )}
              {selectedHarness?.supports_effort && (
                <Field label="effort" hint="passed verbatim to the harness. blank uses the harness default.">
                  <input
                    aria-label="effort"
                    className="input mono"
                    placeholder="harness default"
                    value={effort}
                    onChange={(e) => setEffort(e.target.value)}
                  />
                </Field>
              )}
            </div>
          )}

          <Field label="launch packet (initial prompt)" hint="injected as the first user turn, after asylum context.">
            <textarea
              className="input mono"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={4}
              style={{ fontSize: 12, lineHeight: 1.5, resize: "vertical" }}
            />
          </Field>
        </div>

        <div className="col" style={{ gap: 18 }}>
          <Panel eyebrow="capabilities at launch">
            <div className="capgrid">
              {selectedHarness?.caps.slice(0, 8).map((c) => (
                <Fragment key={c}>
                  <span className="cap">{c}</span>
                  <span className="ok">✓</span>
                </Fragment>
              ))}
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}
