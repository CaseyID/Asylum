// asylum cockpit — NodeSession: the one terminal/chat primitive.
// every "chat" surface in the app is a viewport onto this. modes only change
// the chrome (compact vs full); the harness-authentic TUI inside is the same.

// canned harness reply sequences (cc gets orchestration, workers get direct-input replies)
const CC_RESPONSES = {
  default: [
    { kind: 'thought', text: 'thinking…', delay: 200 },
    { kind: 'text', text: 'i see the asylum context. 8 nodes registered (4 running, 1 waiting, 1 errored). what would you like me to do?' },
  ],
  spawn: [
    { kind: 'thought', text: 'planning fan-out · 1 supervisor + 2 workers on loon-us-west', delay: 300 },
    { kind: 'tool', name: 'substrate.inspect', args: { id: 'loon-us-west' }, output: 'host loon.iad1.fc · capacity 0.61 · vms 4/8\nharnesses: codex, claude-code · rtt 38ms', state: 'ok' },
    { kind: 'tool', name: 'node.create', args: { harness: 'claude-code', role: 'supervisor', substrate: 'loon-us-west' }, output: 'created sup-2f9d (claude-code · loon-us-west)\nattach: https://localhost:5173/attach/sup-2f9d?t=…b91', state: 'ok', spawn: { id: 'sup-2f9d', role: 'supervisor', harness: 'claude-code', substrate: 'loon-us-west', parent: 'cc-7c2af' } },
    { kind: 'tool', name: 'node.create', args: { harness: 'codex', role: 'worker', substrate: 'loon-us-west', parent: 'sup-2f9d' }, output: 'created w-c014 (codex · loon-us-west)', state: 'ok', spawn: { id: 'w-c014', role: 'worker', harness: 'codex', substrate: 'loon-us-west', parent: 'sup-2f9d' } },
    { kind: 'tool', name: 'node.create', args: { harness: 'codex', role: 'worker', substrate: 'loon-us-west', parent: 'sup-2f9d' }, output: 'created w-c015 (codex · loon-us-west)', state: 'ok', spawn: { id: 'w-c015', role: 'worker', harness: 'codex', substrate: 'loon-us-west', parent: 'sup-2f9d' } },
    { kind: 'text', text: 'spawned 1 supervisor and 2 workers on loon-us-west. they are coming up — you should see them in the graph.' },
  ],
  status: [
    { kind: 'tool', name: 'graph.get', args: { scope: 'fleet' }, output: 'returned 8 nodes, 6 edges', state: 'ok' },
    { kind: 'text', text: 'fleet right now:' },
    { kind: 'list', items: [
      '4 running, 1 waiting (w-2b0c8 · permission), 1 errored (w-4e7b · oom on loon-us-east), 2 idle',
      'longest run: w-4e7b · 1h 12m before exit',
      'highest context: cc-7c2af at 72%',
    ]},
  ],
  attach: [
    { kind: 'tool', name: 'node.attach.browser', args: { node: 'w-9a4f1' }, output: 'issued attach url\ntoken ttl 3600s · renders tui', state: 'ok' },
    { kind: 'attach', node: 'w-9a4f1', url: 'https://cockpit.local/attach/w-9a4f1?t=…8e2' },
  ],
  attention: [
    { kind: 'text', text: 'two things want your attention:' },
    { kind: 'list', items: [
      'w-2b0c8 — permission to write package.json (waiting 2m)',
      "w-4e7b — oom'd on loon-us-east. retry on loon-us-west?",
    ]},
    { kind: 'text', text: 'reply with `approve w-2b0c8` or `retry w-4e7b`.' },
  ],
};

const WORKER_RESPONSES = {
  default: [
    { kind: 'thought', text: 'reading your message…', delay: 180 },
    { kind: 'text', text: "got it. continuing on the current task — i'll surface anything that needs you." },
  ],
  status: [
    { kind: 'text', text: 'current state:' },
    { kind: 'list', items: ['working in ~/work/refactor-router', 'last tool: apply_patch src/router/match.ts (+114)', 'tests passing · 12/12', 'ctx 28% · 4018 tokens out'] },
  ],
  pause: [
    { kind: 'text', text: 'pausing after the current tool call. resume with anything you type next.' },
  ],
};

