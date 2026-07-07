# Phase C plan — durability and Loon parity

Part of the completion mission (see ../specs/2026-07-06-asylum-completion-mission.md). Loon-side prerequisites are DONE (LoonV2 fixes merged, host reinstalled, claude-dev guest image live-verified) — the verified guest contract is ../specs/2026-07-07-loon-guest-contract.md. Harness facts: ../specs/2026-07-06-harness-contract-notes.md. Status at the bottom.

## Workstreams

C1 — Loon substrate rewrite (crates/asylum-daemon: substrate/loon.rs, capability_service.rs guards, app.rs observe/attach, harness adapters as needed)

The old substrate was written against a CLI generation that no longer exists. Rewrite against the verified v0.1.5 contract (guest-contract doc):
- Lifecycle: create = `loon vm create` from the claude-dev image + per-VM provisioning (credential injection via `loon cp`, `{"hasCompletedOnboarding": true}` /root/.claude.json, workspace dir in-guest, repo clone in-guest when requested) + harness launch as a persistent interactive session under `loon exec` PTY (`exec attach` stream). Observe/attach stream that PTY (today Local-only in app.rs — un-guard). send_input follows the W0 submit contract (body write, gap, distinct CR). Interrupt = `loon exec signal` (Ctrl-C semantics, no forced Stopped). Stop = graceful harness stop then `loon vm stop`/`rm`; prune tombstones after teardown. Enumerate VMs with the default (destroyed-hidden) listing.
- Asylum-in-guest: the injected MCP server and harness-event bridge run the `asylum` binary INSIDE the guest — build a static musl `asylum` and stage it into the VM at provision (`loon cp`). Hooks/statusline/notify + MCP injection mirror W3/Local, except daemon resolution is HTTP: mint a per-node token, inject ASYLUM_BASE_URL (daemon HTTP address reachable from the 10.42.0.0/16 guest network — daemon must listen there; make the bind address configurable) + ASYLUM_TOKEN + ASYLUM_NODE_ID. The W2 bridge and MCP already support the HTTP+token path.
- Honesty rules: no lossy shims — workspace/harness/launch-args must survive into the guest launch; if something is genuinely unsupportable on Loon (e.g. fork), reject loudly rather than silently degrade. Quiescence-idle sweep currently Local-only: extend to Loon nodes (codex) or record why not.
- Tests: arg/provision-plan construction, guard removals, token minting; live gate below is the real proof.

C2 — durability + resume (crates/asylum-daemon capability_service.rs/storage.rs, asylum-cli, cockpit)
- Startup reconciliation: on daemon boot, reconcile DB liveness against reality (local PTYs are gone after restart -> mark honestly; Loon VMs may still exist -> query and adopt or mark). No eternal-Running rows. `list_nodes_by_liveness` finally earns its keep.
- Resume: claude `--resume <harness_session_id>` from the node workspace dir (id recorded at create since W3); codex `codex resume <thread-id>` (recorded from first notify). Surface as a node action (API + CLI + MCP + Cockpit button) that relaunches the harness in the same node identity (new PTY, same node row, session id preserved). Honest failure when no session id or workspace is gone.
- Sequencing: C2 daemon work after C1 merges (both touch capability_service.rs). C2 cockpit bits after W5 merges.

## C1 delivered contract (2026-07-07)

