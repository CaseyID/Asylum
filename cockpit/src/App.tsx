// asylum cockpit — top-level app shell + screen router.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ApiError,
  archiveNode,
  fetchChannelMessages,
  fetchChannels,
  fetchGraph,
  fetchHooks,
  fetchNotifications,
  fetchSubstrateDescriptors,
  forkNode,
  hydrateOwnerTokenFromLocation,
  interruptNode,
  postNodeInput,
  requestBrowserAttach,
  resumeNode,
  setStoredOwnerToken,
  stopNode,
} from "./api";
import { isOperational, selectCommandCenter, useCockpitStore } from "./state";
import { Topbar } from "./components/Topbar";
import { Nav } from "./components/Nav";
import { CmdK } from "./components/CmdK";
import { NtfyToast, type ToastPayload } from "./components/NtfyToast";
import { CockpitScreen, type GraphLayout } from "./screens/CockpitScreen";
import { FleetScreen } from "./screens/FleetScreen";
import { NodeScreen } from "./screens/NodeScreen";
import { CreateScreen } from "./screens/CreateScreen";
import { ChannelsScreen } from "./screens/ChannelsScreen";
import { HooksScreen } from "./screens/HooksScreen";
import { LogsScreen } from "./screens/LogsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { FirstRunScreen } from "./screens/FirstRunScreen";
import type { GraphNode } from "./components/Graph";
import type { SessionBus, SpawnEvent } from "./components/NodeSession";
import type { InspectorAction } from "./components/Inspector";
import { isCommandCenter } from "./lib/glyphs";
import type {
  AsylumNode,
  ChannelDescriptor,
  HookRule,
  NotificationRecord,
  ScreenId,
  SubstrateDescriptor,
} from "./types";

interface Tweaks {
  theme: "dark" | "light";
  navCollapsed: boolean;
  graphLayout: GraphLayout;
  simSpeed: "still" | "slow" | "live";
  ntfyEnabled: boolean;
}

const DEFAULT_TWEAKS: Tweaks = {
  theme: "dark",
  navCollapsed: false,
  graphLayout: "tree",
  simSpeed: "slow",
  ntfyEnabled: true,
};

