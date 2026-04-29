// ports prototype SettingsScreen plus NtfySettings, AuthSettings, NetSettings,
// StorageSettings, ApiSettings, CliSettings, McpSettings
import { useEffect, useState, type JSX } from "react";
import { Btn, Panel, Pill, Tag } from "../lib/ui";
import { fetchHarnessDescriptors, fetchSubstrateDescriptors } from "../api";
import type { HarnessDescriptor, SubstrateDescriptor } from "../types";

function NtfySettings() {
  return (
    <Panel eyebrow="ntfy channels" flush actions={<Btn size="sm" icon="plus">add channel</Btn>}>
      {(
        [
          ["asylum-aaron", "ntfy.sh/asylum-aaron-7c2af", "12 sent · 4 received"],
          ["asylum-oncall", "ntfy.sh/asylum-oncall", "0 sent · 0 received"],
        ] as [string, string, string][]
      ).map(([n, t, m]) => (
        <div key={n} className="connection-row">
          <div className="ico">∝</div>
          <div>
            <div className="name">{n}</div>
            <div className="meta">
              {t} · {m}
            </div>
          </div>
          <Pill status="running">subscribed</Pill>
          <Btn size="sm" kind="ghost" icon="more-horizontal" iconOnly />
        </div>
      ))}
    </Panel>
  );
}

function AuthSettings() {
  return (
    <Panel eyebrow="tokens">
      <div className="kv">
        <span className="k">owner token</span>
        <span className="v">
          a8x7…b91 <Btn size="sm" kind="ghost" icon="copy" iconOnly />
        </span>
        <span className="k">pairing code</span>
        <span className="v">ASLM-2F9D-C014</span>
        <span className="k">issued tokens</span>
        <span className="v">3 active · 0 revoked today</span>
        <span className="k">attach urls</span>
        <span className="v">2 active · ttl 3600s</span>
      </div>
      <div className="hr" />
      <Btn icon="rotate-ccw" size="sm">
        rotate owner token
      </Btn>
    </Panel>
  );
}

