// asylum cockpit — chat screen.
// fullscreen viewport into a single node session, with a rail listing every node.

import { type JSX, type ReactNode } from "react";
import { Btn, Empty } from "../lib/ui";
import { NodeSession, type SessionBus, type SpawnEvent } from "../components/NodeSession";
import {
  ROLE_GLYPH,
  harnessLabel,
  isCommandCenter,
  shortNodeId,
  uiStateOf,
} from "../lib/glyphs";
import type { AsylumNode } from "../types";

export interface ChatScreenProps {
  nodes: AsylumNode[];
  chatNodeId?: string;
  onSelectChat: (id: string) => void;
  simSpeed: "still" | "slow" | "live";
  onSpawn: (spawn: SpawnEvent) => void;
  sessionBus: { current: SessionBus };
  onLaunch: () => void;
}

export function ChatScreen({
  nodes,
  chatNodeId,
  onSelectChat,
  simSpeed,
  onSpawn,
  sessionBus,
  onLaunch,
}: ChatScreenProps): JSX.Element {
  const cc = nodes.find((n) => isCommandCenter(n));
  const supervisors = nodes.filter((n) => n.role_hint === "supervisor");
  const others = nodes.filter((n) => !isCommandCenter(n) && n.role_hint !== "supervisor");
  const active = nodes.find((n) => n.id === chatNodeId) ?? cc ?? nodes[0];

  return (
    <div className="chat-screen">
      <div className="chat-rail">
        <div className="rail-head">
          <div className="title">nodes</div>
          <div className="sub">every node is a live tui session</div>
        </div>

        <RailGroup label="command center">
          {cc && <RailItem node={cc} active={active?.id === cc.id} onClick={() => onSelectChat(cc.id)} />}
          {!cc && (
            <div style={{ padding: "8px 10px", fontSize: 11, color: "var(--fg-subtle)" }}>
              no command center · launch one
            </div>
          )}
        </RailGroup>

        {supervisors.length > 0 && (
          <RailGroup label="supervisors">
            {supervisors.map((n) => (
              <RailItem key={n.id} node={n} active={active?.id === n.id} onClick={() => onSelectChat(n.id)} />
            ))}
          </RailGroup>
        )}

        {others.length > 0 && (
          <RailGroup label="workers · evaluators · assistants">
            {others.map((n) => (
              <RailItem key={n.id} node={n} active={active?.id === n.id} onClick={() => onSelectChat(n.id)} />
            ))}
          </RailGroup>
        )}

        <div style={{ marginTop: "auto", padding: 12, borderTop: "1px solid var(--border-subtle)" }}>
          <Btn size="sm" icon="plus" onClick={onLaunch} style={{ width: "100%", justifyContent: "flex-start" }}>
            new node
          </Btn>
          <div className="rail-hint">
            <div>· chat = live tui session</div>
            <div>· same session as cockpit panel</div>
            <div>· press ⌘k to jump nodes</div>
          </div>
        </div>
      </div>
      <div className="chat-stage">
        {active ? (
          <NodeSession
            key={active.id}
            node={active}
            mode="fullscreen"
            simSpeed={simSpeed}
            onSpawn={onSpawn}
            onAction={isCommandCenter(active) ? sessionBus : undefined}
          />
        ) : (
          <Empty glyph="⌬" lead="no nodes" sub="launch a command center to start" />
        )}
      </div>
    </div>
  );
}

function RailGroup({ label, children }: { label: string; children: ReactNode }): JSX.Element {
  return (
    <div className="rail-group">
      <div className="lab">{label}</div>
      {children}
    </div>
  );
}

function RailItem({
  node,
  active,
  onClick,
}: {
  node: AsylumNode;
  active: boolean;
  onClick: () => void;
}): JSX.Element {
  const harnessShort = node.harness === "claude_code" ? "claude" : "codex";
  const state = uiStateOf(node);
  return (
    <div className={`rail-item ${active ? "on" : ""} st-${state}`} onClick={onClick}>
      <span className="g" aria-hidden>
        {ROLE_GLYPH[node.role_hint] ?? "·"}
      </span>
      <span className="id">{shortNodeId(node.id)}</span>
      <span className="meta">
        {harnessShort} · {node.substrate}
      </span>
      <span className={`dot st-${state}`} />
    </div>
  );
}
