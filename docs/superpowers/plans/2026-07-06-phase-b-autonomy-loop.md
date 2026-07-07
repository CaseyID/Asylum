# Phase B plan — the autonomy loop

Part of the completion mission (see ../specs/2026-07-06-asylum-completion-mission.md). Harness facts: ../specs/2026-07-06-harness-contract-notes.md. Status tracked at the bottom.

## Goal

Make Asylum able to hear its nodes and act on what it hears, so a supervisor node (or a human via hooks) can babysit workers without streaming raw terminals. Producers: harness-native hooks. Consumers: the existing hook engine, decisions, and Cockpit.

## Event contract (the spine; workstream W1 defines it, everything else conforms)

New/now-real node events, emitted via a new daemon ingestion capability:

| Event | Producer | Payload highlights |
|---|---|---|
| `node.turn_complete` | claude Stop hook; codex notify `agent-turn-complete` | last assistant message (codex), session/turn ids |
| `node.awaiting_input` | claude Notification `permission_prompt`/`agent_needs_input`/`elicitation_dialog` | type, message |
| `node.idle` | claude Notification `idle_prompt`; fallback: daemon quiescence timer (codex) | idle source |
| `node.ctx_pressure` | claude statusline bridge when `used_percentage` crosses configured thresholds (default 75/90) | pct, threshold |
| `node.session_started` | claude SessionStart (source: startup/resume/clear/compact) | harness session id — recorded on the node row |
| `node.session_end` | claude SessionEnd | reason |
| `node.tool_call` | claude PostToolUse (async hook) | tool name; input truncated |
| `node.errored` | daemon exit sink on nonzero/abnormal exit | exit info |

Rules: ingestion validates the node id exists and is running; events are stored (existing events table), posted to the hook engine, and update liveness where meaningful (`awaiting_input` -> WaitingForInput; `turn_complete`/`idle` -> a truthful non-busy state; `session_started` clears it). Catalog in `hooks/mod.rs` is trimmed/renamed to exactly the set that can actually fire (`node.permission_requested` merges into `node.awaiting_input`; `substrate.unreachable`/`schedule.cron` removed unless implemented). Cockpit's event pickers follow the catalog.

## Workstreams

W1 — daemon ingestion + event truth (crates/asylum-daemon)
- New capability `node.post_harness_event` (HTTP + socket, token-protected like the rest): body = source (claude_hook|claude_statusline|codex_notify|daemon), raw payload, mapped event kind. Mapping lives daemon-side so the CLI stays thin.
- Liveness updates per table above; interrupt fix: `interrupt_node` sends Ctrl-C WITHOUT forcing Stopped/`node.exited` (liveness now follows real signals; exit sink still owns termination).
- `node.ctx_pressure` thresholding; telemetry columns already hydrated in storage.rs get updated from statusline posts too.
- Quiescence-based `node.idle` fallback timer for nodes whose harness lacks an idle signal (codex), config-defaulted (e.g. 120s no PTY output while Running).
- Catalog cleanup per rules above. Unit tests for mapping, liveness transitions, thresholds, catalog.

W2 — CLI bridge (crates/asylum-cli)
- `asylum harness-event <source>`: reads claude hook JSON from stdin, or codex notify JSON from argv (`--payload <json>`; codex nulls stdin), resolves node identity from ASYLUM_NODE_ID and daemon from ASYLUM_SOCKET_PATH (HTTP fallback via ASYLUM_BASE_URL+ASYLUM_TOKEN for future Loon guests), POSTs to W1's endpoint. Must be fast, exit 0 always (never break a session), swallow-and-log on daemon-unreachable. Also `asylum harness-event claude-statusline` mode reading the statusline JSON and posting telemetry. Unit tests with recorded fixture payloads (fixtures are fine in tests).

W3 — launch injection (crates/asylum-daemon harness adapters)
- claude: extend `asylum_control_args` with pre-assigned `--session-id <uuid>` (stored on the node row at create) and `--settings '<inline json>'` carrying hooks (Stop, Notification, SessionStart, SessionEnd, PostToolUse async) and statusLine, all invoking the installed `asylum` binary (same path resolution as MCP injection) with the injected env. Keep `--dangerously-skip-permissions` leading (documented flag-order edge case).
- codex: add `-c notify=["<asylum-path>","harness-event","codex-notify"]`; capture thread-id from the first notify post (W1 records it as harness session id).
- Existing user `startup_args` must still append cleanly. Tests assert exact argv construction.

W4 — actions + decisions (daemon + cli/mcp)
- Hook actions: add `send_input` (target node, text). Replace the disabled recipes concept with an honest `spawn` action carrying an inline node spec (harness, substrate, role, workspace, prompt); DELETE the recipe surface (recipe.list/recipe.spawn MCP tools, /api/recipes, cockpit recipes gating) — fewer concepts, nothing inert. Validation un-rejects `spawn`.
- Decision producer: `node.awaiting_input` with type permission/elicitation auto-creates a pending decision (dedup while one is pending per node).
- Decision feedback: `resolve_decision` injects the resolution (approve/deny/free-text answer) into the node PTY via send_input and posts events; ntfy reply correlation routes into the same path. Notifications on create/resolve already exist.
- MCP: expose `node.post_harness_event`? No — bridge-only. But add `decision.pending` filter if trivial. Tests: producer dedup, feedback injection, spawn action end-to-end against a fake substrate in tests.

W5 — Cockpit surfacing (cockpit/)
- Hooks screen: event/action pickers match the new catalog (add send_input + spawn action forms; drop recipe gating).
- Decisions: pending-decision badge on node cards/graph + resolve affordance already present on DecisionsScreen; wire NodeScreen indicator.
- Node liveness chips reflect the new truthful states (idle/awaiting input).
- Vitest coverage for new forms/states.

Order: W1 first (contract), then W2+W3+W4 in parallel (W2/W3 pair on the bridge contract; W4 touches capability_service alongside W1's edits — sequence W4 after W1 merges to avoid conflicts), W5 last, then a live integration check.

## Live gate check (frugal)

1. Create a claude node in a scratch dir; wait for `node.session_started` (session id recorded), let its trivial task finish; observe `node.turn_complete` and `node.idle` events arriving with no PTY parsing.
2. Create a hook: trigger `node.idle`, action `send_input` ("continue: reply done"); watch it fire and the node respond.
3. Decision loop: node instructed to ask the human a question (elicitation/agent_needs_input path) -> pending decision appears in Cockpit -> resolve with an answer -> node receives it and completes. If ntfy is configured on this box, verify the phone path too; otherwise verify the channel-inbound simulation via the existing /api/channels inbound route with a real ntfy topic later.
4. Codex spot check: one codex node, verify `node.turn_complete` from notify and thread-id recorded.

## Release status

Local-only mission work; not released. Last published release: v0.1.10 (2026-05-07).

## Status

- W1: not started
- W2: not started
- W3: not started
- W4: not started
- W5: not started
- Gate: not run