function _intent(s) {
  const q = s.toLowerCase();
  if (/spawn|launch|fan|workers?\b/.test(q)) return 'spawn';
  if (/status|what.*happen|fleet|state|doing/.test(q)) return 'status';
  if (/attach/.test(q)) return 'attach';
  if (/attention|need|wait|stuck/.test(q)) return 'attention';
  if (/pause|stop|hold/.test(q)) return 'pause';
  return 'default';
}

function _sleep(ms) { return ms ? new Promise(r => setTimeout(r, ms)) : Promise.resolve(); }

function _initialTranscript(node) {
  const isCC = !!node?.isCommandCenter;
  const harnessId = node?.harness === 'claude-code' ? 'claude-code' : 'codex';
  if (!isCC) {
    return [
      { kind: 'sys-line', text: `attached to ${node.id} · ${harnessId} · ${node.substrate} · workspace ${node.workspace}` },
      { kind: 'sys-line', text: `role ${node.role} · uptime ${node.duration} · ctx ${Math.round((node.ctx||0)*100)}%` },
      { kind: 'tool', name: 'node.observe', args: { node: node.id, tail: 6 }, output: (node.preview || '— no recent output').slice(0, 240), state: 'ok' },
      { kind: 'text', text: `you are now in this node's live ${harnessId} session. anything you type goes to its harness as input. (try: status, what are you working on, pause)` },
      { kind: 'prompt' },
    ];
  }
  if (harnessId === 'claude-code') {
    return [
      { kind: 'sys-line', text: 'workspace ~/src/asylum · 47 tools available · supervisor recipe loaded' },
      { kind: 'tool', name: 'context.current_system_map', args: { scope: 'asylum' }, output: '8 nodes registered\n4 substrates configured (1 unhealthy: loon-eu)\n2 harness adapters: codex, claude-code\n3 ntfy channels active', state: 'ok' },
      { kind: 'text', text: 'asylum context loaded. 8 nodes are alive across 3 substrates. one node (w-2b0c8) is waiting on a permission decision; one (w-4e7b) errored. ready for instructions — try:' },
      { kind: 'list', items: ['"spawn 2 workers to finish the router refactor"', '"what needs my attention?"', '"attach to w-9a4f1"'] },
      { kind: 'prompt' },
    ];
  }
  return [
    { kind: 'thought', text: 'launched in workspace ~/src/asylum · context window 200k · 47 tools · supervisor recipe loaded' },
    { kind: 'tool', name: 'context.current_system_map', args: { scope: 'asylum' }, output: '8 nodes registered\n4 substrates configured (1 unhealthy: loon-eu)\n2 harness adapters: codex, claude-code\n3 ntfy channels active', state: 'ok' },
    { kind: 'text', text: 'asylum context loaded. 8 nodes are alive across 3 substrates. one node (w-2b0c8) is waiting on a permission decision; one (w-4e7b) errored. ready for instructions — try:' },
    { kind: 'list', items: ['"spawn 2 workers to finish the router refactor"', '"what needs my attention?"', '"attach to w-9a4f1"'] },
    { kind: 'prompt' },
  ];
}

