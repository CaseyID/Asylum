// ports prototype HooksScreen, HookCard, HookEditor — backed by daemon /hooks api
import { useEffect, useMemo, useState, type JSX } from "react";
import { Btn, Empty, Field, Modal, Pill } from "../lib/ui";
import {
  createHook,
  deleteHook,
  dryRunHook,
  fetchHookEvents,
  fetchHookFirings,
  fetchHooks,
  updateHook,
} from "../api";
import type {
  HookAction,
  HookEventCatalogEntry,
  HookFiringRecord,
  HookRule,
} from "../types";


function fmtTs(epoch: number): string {
  if (!epoch) return "—";
  const d = new Date(epoch * 1000);
  if (Number.isNaN(d.getTime())) return "—";
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

function fmtRelative(epoch: number): string {
  if (!epoch) return "—";
  const delta = Math.max(0, Math.floor(Date.now() / 1000 - epoch));
  if (delta < 30) return "now";
  if (delta < 60) return `${delta}s ago`;
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
}

interface HookStats {
  fired24h: number;
  lastAt: string;
}

function statsFor(hookId: string, firings: HookFiringRecord[]): HookStats {
  const now = Date.now() / 1000;
  let fired24h = 0;
  let latest = 0;
  for (const f of firings) {
    if (f.hook_id !== hookId) continue;
    if (now - f.ts_epoch_secs < 86400) fired24h += 1;
    if (f.ts_epoch_secs > latest) latest = f.ts_epoch_secs;
  }
  return { fired24h, lastAt: latest ? fmtRelative(latest) : "—" };
}

function HookCard({
  hook,
  stats,
  onToggle,
  onEdit,
  onDryRun,
  disabled,
}: {
  hook: HookRule;
  stats: HookStats;
  onToggle: () => void;
  onEdit: () => void;
  onDryRun: () => void;
  disabled?: boolean;
}) {
  return (
    <div className={`hook-card ${hook.enabled ? "" : "off"} ${hook.future ? "future" : ""}`}>
      <div className="hd">
        <span className={`led ${hook.enabled ? "on" : "off"}`} />
        <span className="nm">{hook.name}</span>
        <span className="tog">
          <button
            className={`toggle ${hook.enabled ? "on" : ""}`}
            onClick={onToggle}
            title={hook.enabled ? "disable" : "enable"}
            disabled={disabled}
          >
            <span className="knob" />
          </button>
        </span>
      </div>

      <div className="when">
        <span className="lab">when</span>
        <code className="evt">{hook.event}</code>
        {hook.filter && hook.filter !== "any" && (
          <>
            <span className="lab">where</span>
            <code className="filt">{hook.filter}</code>
          </>
        )}
      </div>

      <div className="then">
        <span className="lab">then</span>
        <ol>
          {hook.actions.map((a, i) => (
            <li key={i}>
              <span className="step">{i + 1}</span>
              <span className="kind">{a.kind}</span>
              <code className="trg">{a.target}</code>
              {a.template && <span className="tpl">{`"${a.template}"`}</span>}
            </li>
          ))}
        </ol>
      </div>

      <div className="ft">
        <span className="stat">
          <span className="lab">fired</span> <b>{stats.fired24h}</b>
          <span className="lab">/24h</span>
        </span>
        <span className="stat">
          <span className="lab">last</span> <b>{stats.lastAt}</b>
        </span>
        <span className="stat r">
          <Btn
            size="sm"
            kind="ghost"
            icon="play"
            iconOnly
            title="dry-run"
            onClick={onDryRun}
            disabled={disabled}
          />
          <Btn size="sm" kind="ghost" icon="edit-2" iconOnly title="edit" onClick={onEdit} />
        </span>
      </div>
    </div>
  );
}

// send_input and spawn are honest, always-available hook actions (W4).
const ACTION_KINDS = ["channel", "send_input", "spawn", "tool", "pause_node", "archive"];

function HookEditor({
  hookId,
  presetEvent,
  hooks,
  events,
  onClose,
  onSaved,
}: {
  hookId: string;
  presetEvent?: string;
  hooks: HookRule[];
  events: HookEventCatalogEntry[];
  onClose: () => void;
  onSaved: () => Promise<void> | void;
}) {
  const allowedActionKinds = ACTION_KINDS;

  const isNew = hookId === "__new";
  const existing = isNew ? null : hooks.find((h) => h.id === hookId) ?? null;

  const fallbackEvent = presetEvent ?? events[0]?.id ?? "node.permission_requested";
  const [name, setName] = useState<string>(existing?.name ?? "");
  const [event, setEvent] = useState<string>(existing?.event ?? fallbackEvent);
  const [filter, setFilter] = useState<string>(existing?.filter ?? "");
  const [actions, setActions] = useState<HookAction[]>(
    existing?.actions ?? [
      { kind: "channel", target: "ntfy-default", template: "{node.id} triggered" },
    ],
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  function setAction(i: number, patch: Partial<HookAction>) {
    setActions((cur) => cur.map((a, idx) => (idx === i ? { ...a, ...patch } : a)));
  }
  function addAction() {
    setActions((cur) => [...cur, { kind: "channel", target: "" }]);
  }
  function removeAction(i: number) {
    setActions((cur) => cur.filter((_, idx) => idx !== i));
  }

  async function onSave() {
    setBusy(true);
    setError(null);
    try {
      if (isNew) {
        await createHook({ name, enabled: true, event, filter, actions });
      } else {
        await updateHook(hookId, { name, event, filter, actions });
      }
      await onSaved();
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function onDelete() {
    if (isNew) return;
    setBusy(true);
    setError(null);
    try {
      await deleteHook(hookId);
      await onSaved();
      onClose();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={isNew ? "new hook" : `edit · ${existing?.name ?? hookId}`}
      onClose={onClose}
      width={620}
      foot={
        <>
          {!isNew && (
            <Btn kind="danger" icon="trash-2" onClick={onDelete} disabled={busy}>
              delete
            </Btn>
          )}
          <Btn onClick={onClose} disabled={busy}>
            cancel
          </Btn>
          <Btn kind="primary" icon="save" onClick={onSave} disabled={busy}>
            {isNew ? "create hook" : "save"}
          </Btn>
        </>
      }
    >
      {error && <div className="error">{error}</div>}
      <Field label="name" hint="short, descriptive — shows up in firings log">
        <input
          className="input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. high-context → checkpoint"
        />
      </Field>
      <Field label="when (event)" hint="pick a trigger from the event catalog">
        <select
          className="input"
          value={event}
          onChange={(e) => setEvent(e.target.value)}
        >
          {events.length === 0 && (
            <option value={event}>{event}</option>
          )}
          {events.map((e) => (
            <option key={e.id} value={e.id}>
              {e.id} — {e.label}
            </option>
          ))}
        </select>
      </Field>
      <Field label="where (filter)" hint="optional — runs against event payload (jmespath-like)">
        <input
          className="input mono"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder='e.g. role == "worker" && ctx >= 0.8'
        />
      </Field>
      <Field label="then (actions)" hint="executed in order, halts on failure unless `try` is set">
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {actions.map((a, i) => (
            <div key={i} style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <div className="action-row">
                <span className="step">{i + 1}</span>
                <select
                  className="input mono"
                  value={a.kind}
                  onChange={(e) => setAction(i, { kind: e.target.value })}
                  disabled={!allowedActionKinds.includes(a.kind)}
                  style={{ width: 120 }}
                >
                  {!allowedActionKinds.includes(a.kind) && (
                    <option value={a.kind}>
                      {`${a.kind} (disabled)`}
                    </option>
                  )}
                  {allowedActionKinds.map((k) => (
                    <option key={k} value={k}>
                      {k}
                    </option>
                  ))}
                </select>
                <input
                  className="input mono"
                  value={a.target}
                  onChange={(e) => setAction(i, { target: e.target.value })}
                  style={{ flex: 1 }}
                />
                <Btn
                  size="sm"
                  kind="ghost"
                  icon="x"
                  iconOnly
                  onClick={() => removeAction(i)}
                />
              </div>
              {(a.kind === "channel" || a.kind === "send_input") && (
                <input
                  className="input mono"
                  value={a.template ?? ""}
                  onChange={(e) => setAction(i, { template: e.target.value })}
                  placeholder={
                    a.kind === "send_input"
                      ? "text — e.g. continue: {event}"
                      : "template — e.g. {node.id} triggered"
                  }
                  style={{ marginLeft: 28 }}
                />
              )}
            </div>

          ))}
          <div style={{ alignSelf: "flex-start" }}>
            <Btn size="sm" kind="ghost" icon="plus" onClick={addAction}>
              add action
            </Btn>
          </div>
        </div>
      </Field>
      <div
        className="muted mono"
        style={{
          fontSize: 11,
          marginTop: 12,
          padding: 10,
          background: "var(--bg-sunken)",
          border: "1px solid var(--border-subtle)",
        }}
      >
        <span className="b" style={{ color: "var(--fg)" }}>
          preview
        </span>{" "}
        · this hook will fire when{" "}
        <code style={{ color: "var(--fg)" }}>{event}</code>
        {filter && filter !== "any" && (
          <>
            {" "}
            and <code style={{ color: "var(--fg)" }}>{filter}</code>
          </>
        )}
        , then run {actions.length} action(s).
      </div>
    </Modal>
  );
}

interface DrawerState {
  id: string;
  presetEvent?: string;
}

export function HooksScreen(): JSX.Element {
  const [hooks, setHooks] = useState<HookRule[]>([]);
  const [firings, setFirings] = useState<HookFiringRecord[]>([]);
  const [events, setEvents] = useState<HookEventCatalogEntry[]>([]);
  const [tab, setTab] = useState<string>("rules");

  const [drawer, setDrawer] = useState<DrawerState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [busyHookId, setBusyHookId] = useState<string | null>(null);

  function formatError(prefix: string, err: unknown): string {
    return `${prefix}: ${err instanceof Error ? err.message : String(err)}`;
  }

  async function reloadHooks() {
    try {
      const list = await fetchHooks();
      setHooks(list);
      setLoadError(null);
    } catch (err) {
      setLoadError(formatError("failed to reload hooks", err));
    }
  }

  async function reloadFirings() {
    try {
      const list = await fetchHookFirings();
      setFirings(list);
      setLoadError(null);
    } catch (err) {
      setLoadError(formatError("failed to reload firings", err));
    }
  }

  useEffect(() => {
    reloadHooks();
    fetchHookEvents()

      .then(setEvents)
      .catch((err) => {
        setLoadError(formatError("failed to load event catalog", err));
      });
  }, []);

  useEffect(() => {
    if (tab !== "rules") return;
    const t = setInterval(reloadHooks, 6000);
    return () => clearInterval(t);
  }, [tab]);

  useEffect(() => {
    if (tab !== "firings") return;
    reloadFirings();
    const t = setInterval(reloadFirings, 6000);
    return () => clearInterval(t);
  }, [tab]);

  const enabled = hooks.filter((h) => h.enabled).length;
  const firings24h = useMemo(() => {
    const now = Date.now() / 1000;
    return firings.filter((f) => now - f.ts_epoch_secs < 86400).length;
  }, [firings]);

  async function toggle(hook: HookRule) {
    setActionError(null);
    setBusyHookId(hook.id);
    try {
      const updated = await updateHook(hook.id, { enabled: !hook.enabled });
      setHooks((hs) => hs.map((h) => (h.id === updated.id ? updated : h)));
    } catch (err) {
      setActionError(`toggle failed for ${hook.name}: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusyHookId(null);
    }
  }

  async function onDryRun(hook: HookRule) {
    setActionError(null);
    setBusyHookId(hook.id);
    try {
      const firing = await dryRunHook(hook.id);
      setFirings((cur) => [firing, ...cur]);
      setTab("firings");
    } catch (err) {
      setActionError(`dry-run failed for ${hook.name}: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusyHookId(null);
    }
  }

  return (
    <div className="page hooks-page">
      <div className="page-head">
        <div>
          <h1 className="page-title">hooks</h1>
          <div className="page-sub">
            if-this-then-that for the fleet · {enabled}/{hooks.length} enabled · {firings24h} firings / 24h
          </div>
        </div>
        <div className="page-actions">
          <Btn kind="primary" icon="plus" onClick={() => setDrawer({ id: "__new" })}>
            new hook
          </Btn>
        </div>
      </div>

      {loadError && <div className="error">{loadError}</div>}
      {actionError && <div className="error">{actionError}</div>}

      <div className="hooks-tabs">
        {(
          [
            ["rules", "rules", hooks.length],
            ["firings", "recent firings", firings.length],
            ["catalog", "event catalog", events.length],
          ] as [string, string, number][]
        ).map(([id, lab, ct]) => (
          <div key={id} className={`tab ${tab === id ? "on" : ""}`} onClick={() => setTab(id)}>
            {lab} <span className="ct">{ct}</span>
          </div>
        ))}
      </div>

      {tab === "rules" && (
        hooks.length === 0 ? (
          <Empty
            lead="no rules yet"
            sub="rules show up here when you create one"
            action={
              <Btn kind="primary" icon="plus" onClick={() => setDrawer({ id: "__new" })}>
                new hook
              </Btn>
            }
          />
        ) : (
          <div className="hooks-grid">
            {hooks.map((h) => (
              <HookCard
                key={h.id}
                hook={h}
                stats={statsFor(h.id, firings)}
                onToggle={() => void toggle(h)}
                onEdit={() => setDrawer({ id: h.id })}
                onDryRun={() => void onDryRun(h)}
                disabled={busyHookId === h.id}
              />
            ))}
          </div>
        )
      )}

      {tab === "firings" && (
        firings.length === 0 ? (
          <Empty
            lead="no firings yet"
            sub="firings show up here when a rule triggers"
          />
        ) : (
          <div className="firings-list">
            <div className="firings-head">
              <span>time</span>
              <span>hook</span>
              <span>trigger</span>
              <span>outcome</span>
              <span></span>
            </div>
            {firings.map((f) => {
              const hk = hooks.find((h) => h.id === f.hook_id);
              return (
                <div key={f.id} className="firing-row">
                  <span className="ts">{fmtTs(f.ts_epoch_secs)}</span>
                  <span className="hk">{hk?.name ?? f.hook_id}</span>
                  <span className="tr">
                    <code>{f.trigger}</code>
                  </span>
                  <span className="oc">{f.outcome}</span>
                  <span className="st">
                    {f.ok ? <Pill status="running">ok</Pill> : <Pill status="errored">err</Pill>}
                  </span>
                </div>
              );
            })}
          </div>
        )
      )}

      {tab === "catalog" && (
        <div className="catalog-grid">
          {events.map((e) => (
            <div key={e.id} className="cat-card">
              <div className="id">
                <code>{e.id}</code>
              </div>
              <div className="lab">{e.label}</div>
              <Btn
                size="sm"
                kind="ghost"
                icon="plus"
                onClick={() => setDrawer({ id: "__new", presetEvent: e.id })}
              >
                new hook
              </Btn>
            </div>
          ))}
        </div>
      )}

      {drawer && (
              <HookEditor
                hookId={drawer.id}
                presetEvent={drawer.presetEvent}
                hooks={hooks}
                events={events}
                onClose={() => setDrawer(null)}

                onSaved={reloadHooks}
              />
      )}
    </div>
  );
}
