// asylum cockpit — graph layouts (tree, free, swimlanes, force)

const NODE_W = 184;
const NODE_H_BASE = 88;
const NODE_H_CC = 100;

// ─── layout: hierarchical tree ────────────────────────
function layoutTree(nodes, w, h) {
  // group by parent
  const byParent = {};
  nodes.forEach(n => {
    const p = n.parent || '__root';
    (byParent[p] ||= []).push(n);
  });
  const positions = {};
  // place command-centers at top center
  const roots = byParent['__root'] || [];
  const yPad = 56;
  const xCenter = w / 2;
  const colSpacing = NODE_W + 28;

  // place roots in a row
  const rootY = yPad;
  const rootXStart = xCenter - ((roots.length - 1) * colSpacing) / 2;
  roots.forEach((r, i) => {
    positions[r.id] = { x: rootXStart + i * colSpacing - NODE_W / 2, y: rootY };
  });

  // place children level by level
  const levelGap = 132;
  const placeChildren = (parentId, level) => {
    const kids = byParent[parentId] || [];
    if (!kids.length) return;
    const parent = positions[parentId];
    const totalW = kids.length * NODE_W + (kids.length - 1) * 28;
    const startX = parent.x + NODE_W / 2 - totalW / 2;
    kids.forEach((k, i) => {
      positions[k.id] = { x: startX + i * (NODE_W + 28), y: parent.y + levelGap };
      placeChildren(k.id, level + 1);
    });
  };
  roots.forEach(r => placeChildren(r.id, 1));

  return positions;
}

// ─── layout: free / dotted-grid hand-arranged ─────────
function layoutFree(nodes, w, h) {
  // hand-tuned poses for the 8 mock nodes; falls back to a spiral for unknowns
  const seed = {
    'cc-7c2af':  { x: 120, y: 60 },
    'sup-3d1e':  { x: 380, y: 220 },
    'sup-aa01':  { x: 120, y: 280 },
    'asst-d2c9': { x: 600, y: 60 },
    'w-9a4f1':   { x: 240, y: 400 },
    'w-2b0c8':   { x: 460, y: 400 },
    'w-4e7b':    { x: 660, y: 320 },
    'w-1f3a':    { x: 60,  y: 440 },
  };
  const positions = {};
  nodes.forEach((n, i) => {
    if (seed[n.id]) positions[n.id] = seed[n.id];
    else positions[n.id] = { x: 80 + (i % 4) * 200, y: 60 + Math.floor(i / 4) * 160 };
  });
  return positions;
}

// ─── layout: swimlanes by substrate ───────────────────
function layoutSwimlanes(nodes, w, h, substrates) {
  const positions = {};
  const lanes = substrates.filter(s => s.healthy || nodes.some(n => n.substrate === s.id));
  const laneH = Math.max(160, (h - 40) / lanes.length);
  lanes.forEach((s, li) => {
    const laneNodes = nodes.filter(n => n.substrate === s.id);
    const yMid = 40 + li * laneH + laneH / 2;
    laneNodes.forEach((n, ni) => {
      positions[n.id] = { x: 240 + ni * (NODE_W + 32), y: yMid - NODE_H_BASE / 2 };
    });
  });
  return { positions, lanes, laneH };
}

// ─── force: simple symmetric pull along edges ─────────
function layoutForce(nodes, w, h) {
  // not a real force sim — a deterministic placement that *looks* clustered
  const positions = {};
  const cx = w / 2, cy = h / 2;
  const cc = nodes.find(n => n.isCommandCenter);
  if (cc) positions[cc.id] = { x: cx - NODE_W / 2, y: cy - 110 };

  const sups = nodes.filter(n => n.role === 'supervisor');
  sups.forEach((s, i) => {
    const a = (i - (sups.length - 1) / 2) * 0.7;
    positions[s.id] = { x: cx - NODE_W / 2 + Math.sin(a) * 200, y: cy + Math.cos(a) * 60 + 20 };
  });

  // workers cluster around their supervisor (or cc)
  const workers = nodes.filter(n => !positions[n.id]);
  workers.forEach((wn, i) => {
    const parent = positions[wn.parent] || { x: cx - NODE_W / 2, y: cy };
    const a = (i % 6) * (Math.PI / 3) + 0.4;
    const r = 150 + (i % 3) * 20;
    positions[wn.id] = { x: parent.x + Math.cos(a) * r, y: parent.y + Math.sin(a) * r + 80 };
  });
  return positions;
}

