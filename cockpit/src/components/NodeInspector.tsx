import { type FC, useMemo, useState } from "react";
import {
  AlertTriangle,
  Archive,
  ArrowRightLeft,
  Plug2,
  Power,
  Send,
  TerminalSquare,
} from "lucide-react";
import {
  type AsylumNode,
  type GraphRelationship,
  createRelationship,
  deleteRelationship,
  interruptNode,
  postNodeInput,
  requestBrowserAttach,
  requestNativeTarget,
  stopNode,
  archiveNode,
  resumeNode,
} from "../api";
import { isOperational } from "../state";
import { AttachTerminal } from "./AttachTerminal";

export interface NodeInspectorProps {
  node?: AsylumNode;
  nodes: AsylumNode[];
  relationships: GraphRelationship[];
  onActionComplete: () => void;
}

interface ActionLog {
  key: string;
  message: string;
}

export const NodeInspector: FC<NodeInspectorProps> = ({ node, nodes, relationships, onActionComplete }) => {
  const [customInput, setCustomInput] = useState("");
  const [relationshipTarget, setRelationshipTarget] = useState("");
  const [relationshipKind, setRelationshipKind] = useState("supervises");
  const [statusMessages, setStatusMessages] = useState<ActionLog[]>([]);
  const [attachToken, setAttachToken] = useState<string | undefined>(undefined);
  const [nativeTarget, setNativeTarget] = useState<string | undefined>(undefined);

  const related = useMemo(
    () => (node ? nodes.filter((candidate) => candidate.id !== node.id).slice(0, 30) : []),
    [node, nodes],
  );
  const outgoing = useMemo(
    () => (node ? relationships.filter((relationship) => relationship.source_node_id === node.id) : []),
    [node, relationships],
  );

  if (!node) {
    return (
      <section className="panel node-inspector">
        <h3>Node Inspector</h3>
        <p className="empty-cell">Select a node in the graph or table to inspect it.</p>
      </section>
    );
  }

  const addStatus = (message: string) =>
    setStatusMessages((prev) => [...prev, { key: `${Date.now()}-${Math.random()}`, message }].slice(-8));

  const handle = async (callback: () => Promise<void>) => {
    try {
      await callback();
      addStatus("Action executed.");
      onActionComplete();
    } catch (err) {
      addStatus(`Action failed: ${String(err instanceof Error ? err.message : err)}`);
    }
  };

  const sendDirectInput = async () => {
    const trimmed = customInput.trim();
    if (!trimmed || !node) {
      return;
    }
    await handle(async () => {
      await postNodeInput(node.id, trimmed);
      setCustomInput("");
    });
  };

  const openBrowserAttach = async () => {
    await handle(async () => {
      const result = await requestBrowserAttach(node.id);
      if (result.token) {
        setAttachToken(result.token);
      }
    });
  };

  const openNativeTarget = async () => {
    await handle(async () => {
      const target = await requestNativeTarget(node.id);
      setNativeTarget(`${target.command} ${target.args.join(" ")}`);
    });
  };

  const createRel = async () => {
    if (!relationshipTarget) return;
    await handle(async () => {
      await createRelationship({
        source_node_id: node.id,
        target_node_id: relationshipTarget,
        kind: relationshipKind || "supervises",
      });
      setRelationshipTarget("");
      onActionComplete();
    });
  };

  const removeRel = async (relationshipId: string) =>
    handle(async () => {
      await deleteRelationship(relationshipId);
      onActionComplete();
    });

  return (
    <section className="panel node-inspector">
      <h3>Node Inspector</h3>
      <div className="inspector-main">
        <div>
          <h4>{node.role_hint}</h4>
          <p className="muted">
            {node.id}
          </p>
          <p>
            <strong>{node.harness}</strong> / {node.substrate}
          </p>
          <p>Status: <strong>{node.liveness}</strong></p>
          <p>Updated: {new Date(node.updated_at).toLocaleString()}</p>
        </div>
        <div className="inspector-controls">
          <button
            className="action-btn"
            onClick={() => handle(async () => interruptNode(node.id))}
            disabled={!node.capabilities.interrupt}
          >
            <AlertTriangle size={15} /> Interrupt
          </button>
          <button
            className="action-btn"
            onClick={() => handle(async () => stopNode(node.id))}
            disabled={!node.capabilities.stop}
          >
            <Power size={15} /> Stop
          </button>
          <button className="action-btn" onClick={() => handle(async () => archiveNode(node.id))}>
            <Archive size={15} /> Archive
          </button>
        </div>
        <div className="inline-action-row">
          <input
            value={customInput}
            onChange={(e) => setCustomInput(e.target.value)}
            placeholder="Send input to selected node"
            aria-label="Node input"
          />
          <button onClick={sendDirectInput} type="button">
            <Send size={14} /> Send
          </button>
        </div>
        <div className="inline-action-row">
          <button onClick={openBrowserAttach} type="button" className="ghost-btn">
            <TerminalSquare size={14} /> Browser attach
          </button>
          <button onClick={openNativeTarget} type="button" className="ghost-btn">
            <Plug2 size={14} /> Native target
          </button>
        </div>
        {nativeTarget && (
          <p className="mono small">
            {nativeTarget}
          </p>
        )}
        <div className="inline-action-row relationship-row">
          <label>Create relationship</label>
          <select value={relationshipTarget} onChange={(e) => setRelationshipTarget(e.target.value)}>
            <option value="">Select target</option>
            {related.map((item) => (
              <option key={item.id} value={item.id}>
                {item.role_hint} ({item.id.slice(0, 8)})
              </option>
            ))}
          </select>
          <select value={relationshipKind} onChange={(e) => setRelationshipKind(e.target.value)}>
            <option value="supervises">supervises</option>
            <option value="spawned_for">spawned_for</option>
            <option value="user_created">user_created</option>
            <option value="platform_responsibility">platform_responsibility</option>
          </select>
          <button onClick={createRel} type="button" className="ghost-btn">
            <ArrowRightLeft size={14} /> Add
          </button>
        </div>
        {outgoing.length > 0 ? (
          <div className="relationship-list">
            <p>Outgoing relationships</p>
            <ul>
              {outgoing.map((relationship) => (
                <li key={relationship.id}>
                  {relationship.kind}: {relationship.target_node_id}
                  <button
                    type="button"
                    onClick={() => {
                      void removeRel(relationship.id);
                    }}
                  >
                    Remove
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
      {attachToken && <AttachTerminal token={attachToken} onClose={() => setAttachToken(undefined)} />}
      <div className="status-log">
        {statusMessages.map((message) => (
          <p key={message.key}>{message.message}</p>
        ))}
      </div>
      <div className={isOperational(node) ? "warning hidden" : "warning"}>Node is not currently accepting input.</div>
      {!node.capabilities.resume ? null : (
        <button className="action-btn" onClick={() => handle(async () => resumeNode(node.id))}>
          Resume
        </button>
      )}
      <div className="muted small">
        Capabilities: {Object.entries(node.capabilities).filter(([, value]) => Boolean(value)).length} active
      </div>
    </section>
  );
};
