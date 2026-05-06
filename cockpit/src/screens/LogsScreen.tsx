import { useMemo, useState, type JSX } from "react";
import { shortNodeId } from "../lib/glyphs";
import { Btn } from "../lib/ui";
import type { NotificationRecord } from "../types";
import { Icon } from "../lib/icons";

export interface LogsScreenProps {
  notifications: NotificationRecord[];
  onMarkRead: (id: string) => Promise<void>;
  onOpenNode?: (id: string) => void;
}

type LvlKey = "info" | "warn" | "err";
type FilterScope = "all" | "unread";

function fmtTs(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

function severityToLvl(severity: string): LvlKey {
  if (severity === "warn") return "warn";
  if (severity === "error") return "err";
  return "info";
}

interface LogRow {
  id: string;
  ts: string;
  lvl: LvlKey;
  src: string;
  msg: string;
  read: boolean;
  nodeId?: string;
}

function toRow(n: NotificationRecord): LogRow {
  return {
    id: n.id,
    ts: fmtTs(n.created_at),
    lvl: severityToLvl(n.severity),
    src: n.node_id ? shortNodeId(n.node_id) : "asylum",
    msg: n.body && n.body.trim() ? `${n.title} · ${n.body}` : n.title,
    read: n.read,
    nodeId: n.node_id ?? undefined,
  };
}

export function LogsScreen({ notifications, onMarkRead, onOpenNode }: LogsScreenProps): JSX.Element {
  const [filter, setFilter] = useState<string>("");
  const [lvl, setLvl] = useState<string>("all");
  const [scope, setScope] = useState<FilterScope>("all");
  const [markingId, setMarkingId] = useState<string | null>(null);

  const rows = notifications.map(toRow);
  const unreadCount = rows.filter((n) => !n.read).length;
  const totalCount = rows.length;

  const filtered = useMemo(() => {
    return rows.filter((r) => {
      if (scope === "unread" && r.read) return false;
      if (filter && !(r.msg.includes(filter) || r.src.includes(filter))) return false;
      if (lvl !== "all" && r.lvl !== lvl) return false;
      return true;
    });
  }, [rows, filter, lvl, scope]);

  async function handleMarkRead(id: string): Promise<void> {
    if (markingId === id) return;
    setMarkingId(id);
    try {
      await onMarkRead(id);
    } finally {
      setMarkingId(null);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
      <div className="page" style={{ paddingBottom: 0, flex: "none" }}>
        <div className="page-head">
          <div>
            <h1 className="page-title">logs &amp; telemetry</h1>
            <div className="page-sub">
              daemon notification records with read-state and node context where available
            </div>
          </div>
        </div>
      </div>
      <div className="logs-toolbar">
        <div className="search" style={{ flex: "0 1 320px", position: "relative" }}>
          <Icon
            name="search"
            size={12}
            style={{ position: "absolute", left: 9, top: "50%", transform: "translateY(-50%)", opacity: 0.5 }}
          />
          <input
            className="input mono"
            placeholder="filter by source or message…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            style={{ paddingLeft: 28 }}
          />
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          {(["all", "unread"] as FilterScope[]).map((scopeFilter) => (
            <button
              key={scopeFilter}
              className={`btn btn-sm ${scope === scopeFilter ? "btn-secondary" : "btn-ghost"}`}
              onClick={() => setScope(scopeFilter)}
            >
              {scopeFilter === "all" ? `all (${totalCount})` : `unread (${unreadCount})`}
            </button>
          ))}
          <div style={{ width: 4 }} />
          {["all", "info", "warn", "err"].map((l) => (
            <button
              key={l}
              className={`btn btn-sm ${lvl === l ? "btn-secondary" : "btn-ghost"}`}
              onClick={() => setLvl(l)}
            >
              {l}
            </button>
          ))}
        </div>
        <span className="muted mono logs-count">
          {filtered.length} / {totalCount} events ({unreadCount} unread)
        </span>
      </div>
      <div
        className="log"
        style={{ margin: "0 36px 28px", borderTop: "1px solid var(--border)", border: "1px solid var(--border)" }}
      >
        <div
          className="row log-head"
          style={{
            background: "var(--bg-elev)",
            borderBottom: "1px solid var(--border)",
            fontSize: 10,
            letterSpacing: 0.06,
          }}
        >
          <span className="ts" style={{ color: "var(--fg-muted)" }}>
            time
          </span>
          <span className="status-head" style={{ color: "var(--fg-muted)" }}>
            state
          </span>
          <span className="lvl" style={{ color: "var(--fg-muted)" }}>
            level
          </span>
          <span className="src" style={{ color: "var(--fg-muted)" }}>
            source
          </span>
          <span style={{ color: "var(--fg-muted)" }}>message</span>
          <span style={{ color: "var(--fg-muted)" }}>node</span>
          <span style={{ color: "var(--fg-muted)", textAlign: "right" }}>actions</span>
        </div>
        {filtered.map((r) => (
          <div className="row" key={r.id}>
            <span className="ts">{r.ts}</span>
            <span className={`status ${r.read ? "read" : "unread"}`}>{r.read ? "read" : "unread"}</span>
            <span className={`lvl ${r.lvl}`}>{r.lvl}</span>
            <span className="src">{r.src}</span>
            <span className="msg">{r.msg}</span>
            <span className="mono">{r.nodeId ? shortNodeId(r.nodeId) : "—"}</span>
            <span style={{ display: "flex", justifyContent: "flex-end", gap: 6 }}>
              {r.nodeId && (
                <Btn size="sm" icon="external-link" onClick={() => onOpenNode?.(r.nodeId!)}>
                  open
                </Btn>
              )}
              {!r.read && (
                <Btn
                  size="sm"
                  onClick={() => void handleMarkRead(r.id)}
                  disabled={markingId === r.id}
                  title="Mark this notification as read"
                >
                  {markingId === r.id ? "marking..." : "mark read"}
                </Btn>
              )}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