// ─── public component ────────────────────────────────────
function NodeSession(props) {
  const node = props.node;
  const mode = props.mode || 'cockpit';
  const onSpawn = props.onSpawn;
  const onAttach = props.onAttach;
  const onAction = props.onAction;
  const onExpand = props.onExpand;
  const simSpeed = props.simSpeed || 'slow';

  const [entries, setEntries] = React.useState(() => _initialTranscript(node));
  const [input, setInput] = React.useState('');
  const [streaming, setStreaming] = React.useState(false);
  const [view, setView] = React.useState('tui');
  const termRef = React.useRef(null);

  React.useEffect(() => {
    if (termRef.current) termRef.current.scrollTop = termRef.current.scrollHeight;
  }, [entries]);

  // expose methods to outside callers (inspector buttons)
  React.useEffect(() => {
    if (!onAction) return;
    onAction.current = {
      pushSystem: (text) => setEntries(prev => [...prev, { kind: 'sys-line', text }]),
      pushTool:   (name, args, output, state = 'ok') => setEntries(prev => [...prev, { kind: 'tool', name, args, output, state }]),
      pushUser:   (text) => setEntries(prev => [...prev, { kind: 'user', text }]),
      runResponse,
    };
  });

  const speedMul = simSpeed === 'still' ? 0 : simSpeed === 'slow' ? 1.6 : 0.6;

  async function runResponse(seq) {
    setStreaming(true);
    for (const step of seq) {
      await _sleep((step.delay || 200) * speedMul);
      if (step.kind === 'thought') {
        setEntries(p => [...p, { kind: 'thought', text: step.text }]);
      } else if (step.kind === 'tool') {
        setEntries(p => [...p, { kind: 'tool', name: step.name, args: step.args, output: step.output, state: step.state || 'ok' }]);
        if (step.spawn && onSpawn) onSpawn(step.spawn);
      } else if (step.kind === 'text') {
        await streamText(step.text);
      } else if (step.kind === 'list') {
        setEntries(p => [...p, { kind: 'list', items: step.items }]);
      } else if (step.kind === 'attach') {
        setEntries(p => [...p, { kind: 'attach', node: step.node, url: step.url }]);
        if (onAttach) onAttach(step.node);
      }
    }
    setEntries(p => [...p, { kind: 'prompt' }]);
    setStreaming(false);
  }

  async function streamText(full) {
    const id = Math.random().toString(36).slice(2, 8);
    setEntries(p => [...p, { kind: 'text', id, text: '' }]);
    if (simSpeed === 'still') {
      setEntries(p => p.map(e => e.id === id ? { ...e, text: full } : e));
      return;
    }
    let cur = '';
    const tokens = full.split(/(\s+)/);
    for (const tok of tokens) {
      cur += tok;
      const t = cur;
      setEntries(p => p.map(e => e.id === id ? { ...e, text: t } : e));
      await _sleep((22 + Math.random() * 30) * speedMul);
    }
  }

  function submit() {
    const v = input.trim();
    if (!v || streaming) return;
    setInput('');
    setEntries(p => [...p, { kind: 'user', text: v }]);
    const responses = node.isCommandCenter ? CC_RESPONSES : WORKER_RESPONSES;
    runResponse(responses[_intent(v)] || responses.default);
  }

  const harnessId = node.harness === 'claude-code' ? 'claude-code' : 'codex';
  const last = entries[entries.length - 1];

  return (
    <div className={`session session-${mode} harness-${harnessId}`} data-screen-label={`session-${node.id}`}>
      <SessionHeader node={node} mode={mode} view={view} setView={setView} onExpand={onExpand} />
      <SessionBanner node={node} harnessId={harnessId} />
      <div className="session-body" ref={termRef}>
        {entries.map((e, i) => (
          <TermEntry key={i} e={e} harness={harnessId} view={view}
            streaming={streaming && i === entries.length - 1} />
        ))}
        {!streaming && last && last.kind === 'prompt' && <PromptLine harness={harnessId} />}
      </div>
      <SessionInput node={node} harnessId={harnessId} value={input} streaming={streaming}
        onChange={setInput} onSubmit={submit} />
    </div>
  );
}

