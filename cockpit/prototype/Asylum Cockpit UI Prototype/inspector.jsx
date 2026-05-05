// asylum cockpit — inspector (right rail on the cockpit screen)

function Inspector({ node, onAction, onOpen }) {
  if (!node) {
    return (
      <div className="inspector">
        <div className="inspector-empty">
          <div className="glyph">[ ]</div>
          select a node from the graph<br />
          <span style={{ color: 'var(--fg-subtle)' }}>or use Cmd+K to find one</span>
        </div>
      </div>
    );
  }

  const harness = ASYLUM_DATA.HARNESSES.find(h => h.id === node.harness);
  const sub = ASYLUM_DATA.SUBSTRATES.find(s => s.id === node.substrate);

  return (
    <div className="inspector">
      <div className="inspector-head">
        <span style={{ fontSize: 14, opacity: 0.6 }}>{ROLE_GLYPH[node.role]}</span>
        <div style={{ flex: 1 }}>
          <div className="id">{node.id}</div>
          <div className="role">{node.role} · {harness?.name || node.harness}</div>
        </div>
        <Pill status={node.state}>{nodeStatusLabel(node)}</Pill>
      </div>

      <div className="inspector-body">
        {node.decision && (
          <div className="inspector-section">
            <div className="decision">
              <div className="h"><Icon name="alert-triangle" size={12} /> {node.decision.title}</div>
              <div className="q">{node.decision.body}</div>
              <div className="actions">
                {node.decision.actions.map((a, i) => (
                  <Btn key={a} size="sm" kind={i === 0 ? 'primary' : 'secondary'}>{a}</Btn>
                ))}
              </div>
            </div>
          </div>
        )}

        <div className="inspector-section">
          <div className="h">overview</div>
          <KV items={[
            ['node id', node.id],
            ['role', node.role],
            ['harness', harness?.name || node.harness],
            ['substrate', node.substrate],
            ['workspace', node.workspace],
            ['parent', node.parent || '—'],
            ['uptime', node.duration],
          ]} />
        </div>

        <div className="inspector-section">
          <div className="h">live preview</div>
          <div style={{ background: 'var(--bg-sunken)', border: '1px solid var(--border-subtle)', padding: 12, fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--fg-muted)', maxHeight: 88, overflow: 'hidden' }}>
            {node.preview}
            {node.state === 'running' && <span className="caret" style={{ marginLeft: 4 }} />}
          </div>
        </div>

        <div className="inspector-section">
          <div className="h">telemetry</div>
          <KV items={[
            ['ctx usage', `${Math.round(node.ctx * 100)}%`],
            ['tokens in', node.tokensIn.toLocaleString()],
            ['tokens out', node.tokensOut.toLocaleString()],
            ['tool calls', node.tools],
          ]} />
        </div>

        <div className="inspector-section">
          <div className="h">capabilities</div>
          <div className="capgrid">
            {['observe', 'send_input', 'browser_attach', 'native_attach', 'interrupt', 'transcript_export', 'native_resume', 'subagents'].map(cap => {
              const has = harness?.caps.includes(cap);
              return (
                <Fragment key={cap}>
                  <span className="cap">{cap}</span>
                  <span className={has ? 'ok' : 'no'}>{has ? '✓' : '—'}</span>
                </Fragment>
              );
            })}
          </div>
        </div>

        <div className="inspector-section">
          <div className="h">controls</div>
          <div className="inspector-actions">
            <Btn size="sm" kind="primary" icon="external-link" onClick={() => onAction?.('attach')}>attach</Btn>
            <Btn size="sm" icon="message-square" onClick={() => onAction?.('send')}>send input</Btn>
            <Btn size="sm" icon="square" onClick={() => onAction?.('interrupt')}>interrupt</Btn>
            <Btn size="sm" icon="git-branch" onClick={() => onAction?.('fork')}>fork</Btn>
            <Btn size="sm" icon="rotate-ccw" onClick={() => onAction?.('restart')}>restart</Btn>
            <Btn size="sm" kind="danger" icon="x" onClick={() => onAction?.('terminate')}>terminate</Btn>
          </div>
          <div style={{ marginTop: 10 }}>
            <Btn kind="ghost" size="sm" icon="arrow-right" onClick={() => onOpen?.(node)}>open node detail</Btn>
          </div>
        </div>
      </div>
    </div>
  );
}

window.Inspector = Inspector;
