import { useState, type JSX } from "react";
import { Btn, Modal, Wordmark } from "../lib/ui";

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
  ["open attach tab", "real harness ui at any time, no extra install required"],
  ["receive ntfy", "remote command channel — reply with `approve`, `attach`, `retry`"],
  ["drive from mcp", "asylum tools available in claude desktop, cursor, anything mcp-capable"],
  ["hand off to loon", "workers boot in firecracker vms — same capability surface"],
];

const SPEC_PATH = "docs/specs/asylum-current-product-spec.md";

const CLI_COMMANDS: [string, string][] = [
  ["asylum status", "show daemon status and connected nodes"],
  ["asylum start", "start the daemon (systemd user service)"],
  ["asylum stop", "stop the daemon"],
  ["asylum cockpit", "open the cockpit in your browser"],
  ["asylum doctor", "diagnose PATH, harness binaries, and config issues"],
  ["asylum logs", "tail the daemon log"],
  ["asylum node list", "list all nodes"],
  ["asylum node stop <id>", "stop a running node"],
];

export function FirstRunScreen({
  onLaunch,
  onOpenCli: _onOpenCli,
  onReadSpec: _onReadSpec,
  harnessCount,
  substrateCount,
  nodeCount,
}: FirstRunScreenProps): JSX.Element {
  const [cliOpen, setCliOpen] = useState(false);
  const [specOpen, setSpecOpen] = useState(false);
  const [specCopied, setSpecCopied] = useState(false);

  function handleOpenCli() {
    setCliOpen(true);
  }

  function handleReadSpec() {
    setSpecOpen(true);
    setSpecCopied(false);
    if (typeof navigator !== "undefined" && navigator.clipboard) {
      navigator.clipboard
        .writeText(SPEC_PATH)
        .then(() => setSpecCopied(true))
        .catch(() => {});
    }
  }

  return (
    <div className="firstrun">
      {cliOpen && (
        <Modal title="asylum cli" onClose={() => setCliOpen(false)} width={560}>
          <div
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: 12,
              display: "flex",
              flexDirection: "column",
              gap: 0,
            }}
          >
            {CLI_COMMANDS.map(([cmd, desc]) => (
              <div
                key={cmd}
                style={{
                  display: "grid",
                  gridTemplateColumns: "1fr 1fr",
                  gap: 12,
                  padding: "7px 0",
                  borderBottom: "1px solid var(--border-subtle)",
                }}
              >
                <code style={{ color: "var(--fg)" }}>{cmd}</code>
                <span style={{ color: "var(--fg-muted)", fontSize: 11 }}>{desc}</span>
              </div>
            ))}
          </div>
          <div
            style={{
              marginTop: 14,
              padding: "8px 12px",
              background: "var(--bg-sunken)",
              border: "1px solid var(--border-subtle)",
              fontFamily: "var(--font-mono)",
              fontSize: 11,
              color: "var(--fg-muted)",
            }}
          >
            run <code style={{ color: "var(--fg)" }}>asylum --help</code> for the full command reference
          </div>
        </Modal>
      )}

      {specOpen && (
        <Modal title="asylum spec" onClose={() => setSpecOpen(false)} width={520}>
          <div
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: 12,
              display: "flex",
              flexDirection: "column",
              gap: 12,
            }}
          >
            <div style={{ color: "var(--fg-muted)" }}>
              the current product spec lives at:
            </div>
            <code
              style={{
                color: "var(--fg)",
                background: "var(--bg-sunken)",
                border: "1px solid var(--border-subtle)",
                padding: "10px 12px",
                userSelect: "all",
              }}
            >
              {SPEC_PATH}
            </code>
            <div style={{ color: "var(--fg-muted)", fontSize: 11 }}>
              {specCopied
                ? "path copied to your clipboard."
                : "open it in your editor (the path above is selectable)."}
            </div>
          </div>
        </Modal>
      )}

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
          <Btn icon="terminal" onClick={handleOpenCli}>
            open cli
          </Btn>
          <Btn kind="ghost" icon="book-open" onClick={handleReadSpec}>
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
