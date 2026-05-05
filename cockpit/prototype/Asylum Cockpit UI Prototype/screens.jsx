// asylum cockpit — non-default screens

// ─── Fleet (table) ────────────────────────────────────
function FleetScreen({ nodes, onSelect, onLaunch, onOpen }) {
  const [q, setQ] = useState('');
  const [filter, setFilter] = useState('all');
  const filtered = nodes.filter(n => {
    if (q && !(n.id.includes(q) || n.role.includes(q) || n.harness.includes(q))) return false;
    if (filter !== 'all' && n.state !== filter) return false;
    return true;
  });
  const counts = useMemo(() => {
    const c = { all: nodes.length, running: 0, waiting: 0, idle: 0, errored: 0 };
    nodes.forEach(n => { if (c[n.state] !== undefined) c[n.state]++; });
    return c;
  }, [nodes]);

  return (
    <div className="page" style={{ paddingTop: 28 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">nodes</h1>
          <div className="page-sub">{nodes.length} total · {counts.running} running · {counts.waiting} waiting · {counts.errored} errored</div>
        </div>
        <div className="page-actions">
          <Btn icon="filter" size="sm">filter</Btn>
          <Btn icon="download" size="sm">export</Btn>
          <Btn kind="primary" icon="plus" onClick={onLaunch}>launch node</Btn>
        </div>
      </div>

      <div className="toolbar">
        <div className="search">
          <Icon name="search" size={12} />
          <input className="input mono" placeholder="filter by id, role, harness, substrate…" value={q} onChange={e => setQ(e.target.value)} />
        </div>
        <div style={{ display: 'flex', gap: 4 }}>
          {['all', 'running', 'waiting', 'idle', 'errored'].map(s => (
            <button key={s} className={`btn btn-sm ${filter === s ? 'btn-secondary' : 'btn-ghost'}`} onClick={() => setFilter(s)}>
              {s} <span className="muted" style={{ marginLeft: 4 }}>{counts[s]}</span>
            </button>
          ))}
        </div>
        <div className="stats">
          <span><b>3</b> substrates</span>
          <span><b>2</b> harnesses</span>
        </div>
      </div>

      <table className="table" style={{ borderTop: 'none' }}>
        <thead>
          <tr>
            <th style={{ width: 28 }}></th>
            <th style={{ width: 140 }}>node</th>
            <th style={{ width: 110 }}>role</th>
            <th style={{ width: 110 }}>harness</th>
            <th style={{ width: 130 }}>substrate</th>
            <th style={{ width: 110 }}>state</th>
            <th>preview</th>
            <th style={{ width: 80 }} className="right">ctx</th>
            <th style={{ width: 80 }} className="right">uptime</th>
            <th style={{ width: 32 }}></th>
          </tr>
        </thead>
        <tbody>
          {filtered.map(n => (
            <tr key={n.id} onClick={() => onOpen(n)}>
              <td style={{ color: 'var(--fg-subtle)', textAlign: 'center', fontFamily: 'var(--font-mono)' }}>{ROLE_GLYPH[n.role]}</td>
              <td className="mono" style={{ color: 'var(--fg)' }}>{n.id}{n.isCommandCenter && <span style={{ marginLeft: 8, color: 'var(--fg-muted)', fontSize: 10 }}>[cc]</span>}</td>
              <td className="mono muted">{n.role}</td>
              <td className="mono">{n.harness}</td>
              <td className="mono muted">{n.substrate}</td>
              <td><Pill status={n.state}>{nodeStatusLabel(n)}</Pill></td>
              <td className="mono muted ellipsis" style={{ maxWidth: 0 }}>{n.preview}</td>
              <td className="num">{Math.round(n.ctx * 100)}%</td>
              <td className="num">{n.duration}</td>
              <td><Icon name="chevron-right" size={14} style={{ opacity: 0.4 }} /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ─── Node detail ──────────────────────────────────────
function NodeScreen({ node, nodes, onBack, onOpen, onAction }) {
  const [tab, setTab] = useState('session');
  const [flash, setFlash] = useState(null); // small inline ack near control buttons
  if (!node) return <Empty lead="no node selected" sub="go back to nodes and pick one" action={<Btn icon="arrow-left" onClick={onBack}>back to fleet</Btn>} />;

  const harness = ASYLUM_DATA.HARNESSES.find(h => h.id === node.harness);
  const children = nodes.filter(n => n.parent === node.id);
  const parent = nodes.find(n => n.id === node.parent);

  function fire(action, label) {
    onAction?.(action);
    setFlash({ action, label, t: Date.now() });
    setTimeout(() => setFlash(f => f && f.action === action ? null : f), 2200);
  }

  return (
    <div className="node-page">
      <div className="node-main">
        <div className="node-header">
          <div className="top">
            <Btn kind="ghost" size="sm" icon="arrow-left" iconOnly onClick={onBack} />
            <span style={{ fontSize: 18, opacity: 0.5 }}>{ROLE_GLYPH[node.role]}</span>
            <span className="id">{node.id}</span>
            <Pill status={node.state}>{nodeStatusLabel(node)}</Pill>
            {node.isCommandCenter && <Tag kind="role">command-center</Tag>}
            <span className="right">
              <Btn size="sm" icon="external-link">attach in browser</Btn>
              <Btn size="sm" icon="terminal">native attach</Btn>
              <Btn size="sm" kind="ghost" icon="more-horizontal" iconOnly />
            </span>
          </div>
          <div className="meta">
            <span><b>{harness?.name || node.harness}</b> · {node.role}</span>
            <span>substrate: <b>{node.substrate}</b></span>
            <span>workspace: <b>{node.workspace}</b></span>
            <span>uptime: <b>{node.duration}</b></span>
            <span>ctx: <b>{Math.round(node.ctx * 100)}%</b></span>
          </div>
          <div className="node-tabs">
            {['session', 'events', 'tools', 'capabilities', 'relationships'].map(t => (
              <div key={t} className={`tab ${tab === t ? 'active' : ''}`} onClick={() => setTab(t)}>{t}</div>
            ))}
          </div>
        </div>

        {tab === 'session' && <NodeSession key={node.id} node={node} mode="fullscreen" simSpeed="slow" />}
        {tab === 'events' && <EventsView node={node} />}
        {tab === 'tools' && <ToolsView node={node} />}
        {tab === 'capabilities' && <CapsView harness={harness} />}
        {tab === 'relationships' && <RelView node={node} parent={parent} children={children} />}
      </div>

      <div className="node-side">
        {node.decision && (
          <div className="sect">
            <div className="h">decision needed</div>
            <div className="decision">
              <div className="h"><Icon name="alert-triangle" size={12} /> {node.decision.title}</div>
              <div className="q">{node.decision.body}</div>
              <div className="actions">
                {node.decision.actions.map((a, i) => (
                  <Btn key={a} size="sm" kind={i === 0 ? 'primary' : 'secondary'} onClick={() => fire('decision', a)}>{a}</Btn>
                ))}
              </div>
            </div>
          </div>
        )}

        <div className="sect">
          <div className="h">telemetry</div>
          <KV items={[
            ['tokens in', node.tokensIn.toLocaleString()],
            ['tokens out', node.tokensOut.toLocaleString()],
            ['ctx', Math.round(node.ctx * 100) + '%'],
            ['tool calls', node.tools],
            ['uptime', node.duration],
          ]} />
        </div>

        <div className="sect">
          <div className="h">relationships</div>
          {parent ? (
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: 12, marginBottom: 8 }}>
              <span className="muted">parent: </span>
              <a style={{ color: 'var(--fg)', cursor: 'pointer' }} onClick={() => onOpen(parent)}>{parent.id}</a>
              <span className="muted" style={{ marginLeft: 8, fontSize: 10 }}>({node.edge || 'spawned_for'})</span>
            </div>
          ) : <div className="mono muted">no parent</div>}
          {children.length > 0 && (
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}>
              <span className="muted">children:</span>
              {children.map(c => (
                <div key={c.id} style={{ paddingLeft: 14, marginTop: 4 }}>
                  <span className="muted">└ </span>
                  <a style={{ color: 'var(--fg)', cursor: 'pointer' }} onClick={() => onOpen(c)}>{c.id}</a>
                  <span className="muted" style={{ marginLeft: 6 }}>· {c.role}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="sect">
          <div className="h">controls</div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6 }}>
            <Btn size="sm" icon="message-square" onClick={() => fire('send', 'opened input prompt')}>send input</Btn>
            <Btn size="sm" icon="square" onClick={() => fire('interrupt', 'sigint sent · paused')}>interrupt</Btn>
            <Btn size="sm" icon="rotate-ccw" onClick={() => fire('restart', 'restart issued · ctx reset')}>restart</Btn>
            <Btn size="sm" icon="git-branch" onClick={() => fire('fork', 'forked → see graph')}>fork</Btn>
            <Btn size="sm" icon="archive" onClick={() => fire('archive', 'archived · transcript exported')}>archive</Btn>
            <Btn size="sm" kind="danger" icon="x" onClick={() => fire('terminate', 'terminated · resources released')}>terminate</Btn>
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

function TranscriptView({ node }) {
  return (
    <div className="term" style={{ borderTop: '1px solid var(--border)' }}>
      <div className="sub">[node {node.id} · transcript · {node.duration}]</div>
      <div style={{ marginTop: 12 }}>
        <span className="muted">14:08:02</span> <span className="run">launch</span> harness={node.harness} substrate={node.substrate}<br/>
        <span className="muted">14:08:02</span> <span className="info">context</span> ~/src/asylum (12 files, 4 docs)<br/>
        <span className="muted">14:08:03</span> <span className="run">ready</span><br/>
      </div>
      <div style={{ marginTop: 14 }}>
        <span style={{ color: 'var(--fg)' }}>$ analyze the router</span><br />
        <span className="sub">· loading workspace · 14:08:04</span>
      </div>
      <div style={{ marginTop: 8 }}>
        <ToolCall name="apply_patch" args={{ path: 'src/router/match.ts', diff: '+114 -0' }} output={'1: import { Pattern } from "./pattern";\n2:\n3: export function match(req: Request, routes: Route[]) {\n4:   for (const r of routes) {\n5:     const m = r.pattern.exec(req.url);\n…'} state="ok" />
        <ToolCall name="bash" args={{ cmd: 'npm test' }} output={'PASS  src/router/match.test.ts\nPASS  src/router/parse.test.ts\n12 tests passing in 34ms'} state="ok" />
      </div>
      <div style={{ marginTop: 8, color: 'var(--fg)' }}>
        i finished `match.ts`. tests pass. should i continue with `parse.ts` or hand the result back to the supervisor?
      </div>
      <div style={{ marginTop: 6 }}><span className="prompt-line">{'›'} </span><span className="caret" /></div>
    </div>
  );
}

function EventsView({ node }) {
  const events = ASYLUM_DATA.LOGS.filter(l => l.src === node.id || l.msg.includes(node.id));
  return (
    <div className="log">
      {events.map((e, i) => (
        <div className="row" key={i}>
          <span className="ts">{e.ts}</span>
          <span className={`lvl ${e.lvl}`}>{e.lvl}</span>
          <span className="src">{e.src}</span>
          <span className="nid"></span>
          <span className="msg">{e.msg}</span>
        </div>
      ))}
      {events.length === 0 && <Empty glyph="[ ]" lead="no events for this node yet" sub="events appear as the harness streams output and tool calls" />}
    </div>
  );
}

function ToolsView({ node }) {
  return (
    <div style={{ padding: 24, overflow: 'auto', borderTop: '1px solid var(--border)' }}>
      <div className="muted mono" style={{ fontSize: 11, marginBottom: 12, letterSpacing: 0.06 }}>recent tool calls</div>
      <ToolCall name="apply_patch" args={{ path: 'src/router/match.ts' }} output={'+114 -0\nfile created · syntax checked · 0 lints'} state="ok" />
      <ToolCall name="bash" args={{ cmd: 'npm test' }} output={'PASS src/router/match.test.ts (4 tests, 12ms)\nPASS src/router/parse.test.ts (8 tests, 22ms)\n12 tests, 0 failures'} state="ok" />
      <ToolCall name="read" args={{ path: 'src/router/pattern.ts' }} state="ok" />
      <ToolCall name="ripgrep" args={{ pattern: 'export function match' }} output={'src/router/match.ts:3'} state="ok" />
    </div>
  );
}

function CapsView({ harness }) {
  const ALL = ['launch', 'observe', 'send_input', 'browser_attach', 'native_attach', 'interrupt', 'stop', 'tool_calls', 'transcript_export', 'context_telemetry', 'subagents', 'native_resume', 'permission_prompts', 'auto_compaction', 'checkpoint'];
  return (
    <div style={{ padding: 24, overflow: 'auto', borderTop: '1px solid var(--border)' }}>
      <div className="muted mono" style={{ fontSize: 11, marginBottom: 16, letterSpacing: 0.06 }}>capability matrix · {harness?.name}</div>
      <div className="capgrid" style={{ maxWidth: 480 }}>
        {ALL.map(c => {
          const has = harness?.caps.includes(c);
          return (
            <Fragment key={c}>
              <span className="cap">{c}</span>
              <span className={has ? 'ok' : 'no'}>{has ? '✓ supported' : '— not advertised'}</span>
            </Fragment>
          );
        })}
      </div>
    </div>
  );
}

function RelView({ node, parent, children }) {
  return (
    <div style={{ padding: 24, overflow: 'auto', borderTop: '1px solid var(--border)', fontFamily: 'var(--font-mono)', fontSize: 12 }}>
      <div className="muted" style={{ fontSize: 11, marginBottom: 16, letterSpacing: 0.06 }}>explicit graph relationships</div>
      {parent && <div style={{ marginBottom: 14 }}><span className="muted">parent · </span> <span style={{ color: 'var(--fg)' }}>{parent.id}</span> <span className="muted">({node.edge})</span></div>}
      {children.length > 0 && <>
        <div className="muted" style={{ marginBottom: 8 }}>children:</div>
        {children.map(c => <div key={c.id} style={{ paddingLeft: 14 }}>└ <span style={{ color: 'var(--fg)' }}>{c.id}</span> <span className="muted">· {c.role}</span></div>)}
      </>}
      {!parent && children.length === 0 && <div className="muted">no explicit relationships</div>}
      <div className="hr" />
      <div className="muted" style={{ fontSize: 11, marginBottom: 8 }}>correlations (not edges)</div>
      <div style={{ color: 'var(--fg)' }}>workspace {node.workspace} → 3 nodes</div>
      <div style={{ color: 'var(--fg)' }}>substrate {node.substrate} → 4 nodes</div>
    </div>
  );
}

// ─── Create / launch ──────────────────────────────────
function CreateScreen({ onCreated, onCancel }) {
  const [harness, setHarness] = useState('codex');
  const [substrate, setSubstrate] = useState('local');
  const [role, setRole] = useState('command-center');
  const [workspace, setWorkspace] = useState('~/src/asylum');
  const [recipe, setRecipe] = useState('cc');
  const [prompt, setPrompt] = useState('inspect the asylum context, summarize active nodes, and ask me what to spawn next.');

  return (
    <div className="page" style={{ maxWidth: 880 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">launch node</h1>
          <div className="page-sub">creates a real harness session. capabilities advertised at launch.</div>
        </div>
        <div className="page-actions">
          <Btn onClick={onCancel}>cancel</Btn>
          <Btn kind="primary" icon="play" onClick={onCreated}>launch</Btn>
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 320px', gap: 32 }}>
        <div className="col" style={{ gap: 18 }}>
          <Field label="harness" hint="claude code advertises subagents and native resume; codex advertises tool-call telemetry">
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              {ASYLUM_DATA.HARNESSES.map(h => (
                <button key={h.id} disabled={!h.available}
                  className={`btn ${harness === h.id ? 'btn-primary' : 'btn-secondary'}`}
                  style={{ justifyContent: 'flex-start', padding: '10px 12px', flexDirection: 'column', alignItems: 'flex-start', gap: 4, opacity: h.available ? 1 : 0.45, cursor: h.available ? 'pointer' : 'not-allowed' }}
                  onClick={() => h.available && setHarness(h.id)}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}>
                    <span>{h.name}</span>
                    <span style={{ marginLeft: 'auto', fontFamily: 'var(--font-mono)', fontSize: 10, opacity: 0.6 }}>{h.kind}</span>
                  </div>
                  <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10, opacity: 0.7 }}>
                    {h.available ? `${h.caps.length} caps advertised` : 'future · adapter not built'}
                  </span>
                </button>
              ))}
            </div>
          </Field>

          <Field label="substrate" hint="loon vms boot in <2s. local nodes share your machine's resources.">
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              {ASYLUM_DATA.SUBSTRATES.map(s => (
                <button key={s.id} disabled={!s.healthy}
                  className={`btn ${substrate === s.id ? 'btn-primary' : 'btn-secondary'}`}
                  style={{ justifyContent: 'flex-start', padding: '10px 12px', flexDirection: 'column', alignItems: 'flex-start', gap: 4, opacity: s.healthy ? 1 : 0.5 }}
                  onClick={() => s.healthy && setSubstrate(s.id)}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: '100%' }}>
                    <span>{s.name}</span>
                    <Pill status={s.healthy ? 'running' : 'errored'}>{s.healthy ? 'healthy' : 'down'}</Pill>
                  </div>
                  <span style={{ fontFamily: 'var(--font-mono)', fontSize: 10, opacity: 0.7 }}>
                    {s.host} · {s.healthy ? `cap ${Math.round(s.capacity * 100)}%` : (s.warning || 'unreachable')}
                  </span>
                </button>
              ))}
            </div>
          </Field>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
            <Field label="role hint">
              <select className="input mono" value={role} onChange={e => setRole(e.target.value)}>
                <option value="command-center">command-center</option>
                <option value="supervisor">supervisor</option>
                <option value="worker">worker</option>
                <option value="evaluator">evaluator</option>
                <option value="assistant">assistant</option>
                <option value="custom">custom…</option>
              </select>
            </Field>
            <Field label="workspace" hint="absolute path or repo url">
              <input className="input mono" value={workspace} onChange={e => setWorkspace(e.target.value)} />
            </Field>
          </div>

          <Field label="launch packet (initial prompt)" hint="injected as the first user turn, after asylum context.">
            <textarea className="input mono" value={prompt} onChange={e => setPrompt(e.target.value)} rows={4} style={{ fontSize: 12, lineHeight: 1.5, resize: 'vertical' }} />
          </Field>
        </div>

        <div className="col" style={{ gap: 18 }}>
          <Panel eyebrow="recipes" flush>
            {ASYLUM_DATA.RECIPES.map(r => (
              <div key={r.id} onClick={() => setRecipe(r.id)}
                style={{ padding: '10px 14px', cursor: 'pointer', borderBottom: '1px solid var(--border-subtle)', background: recipe === r.id ? 'var(--bg-elev-2)' : 'transparent' }}>
                <div style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--fg)' }}>{r.name}</div>
                <div style={{ fontFamily: 'var(--font-mono)', fontSize: 10, color: 'var(--fg-muted)', marginTop: 2 }}>{r.sub}</div>
              </div>
            ))}
          </Panel>
          <Panel eyebrow="capabilities at launch">
            <div className="capgrid">
              {ASYLUM_DATA.HARNESSES.find(h => h.id === harness)?.caps.slice(0, 8).map(c => (
                <Fragment key={c}><span className="cap">{c}</span><span className="ok">✓</span></Fragment>
              ))}
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}

// ─── Logs ──────────────────────────────────────────────
function LogsScreen() {
  const [filter, setFilter] = useState('');
  const [lvl, setLvl] = useState('all');
  const filtered = ASYLUM_DATA.LOGS.filter(l => {
    if (filter && !(l.msg.includes(filter) || l.src.includes(filter))) return false;
    if (lvl !== 'all' && l.lvl !== lvl) return false;
    return true;
  });
  return (
    <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minHeight: 0 }}>
      <div className="page" style={{ paddingBottom: 0, flex: 'none' }}>
        <div className="page-head">
          <div>
            <h1 className="page-title">logs &amp; telemetry</h1>
            <div className="page-sub">unified event stream across nodes, substrates, and the asylum service</div>
          </div>
          <div className="page-actions">
            <Btn icon="filter" size="sm">filter</Btn>
            <Btn icon="download" size="sm">export</Btn>
            <Btn icon="play" size="sm">tail live</Btn>
          </div>
        </div>
      </div>
      <div style={{ padding: '0 36px 12px', display: 'flex', gap: 8, alignItems: 'center' }}>
        <div className="search" style={{ flex: '0 1 320px', position: 'relative' }}>
          <Icon name="search" size={12} style={{ position: 'absolute', left: 9, top: '50%', transform: 'translateY(-50%)', opacity: 0.5 }} />
          <input className="input mono" placeholder="filter by source or message…" value={filter} onChange={e => setFilter(e.target.value)} style={{ paddingLeft: 28 }} />
        </div>
        <div style={{ display: 'flex', gap: 4 }}>
          {['all', 'info', 'warn', 'err', 'run', 'dbg'].map(l => (
            <button key={l} className={`btn btn-sm ${lvl === l ? 'btn-secondary' : 'btn-ghost'}`} onClick={() => setLvl(l)}>{l}</button>
          ))}
        </div>
        <span className="muted mono" style={{ marginLeft: 'auto', fontSize: 11 }}>{filtered.length} events</span>
      </div>
      <div className="log" style={{ margin: '0 36px 28px', borderTop: '1px solid var(--border)', border: '1px solid var(--border)' }}>
        <div className="row" style={{ background: 'var(--bg-elev)', borderBottom: '1px solid var(--border)', fontSize: 10, letterSpacing: 0.06 }}>
          <span className="ts" style={{ color: 'var(--fg-muted)' }}>time</span>
          <span className="lvl" style={{ color: 'var(--fg-muted)' }}>level</span>
          <span className="src" style={{ color: 'var(--fg-muted)' }}>source</span>
          <span></span>
          <span className="msg" style={{ color: 'var(--fg-muted)' }}>message</span>
        </div>
        {filtered.map((e, i) => (
          <div className="row" key={i}>
            <span className="ts">{e.ts}</span>
            <span className={`lvl ${e.lvl}`}>{e.lvl}</span>
            <span className="src">{e.src}</span>
            <span></span>
            <span className="msg">{e.msg}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Settings ─────────────────────────────────────────
function SettingsScreen() {
  const [section, setSection] = useState('substrates');
  return (
    <div className="page" style={{ maxWidth: 1120 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">settings</h1>
          <div className="page-sub">single-user · bound to <span className="mono" style={{ color: 'var(--fg)' }}>localhost:5173</span></div>
        </div>
      </div>
      <div className="settings-grid">
        <div className="settings-side">
          <div className="group">connections</div>
          {[['substrates', 'substrates'], ['harnesses', 'harnesses'], ['ntfy', 'ntfy channels']].map(([id, l]) => (
            <div key={id} className={`item ${section === id ? 'active' : ''}`} onClick={() => setSection(id)}>{l}</div>
          ))}
          <div className="group">platform</div>
          {[['auth', 'auth & tokens'], ['network', 'network exposure'], ['storage', 'storage & retention']].map(([id, l]) => (
            <div key={id} className={`item ${section === id ? 'active' : ''}`} onClick={() => setSection(id)}>{l}</div>
          ))}
          <div className="group">developer</div>
          {[['api', 'api & sdk'], ['cli', 'cli'], ['mcp', 'mcp server']].map(([id, l]) => (
            <div key={id} className={`item ${section === id ? 'active' : ''}`} onClick={() => setSection(id)}>{l}</div>
          ))}
        </div>

        <div>
          {section === 'substrates' && (
            <Panel eyebrow="substrates" flush actions={<Btn size="sm" icon="plus">add substrate</Btn>}>
              {ASYLUM_DATA.SUBSTRATES.map(s => (
                <div key={s.id} className="connection-row">
                  <div className="ico">{s.id === 'local' ? '∎' : 'L'}</div>
                  <div>
                    <div className="name">{s.name}</div>
                    <div className="meta">{s.host} · {s.nodes} nodes · cap {Math.round(s.capacity * 100)}%</div>
                    {s.healthy && <div style={{ marginTop: 6, width: 200 }}><div className="health-bar"><div className="fill" style={{ transform: `scaleX(${s.capacity})` }} /></div></div>}
                  </div>
                  <Pill status={s.healthy ? 'running' : 'errored'}>{s.healthy ? 'healthy' : 'unreachable'}</Pill>
                  <Btn size="sm" kind="ghost" icon="more-horizontal" iconOnly />
                </div>
              ))}
            </Panel>
          )}
          {section === 'harnesses' && (
            <Panel eyebrow="harnesses" flush actions={<Btn size="sm" icon="plus">install adapter</Btn>}>
              {ASYLUM_DATA.HARNESSES.map(h => (
                <div key={h.id} className="connection-row" style={{ opacity: h.available ? 1 : 0.55 }}>
                  <div className="ico">{h.name[0].toLowerCase()}</div>
                  <div>
                    <div className="name">{h.name} {!h.available && <Tag future>future</Tag>}</div>
                    <div className="meta">{h.kind} adapter · {h.caps.length} capabilities</div>
                  </div>
                  <Pill status={h.available ? 'running' : 'idle'}>{h.available ? 'installed' : 'not built'}</Pill>
                  <Btn size="sm" kind="ghost" icon="settings" iconOnly />
                </div>
              ))}
            </Panel>
          )}
          {section === 'ntfy' && <NtfySettings />}
          {section === 'auth' && <AuthSettings />}
          {section === 'network' && <NetSettings />}
          {section === 'storage' && <StorageSettings />}
          {section === 'api' && <ApiSettings />}
          {section === 'cli' && <CliSettings />}
          {section === 'mcp' && <McpSettings />}
        </div>
      </div>
    </div>
  );
}

function NtfySettings() {
  return (
    <Panel eyebrow="ntfy channels" flush actions={<Btn size="sm" icon="plus">add channel</Btn>}>
      {[
        ['asylum-aaron', 'ntfy.sh/asylum-aaron-7c2af', '12 sent · 4 received'],
        ['asylum-oncall', 'ntfy.sh/asylum-oncall', '0 sent · 0 received'],
      ].map(([n, t, m]) => (
        <div key={n} className="connection-row">
          <div className="ico">∝</div>
          <div>
            <div className="name">{n}</div>
            <div className="meta">{t} · {m}</div>
          </div>
          <Pill status="running">subscribed</Pill>
          <Btn size="sm" kind="ghost" icon="more-horizontal" iconOnly />
        </div>
      ))}
    </Panel>
  );
}
function AuthSettings() {
  return (
    <Panel eyebrow="tokens">
      <div className="kv">
        <span className="k">owner token</span><span className="v">a8x7…b91 <Btn size="sm" kind="ghost" icon="copy" iconOnly /></span>
        <span className="k">pairing code</span><span className="v">ASLM-2F9D-C014</span>
        <span className="k">issued tokens</span><span className="v">3 active · 0 revoked today</span>
        <span className="k">attach urls</span><span className="v">2 active · ttl 3600s</span>
      </div>
      <div className="hr" />
      <Btn icon="rotate-ccw" size="sm">rotate owner token</Btn>
    </Panel>
  );
}
function NetSettings() {
  return (
    <Panel eyebrow="network exposure">
      <div className="kv">
        <span className="k">bind</span><span className="v">localhost:5173</span>
        <span className="k">remote access</span><span className="v">tailscale (recommended)</span>
        <span className="k">reverse proxy</span><span className="v">none configured</span>
      </div>
      <div style={{ marginTop: 14, padding: 12, border: '1px solid rgba(245,180,84,0.35)', background: 'var(--status-waiting-bg)', fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--status-waiting)' }}>
        ⚠ exposing asylum beyond localhost reveals attach urls and node transcripts. require pairing + tailscale.
      </div>
    </Panel>
  );
}
function StorageSettings() {
  return (
    <Panel eyebrow="storage & retention">
      <div className="kv">
        <span className="k">transcripts</span><span className="v">~/Library/Asylum/transcripts · 1.4 GB</span>
        <span className="k">retention</span><span className="v">30 days (rolling)</span>
        <span className="k">redaction</span><span className="v">on (api keys, jwt-like)</span>
      </div>
    </Panel>
  );
}
function ApiSettings() {
  return (
    <Panel eyebrow="api & sdk">
      <div className="kv">
        <span className="k">base url</span><span className="v">https://localhost:5173/api/v1</span>
        <span className="k">openapi</span><span className="v">/openapi.json (37 endpoints)</span>
        <span className="k">sdk</span><span className="v">@asylum/sdk@0.1.0 (typescript)</span>
      </div>
      <div className="hr" />
      <div className="muted mono" style={{ fontSize: 11, marginBottom: 6 }}>quickstart</div>
      <pre style={{ background: 'var(--bg-sunken)', padding: 12, fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--fg)', border: '1px solid var(--border-subtle)', overflow: 'auto', margin: 0 }}>{`import { Asylum } from "@asylum/sdk";
const a = new Asylum({ baseUrl, token });
const node = await a.node.create({ harness: "codex", substrate: "loon-us-west", role: "worker" });
for await (const ev of a.node.observe(node.id)) console.log(ev);`}</pre>
    </Panel>
  );
}
function CliSettings() {
  return (
    <Panel eyebrow="cli">
      <pre style={{ background: 'var(--bg-sunken)', padding: 12, fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--fg)', border: '1px solid var(--border-subtle)', overflow: 'auto', margin: 0 }}>{`$ asylum nodes
NODE        ROLE              HARNESS       SUBSTRATE       STATE
cc-7c2af    command-center    codex         local           running
sup-3d1e    supervisor        claude-code   loon-us-west    running
…
$ asylum node send w-2b0c8 "approve"
$ asylum attach w-9a4f1 --browser`}</pre>
    </Panel>
  );
}
function McpSettings() {
  return (
    <Panel eyebrow="mcp server">
      <div className="kv">
        <span className="k">endpoint</span><span className="v">stdio · asylum-mcp</span>
        <span className="k">tools exposed</span><span className="v">37 (graph.get, node.create, node.send_input, …)</span>
        <span className="k">connected clients</span><span className="v">claude desktop, cursor</span>
      </div>
    </Panel>
  );
}

// ─── Chat screen ──────────────────────────────────────
// every node IS a session. the chat screen is just a fullscreen viewport into one.
function ChatScreen({ tweaks, nodes, chatNodeId, onSelectChat, onSpawn, simSpeed, onAction }) {
  // group nodes for the rail: command-center first, then by parent
  const cc = nodes.find(n => n.isCommandCenter);
  const supervisors = nodes.filter(n => n.role === 'supervisor');
  const others = nodes.filter(n => !n.isCommandCenter && n.role !== 'supervisor');
  const active = nodes.find(n => n.id === chatNodeId) || cc || nodes[0];

  return (
    <div className="chat-screen">
      <div className="chat-rail">
        <div className="rail-head">
          <div className="title">nodes</div>
          <div className="sub">every node is a live tui session</div>
        </div>

        <RailGroup label="command center">
          {cc && <RailItem node={cc} active={active?.id === cc.id} onClick={() => onSelectChat(cc.id)} />}
        </RailGroup>

        {supervisors.length > 0 && (
          <RailGroup label="supervisors">
            {supervisors.map(n => <RailItem key={n.id} node={n} active={active?.id === n.id} onClick={() => onSelectChat(n.id)} />)}
          </RailGroup>
        )}

        {others.length > 0 && (
          <RailGroup label="workers · evaluators · assistants">
            {others.map(n => <RailItem key={n.id} node={n} active={active?.id === n.id} onClick={() => onSelectChat(n.id)} />)}
          </RailGroup>
        )}

        <div style={{ marginTop: 'auto', padding: 12, borderTop: '1px solid var(--border-subtle)' }}>
          <Btn size="sm" icon="plus" style={{ width: '100%', justifyContent: 'flex-start' }}>new node</Btn>
          <div className="rail-hint">
            <div>· chat = live tui session</div>
            <div>· same session as cockpit panel</div>
            <div>· press ⌘k to jump nodes</div>
          </div>
        </div>
      </div>
      <div className="chat-stage">
        {active ? (
          <NodeSession key={active.id} node={active} mode="fullscreen"
            simSpeed={simSpeed} onSpawn={onSpawn} onAction={active.isCommandCenter ? onAction : undefined} />
        ) : (
          <Empty glyph="⌬" lead="no nodes" sub="launch a command center to start" />
        )}
      </div>
    </div>
  );
}

function RailGroup({ label, children }) {
  return (
    <div className="rail-group">
      <div className="lab">{label}</div>
      {children}
    </div>
  );
}

function RailItem({ node, active, onClick }) {
  const harnessShort = node.harness === 'claude-code' ? 'claude' : 'codex';
  return (
    <div className={`rail-item ${active ? 'on' : ''} st-${node.state}`} onClick={onClick}>
      <span className="g" aria-hidden>{ROLE_GLYPH[node.role] || '·'}</span>
      <span className="id">{node.id}</span>
      <span className="meta">{harnessShort} · {node.substrate.replace('loon-', 'l/')}</span>
      <span className={`dot st-${node.state}`} />
    </div>
  );
}

// ─── Channels (messaging) ─────────────────────────────
// every way asylum reaches you when you're away, and every way commands come back in.
function ChannelsScreen() {
  const [activeId, setActiveId] = useState('ntfy-aaron');
  const [filter, setFilter] = useState('all');
  const channels = ASYLUM_DATA.CHANNELS;
  const active = channels.find(c => c.id === activeId);
  const allMsgs = ASYLUM_DATA.CHANNEL_MESSAGES.filter(m => m.channel === activeId);
  const msgs = filter === 'all' ? allMsgs : allMsgs.filter(m => m.dir === filter);

  const liveCount = channels.filter(c => c.live).length;
  const futureCount = channels.length - liveCount;
  const total24h = channels.reduce((s, c) => s + (c.msg24h || 0), 0);

  return (
    <div className="page channels-page">
      <div className="page-head">
        <div>
          <h1 className="page-title">channels</h1>
          <div className="page-sub">how nodes reach you when you're away · how commands come back in · {liveCount} live, {futureCount} planned · {total24h} msgs / 24h</div>
        </div>
        <div className="page-actions">
          <Btn icon="rss">subscribe…</Btn>
          <Btn kind="primary" icon="plus">new channel</Btn>
        </div>
      </div>

      <div className="channels-layout">
        <div className="channels-list">
          <div className="ch-group">
            <div className="ch-group-lab">live</div>
            {channels.filter(c => c.live).map(c => (
              <ChannelRow key={c.id} ch={c} active={activeId === c.id} onClick={() => setActiveId(c.id)} />
            ))}
          </div>
          <div className="ch-group">
            <div className="ch-group-lab">planned · adapters not built</div>
            {channels.filter(c => !c.live).map(c => (
              <ChannelRow key={c.id} ch={c} active={activeId === c.id} onClick={() => setActiveId(c.id)} />
            ))}
          </div>
        </div>

        <div className="channels-detail">
          {active && <ChannelDetail ch={active} msgs={msgs} filter={filter} setFilter={setFilter} />}
        </div>
      </div>
    </div>
  );
}

function ChannelRow({ ch, active, onClick }) {
  const glyph = {
    ntfy: '◉', webhook: '⇄', sms: '✉', discord: '◈', slack: '◇', email: '✦',
  }[ch.kind] || '·';
  return (
    <div className={`ch-row ${active ? 'on' : ''} ${ch.live ? '' : 'future'}`} onClick={onClick}>
      <span className="g">{glyph}</span>
      <div className="m">
        <div className="r1">
          <span className="nm">{ch.name}</span>
          {!ch.live && <span className="badge-future">future</span>}
        </div>
        <div className="r2">{ch.label}</div>
      </div>
      <div className="r">
        {ch.live ? (
          <>
            <div className="ct">{ch.msg24h ?? 0}</div>
            <div className="lab">/24h</div>
          </>
        ) : (
          <span className="dot future" />
        )}
      </div>
    </div>
  );
}

function ChannelDetail({ ch, msgs, filter, setFilter }) {
  const stat = ch.live ? 'connected' : 'not built';
  return (
    <div className="ch-detail">
      <div className="ch-head">
        <div className="left">
          <div className="ttl">{ch.name}</div>
          <div className="sub">{ch.detail}</div>
        </div>
        <div className="right">
          <Pill status={ch.live ? 'running' : 'idle'}>{stat}</Pill>
          <Btn size="sm" icon="external-link" iconOnly title="open in new tab" disabled={!ch.live} />
          <Btn size="sm" icon="settings" iconOnly title="channel settings" disabled={!ch.live} />
        </div>
      </div>

      <div className="ch-stats">
        <Stat lab="direction" v={ch.direction} />
        <Stat lab="msgs / 24h" v={ch.msg24h ?? '—'} />
        <Stat lab="last activity" v={ch.lastAt || '—'} />
        <Stat lab="subscribers" v={ch.subscribers ?? '—'} />
      </div>

      <div className="ch-toolbar">
        <div className="filt">
          <span className="lab">filter</span>
          {[['all','all'], ['out','out'], ['in','in']].map(([v, l]) => (
            <button key={v} className={`chip ${filter === v ? 'on' : ''}`} onClick={() => setFilter(v)}>{l}</button>
          ))}
        </div>
        <div className="acts">
          <Btn size="sm" icon="send" disabled={!ch.live}>send test</Btn>
          <Btn size="sm" icon="copy" disabled={!ch.live}>copy url</Btn>
        </div>
      </div>

      {ch.live ? (
        <div className="ch-msgs">
          {msgs.length === 0 ? (
            <Empty glyph="◌" lead="no messages with this filter" sub="try `all`" />
          ) : msgs.map((m, i) => (
            <div key={i} className={`msg ${m.dir}`}>
              <span className="ts">{m.ts}</span>
              <span className={`arr ${m.dir}`}>{m.dir === 'out' ? '→' : '←'}</span>
              <div className="b">
                <div className="r1">
                  <span className="from">{m.from}</span>
                  <span className="sep">·</span>
                  <span className="subj">{m.subject}</span>
                </div>
                <div className="r2">{m.body}</div>
                {m.replies && (
                  <div className="r3">
                    <span className="lab">quick replies:</span>
                    {m.replies.map(r => <span key={r} className="reply-chip">{r}</span>)}
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="ch-future">
          <div className="g">⌖</div>
          <div className="t">adapter not built</div>
          <div className="d">{ch.detail}</div>
          <div className="row" style={{ marginTop: 16 }}>
            <Btn size="sm" icon="git-pull-request">view spec</Btn>
            <Btn size="sm" kind="ghost" icon="thumbs-up">upvote</Btn>
          </div>
        </div>
      )}
    </div>
  );
}

function Stat({ lab, v }) {
  return (
    <div className="stat">
      <div className="l">{lab}</div>
      <div className="v">{v}</div>
    </div>
  );
}

// ─── Hooks (event-driven automation) ──────────────────
// declarative if-this-then-that. asylum's automation surface.
function HooksScreen() {
  const [hooks, setHooks] = useState(ASYLUM_DATA.HOOKS);
  const [tab, setTab] = useState('rules');
  const [drawer, setDrawer] = useState(null);  // hook id being edited (null = none)
  const enabled = hooks.filter(h => h.enabled).length;

  function toggle(id) {
    setHooks(hs => hs.map(h => h.id === id ? { ...h, enabled: !h.enabled } : h));
  }

  return (
    <div className="page hooks-page">
      <div className="page-head">
        <div>
          <h1 className="page-title">hooks</h1>
          <div className="page-sub">if-this-then-that for the fleet · {enabled}/{hooks.length} enabled · {ASYLUM_DATA.HOOK_FIRINGS.length} firings / 24h</div>
        </div>
        <div className="page-actions">
          <Btn icon="upload">import</Btn>
          <Btn kind="primary" icon="plus" onClick={() => setDrawer('__new')}>new hook</Btn>
        </div>
      </div>

      <div className="hooks-tabs">
        {[['rules', 'rules', hooks.length], ['firings', 'recent firings', ASYLUM_DATA.HOOK_FIRINGS.length], ['catalog', 'event catalog', ASYLUM_DATA.EVENT_CATALOG.length]].map(([id, lab, ct]) => (
          <div key={id} className={`tab ${tab === id ? 'on' : ''}`} onClick={() => setTab(id)}>
            {lab} <span className="ct">{ct}</span>
          </div>
        ))}
      </div>

      {tab === 'rules' && (
        <div className="hooks-grid">
          {hooks.map(h => <HookCard key={h.id} hook={h} onToggle={() => toggle(h.id)} onEdit={() => setDrawer(h.id)} />)}
        </div>
      )}

      {tab === 'firings' && (
        <div className="firings-list">
          <div className="firings-head">
            <span>time</span>
            <span>hook</span>
            <span>trigger</span>
            <span>outcome</span>
            <span></span>
          </div>
          {ASYLUM_DATA.HOOK_FIRINGS.map((f, i) => {
            const hk = hooks.find(h => h.id === f.hook);
            return (
              <div key={i} className="firing-row">
                <span className="ts">{f.ts}</span>
                <span className="hk">{hk?.name || f.hook}</span>
                <span className="tr"><code>{f.trigger}</code></span>
                <span className="oc">{f.outcome}</span>
                <span className="st">{f.ok ? <Pill status="running">ok</Pill> : <Pill status="errored">err</Pill>}</span>
              </div>
            );
          })}
        </div>
      )}

      {tab === 'catalog' && (
        <div className="catalog-grid">
          {ASYLUM_DATA.EVENT_CATALOG.map(e => (
            <div key={e.id} className="cat-card">
              <div className="id"><code>{e.id}</code></div>
              <div className="lab">{e.label}</div>
              <Btn size="sm" kind="ghost" icon="plus" onClick={() => setDrawer('__new')}>new hook</Btn>
            </div>
          ))}
        </div>
      )}

      {drawer && <HookEditor hookId={drawer} onClose={() => setDrawer(null)} />}
    </div>
  );
}

function HookCard({ hook, onToggle, onEdit }) {
  return (
    <div className={`hook-card ${hook.enabled ? '' : 'off'} ${hook.future ? 'future' : ''}`}>
      <div className="hd">
        <span className={`led ${hook.enabled ? 'on' : 'off'}`} />
        <span className="nm">{hook.name}</span>
        <span className="tog">
          <button className={`toggle ${hook.enabled ? 'on' : ''}`} onClick={onToggle} title={hook.enabled ? 'disable' : 'enable'}>
            <span className="knob" />
          </button>
        </span>
      </div>

      <div className="when">
        <span className="lab">when</span>
        <code className="evt">{hook.event}</code>
        {hook.filter && hook.filter !== 'any' && (
          <>
            <span className="lab">where</span>
            <code className="filt">{hook.filter}</code>
          </>
        )}
      </div>

      <div className="then">
        <span className="lab">then</span>
        <ol>
          {hook.actions.map((a, i) => (
            <li key={i}>
              <span className="step">{i + 1}</span>
              <span className="kind">{a.kind}</span>
              <code className="trg">{a.target}</code>
              {a.template && <span className="tpl">{`"${a.template}"`}</span>}
            </li>
          ))}
        </ol>
      </div>

      <div className="ft">
        <span className="stat">
          <span className="lab">fired</span> <b>{hook.fired24h}</b><span className="lab">/24h</span>
        </span>
        <span className="stat">
          <span className="lab">last</span> <b>{hook.lastAt}</b>
        </span>
        <span className="stat r">
          <Btn size="sm" kind="ghost" icon="play" iconOnly title="dry-run" />
          <Btn size="sm" kind="ghost" icon="edit-2" iconOnly title="edit" onClick={onEdit} />
          <Btn size="sm" kind="ghost" icon="more-horizontal" iconOnly />
        </span>
      </div>
    </div>
  );
}

function HookEditor({ hookId, onClose }) {
  const isNew = hookId === '__new';
  const hook = isNew ? null : ASYLUM_DATA.HOOKS.find(h => h.id === hookId);
  return (
    <Modal title={isNew ? 'new hook' : `edit · ${hook?.name}`} onClose={onClose} width={620}
      foot={<>
        <Btn onClick={onClose}>cancel</Btn>
        <Btn kind="primary" icon="save" onClick={onClose}>{isNew ? 'create hook' : 'save'}</Btn>
      </>}>
      <Field label="name" hint="short, descriptive — shows up in firings log">
        <input className="input" defaultValue={hook?.name || ''} placeholder="e.g. high-context → checkpoint" />
      </Field>
      <Field label="when (event)" hint="pick a trigger from the event catalog">
        <select className="input" defaultValue={hook?.event || 'node.permission_requested'}>
          {ASYLUM_DATA.EVENT_CATALOG.map(e => <option key={e.id} value={e.id}>{e.id} — {e.label}</option>)}
        </select>
      </Field>
      <Field label="where (filter)" hint="optional — runs against event payload (jmespath-like)">
        <input className="input mono" defaultValue={hook?.filter || ''} placeholder='e.g. role == "worker" && ctx >= 0.8' />
      </Field>
      <Field label="then (actions)" hint="executed in order, halts on failure unless `try` is set">
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {(hook?.actions || [{ kind: 'channel', target: 'ntfy-aaron', template: '{node.id} triggered' }]).map((a, i) => (
            <div key={i} className="action-row">
              <span className="step">{i + 1}</span>
              <select className="input mono" defaultValue={a.kind} style={{ width: 120 }}>
                <option>channel</option><option>spawn</option><option>tool</option><option>pause_node</option><option>archive</option>
              </select>
              <input className="input mono" defaultValue={a.target} style={{ flex: 1 }} />
              <Btn size="sm" kind="ghost" icon="x" iconOnly />
            </div>
          ))}
          <div style={{ alignSelf: 'flex-start' }}><Btn size="sm" kind="ghost" icon="plus">add action</Btn></div>
        </div>
      </Field>
      <div className="muted mono" style={{ fontSize: 11, marginTop: 12, padding: 10, background: 'var(--bg-sunken)', border: '1px solid var(--border-subtle)' }}>
        <span className="b" style={{ color: 'var(--fg)' }}>preview</span> · this hook will fire when{' '}
        <code style={{ color: 'var(--fg)' }}>{hook?.event || 'node.permission_requested'}</code>
        {hook?.filter && hook.filter !== 'any' && <> and <code style={{ color: 'var(--fg)' }}>{hook.filter}</code></>}
        , then run {hook?.actions?.length || 1} action(s).
      </div>
    </Modal>
  );
}

Object.assign(window, { FleetScreen, NodeScreen, CreateScreen, LogsScreen, SettingsScreen, ChatScreen, ChannelsScreen, HooksScreen });