function NetSettings() {
  return (
    <Panel eyebrow="network exposure">
      <div className="kv">
        <span className="k">bind</span>
        <span className="v">localhost:5173</span>
        <span className="k">remote access</span>
        <span className="v">tailscale (recommended)</span>
        <span className="k">reverse proxy</span>
        <span className="v">none configured</span>
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

function StorageSettings() {
  return (
    <Panel eyebrow="storage & retention">
      <div className="kv">
        <span className="k">transcripts</span>
        <span className="v">~/Library/Asylum/transcripts · 1.4 GB</span>
        <span className="k">retention</span>
        <span className="v">30 days (rolling)</span>
        <span className="k">redaction</span>
        <span className="v">on (api keys, jwt-like)</span>
      </div>
    </Panel>
  );
}

function ApiSettings() {
  const origin = window.location.origin;
  return (
    <Panel eyebrow="api & sdk">
      <div className="kv">
        <span className="k">base url</span>
        <span className="v">{origin}/api/v1</span>
        <span className="k">openapi</span>
        <span className="v">/openapi.json (37 endpoints)</span>
        <span className="k">sdk</span>
        <span className="v">@asylum/sdk@0.1.0 (typescript)</span>
      </div>
      <div className="hr" />
      <div className="muted mono" style={{ fontSize: 11, marginBottom: 6 }}>
        quickstart
      </div>
      <pre
        style={{
          background: "var(--bg-sunken)",
          padding: 12,
          fontFamily: "var(--font-mono)",
          fontSize: 11.5,
          color: "var(--fg)",
          border: "1px solid var(--border-subtle)",
          overflow: "auto",
          margin: 0,
        }}
      >{`import { Asylum } from "@asylum/sdk";
const a = new Asylum({ baseUrl, token });
const node = await a.node.create({ harness: "codex", substrate: "loon-us-west", role: "worker" });
for await (const ev of a.node.observe(node.id)) console.log(ev);`}</pre>
    </Panel>
  );
}

function CliSettings() {
  return (
    <Panel eyebrow="cli">
      <pre
        style={{
          background: "var(--bg-sunken)",
          padding: 12,
          fontFamily: "var(--font-mono)",
          fontSize: 11.5,
          color: "var(--fg)",
          border: "1px solid var(--border-subtle)",
          overflow: "auto",
          margin: 0,
        }}
      >{`$ asylum nodes
NODE        ROLE              HARNESS       SUBSTRATE       STATE
cc-7c2af    command-center    codex         local           running
sup-3d1e    supervisor        claude-code   loon-us-west    running
…
$ asylum node send w-2b0c8 "approve"
$ asylum attach w-9a4f1 --browser`}</pre>
    </Panel>
  );
}

function McpSettings() {
  return (
    <Panel eyebrow="mcp server">
      <div className="kv">
        <span className="k">endpoint</span>
        <span className="v">stdio · asylum-mcp</span>
        <span className="k">tools exposed</span>
        <span className="v">37 (graph.get, node.create, node.send_input, …)</span>
        <span className="k">connected clients</span>
        <span className="v">claude desktop, cursor</span>
      </div>
    </Panel>
  );
}

type SectionId =
  | "substrates"
  | "harnesses"
  | "ntfy"
  | "auth"
  | "network"
  | "storage"
  | "api"
  | "cli"
  | "mcp";

export function SettingsScreen(): JSX.Element {
  const [section, setSection] = useState<SectionId>("substrates");
  const [harnesses, setHarnesses] = useState<HarnessDescriptor[]>([]);
  const [substrates, setSubstrates] = useState<SubstrateDescriptor[]>([]);

  useEffect(() => {
    let cancelled = false;
    fetchHarnessDescriptors()
      .then((items) => {
        if (!cancelled) setHarnesses(items);
      })
      .catch(() => {});
    fetchSubstrateDescriptors()
      .then((items) => {
        if (!cancelled) setSubstrates(items);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const origin = window.location.origin;

  return (
    <div className="page" style={{ maxWidth: 1120 }}>
      <div className="page-head">
        <div>
          <h1 className="page-title">settings</h1>
          <div className="page-sub">
            single-user · bound to{" "}
            <span className="mono" style={{ color: "var(--fg)" }}>
              {origin}
            </span>
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
              ["storage", "storage & retention"],
            ] as [SectionId, string][]
          ).map(([id, l]) => (
            <div key={id} className={`item ${section === id ? "active" : ""}`} onClick={() => setSection(id)}>
              {l}
            </div>
          ))}
          <div className="group">developer</div>
          {(
            [
              ["api", "api & sdk"],
              ["cli", "cli"],
              ["mcp", "mcp server"],
            ] as [SectionId, string][]
          ).map(([id, l]) => (
            <div key={id} className={`item ${section === id ? "active" : ""}`} onClick={() => setSection(id)}>
              {l}
            </div>
          ))}
        </div>

        <div>
          {section === "substrates" && (
            <Panel eyebrow="substrates" flush actions={<Btn size="sm" icon="plus">add substrate</Btn>}>
              {substrates.map((s) => (
                <div key={s.id} className="connection-row">
                  <div className="ico">{s.id === "local" ? "∎" : "L"}</div>
                  <div>
                    <div className="name">{s.name}</div>
                    <div className="meta">
                      {s.host} · {s.nodes} nodes · cap {Math.round(s.capacity * 100)}%
                    </div>
                    {s.healthy && (
                      <div style={{ marginTop: 6, width: 200 }}>
                        <div className="health-bar">
                          <div className="fill" style={{ transform: `scaleX(${s.capacity})` }} />
                        </div>
                      </div>
                    )}
                  </div>
                  <Pill status={s.healthy ? "running" : "errored"}>{s.healthy ? "healthy" : "unreachable"}</Pill>
                  <Btn size="sm" kind="ghost" icon="more-horizontal" iconOnly />
                </div>
              ))}
            </Panel>
          )}
          {section === "harnesses" && (
            <Panel eyebrow="harnesses" flush actions={<Btn size="sm" icon="plus">install adapter</Btn>}>
              {harnesses.map((h) => (
                <div key={h.id} className="connection-row" style={{ opacity: h.available ? 1 : 0.55 }}>
                  <div className="ico">{h.name[0].toLowerCase()}</div>
                  <div>
                    <div className="name">
                      {h.name} {!h.available && <Tag future>future</Tag>}
                    </div>
                    <div className="meta">
                      {h.kind} adapter · {h.caps.length} capabilities
                    </div>
                  </div>
                  <Pill status={h.available ? "running" : "idle"}>{h.available ? "installed" : "not built"}</Pill>
                  <Btn size="sm" kind="ghost" icon="settings" iconOnly />
                </div>
              ))}
            </Panel>
          )}
          {section === "ntfy" && <NtfySettings />}
          {section === "auth" && <AuthSettings />}
          {section === "network" && <NetSettings />}
          {section === "storage" && <StorageSettings />}
          {section === "api" && <ApiSettings />}
          {section === "cli" && <CliSettings />}
          {section === "mcp" && <McpSettings />}
        </div>
      </div>
    </div>
  );
}
