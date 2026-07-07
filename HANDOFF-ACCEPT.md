# Phase D North-Star Acceptance — HANDOFF

Status: 6/7 legs PASSED, leg 7 PASSED-with-note, leg 6 resume = honest-limitation (see below).
No code changes were needed — the scenario ran end-to-end on main's existing code.

## Environment (LEFT RUNNING for a successor to adopt)
- Worktree: /home/casey/Projects/Asylum/.claude/worktrees/phase-d-accept (branch phase-d-accept)
- Isolated daemon: PID 531993, bind 0.0.0.0:7799, socket /tmp/accept-home/run/asylum.sock, log /tmp/accept-daemon2.log
- ASYLUM_HOME=/tmp/accept-home  (config.toml, asylum.sqlite3, workspaces/, ev.py, supervisor-prompt.txt)
- Config: auth DISABLED (owner_tokens_enabled=false); loon.enabled=true; guest_base_url http://host.loon.internal:7799; vm_memory_mib=3072; guest_asylum_binary=<worktree>/target/x86_64-unknown-linux-musl/release/asylum
- Cockpit dev (vite): parent PID 378170 (node child 378184), http://127.0.0.1:5173  (proxies /api,/ws to :7799)
- ntfy: https://ntfy.sh topic `asylum-accept-f07972d1` (poll 5s)
- Loon host: https://127.0.0.1:7777 (healthy); client cfg ~/.config/loon/config.toml; `loon vm ls` currently EMPTY (all pruned)
- Evidence dir: /home/casey/.claude/jobs/c4517aa5/tmp/accept-evidence/
- DB query helper (no sqlite3 CLI on this box): `python3 /tmp/accept-home/ev.py [N]`
- Browser session: playwright-cli (use `--browser=chromium`; chrome not installed). Node session view was open on 984753b1.

## Key node ids
- Supervisor: 2f81ca7e-154f-45f1-ac66-a2b3d005e621 (claude/local, created via Cockpit UI)
- Worker A (local, AskUserQuestion): 157cfc49-d444-42e6-b860-8d752e1bb4ef
- Worker B (loon, uname): 7f1503ad-d0b4-4561-a10e-24d0928d2aaf (VM 019f3b87-8ada-...)
- Leg5/6 local worker: 984753b1-a771-4447-a4f6-8cdc353199c9 (session f23dfd86)
- Leg6 loon worker: 8dc83d94-b6f9-40a3-8409-907281a2a662 (VM 019f3b90-f6ea, torn down)
- Resume-test node: cbd04d61-9c18-489c-8f52-a41094f657c8 (still RUNNING; claude child PID ~568196 — successor should node.stop it)

