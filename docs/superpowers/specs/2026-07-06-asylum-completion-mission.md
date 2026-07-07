# Asylum Completion Mission

Date started: 2026-07-06. Owner: Casey. Executor: orchestrating agent session (Fable) driving Opus/Sonnet subagents.

This document is the durable anchor for a long autonomous run that takes Asylum (+ LoonV2 where needed) to a genuinely working state on this machine. Any fresh session picking up this work reads this file first, then the Status section at the bottom.

## North star: the acceptance scenario

Via Cockpit or CLI, Casey creates a supervisor node (Claude Code, local substrate) and gives it a real multi-item body of work in one instruction. Using only Asylum's injected MCP tools, the supervisor spawns 2-3 worker nodes (at least one on Loon), sets up monitors/hooks so it is alerted when a worker goes idle, awaits input, or errors — without streaming their raw output. When a worker stalls, the supervisor feeds it input autonomously. At least once it escalates to the human via ntfy, and a reply routes back into the right node. Casey can open any live session from Cockpit at any moment and intervene. When work completes, the supervisor stops its workers. A daemon restart mid-scenario does not corrupt state: orphaned nodes are reconciled honestly and resumable sessions can be resumed.

Everything in scope traces to a gap in that scenario. Anything that does not is out.

## Architecture principle

Asylum is dumb plumbing; intelligence lives in the harnesses. The system's current deafness (advertised events `node.idle`, `node.ctx_pressure`, `node.tool_call`, `node.errored` are never emitted) is fixed by harness-native reporting, not by parsing PTY bytes in Rust:

- Claude Code hooks (Stop, Notification, PreToolUse, SessionStart, etc.) and Codex `notify` shell out to the injected `asylum` CLI to post structured events (tool call, awaiting input, session id, turn complete). Asylum already injects the MCP server, `ASYLUM_NODE_ID`, and `ASYLUM_SOCKET_PATH` into local launches; the hook bridge rides the same launch config.
- Only pure timing signals (output quiescence -> `node.idle`) are computed Asylum-side.
- No enforced harness workflows: anything Claude Code/Codex can do in a bare terminal must still work inside a node.

## Verified current-state audit (2026-07-06)

Three parallel audits of main @ b76e9d9 established:

- Cockpit is ~90-95% real: every screen wired to live daemon endpoints; prototype residue exists only in dead `cockpit/prototype/` (never imported). Remaining gaps: client-derived uptime, hardcoded local capacity 0.0 (`capability_service.rs:1217`), FirstRun static panels, unused api.ts plumbing, no UI for `spawn_peer`.
- Real and end-to-end (local substrate): PTY launch of claude/codex, send/interrupt/stop/archive, observe WS, fork, `node.spawn_peer` + graph edge, MCP injection into local launches (claude `--mcp-config`/`--strict-mcp-config`, codex `-c mcp_servers...`), ~40-tool MCP server over unix socket, ntfy outbound + inbound subscriber with reply correlation, remote-commands, hooks engine with real filter language, SQLite persistence of graph/events/transcripts/hooks/channels/tokens/decisions.
- The autonomy loop is the gap:
  1. Sensory events advertised but never emitted (`hooks/mod.rs:76-98` catalog vs emit sites). Only `graph.spawn`, `node.exited`, `channel.inbound`, `schedule.5m/30m`, `node.permission_requested` actually fire.
  2. Decision loop has no producer (nothing emits the `@@asylum:decision.request` stdout marker) and no feedback (`resolve_decision` never injects the answer into the PTY, `capability_service.rs:2068-2111`).
  3. No durability: hardcoded `resume: false` everywhere; no startup reconciliation — daemon restart leaves DB nodes `Running` forever (`capability_service.rs:222-290`; `list_nodes_by_liveness` unused).
  4. Loon substrate is a lossy CLI shim: `loon spawn` gets only `--prompt` (workspace/harness/launch-args dropped, `substrate/loon.rs:132-147`); no MCP injection for Loon nodes (Local-only guard `capability_service.rs:1324`); observe/attach local-only (`app.rs:550-557`).
  5. Inert surfaces: recipes disabled everywhere (`recipe_spawn_is_enabled() == false` in two places), hook `spawn` action rejected, `transcript.checkpoint` unsupported, MCP `node.archive` falsely claims transcript export (`mcp.rs:167`), token scopes advisory, interrupt marks node Stopped on a mere Ctrl-C (`capability_service.rs:1517-1534`).
