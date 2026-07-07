// asylum cockpit — graph layouts (tree, free, swimlanes, force)
// custom-canvas graph; does not use @xyflow — this is the design-prototype
// implementation that owns zoom/pan, bezier edges, and all four layout modes.

import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from "react";
import { Pill } from "../lib/ui";
import { ROLE_GLYPH, isCommandCenter, shortNodeId, harnessLabel, uptimeLabel, uiStateOf } from "../lib/glyphs";
import type { AsylumNode } from "../types";

const NODE_W = 184;
const NODE_H_BASE = 88;
const NODE_H_CC = 100;

// ─── GraphNode view-model ─────────────────────────────────────────────
export interface GraphNode {
  node: AsylumNode;
  parentId: string | null;
  edgeKind: string;   // e.g. "supervises" or "spawned_for"
  spawning?: boolean; // animate-in flag
}

// ─── component props ──────────────────────────────────────────────────
export interface GraphProps {
  nodes: GraphNode[];
  layout: "tree" | "free" | "force" | "swimlanes";
  selectedId?: string;
  onSelect: (n: GraphNode) => void;
  substrates: { id: string; name: string; healthy: boolean; capacity: number }[];
  // node ids with an unresolved pending decision — renders a small badge on
  // the node card (W5 decision surfacing).
  pendingDecisionNodeIds?: Set<string>;
}

// ─── position map ────────────────────────────────────────────────────
interface Pos { x: number; y: number }
type PosMap = Record<string, Pos>;

// ─── layout: hierarchical tree ────────────────────────────────────────
function layoutTree(nodes: GraphNode[], w: number, _h: number): PosMap {
  const byParent: Record<string, GraphNode[]> = {};
  nodes.forEach(gn => {
    const p = gn.parentId ?? "__root";
    (byParent[p] ||= []).push(gn);
  });
  const positions: PosMap = {};
  const roots = byParent["__root"] || [];
  const yPad = 56;
  const xCenter = w / 2;
  const colSpacing = NODE_W + 28;

  const rootY = yPad;
  const rootXStart = xCenter - ((roots.length - 1) * colSpacing) / 2;
  roots.forEach((r, i) => {
    positions[r.node.id] = { x: rootXStart + i * colSpacing - NODE_W / 2, y: rootY };
  });

  const levelGap = 132;
  const placeChildren = (parentId: string, _level: number) => {
    const kids = byParent[parentId] || [];
    if (!kids.length) return;
    const parent = positions[parentId];
    if (!parent) return;
    const totalW = kids.length * NODE_W + (kids.length - 1) * 28;
    const startX = parent.x + NODE_W / 2 - totalW / 2;
    kids.forEach((k, i) => {
      positions[k.node.id] = { x: startX + i * (NODE_W + 28), y: parent.y + levelGap };
      placeChildren(k.node.id, _level + 1);
    });
  };
  roots.forEach(r => placeChildren(r.node.id, 1));

  return positions;
}

// ─── layout: free / 4-column grid by node order ───────────────────────
function layoutFree(nodes: GraphNode[], _w: number, _h: number): PosMap {
  const positions: PosMap = {};
  nodes.forEach((gn, i) => {
    positions[gn.node.id] = { x: 80 + (i % 4) * 200, y: 60 + Math.floor(i / 4) * 160 };
  });
  return positions;
}

// ─── layout: swimlanes by substrate ───────────────────────────────────
interface SwimlaneResult {
  positions: PosMap;
  lanes: { id: string; name: string; healthy: boolean; capacity: number }[];
  laneH: number;
}
function layoutSwimlanes(
  nodes: GraphNode[],
  _w: number,
  h: number,
  substrates: GraphProps["substrates"],
): SwimlaneResult {
  const positions: PosMap = {};
  const lanes = substrates.filter(
    s => s.healthy || nodes.some(gn => gn.node.substrate === s.id),
  );
  const laneH = Math.max(160, (h - 40) / Math.max(1, lanes.length));
  lanes.forEach((s, li) => {
    const laneNodes = nodes.filter(gn => gn.node.substrate === s.id);
    const yMid = 40 + li * laneH + laneH / 2;
    laneNodes.forEach((gn, ni) => {
      positions[gn.node.id] = { x: 240 + ni * (NODE_W + 32), y: yMid - NODE_H_BASE / 2 };
    });
  });
  return { positions, lanes, laneH };
}

