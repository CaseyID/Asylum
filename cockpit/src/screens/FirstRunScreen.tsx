import type { JSX } from "react";
import { Btn, Wordmark } from "../lib/ui";

export interface FirstRunScreenProps {
  onLaunch: () => void;
  onOpenCli: () => void;
  onReadSpec: () => void;
  harnessCount: number;
  substrateCount: number;
  nodeCount: number;
}

const STEPS: [string, string][] = [
  ["open cockpit", "this screen — fleet view, command center, inspector"],
  ["start a command-center", "codex or claude code, with asylum context preloaded"],
  ["ask it to spawn workers", '"refactor the router with 2 workers on loon-us-west"'],
  ["watch the graph", "spawned nodes appear as supervisor / worker cards with explicit edges"],
  ["inspect any node", "live transcript, capability matrix"],
  ["attach in browser", "real harness ui at any time, no native install required"],
  ["receive ntfy", "remote command channel — reply with `approve`, `attach`, `retry`"],
  ["drive from mcp", "asylum tools available in claude desktop, cursor, anything mcp-capable"],
  ["hand off to loon", "workers boot in firecracker vms — same capability surface"],
];

export function FirstRunScreen({
  onLaunch,
  onOpenCli,
  onReadSpec,
  harnessCount,
  substrateCount,
  nodeCount,
}: FirstRunScreenProps): JSX.Element {
  return (
    <div className="firstrun">
      <div className="left">
        <div style={{ position: "relative" }}>
          <Wordmark size={20} />
        </div>
        <div className="hero">
          <div className="mono-eyebrow">{"["} asylum · single-user · localhost {"]"}</div>
          <h1>
            a control plane for the agent harnesses you already use.{" "}
            <span className="b">[</span>not a harness<span className="b">]</span>.
          </h1>
          <p>
            asylum doesn't replace codex, claude code, or anything else. it launches them, gives
            them shared context and tools, lets them coordinate, and lets you reach them from
            anywhere.
          </p>
        </div>
        <div className="actions">
          <Btn kind="primary" icon="play" onClick={onLaunch}>
            start a command center
          </Btn>
          <Btn icon="terminal" onClick={onOpenCli}>
            open cli
          </Btn>
          <Btn kind="ghost" icon="book-open" onClick={onReadSpec}>
            read the spec
          </Btn>
        </div>
        <div
          style={{
            position: "relative",
            marginTop: "auto",
            display: "flex",
            gap: 24,
            fontFamily: "var(--font-mono)",
            fontSize: 11,
            color: "var(--fg-subtle)",
          }}
        >
          <span>{harnessCount} harnesses ready</span>
          <span>·</span>
          <span>{substrateCount} substrates configured</span>
          <span>·</span>
          <span>{nodeCount} nodes alive</span>
        </div>
      </div>
      <div className="right">
        <div className="checklist-head">{"["} wow sequence {"]"}</div>
        {STEPS.map(([title, sub], i) => (
          <div className="check" key={i}>
            <span className="num">{String(i + 1).padStart(2, "0")}</span>
            <div className="body">
              {title}
              <div className="sub">{sub}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