- Doc contradictions: `docs/README.md` omits the 2026-05-10 plan; CHANGELOG Unreleased vs RELEASES.md open-items disagree; spec describes unreleased main behavior (fine — we are not releasing).

## Phases and gates

Four phases. Each ends with a frugal real-session check of its gate. Details are planned just-in-time per phase (short plan doc in `docs/superpowers/plans/`, or directly in subagent briefs when the work is mechanical).

### Phase A — Foundation and truth
Baseline build + full test suite green (`cargo test-asylum`); verify the on-main `spawn_peer`/MCP-injection path with one real frugal Claude node; Loon host on this machine assessed and brought up (`loon version` + daemon healthy; install/repair from `/home/casey/Projects/LoonV2` if needed); doc contradictions fixed; `cockpit/prototype/` and dead api.ts plumbing removed; false MCP claims corrected.
Gate: clean tree, green tests, one real spawned-peer session observed via Asylum, Loon host answering.

### Phase B — The autonomy loop
Harness hook bridge (Claude hooks / Codex notify -> `asylum` event posts: tool_call, awaiting-input, session-id capture, turn complete/stop); `node.idle` via output quiescence; `node.ctx_pressure` from telemetry already computed in `storage.rs`; `node.errored` on abnormal exit; delete any catalog event that stays unimplementable. Real hook actions: `send_input` plus working `spawn` (un-disable recipes or replace with something honest). Close the input/decision loop: waiting-for-input detection via the hook bridge; `resolve_decision` and ntfy replies inject the answer into the session; Cockpit surfaces pending decisions as a first-class flow; fix interrupt semantics (Ctrl-C cancels a turn, it does not stop the node).
Gate: a monitor created in Cockpit fires on a real stalled session and its action executes; node asks a question -> ntfy -> reply -> node continues; same via Cockpit.

### Phase C — Durability and Loon parity
Startup reconciliation (no eternal-Running lies; orphaned nodes marked honestly); capture harness session ids (claude: pre-assign via `--session-id`; codex: correlate notify `thread-id` with rollout files); implement resume (`claude --resume <session-id>` from the node workspace dir, `codex resume <thread-id>`) surfaced in CLI/Cockpit.

Loon parity — REVISED after the 2026-07-06 host assessment: Asylum's loon substrate was written against a CLI generation that no longer exists (spawn/tell/interrupt/stop/terminate/attach verbs + LOON_* env vars; zero occurrences in LoonV2 v0.1.5). The substrate must be REWRITTEN against the real contract: profile config via `loon connect` (`~/.config/loon/config.toml`, no env-var overrides), verbs `loon run <image> -- <cmd>` / `loon vm create|stop|rm` / `loon exec <vm> -- <cmd>` / `loon exec attach|signal`. Additional realities: host-path bind mounts are unsupported in v2 (Loon node workspaces live INSIDE the guest; repos get cloned/provisioned in-guest); no local guest image has node/npm — a claude/codex-capable guest image must be built (plus a way to pass Casey's subscription credentials into the guest); Asylum MCP + hook bridge from inside the guest reach the daemon over HTTP with a token (socket unavailable across the VM boundary). Change LoonV2 as needed, rebuild + reinstall locally; when installing the host, explicitly restage loon-guest (the staged copy predates the current musl build and the PATH-based installer will not refresh it).
Gate: kill daemon mid-session, restart, state honest, a resumable node resumes; create/observe/send/interrupt/stop a real Claude node on a real Loon microVM from Asylum, and that node successfully calls one Asylum MCP tool.

### Phase D — Fleet acceptance and truthfulness sweep
Supervisor launch-packet guidance (teach a spawned supervisor the Asylum tool surface and frugal-coordination etiquette); Cockpit fleet surfacing (spawn_peer visibility, pending decisions, daemon-provided uptime, real local capacity, terminate/stop label); implement-or-delete every remaining inert surface; run the full north-star scenario, fix what breaks, repeat until it passes; final adversarial code review; docs/spec updated to match reality; RELEASES.md row recording on-main-not-released.
Gate: the north-star scenario, witnessed end-to-end.