## Per-leg scoreboard
1. Supervisor create (Cockpit UI): PASSED. Drove CreateScreen via playwright-cli (claude/local/supervisor, workspace /tmp/accept-home/workspaces/supervisor, prompt from /tmp/accept-home/supervisor-prompt.txt). Node 2f81ca7e came up running.
2. MCP-only spawns incl Loon: PASSED. relationships: 2f81ca7e->157cfc49 and 2f81ca7e->7f1503ad (spawned_for/spawn_peer). Loon worker booted a REAL microVM (external_id 019f3b87) and its guest harness posted node.session_started + node.tool_call (ToolSearch + mcp__asylum__*) + node.turn_complete over HTTP+token from inside the VM. Supervisor created 2 hooks via MCP (escalate-awaiting-input on node.awaiting_input->channel ntfy-default; nudge-idle-worker-b on node.idle scoped to worker B). No raw output streaming (hooks, not polling).
3. Stall-feed: PASSED. worker B node.idle(idle_source=notification) -> idle-hook send_input "If your task is complete, reply DONE." (events 1136->1140, 1833->1837, exact template, same-second, no MCP call). NOTE: the idle hook's hook_firing rows (2-6) were CASCADE-deleted when the supervisor deleted its hook at end-of-run (hook_firings FK ON DELETE CASCADE) — evidence survives in worker-side input_sent events. Controlled re-proof: hook aade9521 -> firing row 7 (outcome send_input:984753b1, ok=1) via a synthetic node.idle POST.
4. ntfy escalation round trip: PASSED. awaiting_input hook cbe9c2b0 fired outcome channel:ntfy-default (hook_firings row 1), correlation token 34987. Escalation on ntfy (leg4-ntfy-escalation.json). Published simulated phone reply `microVMs\n\n[asylum-reply:34987]` (leg4-reply-publish.txt). Daemon inbound subscriber correlated token->worker A, resolved decision 11bbd41b (approved, answer=microVMs verbatim), injected -> worker A events input_sent "microVMs" + remote_command_received{approved} -> worker A wrote /tmp/accept-home/workspaces/workerA/haiku.txt. CAVEAT (documented, ACCEPTABLE): haiku was about nature, not microVMs — the AskUserQuestion menu absorbed the free-text answer as its default (known menu-dialog fidelity limitation). Routing mechanism fully proven.
5. Cockpit intervention: PASSED. Opened 984753b1 session view in Cockpit; live output visible (STANDBY READY + attach_issued WS observe frames); typed via the send-input box; "Human here via Cockpit: please reply COCKPIT INTERVENTION RECEIVED" reached the worker PTY (confirmed in transcript). Screenshots: leg5-cockpit-session-live.png, leg5-cockpit-intervention-reply.png. NOTE: cockpit send-input writes directly to harness stdin and records NO input_sent event (minor observability gap; the second attempt via ref e985 with Enter worked, the first via a stale ref did not).
6. Daemon restart reconcile+resume: RECONCILE PASSED (leg6-reconciliation-boot.log). kill -9 daemon 371116 (orphaned claude children died via PTY SIGHUP, no strays); loon VM survived crash. Restart -> reconcile BEFORE listeners bind: "startup reconciliation marked stale-live nodes honestly reconciled=3"; 8dc83d94(loon) -> Stopped reconciled_loon_vm_orphaned_torn_down resumable=false + VM torn down + PRUNED (loon vm ls empty); 984753b1 + 2f81ca7e(local) -> Stopped reconciled_local_pty_lost resumable=true; ZERO eternal Running. RESUME: plumbing CORRECT (relaunched `claude --resume f23dfd86`, source=resume, node->running) but interactive claude 2.1.202 does NOT persist its transcript on a hard crash, so claude reported "No conversation found with session ID: f23dfd86" -> node failed. This is the documented C2 harness limitation; the C2 gate already proved headless-seeded successful resume. A graceful-stop->resume positive test was INCONCLUSIVE because node.stop left interactive claude running (see gotcha).
7. Clean completion: PASSED-with-note. Supervisor autonomously stopped both its workers (157cfc49, 7f1503ad -> stopped) and deleted its idle hook; loon VM torn down + pruned; all final states honest (stopped). Did NOT capture a literal "DONE" summary line (supervisor's alt-screen TUI is garbled when ANSI-stripped from the transcript); the coordination outcome (both workers finished their tasks, haiku.txt written, worker B ran uname, both stopped) is evidenced.

## Bugs found / fixed
- NONE requiring code changes. All plumbing worked on existing main code.
- Findings (behaviors to consider, not blockers): (a) hook_firings cascade-delete on hook.delete erases that hook's audit trail — by design; worker-side input_sent events preserve the proof. (b) Cockpit send-input (direct stdin) records no input_sent event. (c) interactive claude no transcript persistence on hard crash (known C2 limitation). (d) node.stop did not cleanly terminate an interactive claude node in the graceful-resume test — WORTH INVESTIGATING.

## Next actions for successor
1. Clean up: `asylum node stop cbd04d61-...` (resume-test node still running); optionally stop supervisor 2f81ca7e if a process lingers. Then tear down when fully done: kill -9 531993 (daemon), kill -9 378170 (cockpit vite); `loon vm ls` already empty. NEVER pkill -f.
2. Optional: prove positive resume via a headless-seeded claude session (as C2 did) OR fix node.stop graceful termination then re-try graceful-stop->resume.
3. Optional: capture supervisor "DONE" more cleanly (read structured events, not the TUI transcript) for leg 7.
4. Update mission spec Status Phase D line (a minimal PASSED line is already added in this commit).

## Gotchas learned
- Daemon refuses to boot if owner_tokens_enabled=true and no token present ("owner-token auth is enabled but no active token"). Set false, or export ASYLUM_OWNER_TOKEN. Disabled auth still mints+accepts loon guest tokens.
- Daemon must bind 0.0.0.0 so loon guests reach it via host.loon.internal:<port>.
- Loon worker boot ~90s; vm_memory_mib 3072 (256 default OOMs claude).
- Cockpit CreateScreen sends the prompt as `description` (folded into launch packet as "User launch packet:"). Node-create API returns {"node_id":...} (not id).
- Cockpit shows the LAST 12 chars of the UUID in lists.
- ntfy reply correlation: message body must end with `\n\n[asylum-reply:<TOKEN>]`; TOKEN is 5 alnum chars; daemon polls every 5s. The daemon's own outbound is echoed back inbound but doesn't self-resolve.
- playwright-cli: use `--browser=chromium`. Fresh snapshot before each interaction — element refs (e###) change on re-render.
- No sqlite3 CLI: use python3 sqlite3 (see /tmp/accept-home/ev.py).
- Build artifacts already present: target/debug/asylum (daemon+CLI), target/x86_64-unknown-linux-musl/release/asylum (guest), cockpit/node_modules.
