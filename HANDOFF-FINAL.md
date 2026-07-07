# Phase D FINAL LIVE CHECK — HANDOFF

Branch phase-d-final (= main 1c93ab4). Re-validated the 3 behaviors that changed after the
acceptance run (adversarial-review fixes). One real claude session consumed total.

## Per-check status
1. **Graceful-stop -> resume round trip (LOCAL claude): PASSED.** Closes the one soft leg of the
   north-star acceptance (the positive interactive-claude resume leg 6 could not show pre-fix).
   Created claude node (remember MANGO -> "OK", turn_complete) -> `asylum node stop` exited cleanly
   via `/exit` in 0.77s (`node.exited` exit_code 0 reason "exited"; NO SIGKILL/abnormal_exit; no kill
   lines in daemon log) -> transcript FLUSHED on clean quit (37703 -> 39282 bytes) -> `asylum node
   resume` (gated on the on-disk transcript probe) restored it (`session_end` reason
   "prompt_input_exit", `session_started` source "resume", same session id 87e5bfcc) -> recall
   answered "MANGO" -> stopped again clean (exit_code 0). Evidence:
   jobs/c4517aa5/tmp/final-check-evidence/check1-graceful-stop-resume.txt (+ check1-full-events.txt).

2. **C1 loon stream-loss honesty: BLOCKED — not live-validated.** Could not establish a live loon
   exec to sever. See root blocker below. C1 fix remains unit-test covered (loon.rs exit-outcome
   mapping: ExitWatch::StreamLost -> {success:false, code:None, stream_lost:true}; ~loon.rs:1363-1386).
   Incidentally confirmed the AUTHORITATIVE counterpart live: raced `sudo systemctl restart
   loon-daemon` to sever the SSE while the exec was live; the exec's live window is sub-second, so the
   daemon received the authoritative exit_code first and correctly took Exited(126) -> node.errored
   abnormal_exit + VM teardown (a real parsed exit_code DOES tear the VM down, which is exactly what
   the C1 fix distinguishes from stream_lost). Evidence: check2-3-loon-stream-loss-BLOCKED.txt,
   check2-c1-sever-race.log.

3. **M4 half-open attach: NOT RUN (blocked by the same root blocker).** Per mission guidance I did
   NOT build a network rig for the true silent-TCP-half-open case; the keepalive -> error-propagation
   path is unit-test covered. Note: M4's keepalive ping (ATTACH_WS_PING_INTERVAL=20s, first tick
   skipped) fires ~20s in, so it is NOT implicated in the <1s harness death.

## Root blocker for checks 2 & 3 (fully diagnosed; NOT a phase-d defect)
The loon guest image /var/lib/loon/agent-images/claude-dev.oci.tar was REBUILT 2026-07-06/07 04:04 and
ships claude 2.1.202 + codex 0.142.5. Under asylum's loon PTY exec, BOTH harness processes exit **126**
(authoritative abnormal_exit) within <1s with ZERO output, before node.session_started; loon then tears
the VM down. Observed on 3 nodes (claude x2, codex x1). loon events for a failed VM: every provisioning
exec exits 0 (mkdir, creds, 14-part binary staging, assemble); only the FINAL harness PTY exec is 126.

Hypotheses ruled OUT:
- Guest asylum MCP binary: staged the musl binary into a guest by hand (900KB `loon cp` chunks) and ran
  it — `asylum --version` -> 0.1.10, `asylum mcp` starts. Executes fine in-guest. NOT the cause.
- Provisioning / creds / workspace / cwd: all provisioning execs exit 0; /work is created (loon.rs:376).
- M4 keepalive ping: first ping at 20s, long after the <1s death. NOT the trigger. (The "attach ws
  half-open" warning is a CONSEQUENCE of the exec dying and loon closing the WS.)
- Phase-D fix code: does not touch the loon launch/exec path.
- My claude_command override (initially set to a host path 2.1.145 for check 1) — that caused a DIFFERENT
  earlier failure (exit 127, host path absent in guest); reverted to default "claude" before the 126 runs.

Hypothesis ruled IN (leading, environmental): the harness VERSIONS in the freshly-rebuilt guest image
fail under interactive PTY launch. Corroborated independently on the HOST: claude 2.1.202 also aborts on
launch under the daemon's local portable_pty with "fatal runtime error: assertion failed:
output.write(&bytes).is_ok(), aborting" -> abnormal_exit (CHECK 0). The exact same argv run under a plain
python pty does NOT crash 2.1.202, so it is a portable_pty/loon-pty vs 2.1.202 interaction. claude 2.1.202
was installed on the host 2026-07-06 20:44, AFTER the acceptance run (which used 2.1.145).

FOLLOW-UP (post-mission): (a) claude 2.1.202 interactive PTY-launch regression on host AND guest — pin a
working claude version or fix the reader/pty setup; (b) once a launchable guest harness exists, live re-run
C1 stream-loss (expect node.errored reason "stream_lost", VM kept) and M4 half-open.

## Environment (daemon LEFT RUNNING for a successor)
- Worktree: /home/casey/Projects/Asylum/.claude/worktrees/phase-d-final (branch phase-d-final)
- Isolated daemon: PID 1202648, bind 0.0.0.0:7798, ASYLUM_HOME=/tmp/final-home,
  socket /tmp/final-home/run/asylum.sock, log /tmp/final-home/daemon.log. Owner-token auth DISABLED.
- Config /tmp/final-home/config.toml: loon.enabled=true, guest_base_url http://host.loon.internal:7798,
  vm_memory_mib=3072, guest_asylum_binary=<worktree>/target/x86_64-unknown-linux-musl/release/asylum,
  image=claude-dev.oci.tar. harness.claude_command = DEFAULT "claude" (2.1.202 = BROKEN for local
  launch). To reproduce check 1, set claude_command = /home/casey/.local/share/claude/versions/2.1.145
  and restart the daemon.
- Binaries built: target/debug/asylum (daemon+CLI), target/x86_64-unknown-linux-musl/release/asylum (guest).
- DB helper (no sqlite3 CLI): `python3 /tmp/final-home/ev.py [N] [node_id]`.
- Loon host https://127.0.0.1:7777 (v0.1.5, healthy). `loon vm ls` EMPTY, tombstones pruned. Restarted a
  few times during checks (permitted). No asylum VMs remain.
- Node ids: check1 (local claude, stopped) a4b63716-fc3c-4a04-a961-d7c57078dda3 session 87e5bfcc.
  Failed loon nodes: d30d1e41 (claude 127, bad path), cdf5fbd6 (claude 126), ffb7bcf1 (codex 126),
  1e83b765 (claude 126, sever-race).
- NOT mine, from a prior worktree: a stale vite (PID 378184, phase-d-accept). Left alone.
- Evidence dir: /home/casey/.claude/jobs/c4517aa5/tmp/final-check-evidence/

## Sessions consumed
1 real claude session (check 1 create+resume = one conversation). Loon harness launches consumed 0
(died at 126 before any turn).

## Uncommitted diffs
Docs only: mission spec Status (Phase D final-check line), phase-c plan (graceful-stop ADDENDUM), and
this HANDOFF-FINAL.md. No source changes. The 126 is external (guest image), so no code fix was made.