## Operating rules

- Repos: `/home/casey/Projects/Asylum` and `/home/casey/Projects/LoonV2`. Branch per phase (or per coherent unit), clean merges to local `main`. Nothing pushed to GitHub. No releases, no version ceremony. LoonV2 changes are built and installed locally so the live host runs them.
- Orchestration: the main (Fable) session plans, decomposes, reviews, and unblocks. All implementation is done by Opus/Sonnet subagents with precise briefs. Never Fable subagents. Use plain general-purpose/Explore agents only — the plugin principal-engineer agents (loon/firecracker) are banned as bloat (Casey, 2026-07-06). Keep main-session context lean; durable state lives in this file and phase plan docs, not in conversation memory.
- Testing: solid unit/integration coverage as good engineering dictates — no TDD ceremony. Every phase gate includes a frugal real-session E2E check (Casey's subscriptions, trivial prompts, sessions stopped promptly). No simulated/mocked/stubbed behavior in shipped code, ever.
- Escalation: questions for Casey accumulate in Open Questions below; the run continues around them unless a blocker gates all remaining work.

## Open questions for Casey

(none open)

Resolved: passwordless sudo configured 2026-07-06 via /etc/sudoers.d/99-casey-nopasswd (remove with `sudo rm /etc/sudoers.d/99-casey-nopasswd` when the mission is over).

## Status

- Phase A: COMPLETE (2026-07-06). Gate met: clean tree + green tests (181 Rust + 64 cockpit); phase-a-truth merged at b336d05 (flake fix, doc truth, prototype/dead-api removal); harness contracts recorded (2026-07-06-harness-contract-notes.md); live spawn-peer E2E confirmed a real second worker session + spawned_for edge via injected MCP (two bugs found → Phase B W0: launch prompt not auto-submitted, send_input CR not registering as Enter); Loon host installed and operational at https://127.0.0.1:7777 (btrfs loopback storage, systemd units active, client profile for casey, busybox microVM ran echo ok). TLS fp 53b9f879...; admin key at /etc/loon/admin.key, client config ~/.config/loon/config.toml.
  - Two LoonV2 upstream gaps to address in Phase C (repo editable): (1) PATH-based `loon-host install` does not stage the kernel vmlinux into <state>/kernel/ despite listing it in install-journal.json — had to stage manually; real install bug. (2) Destroyed VMs remain as un-purgeable `destroyed` tombstones in `loon vm ls` (v0.1.5 by-design, loon-daemon store.rs list_instances has no filter) — the Loon-side analog of the eternal-Running problem; the substrate rewrite must filter destroyed rows when enumerating, and ideally add a prune path upstream.
- Phase B: COMPLETE (2026-07-07). All workstreams W0-W5 merged to main (24c2242); live gate PASSED on branch phase-b-gate against real claude sessions and real ntfy.sh traffic: Cockpit-created hook fired on a real idle session and its send_input executed (GATE-2-OK); node asked a question -> pending decision -> ntfy topic (reply marker) -> correlated reply -> answer injected -> node continued; same loop via Cockpit DecisionsScreen. Gate flushed out and fixed 4 real bugs (claude Notification `notification_type` field mapping; ntfy JSON publish to server root; codex submit gap 250ms; codex-only launch submit nudges) — evidence + per-step detail in the plan doc's Gate section. Caveats: codex live turn_complete blocked by host codex OAuth invalidation (needs `codex login` re-auth, then one-node re-check); menu-style dialog answer fidelity recorded as a known limitation for Phase D.
- Phase C: Loon-side prep COMPLETE early (2026-07-07, run in parallel with Phase B): LoonV2 installer-skip + tombstone bugs fixed and merged (LoonV2 main 2662310), live host reinstalled on the fixed build and healthy, claude/codex-capable guest image built and live-verified (GUEST-OK/CODEX-OK from inside a real microVM on Casey's subscriptions), credential-injection recipe proven, guest network egress confirmed. Full contract for the substrate rewrite: specs/2026-07-07-loon-guest-contract.md. Remaining: Asylum-side substrate rewrite, durability/reconciliation, resume surfacing.
- Phase D: not started

Release status: all mission work is local-only by explicit instruction. On main, not released. Last published release: v0.1.10 (2026-05-07).