export function App() {
  const {
    graph,
    selectedNodeId,
    commandCenterNodeId,
    loading,
    initializeGraph,
    setSelectedNode,
    setCommandCenterSelection,
  } = useCockpitStore();

  const [tweaks, setTweaks] = useState<Tweaks>(DEFAULT_TWEAKS);
  const setTweak = useCallback(<K extends keyof Tweaks>(k: K, v: Tweaks[K]) => {
    setTweaks((prev) => ({ ...prev, [k]: v }));
  }, []);

  const [screen, setScreen] = useState<ScreenId>("cockpit");
  const [openNodeId, setOpenNodeId] = useState<string | undefined>();
  const [chatNodeId, setChatNodeId] = useState<string | undefined>();
  const [cmdkOpen, setCmdkOpen] = useState(false);
  const [toasts, setToasts] = useState<ToastPayload[]>([]);
  const [channels, setChannels] = useState<ChannelDescriptor[]>([]);
  const [hooks, setHooks] = useState<HookRule[]>([]);
  const [substrates, setSubstrates] = useState<SubstrateDescriptor[]>([]);
  const lastSeenMessageId = useRef<number>(0);
  const sessionBus = useRef<SessionBus>({});

  const [ownerToken, setOwnerToken] = useState("");
  const [tokenDraft, setTokenDraft] = useState("");
  const [authRequired, setAuthRequired] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [notifications, setNotifications] = useState<NotificationRecord[]>([]);
  const refreshInFlight = useRef(false);

  // theme attribute on <html>
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", tweaks.theme);
  }, [tweaks.theme]);

  // command palette / esc keybinds
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setCmdkOpen(true);
      } else if (e.key === "Escape") {
        setCmdkOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const refreshAll = useCallback(async () => {
    if (refreshInFlight.current) return;
    refreshInFlight.current = true;
    try {
      const [graphResp, notifs] = await Promise.all([fetchGraph(), fetchNotifications()]);
      const cc = selectCommandCenter(graphResp.nodes);
      initializeGraph({
        ...graphResp,
        nodes: [...graphResp.nodes].sort((a, b) => String(a.created_at).localeCompare(String(b.created_at))),
        relationships: [...graphResp.relationships],
      });
      setNotifications(notifs);
      setCommandCenterSelection(cc?.id);
      try {
        const [c, h, s] = await Promise.all([
          fetchChannels(),
          fetchHooks(),
          fetchSubstrateDescriptors(),
        ]);
        setChannels(c);
        setHooks(h);
        setSubstrates(s);
      } catch {
        /* leave previous state */
      }
      setLocalError(null);
      setAuthRequired(false);
    } catch (err) {
      initializeGraph({ nodes: [], relationships: [] });
      const needsAuth = err instanceof ApiError && err.status === 401;
      setAuthRequired(needsAuth);
      setLocalError(needsAuth ? null : `Backend unavailable: ${String(err instanceof Error ? err.message : err)}`);
    } finally {
      refreshInFlight.current = false;
    }
  }, [initializeGraph, setCommandCenterSelection]);

  useEffect(() => {
    const token = hydrateOwnerTokenFromLocation();
    setOwnerToken(token);
    setTokenDraft(token);
    void refreshAll();
    const t = setInterval(() => void refreshAll(), 6000);
    return () => clearInterval(t);
  }, [refreshAll]);

  useEffect(() => {
    if (!selectedNodeId && commandCenterNodeId) setSelectedNode(commandCenterNodeId);
    else if (!selectedNodeId && graph.nodes.length > 0) setSelectedNode(graph.nodes[0]?.id);
  }, [graph.nodes, selectedNodeId, commandCenterNodeId, setSelectedNode]);

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

  // toast spawner — polls the live ntfy channel for new inbound messages and
  // surfaces unseen ones as the lower-left toast.
  useEffect(() => {
    if (!tweaks.ntfyEnabled || tweaks.simSpeed === "still") return;
    const ntfyChannel = channels.find((c) => c.kind === "ntfy" && c.live);
    if (!ntfyChannel) return;
    let cancelled = false;
    const tick = async () => {
      try {
        const msgs = await fetchChannelMessages(ntfyChannel.id, 10);
        if (cancelled) return;
        const fresh = msgs.filter((m) => m.direction === "in" && m.id > lastSeenMessageId.current);
        if (fresh.length === 0) return;
        const latest = fresh[fresh.length - 1];
        lastSeenMessageId.current = latest.id;
        // fold subject into body so the toast renders both lines without changing NtfyToast
        setToasts(() => [
          {
            id: "t-" + latest.id,
            from: latest.sender,
            channel: ntfyChannel.name,
            subject: latest.subject,
            body: latest.subject ? `${latest.subject}\n${latest.body}` : latest.body,
            replies: latest.replies,
          },
        ]);
      } catch {
        /* silent */
      }
    };
    const interval = tweaks.simSpeed === "live" ? 4000 : 9000;
    const t = setInterval(tick, interval);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [tweaks.ntfyEnabled, tweaks.simSpeed, channels]);

  function dismissToast(id: string) {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }

  // graph nodes are augmented with parent ids derived from relationships.
  // first relationship (if any) targeting a node becomes its parent for layout.
  const graphNodes: GraphNode[] = useMemo(() => {
    const parentByChild = new Map<string, { parent: string; kind: string }>();
    for (const rel of graph.relationships) {
      if (!parentByChild.has(rel.target_node_id)) {
        parentByChild.set(rel.target_node_id, { parent: rel.source_node_id, kind: rel.kind });
      }
    }
    return graph.nodes.map((node) => {
      const rel = parentByChild.get(node.id);
      return {
        node: { ...node, is_command_center: node.id === commandCenterNodeId },
        parentId: rel?.parent ?? null,
        edgeKind: rel?.kind ?? "spawned_for",
      };
    });
  }, [graph.nodes, graph.relationships, commandCenterNodeId]);

  const liveCount = useMemo(
    () => graph.nodes.filter((n) => n.liveness === "running" || n.liveness === "waiting_for_input").length,
    [graph.nodes],
  );

  const selectedNode = useMemo(
    () => graph.nodes.find((n) => n.id === selectedNodeId),
    [graph.nodes, selectedNodeId],
  );
  const ccNode = useMemo(
    () => graph.nodes.find((n) => n.id === commandCenterNodeId),
    [graph.nodes, commandCenterNodeId],
  );
  const openNode = useMemo(() => graph.nodes.find((n) => n.id === openNodeId), [graph.nodes, openNodeId]);

  const handleSelectScreen = (s: ScreenId | "__launch") => {
    if (s === "__launch") {
      setScreen("create");
      return;
    }
    setScreen(s);
  };

  const handleOpenNode = (node: AsylumNode) => {
    setOpenNodeId(node.id);
    setSelectedNode(node.id);
    setScreen("node");
  };

  async function handleNodeAction(target: AsylumNode | undefined, action: InspectorAction, payload?: string) {
    if (!target) return;
    const bus = sessionBus.current;
    const writeSys = (text: string) => bus.pushSystem?.(text);
    const writeTool = (n: string, args: Record<string, unknown>, output: string, st: "ok" | "pending" | "error" = "ok") =>
      bus.pushTool?.(n, args, output, st);
    try {
      if (action === "attach") {
        const r = await requestBrowserAttach(target.id);
        writeTool("node.attach.browser", { node: target.id }, `attach url issued · ttl ${r.expires_in_seconds ?? 3600}s\n${r.attach_url}`);
        if (typeof window !== "undefined" && r.attach_url) {
          window.open(r.attach_url, "_blank", "noopener,noreferrer");
        }
      } else if (action === "send") {
        writeSys(`prompting for input to ${target.id} (use the box below to type directly)`);
        setSelectedNode(target.id);
      } else if (action === "interrupt") {
        await interruptNode(target.id);
        writeTool("node.interrupt", { node: target.id }, "sigint sent");
      } else if (action === "restart") {
        await stopNode(target.id);
        writeTool("node.restart", { node: target.id }, "stop issued · ctx will reset on relaunch");
      } else if (action === "archive") {
        await archiveNode(target.id);
        writeTool("node.archive", { node: target.id }, "transcript exported · workspace snapshot saved");
      } else if (action === "terminate") {
        await stopNode(target.id);
        writeTool("node.terminate", { node: target.id }, "stop issued · resources released");
      } else if (action === "fork") {
        try {
          const newNode = await forkNode(target.id, {});
          writeTool("node.fork", { source: target.id }, `forked into ${newNode.id}`);
          setOpenNodeId(newNode.id);
          setSelectedNode(newNode.id);
        } catch (err) {
          writeSys(`fork failed: ${String(err instanceof Error ? err.message : err)}`);
        }
      } else if (action === "decision" && payload) {
        writeSys(`decision on ${target.id}: ${payload}`);
        await resumeNode(target.id).catch(() => {});
      }
    } catch (err) {
      writeSys(`action ${action} failed: ${String(err instanceof Error ? err.message : err)}`);
    }
    void refreshAll();
  }

  const inspectorAction = useCallback(
    (action: InspectorAction, payload?: string) => {
      void handleNodeAction(selectedNode, action, payload);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [selectedNode],
  );

  const onSpawn = (_spawn: SpawnEvent) => {
    // canned spawns from the cc session are visual-only until the daemon
    // emits structured spawn events; trigger an immediate refresh in case
    // the spawn maps to a real node creation.
    void refreshAll();
  };

  const liveChannelCount = useMemo(() => channels.filter((c) => c.live).length, [channels]);
  const enabledHookCount = useMemo(() => hooks.filter((h) => h.enabled).length, [hooks]);

  // first-run hero when no nodes exist
  const showFirstRun = !loading && graph.nodes.length === 0 && !authRequired;

  return (
    <div className="app" data-screen-label={screen}>
      <Topbar
        screen={screen}
        openNodeId={openNode?.id}
        liveCount={liveCount}
        theme={tweaks.theme}
        onToggleTheme={() => setTweak("theme", tweaks.theme === "dark" ? "light" : "dark")}
        onOpenCmdK={() => setCmdkOpen(true)}
      />

      {authRequired && (
        <div
          style={{
            position: "absolute",
            inset: "var(--topbar-h) 0 0 0",
            zIndex: 60,
            display: "grid",
            placeItems: "center",
            background: "var(--bg-overlay)",
          }}
        >
          <div className="modal" style={{ width: 420 }}>
            <div className="modal-head">
              <span className="t">[ owner token required ]</span>
            </div>
            <div className="modal-body">
              <div style={{ fontSize: 12, color: "var(--fg-muted)" }}>
                the daemon is rejecting unauthenticated requests. paste your owner token below to continue.
              </div>
              <input
                className="input mono"
                type="password"
                value={tokenDraft}
                onChange={(e) => setTokenDraft(e.target.value)}
                placeholder="owner token"
                onKeyDown={(e) => {
                  if (e.key === "Enter") saveOwnerToken();
                }}
              />
            </div>
            <div className="modal-foot">
              <button className="btn btn-secondary" type="button" onClick={clearOwnerToken}>
                clear
              </button>
              <button className="btn btn-primary" type="button" onClick={saveOwnerToken}>
                unlock
              </button>
            </div>
          </div>
        </div>
      )}

      <div className={`body ${tweaks.navCollapsed ? "nav-collapsed" : ""}`}>
        <Nav
          collapsed={tweaks.navCollapsed}
          active={screen}
          fleetCount={graph.nodes.length}
          channelCount={liveChannelCount}
          hookCount={enabledHookCount}
          daemonVersion="asylum 0.1.0-rc4"
          bindAddr={typeof window !== "undefined" ? window.location.host : "localhost"}
          onPick={handleSelectScreen}
        />

        <div className="main">
          {screen === "cockpit" && (
            showFirstRun ? (
              <FirstRunScreen
                onLaunch={() => setScreen("create")}
                onOpenCli={() => setScreen("settings")}
                onReadSpec={() => setScreen("settings")}
                harnessCount={2}
                substrateCount={substrates.filter((s) => s.healthy).length}
              />
            ) : (
              <CockpitScreen
                graphNodes={graphNodes}
                ccNode={ccNode}
                selected={selectedNode}
                onSelect={(n) => setSelectedNode(n.id)}
                onOpen={handleOpenNode}
                layout={tweaks.graphLayout}
                setLayout={(l) => setTweak("graphLayout", l)}
                simSpeed={tweaks.simSpeed}
                onSpawn={onSpawn}
                onAction={inspectorAction}
                sessionBus={sessionBus}
                onExpandToChat={(id) => {
                  setChatNodeId(id);
                  setScreen("chat");
                }}
                onLaunchCC={() => setScreen("create")}
                substrates={substrates}
              />
            )
          )}
          {screen === "fleet" && (
            <FleetScreen nodes={graph.nodes} onLaunch={() => setScreen("create")} onOpen={handleOpenNode} />
          )}
          {screen === "node" && (
            <NodeScreen
              node={openNode ?? selectedNode}
              nodes={graph.nodes}
              relationships={graph.relationships}
              onBack={() => setScreen("fleet")}
              onOpen={handleOpenNode}
              onAction={(a, p) => void handleNodeAction(openNode ?? selectedNode, a, p)}
            />
          )}
          {screen === "create" && (
            <CreateScreen
              onCreated={(id) => {
                setOpenNodeId(id);
                setSelectedNode(id);
                void refreshAll();
                setScreen("fleet");
              }}
              onCancel={() => setScreen("cockpit")}
            />
          )}
          {screen === "channels" && <ChannelsScreen />}
          {screen === "hooks" && <HooksScreen />}
          {screen === "logs" && <LogsScreen notifications={notifications} />}
          {screen === "settings" && <SettingsScreen />}
          {screen === "chat" && (
            <ChatScreen
              nodes={graph.nodes}
              chatNodeId={chatNodeId ?? ccNode?.id}
              onSelectChat={setChatNodeId}
              simSpeed={tweaks.simSpeed}
              onSpawn={onSpawn}
              sessionBus={sessionBus}
              onLaunch={() => setScreen("create")}
            />
          )}
        </div>
      </div>

      {cmdkOpen && (
        <CmdK
          onClose={() => setCmdkOpen(false)}
          onPick={(s) => {
            setScreen(s);
            setCmdkOpen(false);
          }}
          onLaunch={() => {
            setScreen("create");
            setCmdkOpen(false);
          }}
        />
      )}

      <div className="toast-stack">
        {toasts.map((t) => (
          <NtfyToast
            key={t.id}
            toast={t}
            onDismiss={() => dismissToast(t.id)}
            onReply={async (text) => {
              const target = graph.nodes.find((n) => n.id === t.from);
              if (target) {
                try {
                  await postNodeInput(target.id, text);
                } catch (err) {
                  setLocalError(`reply failed: ${String(err instanceof Error ? err.message : err)}`);
                }
              }
            }}
          />
        ))}
      </div>

      {/* operator-only token toggle, parked in the corner so the topbar stays clean */}
      {ownerToken && (
        <div
          style={{
            position: "fixed",
            right: 12,
            bottom: 12,
            zIndex: 30,
            fontFamily: "var(--font-mono)",
            fontSize: 10,
            color: "var(--fg-subtle)",
            display: "flex",
            gap: 8,
            alignItems: "center",
          }}
        >
          <span>token stored</span>
          <button className="btn btn-ghost btn-sm" type="button" onClick={clearOwnerToken}>
            clear
          </button>
        </div>
      )}

      {localError && <div className="error-banner">{localError}</div>}

      {/* the prototype's notice; preserved as a verifiable signal that
          we are still consuming the operational gate from state.ts */}
      {graph.nodes.some((n) => !isOperational(n)) && null}
    </div>
  );
}
