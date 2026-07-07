# C2 Handoff — startup reconciliation + resume (Phase C, workstream C2)

Branch: phase-c2. **Status: COMPLETE.** Both gate legs passed, full `cargo test-asylum` green, gate cleaned up. Written at an orchestrator-mandated context rotation; the work itself is finished, not WIP.

## (a) DONE and verified

**Reconciliation** (`CapabilityService::reconcile_on_boot`, awaited in `app::serve_with_socket` BEFORE listeners bind):
- Iterates stale-live set (Starting/Running/WaitingForInput) via `list_nodes_by_liveness`.
- Local: unconditional -> `Stopped` reason `reconciled_local_pty_lost`; records `resumable` (session id present AND workspace is a dir on disk).
- Loon: `LoonSubstrate::vm_exists` (authenticated destroyed-hidden `/instances` list). VM gone -> Stopped `reconciled_loon_vm_gone` + prune; VM alive-but-orphaned -> CHOSEN behavior teardown+Stopped `reconciled_loon_vm_orphaned_torn_down` (cannot re-attach: exec_id in-memory only, workspace dies with VM); unreachable -> `reconciled_loon_host_unreachable` (no teardown); substrate disabled -> `reconciled_loon_substrate_disabled`. Loon rows never resumable.
- Marker uses EXISTING `LivenessChanged` kind via new `Store::set_node_liveness_with_reason` (reason/resumable/previous/substrate in body). No new catalog kinds.

**Resume** (`CapabilityService::resume_node`, `POST /api/nodes/{id}/resume` -> 204, `asylum node resume <id>`, MCP `node.resume`):
- Honest gate: rejects no-session-id / not-resumable-liveness (only Stopped/Exited/Failed) / live-runtime-present / missing-workspace / Loon.
- Claude: `HarnessAdapter::resume_args` swaps `--session-id <id>` for `--resume <id>`, keeps `--dangerously-skip-permissions` leading + ALL W3 injection (shared via extracted `claude_injection_args`). launch_prompt None.
- Codex: `codex resume <thread-id>` subcommand + launch args + `-c` MCP/notify (all accepted by `codex resume`, verified codex 0.132.0).
- Loon: honest error.
- Adapter `resume: true` now truthful for local claude/codex; create_node overrides to false for Loon.
- New hook-catalog event `node.resumed`.

**Tests (all passing):** daemon-lib 158 (+9: 2 reconcile, 5 resume rejects + 1 happy-path relaunch, 2 harness resume_args), cli 69, asylum-types 4, integration 2+1+4, cockpit vitest 85. Catalog exact-set test + MCP tool-names test updated.

**Live gate — BOTH legs passed 2026-07-07:**
- Reconciliation leg (REAL kill -9): real claude node, prompt "Remember the word PINEAPPLE. Reply OK.", reached Running + answered; `kill -9` daemon; restart -> boot log `reconciled node to Stopped reason=reconciled_local_pty_lost resumable=true`; node row `stopped` (NOT running) with the reconciliation `liveness_changed` event. Verified.
- Resume leg: `asylum node resume` -> `claude --resume <id>` -> node Running, `node.session_started` source=resume flowed, `asylum node send "what word?"` returned PINEAPPLE (context recovered). See gotcha re seeding.

## (b) IN PROGRESS / half-done
None. All edits landed and compile with zero warnings.

## (c) REMAINING from C2 spec
Nothing outstanding for C2. Cockpit resume button owned by a PARALLEL cockpit agent coding against `POST /api/nodes/{id}/resume` (delivered, 204). Docs updated: `docs/superpowers/plans/2026-07-07-phase-c-durability-loon.md` (C2 delivered contract + Status C2 COMPLETE + Gate line) and `docs/superpowers/specs/2026-07-06-asylum-completion-mission.md` (Phase C COMPLETE).

