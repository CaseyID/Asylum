import type { JSX } from "react";
import { Icon } from "../lib/icons";
import type { ScreenId } from "../types";

export interface NavItem {
  id: ScreenId | "__launch";
  label: string;
  icon: string;
  count?: number;
  primary?: boolean;
}

export interface NavProps {
  collapsed: boolean;
  active: ScreenId;
  fleetCount: number;
  channelCount: number;
  hookCount: number;
  daemonVersion?: string;
  bindAddr?: string;
  onPick: (id: ScreenId | "__launch") => void;
}

export function Nav({
  collapsed,
  active,
  fleetCount,
  channelCount,
  hookCount,
  daemonVersion,
  bindAddr,
  onPick,
}: NavProps): JSX.Element {
  const mainItems: NavItem[] = [
    { id: "cockpit", label: "cockpit", icon: "layout-grid" },
    { id: "fleet", label: "nodes", icon: "list", count: fleetCount },
    { id: "chat", label: "chat", icon: "terminal" },
    { id: "decisions", label: "decisions", icon: "git-pull-request" },
    { id: "logs", label: "logs", icon: "activity" },
  ];

  const messagingItems: NavItem[] = [
    { id: "channels", label: "channels", icon: "rss", count: channelCount },
    { id: "hooks", label: "hooks", icon: "zap", count: hookCount },
  ];

  const bottomItems: NavItem[] = [
    { id: "__launch", label: "launch node", icon: "plus", primary: true },
    { id: "settings", label: "settings", icon: "settings" },
  ];

  function isActive(id: ScreenId | "__launch"): boolean {
    return id !== "__launch" && active === id;
  }

  function renderItem(item: NavItem) {
    return (
      <div
        key={item.id}
        className={`item ${isActive(item.id) ? "active" : ""}`}
        onClick={() => onPick(item.id)}
        title={collapsed ? item.label : undefined}
        style={item.primary ? { color: "var(--fg)" } : undefined}
      >
        <Icon name={item.icon} />
        {!collapsed && <span className="label">{item.label}</span>}
        {!collapsed && item.count !== undefined && (
          <span className="count">{item.count}</span>
        )}
      </div>
    );
  }

  return (
    <div className="nav">
      {!collapsed && <div className="group-label">cockpit</div>}
      {mainItems.map(renderItem)}

      {!collapsed && (
        <div className="group-label" style={{ marginTop: 18 }}>
          messaging
        </div>
      )}
      {messagingItems.map(renderItem)}

      <div className="spacer" />

      {!collapsed && <div className="group-label">tools</div>}
      {bottomItems.map(renderItem)}

      {!collapsed && (
        <div className="footer-info">
          <div>{daemonVersion ?? "asylum"}</div>
          {bindAddr && <div className="muted">{bindAddr}</div>}
        </div>
      )}
    </div>
  );
}
