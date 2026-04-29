// channels screen — backed by the daemon's /api/channels endpoints
import { useEffect, useState, type JSX } from "react";
import { Btn, Empty, Field, Modal, Pill } from "../lib/ui";
import {
  createChannel,
  deleteChannel,
  fetchChannelMessages,
  fetchChannels,
  testChannel,
  updateChannel,
} from "../api";
import type { ChannelDescriptor, ChannelMessageRecord } from "../types";

function fmtTs(epoch: number): string {
  if (!epoch) return "—";
  const d = new Date(epoch * 1000);
  const m = Math.floor((Date.now() - d.getTime()) / 60000);
  if (m < 1) return "now";
  if (m < 60) return m + "m ago";
  const h = Math.floor(m / 60);
  if (h < 24) return h + "h ago";
  return d.toLocaleDateString();
}

function Stat({ lab, v }: { lab: string; v: string | number }) {
  return (
    <div className="stat">
      <div className="l">{lab}</div>
      <div className="v">{v}</div>
    </div>
  );
}

function ChannelRow({
  ch,
  active,
  onClick,
}: {
  ch: ChannelDescriptor;
  active: boolean;
  onClick: () => void;
}) {
  const glyph: Record<string, string> = {
    ntfy: "◉",
    webhook: "⇄",
    sms: "✉",
    discord: "◈",
    slack: "◇",
    email: "✦",
  };
  const g = glyph[ch.kind] ?? "·";
  return (
    <div className={`ch-row ${active ? "on" : ""} ${ch.live ? "" : "future"}`} onClick={onClick}>
      <span className="g">{g}</span>
      <div className="m">
        <div className="r1">
          <span className="nm">{ch.name}</span>
          {!ch.live && <span className="badge-future">future</span>}
        </div>
        <div className="r2">{ch.label}</div>
      </div>
      <div className="r">
        {ch.live ? (
          <>
            <div className="ct">{ch.message_count_24h}</div>
            <div className="lab">/24h</div>
          </>
        ) : (
          <span className="dot future" />
        )}
      </div>
    </div>
  );
}

