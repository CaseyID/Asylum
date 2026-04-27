import { type FC, useEffect, useState } from "react";
import { CirclePlus, RefreshCw } from "lucide-react";
import {
  type AsylumNode,
  type CreateNodeRequest,
  type HarnessKind,
  type SubstrateKind,
  createNode,
  fetchHarnesses,
  fetchSubstrates,
} from "../api";

export interface CreateNodePanelProps {
  onCreated: (node: AsylumNode) => void;
}

export const CreateNodePanel: FC<CreateNodePanelProps> = ({ onCreated }) => {
  const [harness, setHarness] = useState<HarnessKind>("codex");
  const [substrate, setSubstrate] = useState<SubstrateKind>("local");
  const [harnessOptions, setHarnessOptions] = useState<HarnessKind[]>(["codex", "claude_code"]);
  const [substrateOptions, setSubstrateOptions] = useState<SubstrateKind[]>(["local"]);
  const [roleHint, setRoleHint] = useState("worker");
  const [workspace, setWorkspace] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | undefined>();

  useEffect(() => {
    let alive = true;
    void Promise.all([fetchHarnesses(), fetchSubstrates()])
      .then(([nextHarnesses, nextSubstrates]) => {
        if (!alive) return;
        const usableHarnesses: HarnessKind[] = nextHarnesses.length > 0 ? nextHarnesses : ["codex", "claude_code"];
        const usableSubstrates: SubstrateKind[] = nextSubstrates.length > 0 ? nextSubstrates : ["local"];
        setHarnessOptions(usableHarnesses);
        setSubstrateOptions(usableSubstrates);
        if (!usableHarnesses.includes(harness)) {
          setHarness(usableHarnesses[0]);
        }
        if (!usableSubstrates.includes(substrate)) {
          setSubstrate(usableSubstrates[0]);
        }
      })
      .catch(() => {
        if (alive) {
          setSubstrateOptions(["local"]);
        }
      });
    return () => {
      alive = false;
    };
  }, [harness, substrate]);

  const submit = async () => {
    const request: CreateNodeRequest = {
      harness,
      substrate,
      role_hint: roleHint || "worker",
      workspace: workspace || undefined,
      description: description || undefined,
    };
    setBusy(true);
    setMessage(undefined);
    try {
      const created = await createNode(request);
      setMessage(`Created ${created.role_hint} (${created.id.slice(0, 8)})`);
      onCreated(created);
      setRoleHint("worker");
      setWorkspace("");
      setDescription("");
    } catch (err) {
      setMessage(`Failed: ${String(err instanceof Error ? err.message : err)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel create-panel">
      <h3>Create Node</h3>
      <label>
        Harness
        <select
          value={harness}
          onChange={(event) => setHarness(event.target.value as HarnessKind)}
        >
          {harnessOptions.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      </label>
      <label>
        Substrate
        <select value={substrate} onChange={(event) => setSubstrate(event.target.value as SubstrateKind)}>
          {substrateOptions.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      </label>
      <label>
        Role hint
        <input value={roleHint} onChange={(e) => setRoleHint(e.target.value)} placeholder="worker" />
      </label>
      <label>
        Workspace (optional)
        <input value={workspace} onChange={(e) => setWorkspace(e.target.value)} placeholder="." />
      </label>
      <label>
        Description (optional)
        <textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={2} />
      </label>
      <div className="inline-action-row">
        <button type="button" onClick={submit} disabled={busy}>
          <CirclePlus size={14} /> {busy ? "Creating" : "Create Node"}
        </button>
        <button
          type="button"
          className="ghost-btn"
          onClick={() => {
            setRoleHint("worker");
            setWorkspace("");
            setDescription("");
            setMessage(undefined);
          }}
        >
          <RefreshCw size={14} /> Reset
        </button>
      </div>
      {message && <p className="status-note">{message}</p>}
    </section>
  );
};
