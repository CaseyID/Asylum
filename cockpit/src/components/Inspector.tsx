import { Fragment, type JSX } from "react";
import { Btn, KV, Pill } from "../lib/ui";
import { Icon } from "../lib/icons";
import {
  harnessLabel,
  roleGlyph,
  shortNodeId,
  telemetryFor,
  previewFor,
  uiStateOf,
  uiStateLabel,
  uptimeLabel,
} from "../lib/glyphs";
import type { AsylumNode } from "../types";

export type InspectorAction =
  | "attach"
  | "send"
  | "interrupt"
  | "fork"
  | "restart"
  | "terminate"
  | "archive"
  | "decision";

export interface InspectorProps {
  node?: AsylumNode;
  onAction: (action: InspectorAction, payload?: string) => void;
  onOpen: (node: AsylumNode) => void;
}

const CAPABILITY_KEYS: Array<keyof AsylumNode["capabilities"]> = [
  "browser_attach",
  "native_attach",
  "send_input",
  "interrupt",
  "stop",
  "resume",
  "structured_events",
  "transcript_export",
];

export function Inspector({ node, onAction, onOpen }: InspectorProps): JSX.Element {
  if (!node) {
    return (
      <div className="inspector">
        <div className="inspector-empty">
          <div className="glyph">[ ]</div>
          select a node from the graph
          <br />
          <span style={{ color: "var(--fg-subtle)" }}>or use Cmd+K to find one</span>
        </div>
      </div>
    );
  }

  const uiState = uiStateOf(node);
  const telemetry = telemetryFor(node);
  const preview = previewFor(node);

  return (
    <div className="inspector">
      <div className="inspector-head">
        <span style={{ fontSize: 14, opacity: 0.6 }}>{roleGlyph(node.role_hint)}</span>
        <div style={{ flex: 1 }}>
          <div className="id">{shortNodeId(node.id)}</div>
          <div className="role">
            {node.role_hint} · {harnessLabel(node.harness)}
          </div>
        </div>
        <Pill status={uiState}>{uiStateLabel(uiState)}</Pill>
      </div>

      <div className="inspector-body">
        <div className="inspector-section">
          <div className="h">overview</div>
          <KV
            items={[
              ["node id", node.id],
              ["role", node.role_hint],
              ["harness", harnessLabel(node.harness)],
              ["substrate", node.substrate],
              ["workspace", node.workspace ?? "—"],
              ["parent", "—"],
              ["uptime", uptimeLabel(node)],
            ]}
          />
        </div>

        <div className="inspector-section">
          <div className="h">live preview</div>
          <div
            style={{
              background: "var(--bg-sunken)",
              border: "1px solid var(--border-subtle)",
              padding: 12,
              fontFamily: "var(--font-mono)",
              fontSize: 11.5,
              color: "var(--fg-muted)",
              maxHeight: 88,
              overflow: "hidden",
            }}
          >
            {preview}
            {uiState === "running" && (
              <span className="caret" style={{ marginLeft: 4 }} />
            )}
          </div>
        </div>

        <div className="inspector-section">
          <div className="h">telemetry</div>
          <KV
            items={[
              ["ctx usage", `${Math.round(telemetry.ctx * 100)}%`],
              ["tokens in", telemetry.tokensIn.toLocaleString()],
              ["tokens out", telemetry.tokensOut.toLocaleString()],
              ["tool calls", telemetry.tools],
            ]}
          />
        </div>

        <div className="inspector-section">
          <div className="h">capabilities</div>
          <div className="capgrid">
            {CAPABILITY_KEYS.map((cap) => {
              const has = node.capabilities[cap];
              return (
                <Fragment key={cap}>
                  <span className="cap">{cap}</span>
                  <span className={has ? "ok" : "no"}>{has ? "✓" : "—"}</span>
                </Fragment>
              );
            })}
          </div>
        </div>

        <div className="inspector-section">
          <div className="h">controls</div>
          <div className="inspector-actions">
            <Btn
              size="sm"
              kind="primary"
              icon="external-link"
              onClick={() => onAction("attach")}
            >
              attach
            </Btn>
            <Btn
              size="sm"
              icon="message-square"
              onClick={() => onAction("send")}
            >
              send input
            </Btn>
            <Btn
              size="sm"
              icon="square"
              onClick={() => onAction("interrupt")}
            >
              interrupt
            </Btn>
            <Btn
              size="sm"
              icon="git-branch"
              onClick={() => onAction("fork")}
            >
              fork
            </Btn>
            <Btn
              size="sm"
              icon="rotate-ccw"
              onClick={() => onAction("restart")}
            >
              restart
            </Btn>
            <Btn
              size="sm"
              kind="danger"
              icon="x"
              onClick={() => onAction("terminate")}
            >
              terminate
            </Btn>
          </div>
          <div style={{ marginTop: 10 }}>
            <Btn
              kind="ghost"
              size="sm"
              icon="arrow-right"
              onClick={() => onOpen(node)}
            >
              open node detail
            </Btn>
          </div>
        </div>
      </div>
    </div>
  );
}
