import { Fragment, useState, type JSX } from "react";
import { Icon } from "../lib/icons";
import type { ScreenId } from "../types";

export interface CmdKProps {
  onClose: () => void;
  onPick: (screen: ScreenId) => void;
  onLaunch: () => void;
}

interface CmdKItem {
  sec: string;
  label: string;
  kbd: string;
  icon: string;
  action: () => void;
}

export function CmdK({ onClose, onPick, onLaunch }: CmdKProps): JSX.Element {
  const items: CmdKItem[] = [
    {
      sec: "actions",
      label: "launch new node…",
      kbd: "N",
      icon: "plus",
      action: () => {
        onLaunch();
      },
    },
    {
      sec: "actions",
      label: "attach in browser…",
      kbd: "A",
      icon: "external-link",
      action: () => {
        onPick("cockpit");
      },
    },
    {
      sec: "actions",
      label: "send remote command…",
      kbd: "R",
      icon: "send",
      action: () => {},
    },
    {
      sec: "go to",
      label: "cockpit",
      kbd: "1",
      icon: "layout-grid",
      action: () => onPick("cockpit"),
    },
    {
      sec: "go to",
      label: "fleet",
      kbd: "2",
      icon: "list",
      action: () => onPick("fleet"),
    },
    {
      sec: "go to",
      label: "channels",
      kbd: "3",
      icon: "rss",
      action: () => onPick("channels"),
    },
    {
      sec: "go to",
      label: "chat",
      kbd: "4",
      icon: "terminal",
      action: () => onPick("chat"),
    },
    {
      sec: "go to",
      label: "hooks",
      kbd: "5",
      icon: "zap",
      action: () => onPick("hooks"),
    },
    {
      sec: "go to",
      label: "logs",
      kbd: "6",
      icon: "activity",
      action: () => onPick("logs"),
    },
    {
      sec: "go to",
      label: "settings",
      kbd: ",",
      icon: "settings",
      action: () => onPick("settings"),
    },
  ];

  const [q, setQ] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const filtered = items.filter((x) =>
    x.label.toLowerCase().includes(q.toLowerCase()),
  );

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
          placeholder="run a command, jump to a screen, find a node…"
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
                {showSection && (
                  <div className="cmdk-section">{item.sec}</div>
                )}
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
                  <span className="k">{item.kbd}</span>
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
