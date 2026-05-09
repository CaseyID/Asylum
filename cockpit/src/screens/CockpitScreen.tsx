// asylum cockpit — default screen.
// graph + node session + inspector, all viewing the same fleet.

import { type JSX } from "react";
import { Btn, Empty } from "../lib/ui";
import { Graph, type GraphNode } from "../components/Graph";
import { NodeSession } from "../components/NodeSession";
import { Inspector, type InspectorAction } from "../components/Inspector";
import type { AsylumNode, GraphRelationship } from "../types";

export type GraphLayout = "tree" | "free" | "force" | "swimlanes";

export interface CockpitScreenProps {
  graphNodes: GraphNode[];
  ccNode?: AsylumNode;
  selected?: AsylumNode;
  onSelect: (node: AsylumNode) => void;
  onOpen: (node: AsylumNode) => void;
  layout: GraphLayout;
  setLayout: (layout: GraphLayout) => void;
  onAction: (action: InspectorAction | "native-attach", payload?: string) => void;
  onExpandToChat: (nodeId: string) => void;
  onLaunchCC: () => void;
  substrates: { id: string; name: string; healthy: boolean; capacity: number }[];
  relationships: GraphRelationship[];
}

const LAYOUT_OPTIONS: [GraphLayout, string][] = [
  ["tree", "t"],
  ["free", "f"],
  ["force", "✦"],
  ["swimlanes", "≡"],
];

export function CockpitScreen({
  graphNodes,
  ccNode,
  selected,
  onSelect,
  onOpen,
  layout,
  setLayout,
  onAction,
  onExpandToChat,
  onLaunchCC,
  substrates,
  relationships,
}: CockpitScreenProps): JSX.Element {
  const panelNode = selected ?? ccNode;

  return (
    <div className="cockpit">
      <div className="cockpit-main">
        <div className="cockpit-graph-wrap">
          <Graph
            nodes={graphNodes}
            layout={layout}
            selectedId={selected?.id}
            onSelect={(gn) => onSelect(gn.node)}
            substrates={substrates}
          />
          <div className="graph-controls">
            {LAYOUT_OPTIONS.map(([id, glyph]) => (
              <Btn
                key={id}
                size="sm"
                kind={layout === id ? "secondary" : "ghost"}
                onClick={() => setLayout(id)}
                title={id}
              >
                <span style={{ fontFamily: "var(--font-mono)", fontSize: 10, marginRight: 4 }}>{glyph}</span>
                {id}
              </Btn>
            ))}
          </div>
          <div className="graph-legend">
            <div className="item">
              <span className="swatch" /> supervises
            </div>
            <div className="item">
              <span className="swatch dashed" /> spawned_for
            </div>
            <div className="item">
              <span style={{ fontFamily: "var(--font-mono)", color: "var(--status-info)" }}>━━━</span> live
            </div>
            <div className="item">
              <span style={{ color: "var(--fg-subtle)", fontSize: 10 }}>scroll = zoom · drag = pan</span>
            </div>
          </div>
        </div>
        <div className="cockpit-cc-wrap">
          {panelNode ? (
            <NodeSession
              key={panelNode.id}
              node={panelNode}
              mode="cockpit"
              onInterrupt={() => onAction("interrupt")}
              onExpand={() => onExpandToChat(panelNode.id)}
            />
          ) : (
            <div style={{ flex: 1, display: "grid", placeItems: "center", background: "var(--bg-sunken)" }}>
              <Empty
                glyph="⌬"
                lead="no command center running"
                sub="launch one to get an asylum-aware harness session here"
                action={
                  <Btn kind="primary" icon="plus" onClick={onLaunchCC}>
                    launch command center
                  </Btn>
                }
              />
            </div>
          )}
        </div>
      </div>
      <Inspector node={selected} onAction={onAction} onOpen={onOpen} relationships={relationships} />
    </div>
  );
}
