// asylum cockpit — primitive components
const { useState, useEffect, useRef, useMemo, useCallback, Fragment } = React;

const ICON_BASE = 'https://unpkg.com/lucide-static@0.469.0/icons/';
function Icon({ name, size = 16, style, className }) {
  // we use a mask so currentColor controls fill -- icons inherit text color cleanly in both themes
  return (
    <span
      className={`ico ${className || ''}`}
      aria-hidden="true"
      style={{
        display: 'inline-block',
        width: size, height: size,
        backgroundColor: 'currentColor',
        WebkitMask: `url(${ICON_BASE}${name}.svg) center / contain no-repeat`,
        mask: `url(${ICON_BASE}${name}.svg) center / contain no-repeat`,
        flexShrink: 0,
        ...style,
      }}
    />
  );
}

function Wordmark({ size = 14 }) {
  return (
    <span className="wm" style={{ fontSize: size }}>
      <span className="b">[</span>asylum<span className="b">]</span>
    </span>
  );
}

function Pill({ status, children }) {
  return (
    <span className={`pill pill-${status}`}>
      <span className="dot" />
      {children}
    </span>
  );
}

function Tag({ children, kind = '', future }) {
  return <span className={`tag ${kind} ${future ? 'future' : ''}`}>{children}</span>;
}

function Btn({ kind = 'secondary', size, icon, iconOnly, children, onClick, title, disabled }) {
  return (
    <button
      className={`btn btn-${kind} ${size === 'sm' ? 'btn-sm' : ''} ${iconOnly ? 'btn-icon' : ''}`}
      onClick={onClick}
      title={title}
      disabled={disabled}
    >
      {icon && <Icon name={icon} size={size === 'sm' ? 12 : 14} />}
      {children}
    </button>
  );
}

function Field({ label, hint, children }) {
  return (
    <div className="field">
      {label && <span className="field-label">{label}</span>}
      {children}
      {hint && <span className="field-hint">{hint}</span>}
    </div>
  );
}

function Panel({ title, eyebrow, actions, children, flush }) {
  return (
    <div className="panel">
      {(title || actions) && (
        <div className="panel-head">
          {eyebrow && <><span className="b">[</span><span>{eyebrow}</span><span className="b">]</span></>}
          {title && <span>{title}</span>}
          {actions && <span className="right">{actions}</span>}
        </div>
      )}
      <div className={`panel-body ${flush ? 'flush' : ''}`}>{children}</div>
    </div>
  );
}

function KV({ items }) {
  return (
    <div className="kv">
      {items.map(([k, v, sansFlag], i) => (
        <Fragment key={i}>
          <span className="k">{k}</span>
          <span className={`v ${sansFlag ? 'sans' : ''}`}>{v}</span>
        </Fragment>
      ))}
    </div>
  );
}

