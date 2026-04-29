// ports prototype LogsScreen — backed by real NotificationRecord[] instead of mock LOGS
import { useState, type JSX } from "react";
import { Btn } from "../lib/ui";
import { Icon } from "../lib/icons";
import { shortNodeId } from "../lib/glyphs";
import type { NotificationRecord } from "../types";

export interface LogsScreenProps {
  notifications: NotificationRecord[];
}

function fmtTs(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

type LvlKey = "info" | "warn" | "err";

function severityToLvl(severity: string): LvlKey {
  if (severity === "warn") return "warn";
  if (severity === "error") return "err";
  return "info";
}

interface LogRow {
  ts: string;
  lvl: LvlKey;
  src: string;
  msg: string;
}

function toRow(n: NotificationRecord): LogRow {
  return {
    ts: fmtTs(n.created_at),
    lvl: severityToLvl(n.severity),
    src: n.node_id ? shortNodeId(n.node_id) : "asylum",
    msg: n.body && n.body.trim() ? `${n.title} · ${n.body}` : n.title,
  };
}

export function LogsScreen({ notifications }: LogsScreenProps): JSX.Element {
  const [filter, setFilter] = useState<string>("");
  const [lvl, setLvl] = useState<string>("all");

  const rows = notifications.map(toRow);

  const filtered = rows.filter((r) => {
    if (filter && !(r.msg.includes(filter) || r.src.includes(filter))) return false;
    if (lvl !== "all" && r.lvl !== lvl) return false;
    return true;
  });

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
      <div className="page" style={{ paddingBottom: 0, flex: "none" }}>
        <div className="page-head">
          <div>
            <h1 className="page-title">logs &amp; telemetry</h1>
            <div className="page-sub">
              unified event stream across nodes, substrates, and the asylum service
            </div>
          </div>
          <div className="page-actions">
            <Btn icon="filter" size="sm">
              filter
            </Btn>
            <Btn icon="download" size="sm">
              export
            </Btn>
            <Btn icon="play" size="sm">
              tail live
            </Btn>
          </div>
        </div>
      </div>
      <div style={{ padding: "0 36px 12px", display: "flex", gap: 8, alignItems: "center" }}>
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
          {["all", "info", "warn", "err", "run", "dbg"].map((l) => (
            <button
              key={l}
              className={`btn btn-sm ${lvl === l ? "btn-secondary" : "btn-ghost"}`}
              onClick={() => setLvl(l)}
            >
              {l}
            </button>
          ))}
        </div>
        <span className="muted mono" style={{ marginLeft: "auto", fontSize: 11 }}>
          {filtered.length} events
        </span>
      </div>
      <div
        className="log"
        style={{ margin: "0 36px 28px", borderTop: "1px solid var(--border)", border: "1px solid var(--border)" }}
      >
        <div
          className="row"
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
          <span className="lvl" style={{ color: "var(--fg-muted)" }}>
            level
          </span>
          <span className="src" style={{ color: "var(--fg-muted)" }}>
            source
          </span>
          <span></span>
          <span className="msg" style={{ color: "var(--fg-muted)" }}>
            message
          </span>
        </div>
        {filtered.map((r, i) => (
          <div className="row" key={i}>
            <span className="ts">{r.ts}</span>
            <span className={`lvl ${r.lvl}`}>{r.lvl}</span>
            <span className="src">{r.src}</span>
            <span></span>
            <span className="msg">{r.msg}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