// ─── edge path ────────────────────────────────────────
function edgePath(p1, p2, p1NodeH = NODE_H_BASE) {
  const x1 = p1.x + NODE_W / 2;
  const y1 = p1.y + p1NodeH;
  const x2 = p2.x + NODE_W / 2;
  const y2 = p2.y;
  // smooth bezier
  const my = (y1 + y2) / 2;
  return `M${x1},${y1} C${x1},${my} ${x2},${my} ${x2},${y2}`;
}
function edgePathFreeform(p1, p2, p1NodeH = NODE_H_BASE) {
  // generic edge between any two cards (used for free / force)
  const cx1 = p1.x + NODE_W / 2;
  const cy1 = p1.y + p1NodeH / 2;
  const cx2 = p2.x + NODE_W / 2;
  const cy2 = p2.y + NODE_H_BASE / 2;
  const dx = cx2 - cx1, dy = cy2 - cy1;
  const dist = Math.hypot(dx, dy) || 1;
  // edge enters/exits from card edges roughly
  const sourceEdgeY = p1.y + (dy > 0 ? p1NodeH : 0);
  const targetEdgeY = p2.y + (dy > 0 ? 0 : NODE_H_BASE);
  const my = (sourceEdgeY + targetEdgeY) / 2;
  return `M${cx1},${sourceEdgeY} C${cx1},${my} ${cx2},${my} ${cx2},${targetEdgeY}`;
}