// ─── layout: force (deterministic, looks clustered) ───────────────────
function layoutForce(nodes: GraphNode[], w: number, h: number): PosMap {
  const positions: PosMap = {};
  const cx = w / 2, cy = h / 2;
  const cc = nodes.find(gn => isCommandCenter(gn.node));
  if (cc) positions[cc.node.id] = { x: cx - NODE_W / 2, y: cy - 110 };

  const sups = nodes.filter(gn => gn.node.role_hint === "supervisor");
  sups.forEach((gn, i) => {
    const a = (i - (sups.length - 1) / 2) * 0.7;
    positions[gn.node.id] = {
      x: cx - NODE_W / 2 + Math.sin(a) * 200,
      y: cy + Math.cos(a) * 60 + 20,
    };
  });

  const workers = nodes.filter(gn => !positions[gn.node.id]);
  workers.forEach((gn, i) => {
    const parent = gn.parentId ? positions[gn.parentId] : null;
    const base = parent ?? { x: cx - NODE_W / 2, y: cy };
    const a = (i % 6) * (Math.PI / 3) + 0.4;
    const r = 150 + (i % 3) * 20;
    positions[gn.node.id] = {
      x: base.x + Math.cos(a) * r,
      y: base.y + Math.sin(a) * r + 80,
    };
  });
  return positions;
}

// ─── edge paths ────────────────────────────────────────────────────────
function edgePath(p1: Pos, p2: Pos, p1NodeH = NODE_H_BASE): string {
  const x1 = p1.x + NODE_W / 2;
  const y1 = p1.y + p1NodeH;
  const x2 = p2.x + NODE_W / 2;
  const y2 = p2.y;
  const my = (y1 + y2) / 2;
  return `M${x1},${y1} C${x1},${my} ${x2},${my} ${x2},${y2}`;
}

function edgePathFreeform(p1: Pos, p2: Pos, p1NodeH = NODE_H_BASE): string {
  const cx1 = p1.x + NODE_W / 2;
  const cy1 = p1.y + p1NodeH / 2;
  const cx2 = p2.x + NODE_W / 2;
  const cy2 = p2.y + NODE_H_BASE / 2;
  const dy = cy2 - cy1;
  const sourceEdgeY = p1.y + (dy > 0 ? p1NodeH : 0);
  const targetEdgeY = p2.y + (dy > 0 ? 0 : NODE_H_BASE);
  const my = (sourceEdgeY + targetEdgeY) / 2;
  return `M${cx1},${sourceEdgeY} C${cx1},${my} ${cx2},${my} ${cx2},${targetEdgeY}`;
}

