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

W0 — input delivery correctness (crates/asylum-daemon/src/substrate/local.rs + harness launch). BLOCKER, found in the 2026-07-06 live E2E. Two real bugs on the exact path a supervisor uses to drive a worker:
- The launch prompt (passed as a trailing positional argv to `claude`) is never auto-submitted — it lands in the input box and the session sits idle. Launch must deliver the initial prompt as a submitted message (send it over the PTY after the TUI is ready, or use the harness's documented initial-prompt mechanism, not a bare positional).
- `send_input`/`send_input_raw` write `text + "\r"` in a single PTY `write_all`; Claude's TUI absorbs the CR as pasted content, so nothing submits until a separate lone `\r` is sent. Fix: send the text, then submit Enter as a distinct write (small delay or bracketed-paste-aware sequence) so a single `node send` both enters and submits. Verify with a real claude session: one `node send` delivers AND submits.
Get W0 right first — every downstream gate (hook actions doing send_input, decision feedback injection, supervisor feeding workers) depends on input actually submitting.

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

## E2E baseline (2026-07-06)

Live spawn-peer test PASSED the core capability: a supervisor claude node, via injected MCP, called node.spawn_peer and produced a real second live claude session + a correctly-typed `spawned_for` relationship edge; two independent `claude`+`asylum mcp` process trees confirmed; supervisor also called node.list (got 2) and reported back. MCP injection (`--mcp-config`, `--strict-mcp-config`, ASYLUM_NODE_ID/SOCKET env, `--allowedTools mcp__asylum__*`) works. Bugs found → W0. Also noted: `node.spawn_peer` MCP tool takes `description` not `prompt` (the supervisor adapted); W4/W3 should consider exposing an explicit initial-prompt param for spawned peers so a supervisor's intended first instruction is delivered as the worker's submitted prompt.

## W1 delivered contract (merged to main f7186bb, 2026-07-06)

W2/W3 build against this:
- Ingestion: `POST /api/nodes/{id}/harness-event`, protected router (owner-token, socket + HTTP). Body: `{ "source": "claude_hook"|"claude_statusline"|"codex_notify", "payload": <verbatim harness JSON> }`. Response 200: `{ "accepted": bool, "event": <mapped kind|omitted>, "session_id": <recorded|omitted> }`. 400 on bad UUID / node-not-found / bad body. Mapping is entirely daemon-side — the CLI bridge just forwards source+payload.
- Mapping: claude_hook dispatches on `hook_event_name` (Stop→turn_complete, SessionStart→session_started, SessionEnd→session_end, PostToolUse→tool_call, Notification by `type`: permission_prompt/agent_needs_input/elicitation*→awaiting_input, idle_prompt→idle, agent_completed→turn_complete). codex_notify agent-turn-complete→turn_complete (session id from `thread-id`). claude_statusline reads `context_window.used_percentage`, updates ctx_pct, fires node.ctx_pressure on threshold crossings (config `[autonomy] ctx_pressure_thresholds` default [75,90]).
- Catalog (13, final): graph.spawn, node.session_started, node.turn_complete, node.awaiting_input, node.idle, node.ctx_pressure, node.tool_call, node.session_end, node.exited, node.errored, channel.inbound, schedule.5m, schedule.30m. (removed permission_requested→merged into awaiting_input; removed substrate.unreachable, schedule.cron.)
- Liveness: awaiting_input→WaitingForInput; turn_complete/idle/session_started/tool_call→Running; session_end/ctx_pressure→no change; terminal nodes reject but still record session id.
- Interrupt now sends Ctrl-C only (no forced Stopped/exited). Exit sink: clean exit→Stopped+node.exited(exit_code); abnormal→Failed+node.errored. Quiescence idle: 30s sweep fires node.idle for Running Local nodes with native_idle_signal()==false (codex) after autonomy.idle_quiescence_seconds (default 120); claude skipped (native idle via Notification).
- Schema: nullable nodes.harness_session_id (ensure_column migration). New config `[autonomy]`.

## W0 delivered contract (merged to main, 2026-07-06)

- Launch prompt is no longer passed as a positional argv for Local nodes. It flows via `SubstrateContext.launch_prompt`; `LocalSubstrate::launch` spawns a background task that waits for readiness (first PTY frame, then 600ms output-quiet, bounded 20s first-output / 10s overall — timing only, no output parsing) and delivers the prompt as a submitted message. Loon path unchanged (`LoonContext.prompt`).
- The type-and-submit contract lives in `LocalSubstrate::submit_over_writer`: body write, 50ms gap, then a lone `\r` as a DISTINCT write (claude's TUI absorbs a bundled CR as pasted content). `send_input` uses it — one call both enters AND submits; use it for hook `send_input` actions, `resolve_decision` feedback, supervisor-drives-worker. `send_input_raw` stays truly raw (no appended Enter) and is reserved for the interactive attach WS path.
- Live-verified against real claude 2.1.202 (isolated daemon, one session): launch prompt auto-submitted (LAUNCH-OK) and a single send delivered+submitted (SEND-OK).
- Readiness is a fixed timing heuristic (no config knob); a continuously-rendering harness gets its prompt at the 10s cap.

## W2 delivered contract (merged to main 329761c, 2026-07-06)

W3/W4 build against this:
- New CLI subcommand family in `crates/asylum-cli`: `asylum harness-event <source>` where `<source>` is `claude-hook`, `claude-statusline`, or `codex-notify`. Stays thin — no interpretation of the payload; forwards verbatim JSON as `POST /api/nodes/{id}/harness-event` with body `{"source": "claude_hook"|"claude_statusline"|"codex_notify", "payload": <verbatim JSON>}` (W1's endpoint).
- Input acquisition: `claude-hook`/`claude-statusline` read JSON from stdin; `codex-notify` takes the JSON as an optional trailing positional argv element (never reads stdin, matching Codex nulling stdin/stdout/stderr for the notify subprocess).
- `claude-statusline` always prints exactly one status line to stdout after attempting the post (regardless of success/failure) — dumb formatting: `"<model display_name> | ctx <used_percentage>%"` with graceful fallbacks (`"claude"` when model missing, ctx clause omitted when `context_window.used_percentage` missing). Never prints errors to stdout.
- Node/daemon resolution mirrors `cli::runtime_client`'s existing precedence: `ASYLUM_SOCKET_PATH` (unauthenticated Unix socket, matches `asylum mcp`'s injected env) wins when set; otherwise falls back to HTTP via `ASYLUM_BASE_URL` + bearer `ASYLUM_TOKEN` (for future Loon guests); otherwise falls back to the default local socket path. Added `AsylumClient::new_socket_with_timeout` / `new_with_timeout` (2s request timeout, no retries) and `AsylumClient::post_harness_event` in `crates/asylum-cli/src/client.rs`.
- Core logic lives in `crates/asylum-cli/src/harness_event.rs`: pure `build_request`, `render_statusline`, `resolve_target` functions plus a testable `dispatch` core (returns `Result<_, String>`, never panics). The shipped entry points (`run_claude_hook`, `run_claude_statusline`, `run_codex_notify`) always return `()` — bad JSON, missing `ASYLUM_NODE_ID`, or an unreachable daemon are logged to stderr only and the process always exits 0. Verified live: manual smoke of all three sources against no daemon exits 0 with correct stderr/stdout behavior.
- Tests: 22 new unit tests in `harness_event.rs` (request-shape forwarding for Stop/Notification/SessionStart/codex-notify/statusline; statusline rendering full+fallback; env-resolution precedence; exit-0-on-failure; two real-transport round trips against one-shot TCP and Unix-socket test servers — no stub transports in the shipped path) plus 1 clap-parsing test in `cli.rs`. `asylum-cli` lib tests: 46 pre-existing -> 69 (23 new). Full `cargo test-asylum` green: 136 daemon tests unchanged, 4 asylum-types tests unchanged, 64 cockpit vitest tests unchanged.

## W3 delivered contract (branch phase-b-w3, 2026-07-06)

W4/Phase C build against this. Launch injection lives in the daemon harness adapters; the CLI/bridge is untouched.

- Assembly order (`capability_service::create_node`, Local substrate only): `adapter.launch_args()` (leading `--dangerously-skip-permissions` + config `startup_args`) ++ `adapter.asylum_control_args(binary, socket, node_id, session_id)` ++ per-request `request.launch_args`. The leading trust-bypass flag stays first (documented claude routing quirk); user args always append last. Loon launches inject no control args (unchanged).
- Session pre-assignment: new trait method `HarnessAdapter::preassign_session_id() -> Option<Uuid>`. Claude returns `Some(Uuid::new_v4())` (fresh per launch); codex returns `None`. `create_node` calls it once, records the id on the node row via `set_node_harness_session_id` BEFORE launch, and threads it into `asylum_control_args`. This is the Phase C resume key. W1's ingestion path is unchanged and simply confirms the same id when the SessionStart post arrives (no-op — verified live: pre-assigned id == session_started `session_id`).
- Claude control args (`harness/claude.rs`), in order: `--mcp-config <json>` `--strict-mcp-config` `--allowedTools mcp__asylum__*` (pre-existing) then `--session-id <uuid>` then `--settings <inline json>`.
  - `--settings` JSON shape (verified against claude 2.1.202): `{"hooks":{"Stop":[G],"Notification":[G],"SessionStart":[G],"SessionEnd":[G],"PostToolUse":[Gp]},"statusLine":{"type":"command","command":"'<bin>' harness-event claude-statusline"}}` where each matcher-group `G = {"hooks":[{"type":"command","command":"'<bin>' harness-event claude-hook","timeout":10}]}` and `Gp` is the same plus `"async":true` (PostToolUse only — non-blocking so tool execution is never stalled). No `matcher` field on any group: omitting it fires the hook for ALL occurrences (all Notification types, all SessionStart sources/SessionEnd reasons, all tools incl. MCP) — the daemon maps by payload content (W1), so Asylum stays dumb plumbing. `timeout` is in SECONDS (10s, well under claude's 600s default).
  - `<bin>` is `current_asylum_binary()` (the daemon's own `env::current_exe()`, i.e. the unified `asylum` binary that also exposes `harness-event` and `mcp`), POSIX single-quoted via `shell_quote` since claude runs hook/statusline commands through a shell. The bridge resolves the node from `ASYLUM_NODE_ID` / `ASYLUM_SOCKET_PATH`, which `local_launch_env` already sets on the claude process environment, so hook child processes inherit them (no extra env plumbing).
- Codex control args (`harness/codex.rs`): unchanged MCP `-c` overrides plus one new pair `-c notify=["<bin>","harness-event","codex-notify"]` (TOML array-of-strings, same `toml_string` quoting as the MCP args). Codex has no `--session-id`; `preassign_session_id()` is `None` and `asylum_control_args`' `session_id` param is ignored. W1 records codex's thread-id from the first `agent-turn-complete` notify post — nothing more needed daemon-side.
- Tests: `crates/asylum-daemon/src/harness/mod.rs` extended in place (no duplication). Updated the two existing control-args tests to the new signature and asserted the full injected shape: claude test parses `--settings` and asserts every hook's command/timeout/async and the statusLine command, plus `--session-id <uuid>`; codex test asserts the exact `notify=[...]` value and that no `--session-id`/`--settings` leak in. Added `only_claude_preassigns_a_session_id` (claude Some + fresh each call, codex None) and `claude_control_args_keep_skip_permissions_leading_and_append_user_args` (proves leading flag + user args append after control args). Daemon-lib tests 139 -> 142 (+3 net; two existing tests rewritten). Full `cargo test-asylum` green: 142 daemon-lib, 69 cli, 4 asylum-types, 64 cockpit vitest.
- Live verification (real claude 2.1.202, one isolated daemon on 127.0.0.1:8791, scratch db/socket under /tmp/w3-verify, real HOME for auth, one claude node in a scratch git workspace): node row carried the pre-assigned `harness_session_id` at create; `node.session_started` (source `startup`) arrived carrying the SAME id (no-op confirm); `node.telemetry` from the statusLine bridge updated `ctx_pct` (0.0 -> 0.05 over the run); `node.tool_call` from the async PostToolUse hook captured 45 tool calls (the node self-drove a full Asylum MCP smoke-test off the instruction prompt — spawned+archived a peer, exercised decisions/hooks/channels); after an interrupt + a trivial `send_input`, `node.turn_complete` (source `claude_hook`) arrived. Pre-trust covered the injected settings (hooks/statusline ran — trust gate passed). Node stopped and daemon torn down immediately; one session only.
- Note for W4/Phase C: there is still no explicit initial-prompt param on create/spawn — the launched node receives the auto-generated instruction prompt (W0 submits it), which makes claude immediately go off and exercise its MCP surface. W4's `spawn` action / a spawn `prompt` field should let a supervisor set the worker's first submitted instruction. Phase C resume: use the recorded `harness_session_id` with `claude --resume <id>` from the node's workspace dir (id is cwd/repo-scoped); it is now guaranteed present from create time for Local claude nodes.

## Status

- W0: COMPLETE — merged to main (branch phase-b-w0); live-verified launch-submit and send-submit; 139 daemon-lib tests green (+3). See delivered contract above.
- W1: COMPLETE — merged to main f7186bb; 136 daemon-lib tests green (+12).
- W2 (CLI bridge `asylum harness-event`): COMPLETE — merged to main 329761c; 69 cli tests green (+23). See delivered contract above.
- W3 (launch injection: --session-id, --settings hooks+statusLine for claude; -c notify for codex): COMPLETE — branch phase-b-w3; 142 daemon-lib tests green (+3); live-verified session_started (pre-assigned id confirmed), turn_complete, statusline ctx telemetry, and PostToolUse tool_call all arrived. See delivered contract above.
- W4 (hook actions send_input + honest spawn, delete recipes; decision producer from awaiting_input; resolve_decision feedback injection): not started.
- W5 (cockpit: catalog-matched pickers, decision surfacing, liveness chips): not started.
- Gate: not run