// ─── header ──────────────────────────────────────────────
function SessionHeader(props) {
  const { node, mode, view, setView, onExpand } = props;
  const isCC = node.isCommandCenter;
  const harnessLabel = node.harness === 'claude-code' ? 'claude code' : (node.harness || 'codex');
  return (
    <div className="session-head">
      <span className="hglyph">{node.harness === 'claude-code' ? '◆' : '›_'}</span>
      <span className="hid">{node.id}</span>
      <span className="hsep">·</span>
      <span className="hharn">{harnessLabel}</span>
      <span className="hsep">·</span>
      <span className="hsubs">{node.substrate}</span>
      {node.role && <span className="hsep">·</span>}
      {node.role && <span className="hrole">{node.role}</span>}
      {isCC && <span className="hbadge" title="this node is the command center">cc</span>}
      <Pill status={node.state || 'running'}>{node.state || 'running'}</Pill>

      <span className="hright">
        <div className="view-toggle" role="tablist" title="transcript rendering">
          <button className={view === 'tui' ? 'on' : ''} onClick={() => setView('tui')} title="raw tui replay">tui</button>
          <button className={view === 'structured' ? 'on' : ''} onClick={() => setView('structured')} title="structured / semantic">struct</button>
        </div>
        <Btn kind="ghost" size="sm" icon="external-link" iconOnly title="attach in browser" />
        <Btn kind="ghost" size="sm" icon="terminal" iconOnly title="native attach" />
        <Btn kind="ghost" size="sm" icon="square" iconOnly title="interrupt (ctrl-c)" />
        {mode === 'cockpit' && onExpand && (
          <Btn kind="ghost" size="sm" icon="maximize-2" iconOnly title="open in chat" onClick={onExpand} />
        )}
        <Btn kind="ghost" size="sm" icon="more-horizontal" iconOnly />
      </span>
    </div>
  );
}

// banner — codex (minimal hr) vs claude-code (ASCII box)
function SessionBanner(props) {
  const { node, harnessId } = props;
  const ctxPct = Math.round((node.ctx || 0) * 100);
  const subline = `${node.workspace || '~/'} · ctx ${ctxPct}% · uptime ${node.duration || '—'}`;
  const dashes = '─'.repeat(46);

  if (harnessId === 'claude-code') {
    return (
      <div className="banner cc-claude">
        <div className="row top">{`╭${dashes}╮`}</div>
        <div className="row mid"><span>│ </span><b>Claude Code</b><span> · session </span><span className="m">{node.id}</span></div>
        <div className="row mid"><span>│ </span><span className="m">{subline}</span></div>
        <div className="row bot">{`╰${dashes}╯`}</div>
      </div>
    );
  }
  return (
    <div className="banner cc-codex">
      <div className="hr" />
      <div className="row">
        <span className="b">codex</span>
        <span className="sep">·</span>
        <span className="m">connected to {node.id}</span>
        <span className="m" style={{ marginLeft: 'auto' }}>{subline}</span>
      </div>
      <div className="hr" />
    </div>
  );
}

