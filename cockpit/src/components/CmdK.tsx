import { Fragment, useState, type JSX } from "react";
import { Icon } from "../lib/icons";
import type { AsylumNode, ScreenId } from "../types";
import { shortNodeId } from "../lib/glyphs";

export interface CmdKProps {
  onClose: () => void;
  onPick: (screen: ScreenId) => void;
  onLaunch: () => void;
  onPickNode: (node: AsylumNode) => void;
  onAttachInBrowser: () => void;
  onSendRemoteCommand: () => void;
  nodes: AsylumNode[];
}

interface CmdKItem {
  sec: string;
  label: string;
  icon: string;
  action: () => void;
}

export function CmdK({
  onClose,
  onPick,
  onLaunch,
  onPickNode,
  onAttachInBrowser,
  onSendRemoteCommand,
  nodes,
}: CmdKProps): JSX.Element {
  const baseItems: CmdKItem[] = [
    {
      sec: "actions",
      label: "launch new node…",
      icon: "plus",
      action: () => onLaunch(),
    },
    {
      sec: "actions",
      label: "open attach tab…",
      icon: "external-link",
      action: () => onAttachInBrowser(),
    },
    {
      sec: "actions",
      label: "send remote command…",
      icon: "send",
      action: () => onSendRemoteCommand(),
    },
    {
      sec: "go to",
      label: "cockpit",
      icon: "layout-grid",
      action: () => onPick("cockpit"),
    },
    {
      sec: "go to",
      label: "fleet",
      icon: "list",
      action: () => onPick("fleet"),
    },
    {
      sec: "go to",
      label: "channels",
      icon: "rss",
      action: () => onPick("channels"),
    },
    {
      sec: "go to",
      label: "chat",
      icon: "terminal",
      action: () => onPick("chat"),
    },
    {
      sec: "go to",
      label: "hooks",
      icon: "zap",
      action: () => onPick("hooks"),
    },
    {
      sec: "go to",
      label: "logs",
      icon: "activity",
      action: () => onPick("logs"),
    },
    {
      sec: "go to",
      label: "settings",
      icon: "settings",
      action: () => onPick("settings"),
    },
  ];

  const nodeItems: CmdKItem[] = nodes.map((n) => ({
    sec: "nodes",
    label: `${shortNodeId(n.id)} · ${n.role_hint} · ${n.harness}`,
    icon: n.role_hint === "command-center" ? "circle" : "square",
    action: () => onPickNode(n),
  }));

  const items = [...baseItems, ...nodeItems];

  const [q, setQ] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const filtered = items.filter((x) => x.label.toLowerCase().includes(q.toLowerCase()));

  function onKey(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "ArrowDown") {
      setActiveIndex((v) => Math.min(v + 1, filtered.length - 1));
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      setActiveIndex((v) => Math.max(v - 1, 0));
      e.preventDefault();
    } else if (e.key === "Enter") {
      filtered[activeIndex]?.action();
      onClose();
      e.preventDefault();
    } else if (e.key === "Escape") {
      onClose();
    }
  }

  let lastSec = "";

  return (
    <div className="scrim" onClick={onClose}>
      <div className="cmdk" onClick={(e) => e.stopPropagation()}>
        <input
          autoFocus
          className="cmdk-input"
          placeholder="search nodes, jump to screens, run actions"
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setActiveIndex(0);
          }}
          onKeyDown={onKey}
        />
        <div className="cmdk-list">
          {filtered.map((item, idx) => {
            const showSection = item.sec !== lastSec;
            lastSec = item.sec;
            return (
              <Fragment key={idx}>
                {showSection && <div className="cmdk-section">{item.sec}</div>}
                <div
                  className={`cmdk-item ${idx === activeIndex ? "active" : ""}`}
                  onMouseEnter={() => setActiveIndex(idx)}
                  onClick={() => {
                    item.action();
                    onClose();
                  }}
                >
                  <Icon name={item.icon} size={14} />
                  <span>{item.label}</span>
                </div>
              </Fragment>
            );
          })}
        </div>
        <div className="cmdk-foot">
          <span>
            <b>↵</b> run
          </span>
          <span>
            <b>↑↓</b> navigate
          </span>
          <span>
            <b>esc</b> close
          </span>
        </div>
      </div>
    </div>
  );
}
