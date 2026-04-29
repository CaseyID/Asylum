import type { JSX } from "react";
import { Btn, Wordmark } from "../lib/ui";
import type { ScreenId } from "../types";

export interface TopbarProps {
  screen: ScreenId;
  openNodeId?: string;
  liveCount: number;
  theme: "dark" | "light";
  onToggleTheme: () => void;
  onOpenCmdK: () => void;
}

export function Topbar({
  screen,
  openNodeId,
  liveCount,
  theme,
  onToggleTheme,
  onOpenCmdK,
}: TopbarProps): JSX.Element {
  const crumbLabel = screen === "node" && openNodeId ? openNodeId : screen;

  return (
    <div className="topbar">
      <Wordmark />
      <div className="crumbs">
        <span className="sep">/</span>
        <span style={{ color: "var(--fg)" }}>{crumbLabel}</span>
        {screen === "cockpit" && (
          <span className="live" style={{ marginLeft: 6 }}>
            <span className="dot" /> {liveCount} running
          </span>
        )}
      </div>
      <div className="topbar-right">
        <Btn kind="ghost" size="sm" icon="search" onClick={onOpenCmdK}>
          <span style={{ marginRight: 6, color: "var(--fg-muted)" }}>search…</span>
          <span className="kbd">⌘K</span>
        </Btn>
        <Btn
          kind="ghost"
          size="sm"
          iconOnly
          icon={theme === "dark" ? "sun" : "moon"}
          onClick={onToggleTheme}
          title="toggle theme"
        />
      </div>
    </div>
  );
}
