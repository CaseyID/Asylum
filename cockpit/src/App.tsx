import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ApiError,
  fetchGraph,
  fetchNotifications,
  graphToFlow,
  hydrateOwnerTokenFromLocation,
  setStoredOwnerToken,
  type NotificationRecord,
} from "./api";
import { isOperational, selectCommandCenter, useCockpitStore } from "./state";
import { CommandCenter } from "./components/CommandCenter";
import { CreateNodePanel } from "./components/CreateNodePanel";
import { GraphView } from "./components/GraphView";
import { NodeInspector } from "./components/NodeInspector";
import { NodeTable } from "./components/NodeTable";
import { NotificationCenter } from "./components/NotificationCenter";

type BottomPanel = "command-center" | "table";

export const App = () => {
  const {
    graph,
    selectedNodeId,
    commandCenterNodeId,
    loading,
    initializeGraph,
    setSelectedNode,
    setCommandCenterSelection,
  } = useCockpitStore();

  const [bottomPanel, setBottomPanel] = useState<BottomPanel>("command-center");
  const [notifications, setNotifications] = useState<NotificationRecord[]>([]);
  const [localError, setLocalError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | undefined>();
  const [ownerToken, setOwnerToken] = useState("");
  const [tokenDraft, setTokenDraft] = useState("");
  const [authRequired, setAuthRequired] = useState(false);
  const refreshInFlight = useRef(false);

  const graphData = useMemo(() => graphToFlow(graph), [graph]);
  const [refreshing, setRefreshing] = useState(false);

  const selectedNode = useMemo(
    () => graph.nodes.find((node) => node.id === selectedNodeId),
    [graph.nodes, selectedNodeId],
  );
  const liveCount = useMemo(
    () => graph.nodes.filter((node) => node.liveness === "running" || node.liveness === "waiting_for_input").length,
    [graph.nodes],
  );
  const substrateCount = useMemo(
    () => new Set(graph.nodes.map((node) => node.substrate)).size,
    [graph.nodes],
  );

  const refreshAll = useCallback(async () => {
    if (refreshInFlight.current) return;
    refreshInFlight.current = true;
    setRefreshing(true);
    try {
      const [fetchedGraph, fetchedNotifications] = await Promise.all([fetchGraph(), fetchNotifications()]);
      const commandCenter = selectCommandCenter(fetchedGraph.nodes);
      initializeGraph({
        ...fetchedGraph,
        nodes: [...fetchedGraph.nodes].sort((a, b) => a.created_at.localeCompare(b.created_at)),
        relationships: [...fetchedGraph.relationships],
      });
      setNotifications(fetchedNotifications);
      setCommandCenterSelection(commandCenter?.id);
      setLocalError(null);
      setAuthRequired(false);
      setWarning(
        fetchedGraph.nodes.some((node) => !isOperational(node))
          ? "Some nodes are not operational; check liveness."
          : undefined,
      );
    } catch (err) {
      initializeGraph({ nodes: [], relationships: [] });
      const needsAuth = err instanceof ApiError && err.status === 401;
      setAuthRequired(needsAuth);
      setLocalError(needsAuth ? null : `Backend unavailable: ${String(err instanceof Error ? err.message : err)}`);
    } finally {
      refreshInFlight.current = false;
      setRefreshing(false);
    }
  }, [initializeGraph, setCommandCenterSelection]);

  useEffect(() => {
    const token = hydrateOwnerTokenFromLocation();
    setOwnerToken(token);
    setTokenDraft(token);
    void refreshAll();

    const timer = setInterval(() => {
      void refreshAll();
    }, 6000);
    return () => clearInterval(timer);
  }, [refreshAll]);

  const saveOwnerToken = () => {
    setStoredOwnerToken(tokenDraft);
    setOwnerToken(tokenDraft.trim());
    void refreshAll();
  };

  const clearOwnerToken = () => {
    setStoredOwnerToken("");
    setOwnerToken("");
    setTokenDraft("");
    setAuthRequired(true);
  };

  useEffect(() => {
    const commandCenter = selectCommandCenter(graph.nodes);
    if (!selectedNodeId && commandCenter) {
      setSelectedNode(commandCenter.id);
      setCommandCenterSelection(commandCenter.id);
    } else if (graph.nodes.length > 0 && !selectedNodeId) {
      setSelectedNode(graph.nodes[0]?.id);
    }
  }, [graph.nodes, selectedNodeId, setSelectedNode, setCommandCenterSelection]);

  return (
    <main className="cockpit-root">
      <header className="top-strip">
        <div>
          <span className="eyebrow">ASYLUM://CONTROL-PLANE</span>
          <h1>Asylum Cockpit</h1>
        </div>
        <div className="top-metrics" aria-label="System summary">
          <span><strong>{graph.nodes.length}</strong> nodes</span>
          <span><strong>{liveCount}</strong> live</span>
          <span><strong>{graph.relationships.length}</strong> edges</span>
          <span><strong>{substrateCount}</strong> substrates</span>
        </div>
        <div className="auth-control" data-state={ownerToken ? "stored" : "empty"}>
          <input
            type="password"
            value={tokenDraft}
            onChange={(event) => setTokenDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                saveOwnerToken();
              }
            }}
            placeholder={authRequired ? "owner token required" : "owner token"}
            aria-label="Owner token"
          />
          <button type="button" onClick={saveOwnerToken}>
            Unlock
          </button>
          {ownerToken ? (
            <button type="button" className="ghost-btn" onClick={clearOwnerToken}>
              Clear
            </button>
          ) : null}
        </div>
      </header>
      <section className="operational-grid">
        <aside className="left-toolbar">
          <CreateNodePanel key={ownerToken ? "authed" : "open"} onCreated={() => void refreshAll()} />
          <NotificationCenter notifications={notifications} onRefresh={refreshAll} />
          <section className="panel">
            <h3>Exposure</h3>
            <p>{warning ?? "No known exposure warning."}</p>
          </section>
        </aside>
        <div className="graph-stage">
          {loading ? <p className="empty-cell">Loading cockpit…</p> : <GraphView flow={graphData} selectedNodeId={selectedNode?.id} onSelectNode={setSelectedNode} />}
        </div>
        <aside className="node-inspector-col">
          <NodeInspector
            node={selectedNode}
            nodes={graph.nodes}
            relationships={graph.relationships}
            onActionComplete={refreshAll}
          />
        </aside>
        <footer className="bottom-row">
          <div className="panel tab-strip">
            <button
              type="button"
              className={bottomPanel === "command-center" ? "selected" : ""}
              onClick={() => setBottomPanel("command-center")}
            >
              Command Center
            </button>
            <button
              type="button"
              className={bottomPanel === "table" ? "selected" : ""}
              onClick={() => setBottomPanel("table")}
            >
              Node Table
            </button>
          </div>
          {bottomPanel === "command-center" ? (
            <CommandCenter nodes={graph.nodes} selectedNodeId={commandCenterNodeId} onSelectNode={setSelectedNode} />
          ) : (
            <NodeTable nodes={graph.nodes} selectedNodeId={selectedNode?.id} onSelectNode={setSelectedNode} />
          )}
        </footer>
      </section>
      {localError ? <div className="error-banner">{localError}</div> : null}
    </main>
  );
};
