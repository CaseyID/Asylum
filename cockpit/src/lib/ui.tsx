import { Fragment, type CSSProperties, type PropsWithChildren, type ReactNode, useEffect } from "react";
import { Icon } from "./icons";
import type { UiState } from "../types";

// ─── pill / status chip ────────────────────────────────────────────
export function Pill({ status, children }: { status: UiState | string; children: ReactNode }) {
  return (
    <span className={`pill pill-${status}`}>
      <span className="dot" />
      {children}
    </span>
  );
}

// ─── tag ───────────────────────────────────────────────────────────
export function Tag({
  children,
  kind = "",
  future,
}: PropsWithChildren<{ kind?: string; future?: boolean }>) {
  return <span className={`tag ${kind} ${future ? "future" : ""}`}>{children}</span>;
}

// ─── wordmark ──────────────────────────────────────────────────────
export function Wordmark({ size = 14 }: { size?: number }) {
  return (
    <span className="wm" style={{ fontSize: size }}>
      <span className="b">[</span>asylum<span className="b">]</span>
    </span>
  );
}

// ─── button ────────────────────────────────────────────────────────
type BtnKind = "primary" | "secondary" | "ghost" | "danger";

interface BtnProps {
  kind?: BtnKind;
  size?: "sm";
  icon?: string;
  iconOnly?: boolean;
  children?: ReactNode;
  onClick?: (e: React.MouseEvent<HTMLButtonElement>) => void;
  title?: string;
  disabled?: boolean;
  type?: "button" | "submit";
  style?: CSSProperties;
  className?: string;
}

export function Btn({
  kind = "secondary",
  size,
  icon,
  iconOnly,
  children,
  onClick,
  title,
  disabled,
  type = "button",
  style,
  className,
}: BtnProps) {
  return (
    <button
      type={type}
      className={`btn btn-${kind} ${size === "sm" ? "btn-sm" : ""} ${iconOnly ? "btn-icon" : ""} ${className ?? ""}`}
      onClick={onClick}
      title={title}
      disabled={disabled}
      style={style}
    >
      {icon && <Icon name={icon} size={size === "sm" ? 12 : 14} />}
      {children}
    </button>
  );
}

// ─── field ─────────────────────────────────────────────────────────
export function Field({
  label,
  hint,
  children,
}: PropsWithChildren<{ label?: ReactNode; hint?: ReactNode }>) {
  return (
    <div className="field">
      {label != null && <span className="field-label">{label}</span>}
      {children}
      {hint != null && <span className="field-hint">{hint}</span>}
    </div>
  );
}

// ─── panel ─────────────────────────────────────────────────────────
export function Panel({
  title,
  eyebrow,
  actions,
  children,
  flush,
}: PropsWithChildren<{
  title?: ReactNode;
  eyebrow?: ReactNode;
  actions?: ReactNode;
  flush?: boolean;
}>) {
  return (
    <div className="panel">
      {(title || actions || eyebrow) && (
        <div className="panel-head">
          {eyebrow != null && (
            <>
              <span className="b">[</span>
              <span>{eyebrow}</span>
              <span className="b">]</span>
            </>
          )}
          {title != null && <span>{title}</span>}
          {actions != null && <span className="right">{actions}</span>}
        </div>
      )}
      <div className={`panel-body ${flush ? "flush" : ""}`}>{children}</div>
    </div>
  );
}

// ─── kv ─────────────────────────────────────────────────────────────
type KVRow = [ReactNode, ReactNode] | [ReactNode, ReactNode, true];
export function KV({ items }: { items: KVRow[] }) {
  return (
    <div className="kv">
      {items.map(([k, v, sansFlag], i) => (
        <Fragment key={i}>
          <span className="k">{k}</span>
          <span className={`v ${sansFlag ? "sans" : ""}`}>{v}</span>
        </Fragment>
      ))}
    </div>
  );
}

// ─── modal ─────────────────────────────────────────────────────────
export function Modal({
  title,
  onClose,
  children,
  foot,
  width,
}: PropsWithChildren<{
  title: ReactNode;
  onClose: () => void;
  foot?: ReactNode;
  width?: number;
}>) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="scrim" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} style={width ? { width } : undefined}>
        <div className="modal-head">
          <span className="t">
            <span className="b" style={{ opacity: 0.5 }}>[</span> {title}{" "}
            <span className="b" style={{ opacity: 0.5 }}>]</span>
          </span>
          <span className="x" onClick={onClose}>
            ×
          </span>
        </div>
        <div className="modal-body">{children}</div>
        {foot != null && <div className="modal-foot">{foot}</div>}
      </div>
    </div>
  );
}

// ─── empty state ───────────────────────────────────────────────────
export function Empty({
  glyph = "[ ]",
  lead,
  sub,
  action,
}: {
  glyph?: ReactNode;
  lead: ReactNode;
  sub?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="glyph">{glyph}</div>
      <div className="lead">{lead}</div>
      {sub != null && <div className="sub">{sub}</div>}
      {action != null && <div style={{ marginTop: 18 }}>{action}</div>}
    </div>
  );
}

// ─── ToolCall (structured view of a tool invocation) ───────────────
import { useState } from "react";

export function ToolCall({
  name,
  args,
  output,
  state = "ok",
  collapsed = true,
}: {
  name: string;
  args?: Record<string, unknown>;
  output?: string;
  state?: "ok" | "pending" | "error";
  collapsed?: boolean;
}) {
  const [open, setOpen] = useState(!collapsed);
  return (
    <div className="tcall">
      <div className="h">
        <Icon name="wrench" size={11} />
        <span>{name}</span>
        <span className="right">
          {state === "ok" && <span style={{ color: "var(--status-running)", fontSize: 11 }}>✓ ok</span>}
          {state === "pending" && <span style={{ color: "var(--status-waiting)", fontSize: 11 }}>· pending</span>}
          {state === "error" && <span style={{ color: "var(--status-errored)", fontSize: 11 }}>! err</span>}
          {output && (
            <span
              onClick={() => setOpen(!open)}
              style={{ cursor: "pointer", marginLeft: 8, opacity: 0.7 }}
            >
              {open ? "−" : "+"}
            </span>
          )}
        </span>
      </div>
      {args && (
        <div className="args">
          {Object.entries(args).map(([k, v]) => (
            <div key={k}>
              <span className="muted">{k}:</span> <span className="arg">{String(v)}</span>
            </div>
          ))}
        </div>
      )}
      {output && <div className={`out ${open ? "" : "collapsed"}`}>{output}</div>}
    </div>
  );
}