// ─── Graph component ───────────────────────────────────────────────────
export function Graph({ nodes, layout, selectedId, onSelect, substrates, pendingDecisionNodeIds }: GraphProps): ReactElement {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [size, setSize] = useState({ w: 900, h: 600 });
  const [view, setView] = useState({ x: 0, y: 0, k: 1 });
  const dragRef = useRef<{ x: number; y: number; vx: number; vy: number } | null>(null);
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
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      setView(v => {
        const factor = Math.exp(-e.deltaY * 0.0015);
        const k2 = Math.min(2.4, Math.max(0.35, v.k * factor));
        const wx = (px - v.x) / v.k;
        const wy = (py - v.y) / v.k;
        return { x: px - wx * k2, y: py - wy * k2, k: k2 };
      });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  // drag pan — only when starting on the empty canvas (not a node card)
  const onMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const target = e.target as Element;
    if (target.closest(".node-card")) return;
    dragRef.current = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
    setIsPanning(true);
  }, [view.x, view.y]);

  useEffect(() => {
    if (!isPanning) return;
    const onMove = (e: MouseEvent) => {
      const d = dragRef.current;
      if (!d) return;
      setView(v => ({ ...v, x: d.vx + (e.clientX - d.x), y: d.vy + (e.clientY - d.y) }));
    };
    const onUp = () => { dragRef.current = null; setIsPanning(false); };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [isPanning]);

  const fitView = useCallback(() => setView({ x: 0, y: 0, k: 1 }), []);
  const zoomBy = useCallback((factor: number) => {
    setView(v => {
      const k2 = Math.min(2.4, Math.max(0.35, v.k * factor));
      const cx = size.w / 2, cy = size.h / 2;
      const wx = (cx - v.x) / v.k, wy = (cy - v.y) / v.k;
      return { x: cx - wx * k2, y: cy - wy * k2, k: k2 };
    });
  }, [size]);

  const { positions, lanes, laneH } = useMemo<{
    positions: PosMap;
    lanes?: GraphProps["substrates"];
    laneH?: number;
  }>(() => {
    if (layout === "tree")      return { positions: layoutTree(nodes, size.w, size.h) };
    if (layout === "free")      return { positions: layoutFree(nodes, size.w, size.h) };
    if (layout === "swimlanes") return layoutSwimlanes(nodes, size.w, size.h, substrates);
    if (layout === "force")     return { positions: layoutForce(nodes, size.w, size.h) };
    return { positions: layoutTree(nodes, size.w, size.h) };
  }, [layout, nodes, size, substrates]);

  // derive edges from parent relationships
  interface EdgeDesc { id: string; path: string; kind: string }
  const edges = useMemo<EdgeDesc[]>(() => {
    return nodes
      .filter(gn => gn.parentId !== null)
      .map(gn => {
        const parentGn = nodes.find(p => p.node.id === gn.parentId);
        if (!parentGn) return null;
        const p1 = positions[parentGn.node.id];
        const p2 = positions[gn.node.id];
        if (!p1 || !p2) return null;
        const p1NodeH = isCommandCenter(parentGn.node) ? NODE_H_CC : NODE_H_BASE;
        const path = layout === "tree"
          ? edgePath(p1, p2, p1NodeH)
          : edgePathFreeform(p1, p2, p1NodeH);
        return { id: gn.node.id, path, kind: gn.edgeKind };
      })
      .filter((e): e is EdgeDesc => e !== null);
  }, [nodes, positions, layout]);

  return (
    <div
      className={`graph ${isPanning ? "panning" : ""}`}
      ref={wrapRef}
      onMouseDown={onMouseDown}
    >
      {/* dotted grid background — offset tracks pan so dots move with the canvas */}
      <div
        className="graph-grid"
        style={{
          backgroundPosition: `${view.x}px ${view.y}px`,
          backgroundSize: `${20 * view.k}px ${20 * view.k}px`,
        }}
      />

      <div className="graph-zoom-controls">
        <button className="zc" onClick={() => zoomBy(1.2)} title="zoom in">+</button>
        <button className="zc" onClick={() => zoomBy(1 / 1.2)} title="zoom out">−</button>
        <button className="zc" onClick={fitView} title="fit / reset">⊡</button>
        <span className="zc-label">{Math.round(view.k * 100)}%</span>
      </div>

      <div
        className="graph-viewport"
        style={{
          transform: `translate(${view.x}px, ${view.y}px) scale(${view.k})`,
          transformOrigin: "0 0",
        }}
      >
        {layout === "swimlanes" && lanes && laneH !== undefined && lanes.map((s, i) => (
          <Fragment key={s.id}>
            <div className="lane-label" style={{ top: 40 + i * laneH - 18 }}>
              <span>{s.name}</span>
              <Pill status={s.healthy ? "running" : "errored"}>
                {s.healthy ? "healthy" : "unreachable"}
              </Pill>
              <span className="muted">cap {Math.round((s.capacity || 0) * 100)}%</span>
            </div>
            {i > 0 && <div className="lane-divider" style={{ top: 40 + i * laneH - 24 }} />}
          </Fragment>
        ))}

        <svg className="graph-svg" width={size.w} height={size.h} style={{ overflow: "visible" }}>
          {edges.map(e => (
            <Fragment key={e.id}>
              <path
                className={`graph-edge ${e.kind === "supervises" ? "supervises" : "spawned-for"}`}
                d={e.path}
              />
            </Fragment>
          ))}
          {/* animated dash along the first edge — shows graph is live */}
          {edges.length > 0 && layout !== "swimlanes" && (
            <path className="graph-flow" d={edges[0].path} />
          )}
        </svg>

        {nodes.map(gn => {
          const p = positions[gn.node.id];
          if (!p) return null;
          const cc = isCommandCenter(gn.node);
          const state = uiStateOf(gn.node);
          const displayId = gn.node.description?.length && gn.node.description.length <= 28
            ? gn.node.description
            : shortNodeId(gn.node.id);
          const hLabel = harnessLabel(gn.node.harness);
          const uptime = uptimeLabel(gn.node);
          const roleGlyph = ROLE_GLYPH[gn.node.role_hint] ?? ROLE_GLYPH[gn.node.role_hint?.toLowerCase()] ?? "";
          const pendingDecision = pendingDecisionNodeIds?.has(gn.node.id) ?? false;

          return (
            <div
              key={gn.node.id}
              className={[
                "node-card",
                selectedId === gn.node.id ? "selected" : "",
                cc ? "cc" : "",
                state === "errored" ? "errored" : "",
                gn.spawning ? "spawning" : "",
              ].join(" ").trim()}
              style={{ left: p.x, top: p.y }}
              onClick={(e) => { e.stopPropagation(); onSelect(gn); }}
            >
              {pendingDecision && (
                <span
                  className="node-card-decision-badge"
                  title="pending decision"
                  style={{
                    position: "absolute",
                    top: -6,
                    right: -6,
                    width: 16,
                    height: 16,
                    borderRadius: "50%",
                    background: "var(--status-waiting)",
                    color: "var(--bg)",
                    fontSize: 10,
                    fontWeight: 700,
                    display: "grid",
                    placeItems: "center",
                    lineHeight: 1,
                  }}
                >
                  ?
                </span>
              )}
              <div className="row1">
                <span className="nid">{displayId}</span>
                <span style={{ color: "var(--fg-muted)", fontSize: 14, opacity: 0.7 }}>{roleGlyph}</span>
              </div>
              <div className="nrole">{gn.node.role_hint} · {hLabel}</div>
              <div className="nsub">{gn.node.substrate}</div>
              <div className="nfoot">
                <Pill status={state}>{state}</Pill>
                <span className="ndur">{uptime}</span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
