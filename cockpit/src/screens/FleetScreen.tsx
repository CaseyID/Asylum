// asylum cockpit — fleet table screen.
// ports the FleetScreen from the design prototype, backed by real daemon data.

import { useMemo, useState, type JSX } from "react";
import { Btn, Pill } from "../lib/ui";
import { Icon } from "../lib/icons";
import {
  ROLE_GLYPH,
  harnessLabel,
  isCommandCenter,
  shortNodeId,
  telemetryFor,
  uiStateLabel,
  uiStateOf,
  uptimeLabel,
  previewFor,
} from "../lib/glyphs";
import type { AsylumNode, GraphRelationship, UiState } from "../types";

export interface FleetScreenProps {
  nodes: AsylumNode[];
  onLaunch: () => void;
  onOpen: (node: AsylumNode) => void;
  // node ids with an unresolved pending decision (W5 decision surfacing).
  pendingDecisionNodeIds?: Set<string>;
  // D2: spawn_peer lineage — graph relationships, used to show each node's
  // spawning parent (the same edge data the Graph view already renders as a
  // dashed spawned_for line).
  relationships?: GraphRelationship[];
}

const STATE_FILTERS: ("all" | UiState)[] = ["all", "running", "waiting", "idle", "errored", "stopped", "archived"];

export function FleetScreen({
  nodes,
  onLaunch,
  onOpen,
  pendingDecisionNodeIds,
  relationships,
}: FleetScreenProps): JSX.Element {
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState<(typeof STATE_FILTERS)[number]>("all");

  // D2: first relationship targeting a node is treated as its spawning
  // parent for display, matching App.tsx's graphNodes derivation for the
  // Graph view.
  const parentByChild = useMemo(() => {
    const map = new Map<string, { parentId: string; kind: string }>();
    for (const rel of relationships ?? []) {
      if (!map.has(rel.target_node_id)) {
        map.set(rel.target_node_id, { parentId: rel.source_node_id, kind: rel.kind });
      }
    }
    return map;
  }, [relationships]);

  const filtered = useMemo(() => {
    return nodes.filter((n) => {
      if (q && !(n.id.includes(q) || n.role_hint.includes(q) || n.harness.includes(q))) return false;
      if (filter !== "all" && uiStateOf(n) !== filter) return false;
      return true;
    });
  }, [nodes, q, filter]);

  const counts = useMemo(() => {
    const c: Record<string, number> = {
      all: nodes.length,
      running: 0,
      waiting: 0,
      idle: 0,
      errored: 0,
      stopped: 0,
      archived: 0,
    };
    nodes.forEach((n) => {
      const s = uiStateOf(n);
      c[s] = (c[s] ?? 0) + 1;
    });
    return c;
  }, [nodes]);

  const substrateCount = useMemo(() => new Set(nodes.map((n) => n.substrate)).size, [nodes]);
  const harnessCount = useMemo(() => new Set(nodes.map((n) => n.harness)).size, [nodes]);

  return (
    <div className="page" style={{ paddingTop: 28 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">nodes</h1>
          <div className="page-sub">
            {nodes.length} total · {counts.running ?? 0} running · {counts.waiting ?? 0} waiting · {counts.errored ?? 0} errored
          </div>
        </div>
        <div className="page-actions">
          <Btn kind="primary" icon="plus" onClick={onLaunch}>
            launch node
          </Btn>
        </div>
      </div>

      <div className="toolbar">
        <div className="search">
          <Icon name="search" size={12} />
          <input
            className="input mono"
            placeholder="filter by id, role, harness, substrate…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          {STATE_FILTERS.map((s) => (
            <button
              key={s}
              type="button"
              className={`btn btn-sm ${filter === s ? "btn-secondary" : "btn-ghost"}`}
              onClick={() => setFilter(s)}
            >
              {s}{" "}
              <span className="muted" style={{ marginLeft: 4 }}>
                {counts[s] ?? 0}
              </span>
            </button>
          ))}
        </div>
        <div className="stats">
          <span>
            <b>{substrateCount}</b> substrates
          </span>
          <span>
            <b>{harnessCount}</b> harnesses
          </span>
        </div>
      </div>

      <table className="table" style={{ borderTop: "none" }}>
        <thead>
          <tr>
            <th style={{ width: 28 }}></th>
            <th style={{ width: 140 }}>node</th>
            <th style={{ width: 110 }}>role</th>
            <th style={{ width: 130 }}>harness</th>
            <th style={{ width: 110 }}>substrate</th>
            <th style={{ width: 100 }}>lineage</th>
            <th style={{ width: 110 }}>state</th>
            <th>preview</th>
            <th style={{ width: 80 }} className="right">
              ctx est.
            </th>
            <th style={{ width: 80 }} className="right">
              uptime
            </th>
            <th style={{ width: 32 }}></th>
          </tr>
        </thead>
        <tbody>
          {filtered.map((n) => {
            const tel = telemetryFor(n);
            const cc = isCommandCenter(n);
            const state = uiStateOf(n);
            return (
              <tr key={n.id} onClick={() => onOpen(n)}>
                <td style={{ color: "var(--fg-subtle)", textAlign: "center", fontFamily: "var(--font-mono)" }}>
                  {ROLE_GLYPH[n.role_hint] ?? "·"}
                </td>
                <td className="mono" style={{ color: "var(--fg)" }}>
                  {shortNodeId(n.id)}
                  {cc && <span style={{ marginLeft: 8, color: "var(--fg-muted)", fontSize: 10 }}>[cc]</span>}
                  {pendingDecisionNodeIds?.has(n.id) && (
                    <span
                      className="pill pill-waiting"
                      style={{ marginLeft: 8, fontSize: 9, padding: "1px 6px" }}
                      title="pending decision"
                    >
                      decision
                    </span>
                  )}
                </td>
                <td className="mono muted">{n.role_hint}</td>
                <td className="mono">{harnessLabel(n.harness)}</td>
                <td className="mono muted">{n.substrate}</td>
                <td className="mono muted" onClick={(e) => e.stopPropagation()}>
                  {(() => {
                    const rel = parentByChild.get(n.id);
                    if (!rel) return "—";
                    const parent = nodes.find((p) => p.id === rel.parentId);
                    if (!parent) return `← ${shortNodeId(rel.parentId)}`;
                    return (
                      <a
                        style={{ color: "var(--fg)", cursor: "pointer" }}
                        title={rel.kind}
                        onClick={() => onOpen(parent)}
                      >
                        ← {shortNodeId(rel.parentId)}
                      </a>
                    );
                  })()}
                </td>
                <td>
                  <Pill status={state}>{uiStateLabel(state)}</Pill>
                </td>
                <td className="mono muted ellipsis" style={{ maxWidth: 0 }}>
                  {previewFor(n)}
                </td>
                <td className="num">{Math.round(tel.ctx * 100)}%</td>
                <td className="num">{uptimeLabel(n)}</td>
                <td>
                  <Icon name="chevron-right" size={14} style={{ opacity: 0.4 }} />
                </td>
              </tr>
            );
          })}
          {filtered.length === 0 && (
            <tr>
              <td colSpan={11} style={{ textAlign: "center", padding: 32, color: "var(--fg-muted)" }}>
                no nodes match the filter
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
