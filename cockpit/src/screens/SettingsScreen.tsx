// SettingsScreen — real daemon-backed values; no static mockup data.
// Panels: substrates, harnesses, ntfy channels, auth & tokens, network, storage.
// API/CLI/MCP panels dropped — those features don't exist yet.
import { useEffect, useState, type JSX } from "react";
import { Btn, Panel, Pill, Tag } from "../lib/ui";
import {
  fetchChannels,
  fetchHarnessDescriptors,
  fetchHealth,
  fetchSubstrateDescriptors,
  fetchTokens,
  getStoredOwnerToken,
  rotateToken,
  setStoredOwnerToken,
} from "../api";
import type { ChannelDescriptor, HarnessDescriptor, HealthResponse, SubstrateDescriptor, TokenSummary } from "../types";

// ─── helpers ──────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatSubstrateStatus(status: string | undefined, healthy: boolean): string {
  const normalized = (status ?? "").trim().toLowerCase();
  if (healthy) {
    return "healthy";
  }
  return normalized === "ok" || normalized === "" ? "unavailable" : normalized;
}

function substrateMetricText(status: string, capacity: number): string {
  if (status === "healthy") {
    return `cap ${Math.round(capacity * 100)}%`;
  }
  return "metrics unavailable";
}

function maskToken(token: string): string {
  if (!token || token.length < 8) return "••••••••";
  return `${token.slice(0, 4)}…${token.slice(-4)}`;
}

function isTokenActive(t: TokenSummary): boolean {
  return !t.revoked && t.expires_at_epoch_secs > Math.floor(Date.now() / 1000);
}

// ─── NtfySettings ─────────────────────────────────────────────────

