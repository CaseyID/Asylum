// asylum cockpit — top-level app shell + screen router.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ApiError,
  archiveNode,
  fetchChannelMessages,
  fetchChannels,
  fetchGraph,
  fetchHarnessDescriptors,
  fetchHealth,
  fetchHooks,
  fetchNotifications,
  fetchSubstrateDescriptors,
  markNotificationRead,
  forkNode,
  hydrateOwnerTokenFromLocation,
  interruptNode,
  postNodeInput,
  requestBrowserAttach,
  requestNativeTarget,
  sendRemoteCommand,
  setStoredOwnerToken,
  stopNode,
} from "./api";
import { selectCommandCenter, useCockpitStore } from "./state";
import { Topbar } from "./components/Topbar";
import { Nav } from "./components/Nav";
import { CmdK } from "./components/CmdK";
import { NtfyToast, type ToastPayload } from "./components/NtfyToast";
import { CockpitScreen } from "./screens/CockpitScreen";
import { FleetScreen } from "./screens/FleetScreen";
import { DecisionsScreen } from "./screens/DecisionsScreen";
import { NodeScreen } from "./screens/NodeScreen";
import { CreateScreen } from "./screens/CreateScreen";
import { ChannelsScreen } from "./screens/ChannelsScreen";
import { HooksScreen } from "./screens/HooksScreen";
import { LogsScreen } from "./screens/LogsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { FirstRunScreen } from "./screens/FirstRunScreen";
import type { GraphNode } from "./components/Graph";
import type { InspectorAction } from "./components/Inspector";
import type { NodeScreenAction } from "./screens/NodeScreen";
import { useUiPrefs } from "./lib/uiPrefs";
import type {
  AsylumNode,
  ChannelDescriptor,
  HookRule,
  HealthResponse,
  NotificationRecord,
  ScreenId,
  SubstrateDescriptor,
} from "./types";

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

  const [uiPrefs, setPref] = useUiPrefs();

  const [screen, setScreen] = useState<ScreenId>("cockpit");
  const [openNodeId, setOpenNodeId] = useState<string | undefined>();
  const [chatNodeId, setChatNodeId] = useState<string | undefined>();
  const [cmdkOpen, setCmdkOpen] = useState(false);
  const [toasts, setToasts] = useState<ToastPayload[]>([]);
  const [channels, setChannels] = useState<ChannelDescriptor[]>([]);
  const channelsRef = useRef<ChannelDescriptor[]>([]);
  const [hooks, setHooks] = useState<HookRule[]>([]);
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [harnessCount, setHarnessCount] = useState(0);
  const [substrates, setSubstrates] = useState<SubstrateDescriptor[]>([]);
  const lastSeenMessageId = useRef<number>(0);

  const [ownerToken, setOwnerToken] = useState("");
  const [tokenDraft, setTokenDraft] = useState("");
  const [authRequired, setAuthRequired] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const [localNotice, setLocalNotice] = useState<string | null>(null);
  const [notifications, setNotifications] = useState<NotificationRecord[]>([]);
  const refreshInFlight = useRef(false);

  // theme attribute on <html>
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", uiPrefs.theme);
  }, [uiPrefs.theme]);

  // auto-dismiss notice banner after 2.5s
  useEffect(() => {
    if (!localNotice) return;
    const t = setTimeout(() => setLocalNotice(null), 2500);
    return () => clearTimeout(t);
  }, [localNotice]);

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
      const [graphResp, notifs, daemonHealth] = await Promise.all([fetchGraph(), fetchNotifications(), fetchHealth()]);
      const cc = selectCommandCenter(graphResp.nodes);
      initializeGraph({
        ...graphResp,
        nodes: [...graphResp.nodes].sort((a, b) => String(a.created_at).localeCompare(String(b.created_at))),
        relationships: [...graphResp.relationships],
      });
      setNotifications(notifs);
      setHealth(daemonHealth);
      setCommandCenterSelection(cc?.id);
      try {
        const [c, h, s, harnesses] = await Promise.all([
          fetchChannels(),
          fetchHooks(),
          fetchSubstrateDescriptors(),
          fetchHarnessDescriptors(),
        ]);
        setChannels(c);
        setHooks(h);
        setSubstrates(s);
        setHarnessCount(harnesses.filter((item) => item.available).length);
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

  // keep channelsRef in sync so the toast interval can read current channels
  // without listing channels in the interval effect's deps (which would cause
  // the interval to be torn down and reset on every 6s poll).
  useEffect(() => {
    channelsRef.current = channels;
  }, [channels]);

  // ntfy toast spawner — polls the live ntfy channel for new inbound messages
  // and surfaces unseen ones as a lower-left toast.
  // channelsRef avoids tearing down the timer on every channel-list refresh.
  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      const ntfyChannel = channelsRef.current.find((c) => c.kind === "ntfy" && c.live);
      if (!ntfyChannel) return;
      try {
        const msgs = await fetchChannelMessages(ntfyChannel.id, 10);
        if (cancelled) return;
        const fresh = msgs.filter((m) => m.direction === "in" && m.id > lastSeenMessageId.current);
        if (fresh.length === 0) return;
        const latest = fresh[fresh.length - 1];
        lastSeenMessageId.current = latest.id;
        // Append to existing toasts rather than replacing; cap at 3 (L14).
        setToasts((prev) => [
          ...prev,
          {
            id: "t-" + latest.id,
            from: latest.sender,
            nodeId: latest.node_id ?? null,
            channel: ntfyChannel.name,
            subject: latest.subject,
            body: latest.subject ? `${latest.subject}\n${latest.body}` : latest.body,
            replies: latest.replies,
          },
        ].slice(-3));
      } catch {
        /* silent — surface via Logs screen */
      }
    };
    const t = setInterval(tick, 6000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, []);

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

  const handleOpenNotificationNode = (nodeId: string) => {
    const target = graph.nodes.find((n) => n.id === nodeId);
    if (!target) {
      setLocalError(`notification node no longer in graph: ${nodeId}`);
      return;
    }
    handleOpenNode(target);
  };

  async function handleMarkNotificationRead(id: string): Promise<void> {
    try {
      await markNotificationRead(id);
      await refreshAll();
    } catch (err) {
      setLocalError(`mark notification read failed: ${String(err instanceof Error ? err.message : err)}`);
      throw err;
    }
  }

  async function handleNodeAction(target: AsylumNode | undefined, action: InspectorAction | NodeScreenAction, _payload?: string) {
    if (!target) return;
    try {
      if (action === "attach") {
        const r = await requestBrowserAttach(target.id);
        setLocalNotice(r.note ?? `attach link issued · ttl ${r.expires_in_seconds ?? 3600}s`);
        if (typeof window !== "undefined" && r.attach_url) {
          window.open(r.attach_url, "_blank", "noopener,noreferrer");
        }
      } else if (action === "native-attach") {
        const r = await requestNativeTarget(target.id);
        const cmd = [r.command, ...r.args].join(" ");
        if (typeof navigator !== "undefined" && navigator.clipboard) {
          await navigator.clipboard.writeText(cmd);
        }
        setLocalNotice("terminal attach command copied to clipboard");
      } else if (action === "send") {
        setSelectedNode(target.id);
        setChatNodeId(target.id);
        setScreen("chat");
        setLocalNotice("opened session input");
      } else if (action === "interrupt") {
        await interruptNode(target.id);
        setLocalNotice("interrupt sent");
      } else if (action === "stop") {
        await stopNode(target.id);
        setLocalNotice("stop issued");
      } else if (action === "archive") {
        await archiveNode(target.id);
        setLocalNotice("archive issued");
      } else if (action === "terminate") {
        await stopNode(target.id);
        setLocalNotice("stop issued; resources will be released");
      } else if (action === "fork") {
        const newNode = await forkNode(target.id, {});
        setLocalNotice(`forked into ${newNode.id}`);
        setOpenNodeId(newNode.id);
        setSelectedNode(newNode.id);
      }
    } catch (err) {
      setLocalError(`${action} failed: ${String(err instanceof Error ? err.message : err)}`);
    }
    void refreshAll();
  }

  const inspectorAction = useCallback(
    (action: InspectorAction | "native-attach", payload?: string) => {
      void handleNodeAction(selectedNode, action, payload);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [selectedNode],
  );

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
        theme={uiPrefs.theme}
        onToggleTheme={() => setPref("theme", uiPrefs.theme === "dark" ? "light" : "dark")}
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

      <div className={`body ${uiPrefs.navCollapsed ? "nav-collapsed" : ""}`}>
        <Nav
          collapsed={uiPrefs.navCollapsed}
          active={screen}
          fleetCount={graph.nodes.length}
          channelCount={liveChannelCount}
          hookCount={enabledHookCount}
          daemonVersion={health?.daemon_version ? `asylum ${health.daemon_version}` : undefined}
          bindAddr={health?.bind_addr ?? (typeof window !== "undefined" ? window.location.host : "localhost")}
          onPick={handleSelectScreen}
        />

        <div className="main">
          {screen === "cockpit" && (
            showFirstRun ? (
              <FirstRunScreen
                onLaunch={() => setScreen("create")}
                onOpenCli={() => setScreen("settings")}
                onReadSpec={() => setScreen("settings")}
                harnessCount={harnessCount}
                substrateCount={substrates.filter((s) => s.healthy).length}
                nodeCount={graph.nodes.length}
              />
            ) : (
              <CockpitScreen
                graphNodes={graphNodes}
                ccNode={ccNode}
                selected={selectedNode}
                onSelect={(n) => setSelectedNode(n.id)}
                onOpen={handleOpenNode}
                layout={uiPrefs.graphLayout}
                setLayout={(l) => setPref("graphLayout", l)}
                onAction={inspectorAction}
                onExpandToChat={(id) => {
                  setChatNodeId(id);
                  setScreen("chat");
                }}
                onLaunchCC={() => setScreen("create")}
                substrates={substrates}
                relationships={graph.relationships}
              />
            )
          )}
          {screen === "fleet" && (
            <FleetScreen nodes={graph.nodes} onLaunch={() => setScreen("create")} onOpen={handleOpenNode} />
          )}
          {screen === "decisions" && <DecisionsScreen />}
          {screen === "node" && (
            <NodeScreen
              node={openNode ?? selectedNode}
              nodes={graph.nodes}
              relationships={graph.relationships}
              onGraphRefresh={() => void refreshAll()}
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
                setScreen("node");
              }}
              onCancel={() => setScreen("cockpit")}
            />
          )}
          {screen === "channels" && <ChannelsScreen />}
          {screen === "hooks" && <HooksScreen />}
          {screen === "logs" && (
            <LogsScreen
              notifications={notifications}
              onMarkRead={handleMarkNotificationRead}
              onOpenNode={handleOpenNotificationNode}
            />
          )}
          {screen === "settings" && <SettingsScreen />}
          {screen === "chat" && (
            <ChatScreen
              nodes={graph.nodes}
              chatNodeId={chatNodeId ?? ccNode?.id}
              onSelectChat={setChatNodeId}
              onInterrupt={(node) => void handleNodeAction(node, "interrupt")}
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
          nodes={graph.nodes}
          onPickNode={(node) => {
            setOpenNodeId(node.id);
            setSelectedNode(node.id);
            setScreen("node");
            setCmdkOpen(false);
          }}
          onAttachInBrowser={() => {
            const target =
              graph.nodes.find((n) => n.id === selectedNodeId) ??
              graph.nodes.find((n) => n.id === openNodeId);
            setCmdkOpen(false);
            if (!target) {
              setLocalNotice("select a node first");
              return;
            }
            void handleNodeAction(target, "attach");
          }}
          onSendRemoteCommand={() => {
            setCmdkOpen(false);
            const example =
              "send token=<owner-token> node=<node-id> text=<message>";
            const raw = window.prompt(
              `remote command:\n  status token=…\n  ${example}\n  interrupt token=… node=…\n  stop token=… node=…\n  attach token=… node=…`,
              "",
            );
            if (!raw || !raw.trim()) return;
            sendRemoteCommand(raw.trim())
              .then((res) => {
                setLocalNotice(`remote command ${res.kind}: ${res.status}`);
              })
              .catch((err) => {
                setLocalError(
                  `remote command failed: ${String(err instanceof Error ? err.message : err)}`,
                );
              });
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
              if (!t.nodeId) return;
              const target = graph.nodes.find((n) => n.id === t.nodeId);
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
      {localNotice && <div className="notice-banner">{localNotice}</div>}
    </div>
  );
}
