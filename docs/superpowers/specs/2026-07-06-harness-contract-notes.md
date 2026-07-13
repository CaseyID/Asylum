# Harness integration contract notes (researched 2026-07-06)

Reference for the mission's Phase B/C work (see 2026-07-06-asylum-completion-mission.md). Facts verified against live docs (code.claude.com/docs) and local `claude --help` / `codex --help` on this machine. Re-verify obscure hook events against a live session before shipping code that depends on them.

## Claude Code: the channels Asylum will use

- Per-launch injection without touching user config: `--settings '<inline JSON>'` (overrides matching settings keys for this run only). Already-used: `--mcp-config '<json>' --strict-mcp-config`. Also available: `--append-system-prompt`, `--add-dir`, `--session-id <uuid>`.
- `--session-id <uuid>`: Asylum can PRE-ASSIGN the session id at spawn. This is the resume key. `claude --resume <id>` must run from the same project directory (id lookup is cwd/worktree-scoped). `--continue` resumes most recent in cwd. Both accept a positional prompt in `-p` mode.
- Hooks (configured via the injected `--settings` JSON under `"hooks"` key):
  - Common stdin JSON fields: `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `permission_mode`.
  - `Notification` hook, matcher values: `permission_prompt`, `idle_prompt`, `agent_needs_input`, `agent_completed`, `auth_success`, elicitation_*. Payload adds `type`, `message`. Non-blocking. This is the awaiting-input / idle producer.
  - `Stop` hook: fires when the agent finishes a turn. Turn-complete producer.
  - `SessionStart`: `source` = startup|resume|clear|compact, plus `model`. Session-identity confirmation producer.
  - `PreToolUse`/`PostToolUse`: `tool_name`, `tool_input` (+`tool_response`). Optional tool_call producer (chatty; use async hooks `"async": true` to avoid blocking the session).
  - `SessionEnd`: session termination producer.
  - Command hooks: exit 0 non-blocking for Notification/PostToolUse/SessionStart/SessionEnd regardless of code. Default timeout 600s; set low timeouts + `async` for reporting hooks.
- Statusline: `"statusLine": {"type":"command","command":"..."}` in settings. Command receives full JSON on stdin after every assistant message, including `context_window.used_percentage` / `remaining_percentage`, `cost.total_cost_usd`, `session_id`, `model`. This is the `node.ctx_pressure` producer (pre-calculated percent; no transcript parsing).
- Trust gate: hooks + statusline only run in trusted workspaces. Asylum already pre-trusts workspaces at launch (`pre_trust_workspace`), which covers this.
- `--dangerously-skip-permissions` should be the leading flag when combined with `--resume` (documented routing edge case in v2.1.199+).
- Transcript JSONL at `~/.claude/projects/<slug>/<session-id>.jsonl` is version-unstable; do NOT parse it. Use hooks/statusline payloads.
- `CLAUDE_CONFIG_DIR` can fully relocate `~/.claude` per instance if isolation is ever needed (not planned; note only).

## Codex: the channels Asylum will use

- Per-launch config: repeatable `-c key=value` (TOML-parsed values, dotted paths). Already-used for MCP injection: `-c mcp_servers.asylum.command=... -c mcp_servers.asylum.args=["mcp"] ...`.
- `notify` config (root key, array-of-strings command): Codex appends one argv element of JSON. VERIFIED against openai/codex tag rust-v0.132.0 (matches installed codex-cli 0.132.0), `codex-rs/hooks/src/legacy_notify.rs`: exactly ONE event type, `agent-turn-complete` (there is no `approval-requested` in the external notify contract — that name is TUI-internal only). Flat JSON, kebab-case fields: `type`, `thread-id` (UUID; exact match to the rollout filename suffix `rollout-<date>-<thread-id>.jsonl` under `~/.codex/sessions/<Y>/<M>/<D>/` — glob on `*-<thread-id>.jsonl`, the date is session-start time), `turn-id`, `cwd`, `client` (omitted when absent), `input-messages` (array of strings), `last-assistant-message` (string|null). Fires once per completed turn, in both TUI and `codex exec` modes, whenever `notify` is a non-empty array (no feature flag). The notify subprocess gets stdin/stdout/stderr all nulled — read the JSON from argv, never stdin. Injectable per-launch via `-c notify=["/path/to/cmd","arg"]`. Codex also has a newer declarative hooks.json system (PermissionRequest etc.) if more signals are ever needed.
- Resume: `codex resume <SESSION_ID> [PROMPT]` (interactive), `codex exec resume` (scripted), `--last`, `--all` (disable cwd filter). Session id is NOT pre-assignable; discover it from rollout files at `~/.codex/sessions/<Y>/<M>/<D>/rollout-<timestamp>-<uuid>.jsonl` (confirmed layout on this machine) or the notify payload if it carries one.
- No ephemeral MCP-injection flag equivalent to --strict-mcp-config; `-c mcp_servers.*` overrides are the per-launch mechanism (already in use).
- Permission bypass: `--dangerously-bypass-approvals-and-sandbox` (already in use). `--dangerously-bypass-hook-trust` exists for the hook-trust gate.
- No statusline equivalent: context-pressure signal for codex is not directly available; options are the notify turn-complete cadence + rollout file token metadata (verify) or accept ctx_pressure as claude-only initially.

## Design implications adopted for Phase B

1. The hook bridge = an `asylum` CLI subcommand (e.g. `asylum harness-event <event>`) invoked by injected Claude hooks / Codex notify, reading the harness JSON from stdin/argv and POSTing a structured event to the daemon over the already-injected `ASYLUM_SOCKET_PATH` with `ASYLUM_NODE_ID`. Asylum stays dumb plumbing.
2. `node.idle` for claude comes from Notification `idle_prompt` (native), not byte-quiescence. Quiescence timer remains only as a codex fallback if notify proves insufficient.
3. `node.ctx_pressure` comes from an injected statusLine command (claude). Codex initially exempt.
4. Awaiting-input/decision producer = Notification `permission_prompt`/`agent_needs_input` (claude) and `approval-requested` (codex).
5. Session identity: claude = pre-assigned `--session-id` UUID recorded at create time; codex = discovered from rollout/notify and recorded via the bridge.
6. Resume: claude `--resume <recorded-id>` from the node's workspace dir; codex `codex resume <recorded-id>`.

## 2026-07-13 -- Launch profile flags (verified live)

Verified live on this machine against the installed harness versions. Asylum
passes every value through VERBATIM -- no model/effort catalogs, no validation.
The harness is authoritative and rejects a bad value itself.

- claude 2.1.207:
  - Model: per-launch flag `--model <value>`. Accepts an alias (`sonnet`,
    `opus`) or a full model name.
  - Reasoning effort: per-launch flag `--effort <level>`. Accepted values:
    `low`, `medium`, `high`, `xhigh`, `max`.
- codex 0.144.1:
  - Model: dotted TOML config override `-c model=<value>`.
  - Reasoning effort: dotted TOML config override `-c model_reasoning_effort=<value>`.
  - Each `-c` value is parsed as TOML with a raw-string fallback, so a bare model
    name (e.g. `-c model=gpt-5-codex`) is accepted unquoted.

Implementation notes (WS2):
- Adapter surface: `HarnessAdapter::profile_args(model, effort)` translates the
  launch profile into the per-launch argv above; `supports_model()` /
  `supports_effort()` advertise the capability honestly on the harness
  descriptor. Both bundled adapters support both options; an adapter that cannot
  express a requested option returns an explicit `UnsupportedProfileOption` error
  (CAP-012 shape) that propagates out of `create_node`/`spawn_peer` -- never a
  silent no-op.
- Argv order in `create_node`: `launch_args()` ++ control injection ++
  `profile_args` ++ per-request `launch_args` (trailing user args stay last).
- Persistence: the effective model/effort are recorded on the node row at launch
  time (NULL = harness default). Resume re-derives and re-applies the recorded
  profile the same way it reuses stored `launch_args`.
- Peers do NOT inherit the parent's profile (HARN-006); only an explicitly-set
  `model`/`effort` on `spawn_peer` is applied.

## 2026-07-13 -- Local portable_pty crash (2026-07-07) closed as not reproducible

The one-time claude 2.1.202 local-`portable_pty` "output.write assertion" crash
from the v0.2.0 final live check does not reproduce on current versions.
Method: a standalone repro crate mirroring `substrate/local.rs`'s exact launch
sequence (openpty -> spawn_command -> try_clone_reader/take_writer -> drop
slave -> reader thread -> two-write `/exit` submit) ran 5 times against the
interactive claude TUI up to and including a clean `/exit`, never submitting a
prompt/turn. Versions: claude 2.1.207, portable-pty 0.8.1 (the crashing
2.1.202 binary is no longer available to test). Outcome: 5/5 clean launches
and exits, exit code 0, no assertion or panic, no hung children. No code
change made. If the crash recurs, capture the exact assertion text and
`claude --version` at the time -- the failing range could not be pinned from a
single historical occurrence.

## 2026-07-13 -- Claude AskUserQuestion menu contract (verified live)

Verified empirically against claude 2.1.207 in a raw PTY (no daemon):

- Hook payload: a `PreToolUse` hook with matcher `AskUserQuestion` receives
  structured options. `tool_input.questions` is an array; each question has
  `question` (string), `header` (string), `options` (array of
  `{label, description}`), and `multiSelect` (bool). The top-level payload also
  carries `tool_name`, `tool_use_id`, `session_id`, `transcript_path`, `cwd`,
  `permission_mode`, and `effort`.
- Menu input: the single-select menu opens with option index 0 highlighted. To
  select 0-based index N, emit N down-arrow sequences (`0x1b 0x5b 0x42`), each
  as its own PTY write with pacing, then a lone carriage return (`0x0d`) as its
  own write. Proven live: one down + CR selected option 2, two down + CR
  selected option 3 (both non-default). A single coalesced burst risks
  paste-detection; distinct paced writes are required.
- The TUI appends extra trailing items ("Type something", "Chat about this")
  after the model's options, but down-count navigation from the top is
  unaffected because the real options come first.
- Asylum applies typed delivery only to single-question, `multiSelect=false`
  dialogs; multi-select and multi-question dialogs fall through to the
  free-text awaiting-input path unchanged.

## 2026-07-13 -- Claude 2.1.207 startup swallows early PTY input (launch-prompt readiness)

Observed live: after the welcome box renders and the SessionStart hook fires,
claude 2.1.207's composer still swallows PTY input for ~9 more seconds (a
"connecting" phase; the swallowed bytes are never echoed or queued). Neither
PTY-output quiescence nor the `session_started` event is a sufficient
readiness gate -- both occur inside the swallow window. This regressed
launch-prompt auto-delivery, which worked on 2.1.202 during the v0.2.0 gates.

Asylum's contract (local substrate, claude only): deliver-and-confirm.
Delivery is floor-gated on `SessionStart`, then confirmed via an injected
async `UserPromptSubmit` hook (fires at prompt submission, mechanical, no TUI
parsing); unconfirmed deliveries are retried at a 15s interval up to 3
attempts, every redelivery warn-logged. Bounded residual: confirmations
slower than 15s can cause up to 2 duplicate submissions (logged); an operator
prompt during the retry window latches the confirmation and stops the loop
(logged). Codex keeps its original timing + submit-nudge path; the loon
substrate keeps timing-based delivery and needs the same port if loon guest
images move to claude >= 2.1.207. Also verified in this build:
`UserPromptSubmit` and `SessionStart` hook payloads arrive reliably over the
injected `--settings` hooks, while claude statusline telemetry carries only
`used_percentage` (so `tokens_in` cannot be populated from it).

## 2026-07-13 -- Claude 2.1.207 statusline payload (verified live)

The statusline JSON piped to the injected statusLine command carries:
`session_id`, `transcript_path`, `cwd`, `effort.level`,
`model.{id,display_name}`, `workspace`, `version`, `output_style`,
`cost.{total_cost_usd,total_duration_ms,total_api_duration_ms,total_lines_added,total_lines_removed}`,
`context_window.{total_input_tokens,total_output_tokens,context_window_size,current_usage.{input_tokens,output_tokens,cache_creation_input_tokens,cache_read_input_tokens},used_percentage,remaining_percentage}`,
`exceeds_200k_tokens`, `fast_mode`, `thinking.enabled`, and post-turn
`rate_limits.{five_hour,seven_day}`.

Semantics (established from the captured sample's own arithmetic):
`total_input_tokens` equals `current_usage` input + cache_creation +
cache_read and divided by `context_window_size` reproduces
`used_percentage` -- it is the CURRENT context-window occupancy (including
cached tokens), not a monotonic per-session total; `total_output_tokens`
mirrors `current_usage.output_tokens`. Both are 0/null on the pre-turn
render and populate after the first API turn. Asylum passes these through
to `tokens_in`/`tokens_out` as the harness-reported occupancy snapshot
(char/4 estimate remains the fallback), documented as not
magnitude-comparable to codex's cumulative estimate. Statusline renders
multiple times per turn, so delta-accumulating these snapshots would
double-count -- a true cumulative needs per-turn structured usage instead.

The loon substrate now shares the local claude launch-prompt
deliver-and-confirm contract (see the 2026-07-13 readiness section above);
proven live on a real microVM guest: session_started posted from the guest,
prompt delivered exactly once, prompt-accepted confirmation dispatched to
the loon substrate.