function ChannelDetail({
  ch,
  msgs,
  filter,
  setFilter,
  onSendTest,
  onOpenSettings,
  onSubscribe,
  sendStatus,
}: {
  ch: ChannelDescriptor;
  msgs: ChannelMessageRecord[];
  filter: string;
  setFilter: (f: string) => void;
  onSendTest: () => void;
  onOpenSettings: () => void;
  onSubscribe: () => void;
  sendStatus: { ok: boolean; text: string } | null;
}) {
  const stat = ch.live ? "connected" : "not built";
  const lastAt = msgs.length > 0 ? fmtTs(msgs[0].ts_epoch_secs) : "—";
  return (
    <div className="ch-detail">
      <div className="ch-head">
        <div className="left">
          <div className="ttl">{ch.name}</div>
          <div className="sub">{ch.detail}</div>
        </div>
        <div className="right">
          <Pill status={ch.live ? "running" : "idle"}>{stat}</Pill>
          {ch.kind === "ntfy" && ch.live && (
            <Btn size="sm" icon="rss" iconOnly title="subscribe (show topic)" onClick={onSubscribe} />
          )}
          <Btn
            size="sm"
            icon="settings"
            iconOnly
            title="channel settings"
            disabled={ch.builtin}
            onClick={onOpenSettings}
          />
        </div>
      </div>

      <div className="ch-stats">
        <Stat lab="direction" v={ch.direction} />
        <Stat lab="msgs / 24h" v={ch.message_count_24h} />
        <Stat lab="last activity" v={lastAt} />
        <Stat lab="status" v={ch.status} />
      </div>

      <div className="ch-toolbar">
        <div className="filt">
          <span className="lab">filter</span>
          {(
            [
              ["all", "all"],
              ["out", "out"],
              ["in", "in"],
            ] as [string, string][]
          ).map(([v, l]) => (
            <button key={v} className={`chip ${filter === v ? "on" : ""}`} onClick={() => setFilter(v)}>
              {l}
            </button>
          ))}
        </div>
        <div className="acts">
          <Btn size="sm" icon="send" onClick={onSendTest}>
            send test
          </Btn>
        </div>
      </div>
      {sendStatus && (
        <div
          className={`send-status ${sendStatus.ok ? "ok" : "warn"}`}
          style={{
            margin: "0 16px 8px",
            padding: "8px 10px",
            fontSize: 11,
            fontFamily: "var(--font-mono)",
            color: sendStatus.ok ? "var(--fg)" : "var(--fg-muted)",
            background: "var(--bg-sunken)",
            border: "1px solid var(--border-subtle)",
            borderRadius: 4,
          }}
        >
          {sendStatus.text}
        </div>
      )}

      {ch.live ? (
        <div className="ch-msgs">
          {msgs.length === 0 ? (
            <Empty glyph="◌" lead="no messages with this filter" sub="try `all`" />
          ) : (
            msgs.map((m) => {
              const dir = m.direction;
              return (
                <div key={m.id} className={`msg ${dir}`}>
                  <span className="ts">{fmtTs(m.ts_epoch_secs)}</span>
                  <span className={`arr ${dir}`}>{dir === "out" ? "→" : "←"}</span>
                  <div className="b">
                    <div className="r1">
                      <span className="from">{m.sender}</span>
                      <span className="sep">·</span>
                      <span className="subj">{m.subject}</span>
                    </div>
                    <div className="r2">{m.body}</div>
                    {m.replies && m.replies.length > 0 && (
                      <div className="r3">
                        <span className="lab">quick replies:</span>
                        {m.replies.map((r) => (
                          <span key={r} className="reply-chip">
                            {r}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
      ) : (
        <div className="ch-future">
          <div className="g">⌖</div>
          <div className="t">adapter not built</div>
          <div className="d">{ch.detail}</div>
          <div className="row" style={{ marginTop: 16 }}>
            <Btn size="sm" icon="git-pull-request">
              view spec
            </Btn>
            <Btn size="sm" kind="ghost" icon="thumbs-up">
              upvote
            </Btn>
          </div>
        </div>
      )}
    </div>
  );
}

function CreateChannelModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => void;
}) {
  const [kind, setKind] = useState("ntfy");
  const [name, setName] = useState("");
  const [label, setLabel] = useState("");
  const [direction, setDirection] = useState("outbound");
  const [detail, setDetail] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setErr(null);
    try {
      await createChannel({ kind, name, label, direction, detail });
      onCreated();
      onClose();
    } catch (e) {
      setErr(String((e as Error).message));
      setBusy(false);
    }
  };

  return (
    <Modal
      title="new channel"
      onClose={onClose}
      foot={
        <>
          <Btn onClick={onClose}>cancel</Btn>
          <Btn kind="primary" onClick={submit} disabled={busy || !name.trim()}>
            create
          </Btn>
        </>
      }
    >
      <Field label="kind">
        <select value={kind} onChange={(e) => setKind(e.target.value)}>
          <option value="ntfy">ntfy</option>
          <option value="webhook">webhook</option>
          <option value="sms">sms</option>
          <option value="discord">discord</option>
          <option value="slack">slack</option>
          <option value="email">email</option>
        </select>
      </Field>
      <Field label="name">
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-channel" />
      </Field>
      <Field label="label">
        <input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="short description" />
      </Field>
      <Field label="direction">
        <select value={direction} onChange={(e) => setDirection(e.target.value)}>
          <option value="outbound">outbound</option>
          <option value="inbound">inbound</option>
          <option value="duplex">duplex</option>
        </select>
      </Field>
      <Field label="detail">
        <input value={detail} onChange={(e) => setDetail(e.target.value)} placeholder="topic / endpoint / etc" />
      </Field>
      {err && <div style={{ color: "var(--status-errored)", fontSize: 12 }}>{err}</div>}
    </Modal>
  );
}

function EditChannelModal({
  ch,
  onClose,
  onSaved,
  onDeleted,
}: {
  ch: ChannelDescriptor;
  onClose: () => void;
  onSaved: () => void;
  onDeleted: () => void;
}) {
  const [name, setName] = useState(ch.name);
  const [label, setLabel] = useState(ch.label);
  const [detail, setDetail] = useState(ch.detail);
  const [live, setLive] = useState(ch.live);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const save = async () => {
    setBusy(true);
    setErr(null);
    try {
      await updateChannel(ch.id, { name, label, detail, live });
      onSaved();
      onClose();
    } catch (e) {
      setErr(String((e as Error).message));
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!window.confirm(`delete channel "${ch.name}"? this cannot be undone.`)) return;
    setBusy(true);
    setErr(null);
    try {
      await deleteChannel(ch.id);
      onDeleted();
      onClose();
    } catch (e) {
      setErr(String((e as Error).message));
      setBusy(false);
    }
  };

  return (
    <Modal
      title={`edit channel · ${ch.name}`}
      onClose={onClose}
      foot={
        <>
          {!ch.builtin && (
            <Btn kind="danger" onClick={remove} disabled={busy}>
              delete
            </Btn>
          )}
          <span style={{ flex: 1 }} />
          <Btn onClick={onClose}>cancel</Btn>
          <Btn kind="primary" onClick={save} disabled={busy}>
            save
          </Btn>
        </>
      }
    >
      <Field label="name">
        <input value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label="label">
        <input value={label} onChange={(e) => setLabel(e.target.value)} />
      </Field>
      <Field label="detail">
        <input value={detail} onChange={(e) => setDetail(e.target.value)} />
      </Field>
      <Field label="live">
        <label style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input type="checkbox" checked={live} onChange={(e) => setLive(e.target.checked)} />
          <span>{live ? "active" : "inactive"}</span>
        </label>
      </Field>
      {err && <div style={{ color: "var(--status-errored)", fontSize: 12 }}>{err}</div>}
    </Modal>
  );
}

function SubscribeModal({ ch, onClose }: { ch: ChannelDescriptor; onClose: () => void }) {
  const cfg = ch.config as Record<string, unknown>;
  const topic = typeof cfg.topic === "string" ? cfg.topic : "";
  const url = topic ? `https://ntfy.sh/${topic}` : "";
  return (
    <Modal title={`subscribe · ${ch.name}`} onClose={onClose} foot={<Btn onClick={onClose}>close</Btn>}>
      <Field label="topic">
        <input value={topic} readOnly />
      </Field>
      {url && (
        <Field label="url" hint="open this url or use the ntfy app to subscribe">
          <input value={url} readOnly />
        </Field>
      )}
      {!topic && <div style={{ fontSize: 12, opacity: 0.7 }}>no topic configured for this channel</div>}
    </Modal>
  );
}

export function ChannelsScreen(): JSX.Element {
  const [channels, setChannels] = useState<ChannelDescriptor[]>([]);
  const [messages, setMessages] = useState<ChannelMessageRecord[]>([]);
  const [activeId, setActiveId] = useState<string>("");
  const [filter, setFilter] = useState<string>("all");
  const [showCreate, setShowCreate] = useState(false);
  const [showEdit, setShowEdit] = useState(false);
  const [showSubscribe, setShowSubscribe] = useState(false);
  const [sendStatus, setSendStatus] = useState<{ ok: boolean; text: string } | null>(null);

  const refreshChannels = async () => {
    const cs = await fetchChannels();
    setChannels(cs);
  };

  const refreshMessages = async (id: string) => {
    if (!id) return;
    const ms = await fetchChannelMessages(id);
    setMessages(ms);
  };

  useEffect(() => {
    refreshChannels();
  }, []);

  useEffect(() => {
    if (channels.length > 0 && activeId === "") {
      const firstLive = channels.find((c) => c.live);
      setActiveId((firstLive ?? channels[0]).id);
    }
  }, [channels, activeId]);

  useEffect(() => {
    if (!activeId) return;
    refreshMessages(activeId);
    const t = window.setInterval(() => refreshMessages(activeId), 5000);
    return () => window.clearInterval(t);
  }, [activeId]);

  const active = channels.find((c) => c.id === activeId);
  const allMsgs = messages;
  const msgs = filter === "all" ? allMsgs : allMsgs.filter((m) => m.direction === filter);

  const liveCount = channels.filter((c) => c.live).length;
  const futureCount = channels.length - liveCount;
  const total24h = channels.reduce((s, c) => s + c.message_count_24h, 0);

  const onSendTest = async () => {
    if (!active) return;
    setSendStatus(null);
    try {
      const r = await testChannel(active.id, {
        title: "asylum cockpit test",
        body: "hello from the cockpit · " + new Date().toLocaleTimeString(),
      });
      const fresh = await fetchChannelMessages(active.id);
      setMessages(fresh);
      setSendStatus({
        ok: r.sent,
        text: r.sent ? "sent · message recorded" : "recorded but not delivered (adapter not live)",
      });
    } catch (e) {
      setSendStatus({ ok: false, text: "send failed: " + String((e as Error).message) });
    }
  };

  if (channels.length === 0) {
    return (
      <div className="page channels-page">
        <div className="page-head">
          <div>
            <h1 className="page-title">channels</h1>
            <div className="page-sub">how nodes reach you when you&apos;re away · how commands come back in</div>
          </div>
        </div>
        <Empty glyph="◌" lead="loading channels…" sub="" />
      </div>
    );
  }

  return (
    <div className="page channels-page">
      <div className="page-head">
        <div>
          <h1 className="page-title">channels</h1>
          <div className="page-sub">
            how nodes reach you when you&apos;re away · how commands come back in · {liveCount} live, {futureCount}{" "}
            planned · {total24h} msgs / 24h
          </div>
        </div>
        <div className="page-actions">
          <Btn kind="primary" icon="plus" onClick={() => setShowCreate(true)}>
            new channel
          </Btn>
        </div>
      </div>

      <div className="channels-layout">
        <div className="channels-list">
          <div className="ch-group">
            <div className="ch-group-lab">live</div>
            {channels
              .filter((c) => c.live)
              .map((c) => (
                <ChannelRow key={c.id} ch={c} active={activeId === c.id} onClick={() => setActiveId(c.id)} />
              ))}
          </div>
          <div className="ch-group">
            <div className="ch-group-lab">planned · adapters not built</div>
            {channels
              .filter((c) => !c.live)
              .map((c) => (
                <ChannelRow key={c.id} ch={c} active={activeId === c.id} onClick={() => setActiveId(c.id)} />
              ))}
          </div>
        </div>

        <div className="channels-detail">
          {active && (
            <ChannelDetail
              ch={active}
              msgs={msgs}
              filter={filter}
              setFilter={setFilter}
              onSendTest={onSendTest}
              onOpenSettings={() => setShowEdit(true)}
              onSubscribe={() => setShowSubscribe(true)}
              sendStatus={sendStatus}
            />
          )}
        </div>
      </div>

      {showCreate && (
        <CreateChannelModal
          onClose={() => setShowCreate(false)}
          onCreated={() => {
            refreshChannels();
          }}
        />
      )}
      {showEdit && active && (
        <EditChannelModal
          ch={active}
          onClose={() => setShowEdit(false)}
          onSaved={() => {
            refreshChannels();
          }}
          onDeleted={() => {
            setActiveId("");
            refreshChannels();
          }}
        />
      )}
      {showSubscribe && active && <SubscribeModal ch={active} onClose={() => setShowSubscribe(false)} />}
    </div>
  );
}