Rewritten `crates/asylum-daemon/src/substrate/loon.rs` against loon v0.1.5. Live-verified end to end on this machine (real claude node in a real microVM, MCP round-trip on Casey's subscription).

Architecture — CLI vs HTTP per operation:
- `loon` CLI (shells out, `--config`/`--profile` passed when configured): `vm create --memory --cpus`, `vm stop`, `vm rm`, `vm prune`, `cp` (file staging), non-PTY `exec` (mkdir/assembly/readiness probe).
- loon daemon HTTPS API directly (reqwest + rustls pinned to the profile's `fingerprint_sha256`, credentials read from the loon client config `~/.config/loon/config.toml`): `POST /instances/{id}/exec` with `pty:true` (the CLI cannot allocate a detached PTY exec), the bidirectional attach WebSocket `/instances/{id}/attach/{exec_id}` (PTY bytes both ways), `POST .../signal` and `.../resize`, and the SSE exec stream `GET /instances/{id}/exec/{exec_id}` held open purely as the exit signal. `GET /instances` is the health probe.

Lifecycle semantics:
- create: `vm create` from the configured OCI-tar image, wait for the guest agent (bounded ~90s), provision (claude+codex credentials via `cp` mode 384; `/root/.claude.json` with `hasCompletedOnboarding` + `bypassPermissionsModeAccepted` + workspace trust; `/root/.codex/config.toml` trust; workspace dir; static musl asylum staged to `/usr/local/bin/asylum` in 1 MiB chunks reassembled in-guest — loon cp bodies are capped ~2 MiB), then launch the harness as a PTY exec (`sh -lc 'exec "$@"' …` passes the full argv through verbatim; `HOME=/root`, `IS_SANDBOX=1` because exec runs as root inside the microVM sandbox). Any provisioning failure tears the VM down before returning.
- Output frames flow through the SAME sinks as Local (transcript persistence, decision ingestion, broadcast for observe/attach); the W1 quiescence-idle sweep now covers Loon nodes too.
- launch prompt: delivered over the guest PTY as a submitted message after a timing-only readiness gate (first frame + quiet window, all bounded); W0 submit contract (body write, 50 ms gap, distinct lone CR).
- send_input: W0 submit contract over the attach WS. send_input_raw (interactive attach) appends nothing.
- interrupt: ETX (0x03) over the PTY — SIGINT to the foreground process group; cancels the turn, node stays Running (verified live).
- stop/archive: SIGTERM to the exec, abort stream tasks, then `vm stop` + `vm rm` + `vm prune` (tombstone removed). A guest harness that dies on its own is detected by the SSE exec stream ending -> exit sink maps to node.exited/node.errored (same rules as Local) AND the VM is torn down automatically (verified live: the failed-launch and harness-death paths both left zero VMs).
- fork: rejected loudly for Loon sources (a Local fork shares real workspace files; a Loon fork would get an empty same-named dir — silent degradation). spawn/create of new Loon nodes is the supported path.
- observe/attach: un-guarded in app.rs; both substrates stream the live PTY broadcast over the observe WS and the bidirectional attach WS (input routed to the owning substrate).

Config keys (`[loon]`, crates/asylum-types/src/config.rs):
- `enabled`, `endpoint` (override; default = profile URL), `cli_path`, `config_path` (loon client config; default `~/.config/loon/config.toml`), `profile` (default = config's `default_profile`)
- `image` (default `/var/lib/loon/agent-images/claude-dev.oci.tar`)
- `workspace_dir` (default `/work`) — Loon workspaces are IN-GUEST paths; host paths are never mounted
- `vm_memory_mib` (default 2048; live run used 3072) / `vm_cpus` (default 2) — loon's 256 MiB default OOM-freezes claude
- `guest_asylum_binary` — host path to the static musl asylum; REQUIRED for guest MCP (launch fails loudly without it)
- `guest_base_url` — URL guests use to reach the daemon; default `http://host.loon.internal:<bind port>` (loon-netd injects `host.loon.internal` -> per-VM gateway in every guest's /etc/hosts). The daemon must bind a guest-reachable address (`listen = "0.0.0.0:7799"` in the live run).
- Note: `AsylumFileConfig` uses serde flatten, which strips nested field defaults — new non-Option `[loon]` keys must carry `#[serde(default)]`, and a partial `[loon]` table in an existing config file parses fine.
- `api_key_file`/`cert_fingerprint_file` remain accepted but unused (superseded by the loon client config; kept for config back-compat).

musl build step: `scripts/build-guest-asylum.sh` — `cargo build -p asylum --release --target x86_64-unknown-linux-musl` (CC=musl-gcc), strips the binary (~13 MB), and stages an inert `cockpit/dist` placeholder if absent so the release rust-embed compiles (the guest never serves cockpit). Artifact lands in `target/` (gitignored); the script is committed.

Token + URL mechanics: per-node bearer token minted at create (`issue_owner_token`, name `loon-node-<id>`, 30-day TTL, stored hashed). Asylum tokens are ALL-OR-NOTHING today — any valid token passes `AuthMode::Token`; the `loon-node` scope string is descriptive, not enforced (narrowing is future work). Guest env: `ASYLUM_BASE_URL` + `ASYLUM_TOKEN` + `ASYLUM_NODE_ID` (+ role/harness/substrate/capabilities/graph summary; `ASYLUM_CONTROL_TRANSPORT=http`; no socket path). MCP + hooks injection mirrors W3 shapes with `/usr/local/bin/asylum` as the binary and HTTP resolution in the injected env (`DaemonResolution::Http` in harness adapters).

Live verification (2026-07-07, node a6fed776, VM asylum-a6fed776..., one session): VM booted from claude-dev; harness started; launch prompt auto-submitted; `node.session_started`, `node.tool_call` (ToolSearch + `mcp__asylum__node_list`), `node.turn_complete`, `node.telemetry` all arrived over HTTP+token from inside the guest; the MCP node.list call succeeded and claude replied DONE; observe WS streamed (101 + live frames); send_input round-tripped (SEND-OK); interrupt mid-turn left the node Running and responsive (ALIVE-AFTER-INTERRUPT); stop tore down VM + pruned the tombstone with no stray processes.

What C2 must know: Loon node rows carry `external_id` = loon instance id; on daemon restart the attach WS/SSE tasks are gone (in-memory runtimes), so reconciliation must query `loon vm ls`, and either re-exec/re-attach into a still-running VM (the harness process survives the daemon; a fresh PTY exec would start a NEW harness — claude `--resume <harness_session_id>` inside the guest is the honest resume) or mark the node dead and tear down. Tokens outlive the daemon (DB-stored, 30-day TTL). `loon vm ls` uses destroyed-hidden listing; prune after every teardown keeps it clean.

## Live gate (frugal)

1. Create/observe/send/interrupt/stop a real Claude node on a real Loon microVM from Asylum; the node calls at least one Asylum MCP tool from inside the guest (C1, one session).
2. Kill the daemon mid-session (local node running), restart: state honest (no eternal Running), and a resumable claude node resumes and continues (C2, one session, may reuse the C1 session's sibling).

## Status

- C1 (Loon substrate rewrite): COMPLETE (delivered contract above; live gate 1 passed 2026-07-07)
- C2 (reconciliation + resume): not started
- Gate: C1 gate passed (real claude node on a real Loon microVM: MCP round-trip, observe/send/interrupt/stop). C2 gate not run.

Release status: local-only mission work; not released. Last published release: v0.1.10 (2026-05-07).