function NtfySettings() {
  const [channels, setChannels] = useState<ChannelDescriptor[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchChannels()
      .then((all) => {
        if (!cancelled) {
          setChannels(all.filter((c) => c.kind === "ntfy"));
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorText(err));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) {
    return <ErrorPanel eyebrow="ntfy channels" error={error} />;
  }

  if (channels.length === 0) {
    return (
      <Panel eyebrow="ntfy channels">
        <div className="muted" style={{ padding: "12px 0" }}>
          no ntfy channels configured — add one via the Channels screen.
        </div>
      </Panel>
    );
  }

  return (
    <Panel eyebrow="ntfy channels" flush>
      {channels.map((c) => (
        <div key={c.id} className="connection-row">
          <div className="ico">∝</div>
          <div>
            <div className="name">{c.name}</div>
            <div className="meta">
              {c.detail || c.label} · {c.message_count_24h} msgs (24h)
            </div>
          </div>
          <Pill status={c.live ? "running" : "idle"}>{c.live ? "subscribed" : "configured"}</Pill>
        </div>
      ))}
    </Panel>
  );
}

// ─── AuthSettings ─────────────────────────────────────────────────

function AuthSettings() {
  const [tokens, setTokens] = useState<TokenSummary[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [copyNotice, setCopyNotice] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [rotateError, setRotateError] = useState<string | null>(null);
  const [newTokenNotice, setNewTokenNotice] = useState<string | null>(null);

  const ownerToken = getStoredOwnerToken();

  useEffect(() => {
    let cancelled = false;
    fetchTokens()
      .then((res) => {
        if (!cancelled) {
          setTokens(res.tokens);
          setLoadError(null);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setTokens([]);
          setLoadError(errorText(err));
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const activeCount = tokens ? tokens.filter(isTokenActive).length : 0;
  const revokedCount = tokens ? tokens.filter((t) => t.revoked).length : 0;

  function handleCopy() {
    if (!ownerToken) return;
    navigator.clipboard.writeText(ownerToken).then(() => {
      setCopyNotice(true);
      setTimeout(() => setCopyNotice(false), 2000);
    });
  }

  async function handleRotate() {
    if (!tokens) return;
    // pragmatic v1: rotate the first active token found.
    // if there are multiple active tokens, the operator should use the CLI.
    const firstActive = tokens.find(isTokenActive);
    if (!firstActive) {
      setRotateError("no active token to rotate");
      return;
    }
    if (activeCount > 1) {
      setRotateError("multiple active tokens — rotate via CLI to avoid ambiguity");
      return;
    }
    if (!window.confirm("rotate owner token? the current token will be revoked and a new one issued. you must save the new value.")) {
      return;
    }
    setRotating(true);
    setRotateError(null);
    try {
      const result = await rotateToken(firstActive.id);
      setStoredOwnerToken(result.new_token.raw_token);
      setNewTokenNotice(result.new_token.raw_token);
      const updated = await fetchTokens();
      setTokens(updated.tokens);
    } catch (err) {
      setRotateError(String(err));
    } finally {
      setRotating(false);
    }
  }

  return (
    <Panel eyebrow="tokens">
      <div className="kv">
        <span className="k">owner token</span>
        <span className="v">
          {ownerToken ? maskToken(ownerToken) : <em className="muted">not stored locally</em>}{" "}
          {ownerToken && (
            <Btn size="sm" kind="ghost" icon="copy" iconOnly onClick={handleCopy} />
          )}
          {copyNotice && <span className="muted" style={{ marginLeft: 6, fontSize: 11 }}>copied</span>}
        </span>
        <span className="k">issued tokens</span>
        <span className="v">
          {loadError
            ? `unavailable · ${loadError}`
            : tokens === null
            ? "loading…"
            : `${activeCount} active · ${revokedCount} revoked`}
        </span>
        <span className="k">scopes</span>
        <span className="v">advisory labels; route enforcement is owner-token level</span>
      </div>
      {newTokenNotice && (
        <div
          style={{
            marginTop: 10,
            padding: 10,
            background: "var(--status-waiting-bg)",
            border: "1px solid rgba(245,180,84,0.35)",
            fontFamily: "var(--font-mono)",
            fontSize: 11,
            wordBreak: "break-all",
          }}
        >
          new token (save this now — shown once):{" "}
          <strong>{newTokenNotice}</strong>
        </div>
      )}
      {rotateError && (
        <div style={{ marginTop: 8, color: "var(--status-errored)", fontSize: 12 }}>
          {rotateError}
        </div>
      )}
      <div className="hr" />
      <Btn icon="rotate-ccw" size="sm" onClick={handleRotate} disabled={rotating}>
        {rotating ? "rotating…" : "rotate owner token"}
      </Btn>
    </Panel>
  );
}

// ─── NetSettings ──────────────────────────────────────────────────

function NetSettings({ health }: { health: HealthResponse | null }) {
  return (
    <Panel eyebrow="network exposure">
      <div className="kv">
        <span className="k">bind</span>
        <span className="v mono">{health ? health.bind_addr : "loading…"}</span>
      </div>
      <div
        style={{
          marginTop: 14,
          padding: 12,
          border: "1px solid rgba(245,180,84,0.35)",
          background: "var(--status-waiting-bg)",
          fontFamily: "var(--font-mono)",
          fontSize: 11.5,
          color: "var(--status-waiting)",
        }}
      >
        exposing asylum beyond localhost reveals attach urls and node transcripts. require pairing + tailscale.
      </div>
    </Panel>
  );
}

// ─── StorageSettings ──────────────────────────────────────────────

function StorageSettings({ health }: { health: HealthResponse | null }) {
  return (
    <Panel eyebrow="storage">
      <div className="kv">
        <span className="k">transcripts</span>
        <span className="v mono">{health ? health.transcripts_dir : "loading…"}</span>
        <span className="k">database</span>
        <span className="v">
          {health ? (
            <>
              <span className="mono">{health.database_path}</span>
              {" · "}
              {formatBytes(health.database_size_bytes)}
            </>
          ) : (
            "loading…"
          )}
        </span>
      </div>
    </Panel>
  );
}

// ─── Section types ────────────────────────────────────────────────

type SectionId =
  | "substrates"
  | "harnesses"
  | "ntfy"
  | "auth"
  | "network"
  | "storage";

// ─── SettingsScreen ───────────────────────────────────────────────

export function SettingsScreen(): JSX.Element {
  const [section, setSection] = useState<SectionId>("substrates");
  const [harnesses, setHarnesses] = useState<HarnessDescriptor[]>([]);
  const [substrates, setSubstrates] = useState<SubstrateDescriptor[]>([]);
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [errors, setErrors] = useState<Partial<Record<"harnesses" | "substrates" | "health", string>>>({});

  useEffect(() => {
    let cancelled = false;
    fetchHarnessDescriptors()
      .then((items) => {
        if (!cancelled) {
          setHarnesses(items);
          setErrors((prev) => ({ ...prev, harnesses: undefined }));
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) setErrors((prev) => ({ ...prev, harnesses: errorText(err) }));
      });
    fetchSubstrateDescriptors()
      .then((items) => {
        if (!cancelled) {
          setSubstrates(items);
          setErrors((prev) => ({ ...prev, substrates: undefined }));
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) setErrors((prev) => ({ ...prev, substrates: errorText(err) }));
      });
    fetchHealth()
      .then((h) => {
        if (!cancelled) {
          setHealth(h);
          setErrors((prev) => ({ ...prev, health: undefined }));
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) setErrors((prev) => ({ ...prev, health: errorText(err) }));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="page" style={{ maxWidth: 1120 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">settings</h1>
          <div className="page-sub">
            single-user · bound to{" "}
            <span className="mono" style={{ color: "var(--fg)" }}>
              {health ? health.bind_addr : window.location.origin}
            </span>
            {health && (
              <span className="muted" style={{ marginLeft: 8 }}>
                v{health.daemon_version}
              </span>
            )}
          </div>
        </div>
      </div>
      <div className="settings-grid">
        <div className="settings-side">
          <div className="group">connections</div>
          {(
            [
              ["substrates", "substrates"],
              ["harnesses", "harnesses"],
              ["ntfy", "ntfy channels"],
            ] as [SectionId, string][]
          ).map(([id, l]) => (
            <div key={id} className={`item ${section === id ? "active" : ""}`} onClick={() => setSection(id)}>
              {l}
            </div>
          ))}
          <div className="group">platform</div>
          {(
            [
              ["auth", "auth & tokens"],
              ["network", "network exposure"],
              ["storage", "storage"],
            ] as [SectionId, string][]
          ).map(([id, l]) => (
            <div key={id} className={`item ${section === id ? "active" : ""}`} onClick={() => setSection(id)}>
              {l}
            </div>
          ))}
        </div>

        <div>
          {section === "substrates" && (
            <Panel eyebrow="substrates" flush>
              {errors.substrates ? (
                <PanelError error={errors.substrates} />
              ) : substrates.map((s) => (
                <div key={s.id} className="connection-row">
                  <div className="ico">{s.id === "local" ? "∎" : "L"}</div>
                  <div>
                  <div className="name">{s.name}</div>
                    <div className="meta">
                      {s.host} · {s.nodes} nodes · {substrateMetricText(formatSubstrateStatus(s.status, s.healthy), s.capacity)}
                    </div>
                    {s.healthy && (
                      <div style={{ marginTop: 6, width: 200 }}>
                        <div className="health-bar">
                          <div className="fill" style={{ transform: `scaleX(${s.capacity})` }} />
                        </div>
                      </div>
                    )}
                  </div>
                  <Pill status={s.healthy ? "running" : "errored"}>
                    {formatSubstrateStatus(s.status, s.healthy)}
                  </Pill>
                </div>
              ))}
            </Panel>
          )}
          {section === "harnesses" && (
            <Panel eyebrow="harnesses" flush>
              {errors.harnesses ? (
                <PanelError error={errors.harnesses} />
              ) : harnesses.map((h) => (
                <div key={h.id} className="connection-row" style={{ opacity: h.available ? 1 : 0.55 }}>
                  <div className="ico">{h.name[0].toLowerCase()}</div>
                  <div>
                    <div className="name">
                      {h.name}
                    </div>
                    <div className="meta">
                      {h.kind} adapter · {h.caps.length} capabilities
                      {!h.available && (
                        <span style={{ display: "block", marginTop: 2, color: "var(--fg-muted)", fontSize: 11 }}>
                          {`\`${h.command}\` not found on daemon PATH — run \`asylum doctor\``}
                        </span>
                      )}
                    </div>
                  </div>
                  <Pill status={h.available ? "running" : "idle"}>{h.available ? "installed" : "not on PATH"}</Pill>
                </div>
              ))}
            </Panel>
          )}
          {section === "ntfy" && <NtfySettings />}
          {section === "auth" && <AuthSettings />}
          {section === "network" && (errors.health ? <ErrorPanel eyebrow="network exposure" error={errors.health} /> : <NetSettings health={health} />)}
          {section === "storage" && (errors.health ? <ErrorPanel eyebrow="storage" error={errors.health} /> : <StorageSettings health={health} />)}
        </div>
      </div>
    </div>
  );
}

function ErrorPanel({ eyebrow, error }: { eyebrow: string; error: string }): JSX.Element {
  return (
    <Panel eyebrow={eyebrow}>
      <PanelError error={error} />
    </Panel>
  );
}

function PanelError({ error }: { error: string }): JSX.Element {
  return (
    <div className="muted" style={{ padding: "12px 0", color: "var(--status-errored)" }}>
      failed to load: {error}
    </div>
  );
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
