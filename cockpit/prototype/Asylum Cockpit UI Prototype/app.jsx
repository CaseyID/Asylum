// asylum cockpit — app shell, router, default cockpit screen, tweaks

const { useState: useASt, useEffect: useAEf, useRef: useARf, useMemo: useAMm } = React;

// tweak defaults — must be a single JSON block for editmode persistence
const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "theme": "light",
  "navCollapsed": false,
  "graphLayout": "tree",
  "ccHarness": "codex",
  "simSpeed": "slow",
  "ntfyEnabled": true,
  "showFirstRun": false
}/*EDITMODE-END*/;

function App() {
  const [tweaks, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const [screen, setScreen] = useASt('cockpit');
  const [nodes, setNodes] = useASt(ASYLUM_DATA.NODES);
  const [selectedId, setSelectedId] = useASt('cc-7c2af');
  const [openNode, setOpenNode] = useASt(null);
  const [chatNodeId, setChatNodeId] = useASt(null);
  const [cmdkOpen, setCmdkOpen] = useASt(false);
  const [showLaunch, setShowLaunch] = useASt(false);
  const [toasts, setToasts] = useASt([]);
  const toastIdx = useARf(0);
  // shared bus: terminal exposes pushSystem/pushTool/runResponse on .current.
  // inspector .handle is a function(action) → mutates state and (optionally) writes to terminal.
  const ccBus = useARf({});

  // theme attribute
  useAEf(() => {
    document.documentElement.setAttribute('data-theme', tweaks.theme);
  }, [tweaks.theme]);

  // command palette + esc
  useAEf(() => {
    const handler = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') { e.preventDefault(); setCmdkOpen(true); }
      if (e.key === 'Escape') setCmdkOpen(false);
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  // ntfy toasts on a timer (only on cockpit/coord)
  useAEf(() => {
    if (!tweaks.ntfyEnabled || tweaks.simSpeed === 'still') return;
    const interval = tweaks.simSpeed === 'live' ? 9000 : 18000;
    const first = setTimeout(spawnToast, tweaks.simSpeed === 'live' ? 4000 : 8000);
    const t = setInterval(spawnToast, interval);
    return () => { clearTimeout(first); clearInterval(t); };
  }, [tweaks.ntfyEnabled, tweaks.simSpeed]);

  function spawnToast() {
    const tpl = ASYLUM_DATA.NTFY_TEMPLATES[toastIdx.current % ASYLUM_DATA.NTFY_TEMPLATES.length];
    toastIdx.current++;
    const id = 't-' + Date.now();
    setToasts(prev => [...prev.slice(-1), { ...tpl, id }]);
  }
  function dismissToast(id) { setToasts(prev => prev.filter(t => t.id !== id)); }

  const selected = nodes.find(n => n.id === selectedId);

  // when CC spawns a node, animate it into the graph
  function onSpawn(spawn) {
    const newNode = {
      id: spawn.id, name: spawn.id, role: spawn.role, harness: spawn.harness,
      substrate: spawn.substrate, workspace: '~/work/refactor-router',
      state: 'running', duration: '0s', preview: '> warming up…',
      parent: spawn.parent, edge: spawn.role === 'worker' ? 'supervises' : 'spawned_for',
      tokensIn: 0, tokensOut: 0, ctx: 0.02, tools: 0, _spawning: true,
    };
    setNodes(prev => [...prev, newNode]);
    setTimeout(() => setNodes(prev => prev.map(n => n.id === spawn.id ? { ...n, _spawning: false } : n)), 800);
  }

  function openNodeFn(n) { setOpenNode(n); setScreen('node'); }

  // inspector / node-detail action handler — wires the buttons to demo state changes.
  // also writes a system line to the bottom CC panel so the demo is visible.
  function handleNodeAction(node, action, payload) {
    if (!node) return;
    const term = ccBus.current;
    const writeSys = (text) => term?.pushSystem?.(text);
    const writeTool = (n, args, output, state='ok') => term?.pushTool?.(n, args, output, state);

    if (action === 'attach') {
      writeSys(`opening browser attach for ${node.id}…`);
      writeTool('node.attach.browser', { node: node.id }, `attach url issued · ttl 3600s\nrenders: tui · token a8x7…b91`, 'ok');
      pushToast({ kind: 'attach', title: 'attach url ready', body: `${node.id} · open in any tab`, from: node.id });
    } else if (action === 'send') {
      writeSys(`prompting for input to ${node.id} (use the box below to type directly)`);
      // jump focus to the bottom panel by selecting this node — already selected but be explicit
      setSelectedId(node.id);
    } else if (action === 'interrupt') {
      writeTool('node.interrupt', { node: node.id }, 'sigint sent · harness paused mid-stream', 'ok');
      setNodes(prev => prev.map(n => n.id === node.id ? { ...n, state: 'idle', preview: '— interrupted (' + n.duration + ')' } : n));
    } else if (action === 'restart') {
      writeTool('node.restart', { node: node.id, preserve_workspace: true }, 'restart issued · reusing workspace · context dropped', 'ok');
      setNodes(prev => prev.map(n => n.id === node.id ? { ...n, state: 'running', duration: '0s', tokensIn: 0, tokensOut: 0, ctx: 0.02, preview: '> warming up after restart…' } : n));
    } else if (action === 'fork') {
      const forkId = (node.id.split('-')[0] || 'w') + '-' + Math.random().toString(36).slice(2, 6);
      writeTool('node.fork', { source: node.id, branch: forkId }, `forked → ${forkId}\ncontext + workspace + open files copied`, 'ok');
      const fork = {
        id: forkId, name: forkId, role: node.role, harness: node.harness, substrate: node.substrate,
        workspace: node.workspace, state: 'running', duration: '0s',
        preview: '> forked from ' + node.id, parent: node.parent, edge: 'spawned_for',
        tokensIn: 0, tokensOut: 0, ctx: node.ctx, tools: 0, _spawning: true,
      };
      setNodes(prev => [...prev, fork]);
      setTimeout(() => setNodes(prev => prev.map(n => n.id === forkId ? { ...n, _spawning: false } : n)), 800);
    } else if (action === 'archive') {
      writeTool('node.archive', { node: node.id }, 'transcript exported · workspace snapshot saved · node detached', 'ok');
      setNodes(prev => prev.map(n => n.id === node.id ? { ...n, state: 'idle', preview: '— archived (transcript exported)' } : n));
    } else if (action === 'terminate') {
      writeTool('node.terminate', { node: node.id }, 'sigterm sent · harness exited · resources released', 'ok');
      setNodes(prev => prev.filter(n => n.id !== node.id));
      if (selectedId === node.id) setSelectedId(ccNode?.id || null);
    } else if (action === 'decision' && payload) {
      writeSys(`decision on ${node.id}: ${payload}`);
      setNodes(prev => prev.map(n => n.id === node.id
        ? { ...n, state: 'running', preview: `+ ${payload === 'approve' ? 'approved' : payload === 'deny' ? 'denied' : payload}…`, decision: undefined }
        : n));
    }
  }

  function pushToast(toast) {
    const id = 't-' + Date.now();
    setToasts(prev => [...prev.slice(-1), { ...toast, id }]);
  }

  // assemble the bus that gets passed down — handle is bound to the currently-selected node
  const actionApi = {
    current: ccBus.current,
    handle: (action, payload) => {
      const target = nodes.find(n => n.id === selectedId);
      handleNodeAction(target, action, payload);
    },
  };
  // keep .current pointed at the same object across renders so the terminal can attach methods
  actionApi.current = ccBus.current;

  const nav = [
    { id: 'cockpit',  label: 'cockpit',       icon: 'layout-grid' },
    { id: 'fleet',    label: 'nodes',         icon: 'list',        count: nodes.length },
    { id: 'chat',     label: 'chat',          icon: 'terminal' },
    { id: 'logs',     label: 'logs',          icon: 'activity' },
  ];
  const navMessaging = [
    { id: 'channels', label: 'channels',      icon: 'rss',         count: ASYLUM_DATA.CHANNELS.filter(c => c.live).length },
    { id: 'hooks',    label: 'hooks',         icon: 'zap',         count: ASYLUM_DATA.HOOKS.filter(h => h.enabled).length },
  ];
  const navBottom = [
    { id: '__launch', label: 'launch node',   icon: 'plus', primary: true },
    { id: 'settings', label: 'settings',      icon: 'settings' },
  ];

  function go(s) {
    if (s === '__launch') { setScreen('create'); return; }
    if (s === 'first-run') { setNodes([]); setSelectedId(null); setScreen('cockpit'); setTweak('showFirstRun', true); return; }
    setScreen(s);
  }

  const ccNode = nodes.find(n => n.isCommandCenter);

  return (
    <div className="app" data-screen-label={screen}>
      <div className="topbar">
        <Wordmark />
        <div className="crumbs">
          <span className="sep">/</span>
          <span style={{ color: 'var(--fg)' }}>{screen === 'node' && openNode ? openNode.id : screen}</span>
          {screen === 'cockpit' && (
            <span className="live" style={{ marginLeft: 6 }}>
              <span className="dot" /> {nodes.filter(n => n.state === 'running').length} running
            </span>
          )}
        </div>
        <div className="topbar-right">
          <Btn kind="ghost" size="sm" icon="search" onClick={() => setCmdkOpen(true)}>
            <span style={{ marginRight: 6, color: 'var(--fg-muted)' }}>search…</span>
            <span className="kbd">⌘K</span>
          </Btn>
          <Btn kind="ghost" size="sm" icon="bell" iconOnly title="notifications" />
          <Btn kind="ghost" size="sm" iconOnly icon={tweaks.theme === 'dark' ? 'sun' : 'moon'} onClick={() => setTweak('theme', tweaks.theme === 'dark' ? 'light' : 'dark')} title="toggle theme" />
        </div>
      </div>

      <div className={`body ${tweaks.navCollapsed ? 'nav-collapsed' : ''}`}>
        <div className="nav">
          {!tweaks.navCollapsed && <div className="group-label">cockpit</div>}
          {nav.map(n => (
            <div key={n.id} className={`item ${screen === n.id ? 'active' : ''}`} onClick={() => go(n.id)} title={tweaks.navCollapsed ? n.label : ''}>
              <Icon name={n.icon} />
              <span className="label">{n.label}</span>
              {n.count !== undefined && <span className="count">{n.count}</span>}
            </div>
          ))}
          {!tweaks.navCollapsed && <div className="group-label" style={{ marginTop: 18 }}>messaging</div>}
          {navMessaging.map(n => (
            <div key={n.id} className={`item ${screen === n.id ? 'active' : ''}`} onClick={() => go(n.id)} title={tweaks.navCollapsed ? n.label : ''}>
              <Icon name={n.icon} />
              <span className="label">{n.label}</span>
              {n.count !== undefined && <span className="count">{n.count}</span>}
            </div>
          ))}
          <div className="spacer" />
          {!tweaks.navCollapsed && <div className="group-label">tools</div>}
          {navBottom.map(n => (
            <div key={n.id} className={`item ${screen === n.id ? 'active' : ''}`} onClick={() => go(n.id)} title={tweaks.navCollapsed ? n.label : ''}>
              <Icon name={n.icon} />
              <span className="label">{n.label}</span>
            </div>
          ))}
          {!tweaks.navCollapsed && (
            <div className="footer-info">
              <div>asylum 0.1.0-rc4</div>
              <div className="muted">localhost:5173</div>
              <div className="muted">tailscale: connected</div>
            </div>
          )}
        </div>

        <div className="main">
          {screen === 'cockpit'  && (
            tweaks.showFirstRun || nodes.length === 0
              ? <FirstRun onLaunch={() => { setNodes(ASYLUM_DATA.NODES); setTweak('showFirstRun', false); }} />
              : <CockpitScreen nodes={nodes} ccNode={ccNode} selected={selected} onSelect={n => setSelectedId(n.id)}
                  onOpen={openNodeFn} layout={tweaks.graphLayout} setLayout={l => setTweak('graphLayout', l)}
                  ccHarness={tweaks.ccHarness} simSpeed={tweaks.simSpeed} onSpawn={onSpawn} onAction={actionApi}
                  onExpandToChat={(id) => { setChatNodeId(id); setScreen('chat'); }} />
          )}
          {screen === 'fleet'    && <FleetScreen nodes={nodes} onSelect={n => setSelectedId(n.id)} onLaunch={() => setScreen('create')} onOpen={openNodeFn} />}
          {screen === 'node'     && <NodeScreen node={openNode || selected} nodes={nodes} onBack={() => setScreen('fleet')} onOpen={openNodeFn} onAction={(action, payload) => handleNodeAction(openNode || selected, action, payload)} />}
          {screen === 'create'   && <CreateScreen onCreated={() => setScreen('fleet')} onCancel={() => setScreen('cockpit')} />}
          {screen === 'channels' && <ChannelsScreen />}
          {screen === 'hooks'    && <HooksScreen />}
          {screen === 'logs'     && <LogsScreen />}
          {screen === 'settings' && <SettingsScreen />}
          {screen === 'chat'     && <ChatScreen tweaks={tweaks} nodes={nodes}
            chatNodeId={chatNodeId || ccNode?.id}
            onSelectChat={id => setChatNodeId(id)}
            simSpeed={tweaks.simSpeed} onSpawn={onSpawn} onAction={actionApi} />}
        </div>
      </div>

      {cmdkOpen && <CmdK onClose={() => setCmdkOpen(false)} onPick={go} onLaunch={() => setScreen('create')} />}

      {/* ntfy toasts */}
      <div className="toast-stack">
        {toasts.map(t => (
          <NtfyToast key={t.id} toast={t} onDismiss={() => dismissToast(t.id)} onReply={r => {
            // when a reply matches the current waiting node, resolve it
            if (t.from === 'w-2b0c8' && (r === 'approve' || r === 'always')) {
              setNodes(prev => prev.map(n => n.id === 'w-2b0c8' ? { ...n, state: 'running', preview: '+ writing package.json (approved via ntfy)', decision: undefined } : n));
            }
          }} />
        ))}
      </div>

      {/* tweaks panel */}
      <TweaksPanel title="Tweaks" subtitle="cockpit prototype">
        <TweakSection title="appearance">
          <TweakRadio label="theme" value={tweaks.theme} onChange={v => setTweak('theme', v)}
            options={[{ value: 'dark', label: 'dark' }, { value: 'light', label: 'light' }]} />
          <TweakRadio label="nav" value={tweaks.navCollapsed ? 'collapsed' : 'full'} onChange={v => setTweak('navCollapsed', v === 'collapsed')}
            options={[{ value: 'full', label: 'full' }, { value: 'collapsed', label: 'icons' }]} />
        </TweakSection>
        <TweakSection title="graph">
          <TweakSelect label="layout" value={tweaks.graphLayout} onChange={v => setTweak('graphLayout', v)}
            options={[{ value: 'tree', label: 'hierarchical tree' }, { value: 'free', label: 'free / dotted' }, { value: 'force', label: 'force cluster' }, { value: 'swimlanes', label: 'swimlanes by substrate' }]} />
        </TweakSection>
        <TweakSection title="simulation">
          <TweakRadio label="speed" value={tweaks.simSpeed} onChange={v => setTweak('simSpeed', v)}
            options={[{ value: 'still', label: 'still' }, { value: 'slow', label: 'slow' }, { value: 'live', label: 'live' }]} />
          <TweakSelect label="cc harness" value={tweaks.ccHarness} onChange={v => setTweak('ccHarness', v)}
            options={[{ value: 'codex', label: 'codex' }, { value: 'claude-code', label: 'claude code' }]} />
          <TweakToggle label="ntfy toasts" value={tweaks.ntfyEnabled} onChange={v => setTweak('ntfyEnabled', v)} />
        </TweakSection>
        <TweakSection title="screens">
          <TweakButton onClick={() => go('first-run')}>show first-run hero</TweakButton>
          <TweakButton onClick={() => spawnToast()}>fire ntfy toast now</TweakButton>
        </TweakSection>
      </TweaksPanel>
    </div>
  );
}

// ─── Cockpit (default) screen ────────────────────────
function CockpitScreen({ nodes, ccNode, selected, onSelect, onOpen, layout, setLayout, ccHarness, simSpeed, onSpawn, onAction, onExpandToChat }) {
  // bottom panel targets either the selected node (if explicitly chosen and not the CC)
  // or the command center by default. clicking a graph node swaps the panel to that node.
  const panelNode = selected || ccNode;
  return (
    <div className="cockpit">
      <div className="cockpit-main">
        <div className="cockpit-graph-wrap">
          <Graph nodes={nodes} layout={layout} selectedId={selected?.id} onSelect={onSelect} substrates={ASYLUM_DATA.SUBSTRATES} />
          <div className="graph-controls">
            {[['tree', 't'], ['free', 'f'], ['force', '✦'], ['swimlanes', '≡']].map(([id, glyph]) => (
              <Btn key={id} size="sm" kind={layout === id ? 'secondary' : 'ghost'} onClick={() => setLayout(id)} title={id}>
                <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10, marginRight: 4 }}>{glyph}</span>{id}
              </Btn>
            ))}
          </div>
          <div className="graph-legend">
            <div className="item"><span className="swatch" /> supervises</div>
            <div className="item"><span className="swatch dashed" /> spawned_for</div>
            <div className="item"><span style={{ fontFamily: 'var(--font-mono)', color: 'var(--status-info)' }}>━━━</span> live</div>
            <div className="item"><span style={{ color: 'var(--fg-subtle)', fontSize: 10 }}>scroll = zoom · drag = pan</span></div>
          </div>
        </div>
        <div className="cockpit-cc-wrap">
          {panelNode ? (
            <NodeSession
              key={panelNode.id}
              node={panelNode}
              mode="cockpit"
              onSpawn={onSpawn}
              simSpeed={simSpeed}
              onAction={panelNode.isCommandCenter ? onAction : undefined}
              onExpand={() => onExpandToChat?.(panelNode.id)}
            />
          ) : (
            <div style={{ flex: 1, display: 'grid', placeItems: 'center', background: 'var(--bg-sunken)' }}>
              <Empty glyph="⌬" lead="no command center running" sub="launch one to get an asylum-aware harness session here"
                action={<Btn kind="primary" icon="plus">launch command center</Btn>} />
            </div>
          )}
        </div>
      </div>
      <Inspector node={selected} onAction={onAction?.handle} onOpen={onOpen} />
    </div>
  );
}

// ─── First-run hero ──────────────────────────────────
function FirstRun({ onLaunch }) {
  const steps = [
    ['open cockpit', 'this screen — fleet view, command center, inspector'],
    ['start a command-center', 'codex or claude code, with asylum context preloaded'],
    ['ask it to spawn workers', '"refactor the router with 2 workers on loon-us-west"'],
    ['watch the graph', 'spawned nodes appear as supervisor / worker cards with explicit edges'],
    ['inspect any node', 'live transcript, capability matrix, decision prompts'],
    ['attach in browser', 'real harness ui at any time, no native install required'],
    ['receive ntfy', 'remote command channel — reply with `approve`, `attach`, `retry`'],
    ['drive from mcp', 'asylum tools available in claude desktop, cursor, anything mcp-capable'],
    ['hand off to loon', 'workers boot in firecracker vms — same capability surface'],
  ];
  return (
    <div className="firstrun">
      <div className="left">
        <div style={{ position: 'relative' }}><Wordmark size={20} /></div>
        <div className="hero">
          <div className="mono-eyebrow">{'['} v0.1.0-rc4 · single-user · localhost {']'}</div>
          <h1>a control plane for the agent harnesses you already use. <span className="b">[</span>not a harness<span className="b">]</span>.</h1>
          <p>asylum doesn't replace codex, claude code, or anything else. it launches them, gives them shared context and tools, lets them coordinate, and lets you reach them from anywhere.</p>
        </div>
        <div className="actions">
          <Btn kind="primary" icon="play" onClick={onLaunch}>start a command center</Btn>
          <Btn icon="terminal">open cli</Btn>
          <Btn kind="ghost" icon="book-open">read the spec</Btn>
        </div>
        <div style={{ position: 'relative', marginTop: 'auto', display: 'flex', gap: 24, fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--fg-subtle)' }}>
          <span>2 harnesses ready</span>
          <span>·</span>
          <span>3 substrates configured</span>
          <span>·</span>
          <span>0 nodes alive</span>
        </div>
      </div>
      <div className="right">
        <div className="checklist-head">{'['} wow sequence {']'}</div>
        {steps.map((s, i) => (
          <div className="check" key={i}>
            <span className="num">{String(i + 1).padStart(2, '0')}</span>
            <div className="body">
              {s[0]}
              <div className="sub">{s[1]}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