function Modal({ title, onClose, children, foot, width }) {
  useEffect(() => {
    const onKey = (e) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  return (
    <div className="scrim" onClick={onClose}>
      <div className="modal" onClick={e => e.stopPropagation()} style={width ? { width } : null}>
        <div className="modal-head">
          <span className="t"><span className="b" style={{ opacity: 0.5 }}>[</span> {title} <span className="b" style={{ opacity: 0.5 }}>]</span></span>
          <span className="x" onClick={onClose}>×</span>
        </div>
        <div className="modal-body">{children}</div>
        {foot && <div className="modal-foot">{foot}</div>}
      </div>
    </div>
  );
}

function CmdK({ onClose, onPick, onLaunch }) {
  const items = [
    { sec: 'actions', label: 'launch new node…',     kbd: 'N',  icon: 'plus',         action: () => { onLaunch(); } },
    { sec: 'actions', label: 'attach in browser…',   kbd: 'A',  icon: 'external-link',action: () => { onPick('cockpit'); } },
    { sec: 'actions', label: 'send remote command…', kbd: 'R',  icon: 'send',         action: () => {} },
    { sec: 'go to',   label: 'cockpit',              kbd: '1',  icon: 'layout-grid',  action: () => onPick('cockpit') },
    { sec: 'go to',   label: 'fleet',                kbd: '2',  icon: 'list',         action: () => onPick('fleet') },
    { sec: 'go to',   label: 'channels',             kbd: '3',  icon: 'rss',          action: () => onPick('channels') },
    { sec: 'go to',   label: 'chat',                 kbd: '4',  icon: 'terminal',     action: () => onPick('chat') },
    { sec: 'go to',   label: 'hooks',                kbd: '5',  icon: 'zap',          action: () => onPick('hooks') },
    { sec: 'go to',   label: 'logs',                 kbd: '6',  icon: 'activity',     action: () => onPick('logs') },
    { sec: 'go to',   label: 'settings',             kbd: ',',  icon: 'settings',     action: () => onPick('settings') },
  ];
  const [q, setQ] = useState('');
  const [i, setI] = useState(0);
  const filtered = items.filter(x => x.label.toLowerCase().includes(q.toLowerCase()));
  const onKey = (e) => {
    if (e.key === 'ArrowDown') { setI(v => Math.min(v + 1, filtered.length - 1)); e.preventDefault(); }
    else if (e.key === 'ArrowUp') { setI(v => Math.max(v - 1, 0)); e.preventDefault(); }
    else if (e.key === 'Enter') { filtered[i]?.action(); onClose(); e.preventDefault(); }
    else if (e.key === 'Escape') onClose();
  };
  let lastSec = '';
  return (
    <div className="scrim" onClick={onClose}>
      <div className="cmdk" onClick={e => e.stopPropagation()}>
        <input autoFocus className="cmdk-input" placeholder="run a command, jump to a screen, find a node…"
          value={q} onChange={e => { setQ(e.target.value); setI(0); }} onKeyDown={onKey} />
        <div className="cmdk-list">
          {filtered.map((it, idx) => {
            const sec = it.sec !== lastSec ? <div className="cmdk-section" key={'s'+idx}>{it.sec}</div> : null;
            lastSec = it.sec;
            return (
              <Fragment key={idx}>
                {sec}
                <div className={`cmdk-item ${idx === i ? 'active' : ''}`}
                  onMouseEnter={() => setI(idx)}
                  onClick={() => { it.action(); onClose(); }}>
                  <Icon name={it.icon} size={14} />
                  <span>{it.label}</span>
                  <span className="k">{it.kbd}</span>
                </div>
              </Fragment>
            );
          })}
        </div>
        <div className="cmdk-foot">
          <span><b>↵</b> run</span>
          <span><b>↑↓</b> navigate</span>
          <span><b>esc</b> close</span>
        </div>
      </div>
    </div>
  );
}

// ─── ntfy toast ────────────────────────────────────────
function NtfyToast({ toast, onDismiss, onReply }) {
  const [reply, setReply] = useState('');
  return (
    <div className="toast">
      <div className="h">
        <Icon name="bell" size={12} />
        <span>ntfy</span>
        <span className="ch" style={{ opacity: 0.7 }}>· {toast.channel}</span>
        <span className="x" onClick={onDismiss}><Icon name="x" size={12} /></span>
      </div>
      <div className="body">
        <div className="from">{toast.from} → you</div>
        <div>{toast.body}</div>
      </div>
      <div className="quick">
        {toast.replies.map(r => (
          <button key={r} className="q" onClick={() => { onReply(r); onDismiss(); }}>{r}</button>
        ))}
      </div>
      <div className="reply">
        <span className="glyph">{'>'}</span>
        <input
          placeholder="reply to send command…"
          value={reply}
          onChange={e => setReply(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && reply.trim()) { onReply(reply); onDismiss(); } }}
        />
        <button className="send" onClick={() => { if (reply.trim()) { onReply(reply); onDismiss(); } }}>send ↵</button>
      </div>
    </div>
  );
}

// ─── empty state ──────────────────────────────────────
function Empty({ glyph = '[ ]', lead, sub, action }) {
  return (
    <div className="empty">
      <div className="glyph">{glyph}</div>
      <div className="lead">{lead}</div>
      {sub && <div className="sub">{sub}</div>}
      {action && <div style={{ marginTop: 18 }}>{action}</div>}
    </div>
  );
}

// ─── role glyphs ──────────────────────────────────────
const ROLE_GLYPH = {
  'command-center': '⌬',
  'supervisor':     '◆',
  'worker':         '◇',
  'evaluator':      '◯',
  'assistant':      '·',
};

// ─── status helpers ───────────────────────────────────
function nodeStatusLabel(n) {
  if (n.state === 'running') return 'running';
  if (n.state === 'waiting') return 'waiting';
  if (n.state === 'idle')    return 'idle';
  if (n.state === 'errored') return 'errored';
  if (n.state === 'stopped') return 'stopped';
  return n.state;
}

// ─── tool-call card ──────────────────────────────────
function ToolCall({ name, args, output, state = 'ok', collapsed = true }) {
  const [open, setOpen] = useState(!collapsed);
  return (
    <div className="tcall">
      <div className="h">
        <Icon name="wrench" size={11} />
        <span>{name}</span>
        <span className="right">
          {state === 'ok' && <span style={{ color: 'var(--status-running)', fontSize: 11 }}>✓ ok</span>}
          {state === 'pending' && <span style={{ color: 'var(--status-waiting)', fontSize: 11 }}>· pending</span>}
          {state === 'error' && <span style={{ color: 'var(--status-errored)', fontSize: 11 }}>! err</span>}
          {output && <span onClick={() => setOpen(!open)} style={{ cursor: 'pointer', marginLeft: 8, opacity: 0.7 }}>{open ? '−' : '+'}</span>}
        </span>
      </div>
      {args && (
        <div className="args">
          {Object.entries(args).map(([k, v]) => (
            <div key={k}><span className="muted">{k}:</span> <span className="arg">{String(v)}</span></div>
          ))}
        </div>
      )}
      {output && <div className={`out ${open ? '' : 'collapsed'}`}>{output}</div>}
    </div>
  );
}

Object.assign(window, {
  Icon, Wordmark, Pill, Tag, Btn, Field, Panel, KV, Modal, CmdK, NtfyToast, Empty, ToolCall,
  ROLE_GLYPH, nodeStatusLabel, useState, useEffect, useRef, useMemo, useCallback, Fragment,
});