// ─── entries ────────────────────────────────────────────
function TermEntry(props) {
  const { e, harness, view, streaming } = props;
  if (e.kind === 'user') {
    return (
      <div className="line line-user">
        <span className="g">{harness === 'claude-code' ? '>' : '$'}</span>
        <span className="t">{e.text}</span>
      </div>
    );
  }
  if (e.kind === 'thought') {
    if (harness === 'claude-code') return <div className="line line-thought claude">✻ <i>{e.text}</i></div>;
    return <div className="line line-thought codex">· <i>{e.text}</i></div>;
  }
  if (e.kind === 'text') {
    return (
      <div className="line line-text">
        {e.text}
        {streaming && <span className="caret" />}
      </div>
    );
  }
  if (e.kind === 'list') {
    return (
      <ul className="line line-list">
        {e.items.map((it, i) => (
          <li key={i}>
            <span className="bul">{harness === 'claude-code' ? '⏺' : '·'}</span>
            <span>{it}</span>
          </li>
        ))}
      </ul>
    );
  }
  if (e.kind === 'tool') {
    if (view === 'tui') return <ToolCallTUI name={e.name} args={e.args} output={e.output} state={e.state} harness={harness} />;
    return <ToolCall name={e.name} args={e.args} output={e.output} state={e.state} collapsed />;
  }
  if (e.kind === 'attach') {
    return (
      <div className="attach-preview">
        <div className="h">
          <Icon name="external-link" size={11} />
          <span>browser attach: <span style={{ color: 'var(--fg)' }}>{e.node}</span></span>
          <span className="right" style={{ marginLeft: 'auto', color: 'var(--fg-subtle)' }}>token ttl 3600s</span>
        </div>
        <div className="body">
          <span className="muted">{'>'}</span> <span className="b">refactor:router</span> $ <span className="x">npm test</span>{'\n'}
          <span className="muted">PASS</span> src/router/match.test.ts (4 tests, 12ms){'\n'}
          <span className="muted">PASS</span> src/router/parse.test.ts (8 tests, 22ms){'\n'}
          <span className="muted">$</span> <span className="b">applying patch</span>: src/router/match.ts +114 -0{'\n'}
          <span className="muted">$</span> <span className="b">streaming output</span>… 412 tokens at ctx 41%{'\n'}
        </div>
        <div className="foot">
          <Btn size="sm" kind="primary">open ↗</Btn>
          <Btn size="sm" kind="secondary" icon="copy">copy url</Btn>
          <span className="muted mono" style={{ fontSize: 10, marginLeft: 'auto' }}>{e.url}</span>
        </div>
      </div>
    );
  }
  if (e.kind === 'sys-line') return <div className="line line-sys">· {e.text}</div>;
  if (e.kind === 'prompt') return null;
  return null;
}

function ToolCallTUI(props) {
  const { name, args, output, state, harness } = props;
  const argStr = args ? Object.entries(args).map(([k, v]) => `${k}=${typeof v === 'string' ? v : JSON.stringify(v)}`).join(' ') : '';
  const lines = (output || '').split('\n').slice(0, 8);

  if (harness === 'claude-code') {
    return (
      <div className="tui-tool claude">
        <div className="hd">⏺ <b>{name}</b>{argStr ? <span className="m"> ({argStr})</span> : null}</div>
        {lines.map((l, i) => <div key={i} className="ln">  ⎿  <span className="m">{l}</span></div>)}
      </div>
    );
  }
  return (
    <div className="tui-tool codex">
      <div className="hd"><span className="g">tool</span> {name} <span className="m">{argStr}</span> <span className={`st ${state}`}>· {state}</span></div>
      {lines.map((l, i) => <div key={i} className="ln">  {l}</div>)}
    </div>
  );
}

function PromptLine(props) {
  if (props.harness === 'claude-code') {
    return <div className="prompt-line claude"><span className="g">{'>'}</span> <span className="caret" /></div>;
  }
  return <div className="prompt-line codex"><span className="g">{'›'}</span><span className="caret" /></div>;
}

// ─── input ──────────────────────────────────────────────
function SessionInput(props) {
  const { node, harnessId, value, streaming, onChange, onSubmit } = props;
  const isCC = node.isCommandCenter;
  const placeholder = streaming
    ? 'streaming… (esc to interrupt)'
    : (isCC
        ? `send to ${node.id} · try: spawn 2 workers, status, attach to w-9a4f1`
        : `send input to ${node.id} · this writes directly to its harness stdin`);
  return (
    <div className={`session-input harness-${harnessId}`}>
      <span className="g">{harnessId === 'claude-code' ? '>' : '›'}</span>
      <input
        placeholder={placeholder}
        value={value}
        onChange={e => onChange(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter') onSubmit(); }}
        disabled={streaming}
      />
      <span className="r">
        <span className="kbd">⏎ send</span>
        <Btn kind="ghost" size="sm" icon="paperclip" iconOnly title="attach context" />
      </span>
    </div>
  );
}

// expose to other babel scripts
window.NodeSession = NodeSession;
window.CommandCenter = function CommandCenter(props) {
  return <NodeSession node={props.ccNode} mode="cockpit" onSpawn={props.onSpawn} simSpeed={props.simSpeed} onAction={props.onAction} onExpand={props.onExpand} />;
};