// ─── graph component ─────────────────────────────────
function Graph({ nodes, layout, selectedId, onSelect, substrates }) {
  const wrapRef = useRef(null);
  const [size, setSize] = useState({ w: 900, h: 600 });
  // pan/zoom state
  const [view, setView] = useState({ x: 0, y: 0, k: 1 });
  const dragRef = useRef(null);
  const [isPanning, setIsPanning] = useState(false);

  // reset view when layout changes so the new layout is centered
  useEffect(() => { setView({ x: 0, y: 0, k: 1 }); }, [layout]);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setSize({ w: width, h: height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // wheel zoom — zooms toward the cursor
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const onWheel = (e) => {
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      setView(v => {
        // pinch / ctrl-wheel = zoom; plain wheel also zooms (it's a graph, not a doc)
        const factor = Math.exp(-e.deltaY * 0.0015);
        const k2 = Math.min(2.4, Math.max(0.35, v.k * factor));
        // keep cursor anchor stable: world point under cursor stays under cursor
        const wx = (px - v.x) / v.k;
        const wy = (py - v.y) / v.k;
        return { x: px - wx * k2, y: py - wy * k2, k: k2 };
      });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

  // drag pan — only when starting on the empty canvas (not a node card)
  function onMouseDown(e) {
    if (e.button !== 0) return;
    if (e.target.closest('.node-card')) return;
    dragRef.current = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
    setIsPanning(true);
  }
  useEffect(() => {
    if (!isPanning) return;
    const onMove = (e) => {
      const d = dragRef.current; if (!d) return;
      setView(v => ({ ...v, x: d.vx + (e.clientX - d.x), y: d.vy + (e.clientY - d.y) }));
    };
    const onUp = () => { dragRef.current = null; setIsPanning(false); };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); };
  }, [isPanning]);

  function fitView() { setView({ x: 0, y: 0, k: 1 }); }
  function zoomBy(factor) {
    setView(v => {
      const k2 = Math.min(2.4, Math.max(0.35, v.k * factor));
      // anchor at center
      const cx = size.w / 2, cy = size.h / 2;
      const wx = (cx - v.x) / v.k, wy = (cy - v.y) / v.k;
      return { x: cx - wx * k2, y: cy - wy * k2, k: k2 };
    });
  }

  const { positions, lanes, laneH } = useMemo(() => {
    if (layout === 'tree')      return { positions: layoutTree(nodes, size.w, size.h) };
    if (layout === 'free')      return { positions: layoutFree(nodes, size.w, size.h) };
    if (layout === 'swimlanes') return layoutSwimlanes(nodes, size.w, size.h, substrates);
    if (layout === 'force')     return { positions: layoutForce(nodes, size.w, size.h) };
    return { positions: layoutTree(nodes, size.w, size.h) };
  }, [layout, nodes, size, substrates]);

  // edges
  const edges = nodes.filter(n => n.parent).map(n => {
    const parent = nodes.find(p => p.id === n.parent);
    if (!parent) return null;
    const p1 = positions[parent.id], p2 = positions[n.id];
    if (!p1 || !p2) return null;
    const p1NodeH = parent.isCommandCenter ? NODE_H_CC : NODE_H_BASE;
    const path = (layout === 'tree')
      ? edgePath(p1, p2, p1NodeH)
      : edgePathFreeform(p1, p2, p1NodeH);
    return { id: n.id, path, kind: n.edge || 'spawned_for' };
  }).filter(Boolean);

  return (
    <div
      className={`graph ${isPanning ? 'panning' : ''}`}
      ref={wrapRef}
      onMouseDown={onMouseDown}
    >
      <div className="graph-grid" style={{
        backgroundPosition: `${view.x}px ${view.y}px`,
        backgroundSize: `${20 * view.k}px ${20 * view.k}px`,
      }} />

      <div className="graph-zoom-controls">
        <button className="zc" onClick={() => zoomBy(1.2)} title="zoom in">+</button>
        <button className="zc" onClick={() => zoomBy(1 / 1.2)} title="zoom out">−</button>
        <button className="zc" onClick={fitView} title="fit / reset">⊡</button>
        <span className="zc-label">{Math.round(view.k * 100)}%</span>
      </div>

      <div className="graph-viewport" style={{
        transform: `translate(${view.x}px, ${view.y}px) scale(${view.k})`,
        transformOrigin: '0 0',
      }}>
        {layout === 'swimlanes' && lanes && lanes.map((s, i) => (
          <Fragment key={s.id}>
            <div className="lane-label" style={{ top: 40 + i * laneH - 18 }}>
              <span>{s.name}</span>
              <Pill status={s.healthy ? 'running' : 'errored'}>{s.healthy ? 'healthy' : 'unreachable'}</Pill>
              <span className="muted">cap {Math.round((s.capacity || 0) * 100)}%</span>
            </div>
            {i > 0 && <div className="lane-divider" style={{ top: 40 + i * laneH - 24 }} />}
          </Fragment>
        ))}

        <svg className="graph-svg" width={size.w} height={size.h} style={{ overflow: 'visible' }}>
          {edges.map(e => (
            <Fragment key={e.id}>
              <path className={`graph-edge ${e.kind === 'supervises' ? 'supervises' : 'spawned-for'}`} d={e.path} />
            </Fragment>
          ))}
          {edges.length > 0 && layout !== 'swimlanes' && (
            <path className="graph-flow" d={edges[0].path} />
          )}
        </svg>

        {nodes.map(n => {
          const p = positions[n.id];
          if (!p) return null;
          const recent = n._spawning;
          return (
            <div
              key={n.id}
              className={`node-card ${selectedId === n.id ? 'selected' : ''} ${n.isCommandCenter ? 'cc' : ''} ${n.state === 'errored' ? 'errored' : ''} ${recent ? 'spawning' : ''}`}
              style={{ left: p.x, top: p.y }}
              onClick={(e) => { e.stopPropagation(); onSelect(n); }}
            >
              <div className="row1">
                <span className="nid">{n.name}</span>
                <span style={{ color: 'var(--fg-muted)', fontSize: 14, opacity: 0.7 }}>{ROLE_GLYPH[n.role] || ''}</span>
              </div>
              <div className="nrole">{n.role} · {n.harness}</div>
              <div className="nsub">{n.substrate}</div>
              <div className="nfoot">
                <Pill status={n.state}>{nodeStatusLabel(n)}</Pill>
                <span className="ndur">{n.duration}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

window.Graph = Graph;
