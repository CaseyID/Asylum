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