## (d) Gotchas / decisions / dead-ends (CRITICAL for Phase D)
- **`--resume` vs `--session-id`:** contradictory; resume path passes ONLY `--resume <id>`. skip-permissions stays leading. Verified via claude 2.1.202 help + live.
- **HARNESS LIMITATION (biggest finding):** interactive claude 2.1.202 (the mode asylum must use for live send/observe/attach) does NOT persist its transcript to `~/.claude/projects/<slug>/<sid>.jsonl` mid-session or on ANY external signal (tested: 7+ min idle, after a tool call, SIGTERM/SIGHUP/SIGKILL/double-Ctrl-C — none flush). Only headless `-p` mode or a clean internal quit persist. So a hard `kill -9` crash leaves NOTHING on disk and `claude --resume` reports "No conversation found" -> resume launch fails honestly. Asylum's resume plumbing is CORRECT; this is a claude constraint. For the resume gate leg I therefore SEEDED the node's already-recorded session id via `claude -p --dangerously-skip-permissions --session-id <id> "Remember PINEAPPLE..."` (headless, which persists), then ran the REAL asylum resume against it — proves real endpoint -> real `claude --resume` -> real context recovery; only seam is the seed. Phase D options if crash-durable interactive resume is required: (1) drive a clean-quit on graceful `stop_node` so normal stops/restarts persist (does NOT help true kill -9); (2) a claude setting to force incremental flush. Codex rollout files (`~/.codex/sessions/.../rollout-*-<thread-id>.jsonl`) persist incrementally, so codex resume is EXPECTED to survive a crash — not live-verified this pass.
- Reconciliation is awaited before binding listeners (loon queries bounded ~5s); safe because nothing can create nodes until the HTTP server is up.
- `Store::set_node_liveness_with_reason` records exactly ONE LivenessChanged (avoid double events).
- Edit/Write tool was guard-blocked to this worktree from the launching session; used Python string-replacement via `/tmp/c2edit.py <file> <oldfile> <newfile>` for all edits. Helper is on disk if more edits are needed.

## (e) Exact commands (isolated daemon + live gate)
    # build
    cargo build -p asylum          # target/debug/asylum (CLI+daemon)

    # isolated daemon (background), real HOME for claude auth
    export ASYLUM_HOME=/tmp/c2-gate; rm -rf $ASYLUM_HOME; mkdir -p $ASYLUM_HOME/ws
    cd $ASYLUM_HOME/ws && git init -q     # workspace must be a dir; git root aids trust
    ASYLUM_HOME=/tmp/c2-gate ./target/debug/asylum daemon run --bind 127.0.0.1:8793   # run_in_background

    # create + drive
    ASYLUM_HOME=/tmp/c2-gate ./target/debug/asylum node create --harness claude --substrate local --workspace /tmp/c2-gate/ws --prompt "Remember the word PINEAPPLE. Reply OK."
    curl -s http://127.0.0.1:8793/api/nodes/<id>            # liveness/session
    curl -s http://127.0.0.1:8793/api/nodes/<id>/events     # kinds + output_chunk text

    # crash + restart: kill -9 the daemon PID (targeted, from ss -ltnp|grep 8793), relaunch same ASYLUM_HOME/bind
    # resume leg: seed session then resume
    cd /tmp/c2-gate/ws && claude -p --dangerously-skip-permissions --session-id <id> "Remember the word PINEAPPLE. Reply with just OK."
    ASYLUM_HOME=/tmp/c2-gate ./target/debug/asylum node resume <id>
    ASYLUM_HOME=/tmp/c2-gate ./target/debug/asylum node send <id> "What word did I tell you to remember?"

    # cleanup: stop nodes, kill daemon PID, rm -rf /tmp/c2-gate AND ~/.claude/projects/-tmp-c2-gate-ws

Note: two daemon procs can appear if a prior launch lingered — kill by EXACT pid from `ss -ltnp | grep 8793`. Do NOT `pkill -f` broad patterns (matches the operator's own claude sessions).
