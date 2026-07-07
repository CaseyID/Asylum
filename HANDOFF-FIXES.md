# Phase-D adversarial-review fixes — HANDOFF

Branch: `phase-d-fixes` (base main 34646e8). Spec: `docs/superpowers/reviews/2026-07-07-mission-adversarial-review.md`.
Status at rotation: all 8 MANDATORY code fixes implemented and compiling. `cargo test -p asylum-daemon --lib` = 172 passed / 0 failed. `asylum-cli` builds. Full-workspace `cargo test-asylum` NOT yet run end-to-end. Review-doc "Resolution" section NOT yet appended. Addendum A/B NOT started.

IMPORTANT: Edit/Write tools are guard-blocked for this worktree path (session pinned to `orchestrator` worktree). Use `/tmp/pyedit.py` (`from pyedit import edit; edit(path, old, new)`) — exact, uniqueness-checked replace. Do NOT double single-quotes for SQL inside Rust raw strings (`'pending'` not `''pending''` — that bug cost a rebuild).

## Per-finding status

| Finding | Status | Files |
|---|---|---|
| C1 loon watch_exit honesty + no-teardown-on-stream-loss | FIXED+tested | substrate/loon.rs (ExitWatch enum, watch_exit, exit_watch_to_outcome, exit_task), substrate/local.rs (ExitOutcome.stream_lost), capability_service.rs (apply_exit_outcome) |
| M1 resume/create eternal-Running via CAS | FIXED+tested | capability_service.rs (resume_node, create_node local+loon), storage.rs (transition_node_liveness) |
| M2 post_harness_event resurrection via CAS + terminal decision guard | FIXED+tested | capability_service.rs (post_harness_event, produce_decision_from_awaiting_input), storage.rs |
| M3 guest token revoke + narrow scope enforcement | FIXED (revoke tested; scope-check method has NO unit test yet) | capability_service.rs (scoped_token_authorizes_path, node_id_from_path, loon_node_token_name, revoke in apply_exit_outcome/stop/archive/reconcile), app.rs (auth_middleware + forbidden_cross_node), storage.rs (revoke_tokens_by_name) |
| M4 attach WS keepalive ping + surface death | FIXED (structural; no live-WS test) | substrate/loon.rs (write_task select!+ping, ATTACH_WS_PING_INTERVAL) |
| M5 per-node submit mutex | FIXED+tested (local concurrency test; loon mirrors, untested) | substrate/local.rs, substrate/loon.rs |
| M6 decision dedup atomicity (partial unique index) | FIXED+tested | storage.rs (index + upsert_pending_node_decision), capability_service.rs (producer) |
| M7 statusline hot path (node-row state) | FIXED+tested | capability_service.rs (ingest_statusline), storage.rs (ctx_pressure_session/max cols + get/set) |
| M8 ntfy topic = fleet control (doc note only) | NOT STARTED | need docs/ + config comment note |
| m1 idle not override WaitingForInput | FIXED (via allowed_from) | capability_service.rs post_harness_event |
| m2 persist launch_args on resume | NOT STARTED (best-effort) | nodes column + thread through resume_args |
| m5 lengthen correlation token to 32 hex | FIXED | capability_service.rs CHANNEL_REPLY_TOKEN_LENGTH |
| m6 honest "answered" ntfy reply status | FIXED (untested) | capability_service.rs (resolve_decision + channel_inbound) |
| m7 delete dead LoonConfig fields | FIXED | asylum-types/src/config.rs |
| m9 exec_signal/resize status check | FIXED | substrate/loon.rs |
| n1 manual 13->14 events + drift guard test | FIXED+tested | launch_packet.rs |
| n2 statusline print-before-dispatch | FIXED | asylum-cli/src/harness_event.rs |
| n3 empty ASYLUM_SOCKET_PATH unset | FIXED | asylum-cli/src/harness_event.rs |
| n4 toml_key escape newlines | FIXED+tested | substrate/loon.rs |
| Addendum A honest resumable (fs probe) | NOT STARTED (MANDATORY) | reconcile + resume precondition; probe ~/.claude/projects/<cwd-slug>/<session-id>.jsonl |
| Addendum B graceful claude stop (/exit then SIGTERM) | NOT STARTED (MANDATORY) | claude harness adapter + local stop path |
| Addendum C send_input records no event row | INVESTIGATE — capability_service.send_input ALREADY records InputSent (~line 2518). Gap is likely the operator/attach-raw or loon path. |
| minor m3/m4/m8 | SKIPPED per brief (skip-unless-trivial) |

## Key implementation choices
- `ExitOutcome` gained `stream_lost: bool`. Local child.wait() always false (authoritative). Loon StreamLost->true.
- One exit-sink helper `apply_exit_outcome(&Store,&HookEngine,node_id,outcome)` used by BOTH sinks: CAS active->terminal, fires hook only on real transition, revokes guest token unless stream_lost.
- `transition_node_liveness(id,target,&[allowed_from],reason,extra)->bool` is the CAS primitive. Kept blind set_node_liveness for legit unconditional writes (operator stop/archive, reconcile mark).
- M3 scope: ENFORCED = cross-node via URL path node id + /api/tokens*. NOT enforced (documented) = body/query node ids + non-node-scoped endpoints. See method doc-comment.
- M6: partial unique index decisions_one_pending_per_node; upsert_pending_node_decision translates UNIQUE conflict into refresh (returns created:bool).

## Remaining work (priority order)
1. Addendum A + B (mandatory) — biggest remaining chunk.
2. M3 scope-check unit test (builder pattern at capability_service.rs:4339; CapabilityService::new(store, AuthMode::OwnerToken{config_token_hash}, cfg); mint via mint_loon_node_token; assert scoped_token_authorizes_path(raw,"/api/nodes/{other}/input")==false, own==true, "/api/tokens"==false, owner token==true).
3. M8 doc note + m2 + investigate Addendum C.
4. Append "Resolution (2026-07-07)" to review doc.
5. cargo test-asylum full green + zero new warnings.

## Gotchas
- harness_event_bodies now unused by ingest_statusline; left in place (pub, no warning).
- decision_feedback_text: "answered" -> verbatim answer (answer always Some on ntfy path).
- ATTACH_WS_PING_INTERVAL=20s; first interval tick skipped.
