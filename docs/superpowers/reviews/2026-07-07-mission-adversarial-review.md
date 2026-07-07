# Final adversarial review — Asylum completion mission

- Date: 2026-07-07
- Reviewer branch/worktree: `phase-d-review` (= `main` @ `a7d6268`)
- Scope: cumulative mission diff `git diff b336d05..main` (harness-event ingestion + mapping, CLI bridge, launch injection incl. inline settings JSON, send_input/spawn hook actions, decision producer/feedback, ntfy publish/reply correlation, cockpit surfacing, Loon substrate rewrite, startup reconciliation, resume, launch packet, uptime/capacity).
- Method: hunk-by-hunk read of the high-risk Rust surface, fanned across subsystem reviews, every reported finding re-verified against source with line refs. Read-only on code; this file is the only write.

## Severity counts

- Critical: 1
- Major: 8
- Minor: 9
- Nit: 4

## Fix these three first

1. **C1 — Loon `watch_exit` reports a clean success on any abnormal stream end, and a transient network blip tears down a live microVM.** Directly violates the no-hardcoded-success cardinal rule *and* destroys running agents' work while telling the operator they finished cleanly.
2. **M3 — The per-node Loon guest token is a full-privilege owner credential: unscoped (guest A can drive/stop/delete node B and the whole fleet) and never revoked on teardown (stays live 30 days after the VM is gone).**
3. **The unconditional-liveness-write family (M1 + M2)** — `set_node_liveness` is a blind overwrite, so resume and late harness events can re-create the exact "Running-but-dead" / "stuck" dishonesty this phase exists to eliminate. Make liveness transitions compare-and-set from active states and leave terminal truth to the exit sink.

(Close fourth: **M8** — a correlated ntfy reply auto-approves a decision and injects its body verbatim into the agent PTY, gated only by a cleartext-in-push 20-bit token; ntfy topic write access is effectively fleet control and is under-documented.)

---

## CRITICAL

### C1 — `watch_exit` returns `success: true` on stream error / close-without-exit-code, and `exit_task` tears down the VM on any return
`crates/asylum-daemon/src/substrate/loon.rs:941-968` and `:638-646`

```rust
// watch_exit
941  while let Some(chunk) = stream.next().await {
942      let bytes = match chunk {
943          Ok(b) => b,
944          Err(_) => break,          // transient network error -> break
...
963  // Stream ended without a parseable exit code: treat as a clean end.
964  super::ExitOutcome { success: true, code: None }
```
```rust
// exit_task
639  let outcome = watch_exit(&http, &sse_url, &sse_key).await;
642  runtimes.write().await.remove(&vm_id_owned);
643  exit_sink(node_id, outcome);
644  let _ = teardown.teardown_vm(&vm_id_owned).await;
```

`watch_exit` is the authoritative exit signal (module doc, loon.rs:13), called once with no reconnect. Any mid-stream byte error (944) or a stream close before an `exit_code` frame falls through to `success: true`. In `capability_service.rs:457-479` a successful outcome records `NodeLiveness::Stopped` + `node.exited (reason: "exited")`; only `success:false` records `Failed`/`node.errored`/`abnormal_exit`. So an OOM-kill, host reboot, or dropped daemon->loon SSE is reported to the operator as a clean exit — hardcoded success on an error path. Worse, `exit_task` reacts to *any* `watch_exit` return by removing the runtime and calling `teardown_vm` (`vm stop`/`rm`/`prune`): a single ~200ms network hiccup on the SSE connection destroys a healthy VM, loses the guest workspace, and marks the node cleanly Stopped.

Fix direction: an unexpected stream end must be `success:false` (ideally a distinct lost-stream outcome), and teardown must not be triggered by a lost stream alone — reconnect the exec stream (stable `exec_id`) or confirm VM/exec death via the API before destroying the VM. Only a parsed `exit_code` event is authoritative.

---

## MAJOR

### M1 — `resume_node` re-introduces the eternal-Running lie via an exit-sink TOCTOU
`crates/asylum-daemon/src/capability_service.rs:2320-2334`

`resume_node` launches the harness while persisted liveness is still terminal, then flips to Running afterward:
```rust
2320  if let Err(launch_err) = self.local_substrate.launch(context).await { ... }
2329  self.store.set_node_liveness_with_reason(node.id, NodeLiveness::Running, "resumed", ...)?;
```
`launch()` returns as soon as the child is spawned. The exit sink only acts on live-ish states (`:355-360` matches `Running|Starting|WaitingForInput`), but the pre-launch liveness here is `Stopped` (guard at `:2250` only admits `Stopped|Exited|Failed`). If the resumed child dies in the window between 2320 and 2329 — e.g. a stale/invalid session id where `claude --resume`/`codex resume` errors and exits within milliseconds — the exit sink reads liveness `Stopped`, its guard fails, it no-ops; then 2329 writes `Running`. Net: node persisted Running, process dead, no runtime in the map, and **no future exit event will ever correct it** — self-heals only on the next daemon restart via `reconcile_on_boot`.

Fix direction: set liveness to `Starting` *before* `launch()` (mirroring the create path's insert at storage.rs:380, a state the exit-sink guard matches) and let the exit sink own the terminal decision. Same hardening applies to the create path, which overwrites with Running post-launch.

### M2 — `post_harness_event` can resurrect a node that exited concurrently (unconditional liveness write)
`crates/asylum-daemon/src/capability_service.rs:646-729`, `storage.rs:400-409`

`post_harness_event` snapshots the node at `:646`, checks `is_active_liveness` at `:664` against that stale snapshot, then at `:723-729` unconditionally calls `set_node_liveness(node_id, target)` when `snapshot.liveness != target`. `set_node_liveness` is a blind `UPDATE nodes SET liveness=?` (storage.rs:403-406) with no current-state guard. If the process exits between the read and the write, the exit sink sets a terminal state (Stopped/Failed) and posts `node.exited`, then the in-flight harness event overwrites it back to `Running`/`WaitingForInput`. A `Notification permission_prompt` racing an exit even produces a *pending decision on a dead node* (`:716-718`). The exit sink guards carefully (re-reads, only acts on active states); `post_harness_event` does not symmetrically re-check before writing.

Fix direction: make the liveness update a compare-and-set that only transitions *from* an active state (or re-read inside a transaction), so a concurrent terminal state wins.

### M3 — Loon guest token is a full owner credential: unscoped and never revoked
`crates/asylum-daemon/src/capability_service.rs:2421-2433` (mint), `auth` validation, all teardown paths

`mint_loon_node_token` issues `issue_owner_token(name, &["loon-node"], Some(30*24*3600))` with a candid comment "Tokens are all-or-nothing today (scope is inert)". Validation (`validate_owner_token`, `auth_middleware`) never inspects scope or binds to a node — any valid DB token is full owner. This token is shipped into every guest as `ASYLUM_TOKEN` (consumed at loon.rs:517). Consequences, both verified:
- **Cross-node authority:** from inside VM A the harness (or a prompt-injected agent, or anything reading its env) can call the daemon HTTP API to `send_input`/`stop`/`archive`/delete node B, spawn nodes, or revoke tokens. The review's exact hypothesis holds.
- **No revocation on end:** `stop_node` (`:2546`), `archive_node` (`:2565`), the `exit_task` teardown (loon.rs:638-646), and `force_teardown`/reconcile paths all destroy the VM but never revoke `loon-node-{node_id}`. A leaked guest token remains a live full-owner credential for 30 days after the VM is gone. The token name carries the node id, so revocation is trivial — nothing does it.

Fix direction: scope-enforce guest tokens to their own node (or a guest-callable subset) and revoke `loon-node-{node_id}` in every teardown/exit path.

### M4 — Attach WebSocket has no keepalive/half-open detection; `send_input`/`interrupt` report success into a dead socket
`crates/asylum-daemon/src/substrate/loon.rs:591-628`, `:674-701`

The attach WS read loop only terminates on explicit `Close`/`Err` (`:597`); there is no ping/keepalive and no read timeout, and the SSE exit watcher is a separate connection. On a TCP half-open (silent peer death, NAT idle drop) the WS stays "open" while the node still appears Running. Meanwhile `send_input` (`:674`), `send_input_raw` (`:686`), and `interrupt` (`:693`) only push into the `input_tx` mpsc channel and return `Ok(())` as long as the channel is open — which it is, because `write_task` (`:621`) hasn't yet observed the dead socket. Input is silently accepted and dropped; the caller is told the keystroke/CR/Ctrl-C was delivered when it never reached the guest. This is an error path pretending success on the control surface.

Fix direction: WS pings with a pong deadline (or read-idle timeout) so a half-open attach is detected and the runtime torn down/marked; consider surfacing send failures rather than treating channel-accept as delivery.

### M5 — Concurrent `send_input` interleaves bytes on the PTY; submits get garbled
`crates/asylum-daemon/src/substrate/local.rs:45-57`

```rust
async fn submit_over_writer(writer: &Arc<Mutex<Box<dyn Write + Send>>>, text: &str) -> Result<()> {
    { let mut w = writer.lock().await; w.write_all(text.as_bytes())?; w.flush()?; }   // 47-49
    tokio::time::sleep(SUBMIT_GAP).await;                                              // 51  (250ms, lock RELEASED)
    { let mut w = writer.lock().await; w.write_all(b"\r")?; w.flush()?; }              // 53-55
}
```
The writer mutex is dropped for the full 250ms `SUBMIT_GAP` between the body write and the submitting `\r`, with no per-node "submit in progress" guard. Two concurrent `send_input` calls on one node interleave as `bodyA, bodyB, CR, CR`: both bodies land in the composer, the first CR submits the concatenation as one message, the second CR is a stray Enter. This phase newly wires automated `send_input` producers that race with each other and with operator input: decision feedback (`capability_service.rs:3111`), hook `send_input` actions (`:3372`, `:3449`), auto-nudge (`:2782`), and the operator HTTP endpoint (`app.rs:424`) — nothing serializes them.

Fix direction: hold a per-node submit lock (`AsyncMutex<()>` keyed by node) across the whole body->gap->CR sequence; the writer mutex alone is insufficient because it is released during the gap.

### M6 — Decision dedup is check-then-insert with no atomicity; violates the explicit "one pending decision per node" guarantee
`crates/asylum-daemon/src/capability_service.rs:715-785`, `storage.rs:872-932`

`produce_decision_from_awaiting_input` does `pending_decision_for_node(node_id)` then `insert_decision(...)` as two non-atomic autocommit statements, and there is no DB uniqueness on `(node_id, status='pending')` (the only unique index is `events(node_id, sequence)`, storage.rs:235). Two concurrent `awaiting_input` posts for one node (e.g. a `permission_prompt` and an `elicitation` arriving close together, each handled on its own task against the connection pool) both read `None` and both insert -> two pending decisions. The code comment at `:715` ("Deduped to at most one pending decision per node") and the operating manual (`launch_packet.rs:80-82`, "deduplicated: one pending decision per node") both assert a guarantee the implementation does not enforce — an honesty-adjacent overclaim.

Fix direction: add a partial unique index on `decisions(node_id) WHERE status='pending'` and treat the insert conflict as the refresh path, or serialize decision production per node.

### M7 — `ingest_statusline` loads and scans *all* prior harness-event bodies on every statusline post (hot path, unbounded growth)
`crates/asylum-daemon/src/capability_service.rs:807-850`, `storage.rs:486-501`

Each statusline post first records a `node.telemetry` HarnessEvent (`:814`), then calls `harness_event_bodies(node_id)` (`:817`) which `SELECT`s and JSON-parses **every** HarnessEvent row for the node to dedup ctx_pressure thresholds. Claude runs the statusline command after every assistant render, and every telemetry datapoint is itself persisted as a HarnessEvent, so the scanned set grows with every post: O(n) work per statusline, O(n^2) over a session, with unbounded event-row accumulation. On a long-running node this becomes a real latency/storage problem on one of the most frequent hot paths.

Fix direction: dedup ctx_pressure via a bounded query (e.g. a `MAX(threshold)` fired-per-session lookup or a small per-node in-memory set), not a load-all-and-scan; and/or stop persisting every telemetry tick as a full event row.

### M8 — A correlated ntfy reply auto-approves the decision and injects its body verbatim into the agent PTY; the correlation token is not authentication
`crates/asylum-daemon/src/capability_service.rs:3362-3379`, `:1159`, `channels/ntfy_inbound.rs:169-171`

When an inbound ntfy message carries a valid correlation token, the daemon resolves the pending decision as `{status:"approved", answer: Some(request.body)}`, and `resolve_decision` injects that answer verbatim via `send_input` (types AND submits into the agent); with no pending decision it still `send_input`s the raw body. The gating token is transmitted **in cleartext inside the pushed notification body** (`append_reply_marker`). ntfy topics are the only trust boundary: any party that can publish to the topic can read the escalation, extract the exact token, and reply — driving arbitrary input into any worker and auto-approving its decision. The token is correlation, not a secret. The operating manual (`launch_packet.rs:57-59`) frames this only as "routes back to that exact node," understating that topic write-access equals fleet control.

(Note: `ntfy_inbound.rs` itself predates the baseline, but the reply->`resolve_decision`->`send_input` wiring, the marker emission, and the hardcoded-approved resolution are all in the mission's `capability_service.rs` changes.)

Fix direction: gate decision resolution behind the owner token even on the reply path, or at minimum document explicitly that ntfy topic write access is full fleet control.

---

## MINOR

### m1 — `idle_prompt` forces `liveness = Running`, silently clearing `WaitingForInput`
`crates/asylum-daemon/src/capability_service.rs:240-244`, `:723-729`

Claude's `idle_prompt` Notification maps to `node.idle` with `liveness = Some(Running)`, applied unconditionally at `:723-729`. `idle_prompt` fires precisely when Claude has been sitting waiting for the user, so if it arrives while the node is `WaitingForInput` (a `permission_prompt`/`elicitation` already produced a pending decision at `:716-718`), the node flips back to Running while genuinely blocked with an open decision. Cockpit then shows a healthy Running node that is actually stuck. (Same unconditional-write root cause as M2.) Fix: don't downgrade `WaitingForInput -> Running` on idle; omit the liveness change or apply it only from `Running`/`Starting`.

### m2 — Resume rebuilds argv from the adapter baseline, dropping per-node `launch_args`
`crates/asylum-daemon/src/harness/claude.rs:165`, `harness/codex.rs:139`

Create appends `request.launch_args` (`capability_service.rs:1991`); resume builds only from the adapter baseline `self.launch_args`. `request.launch_args` is never persisted (`NodeRecord`/`nodes` table have no column), so per-node launch flags supplied at create time are lost on resume — the resumed process runs a different argv than the original (the "model flags on resume" concern: a model flag passed via `launch_args` would silently revert to default). Session id is still correct, so it does not resume the wrong session. Fix: persist `launch_args` on the node row and thread through `resume_args`, or document resume as baseline-only.

### m3 — Reconciliation assumes local PTYs always died with the daemon
`crates/asylum-daemon/src/capability_service.rs:2107-2116`

"All local PTYs died with the daemon" is asserted, not enforced — it relies on children receiving SIGHUP when the PTY master fd closes. A harness that ignores SIGHUP (or re-parented to init) survives yet is marked `Stopped`; if it recorded a session id + workspace, `local_node_resumable` returns true and a later `resume_node` spawns a *competing* `--resume` against a session a live orphan still holds (`has_runtime` at `:2272` only checks the in-memory map, empty after restart, so it cannot detect the orphan). Mirror of the "marked dead while alive" hunt item. Fix: record child PIDs and probe liveness, or kill the process group on daemon shutdown.

### m4 — `set_node_liveness_with_reason` is two non-atomic autocommit writes
`crates/asylum-daemon/src/storage.rs:419-448`

The `UPDATE nodes ...` (`:431`) and the `record_event_with_conn(LivenessChanged)` (`:442`) are separate autocommit statements with no enclosing transaction. A crash between them leaves liveness changed but the auditable event missing — undermining the function's stated purpose ("the honest transition is auditable"). Audit-completeness gap, not corruption (per-statement SQLite writes stay atomic/durable). Fix: wrap both in one transaction.

### m5 — Correlation token is ~20 bits, and the PK is `INSERT OR REPLACE`
`crates/asylum-daemon/src/capability_service.rs:65`, `:1260-1267`, `storage.rs:199`, `:1252`

`CHANNEL_REPLY_TOKEN_LENGTH = 5` chars taken from a UUID-v4 hex string -> keyspace 16^5 ~= 1.05M (~20 bits), not the 62^5 the parser's `is_ascii_alphanumeric` check implies. Brute-force within the 30-min TTL is feasible with no daemon-side rate limiting (relevant in the write-without-read ntfy ACL case). The correlations table is `token TEXT PRIMARY KEY` written `INSERT OR REPLACE`, so a birthday collision among concurrently live tokens silently overwrites the earlier correlation — a reply meant for node A resolves node B. Fix: use a full-length (>=128-bit) token.

### m6 — Reply path hardcodes `status:"approved"`, so a human denial is recorded as an approval
`crates/asylum-daemon/src/capability_service.rs:3366`

Every phone reply resolves the decision as `"approved"` regardless of content. The verbatim `answer` reaches the worker correctly, but the decision audit record (surfaced by `decision.list` and the "Decision resolved" notification) always says `approved` even when the human replied "no, stop." No phone reply can produce a `denied` record — a dishonest audit trail. Fix: derive status from the reply, or record a neutral `answered` status.

### m7 — Two advertised-but-dead `LoonConfig` fields
`crates/asylum-types/src/config.rs:100` (`api_key_file`), `:102` (`cert_fingerprint_file`)

Serialized config knobs a user can set, with **zero readers anywhere** (verified: no matches outside the struct + its `Default`). This diff rewrote `LoonConfig`, adding `config_path`/`profile` (the loon client auth path) that supersede them, but left these two orphaned — dead knobs still advertised. Cardinal-rule adjacent. Fix: delete or wire them.

### m8 — Loon capabilities advertised regardless of reachability
`crates/asylum-daemon/src/substrate/loon.rs:77-87`, `:296`

`browser_attach` and `native_attach` are hardcoded `true` even when `health.status != "ok"` (host unreachable); only `send_input`/`interrupt`/`stop` gate on `reachable`. `harness_profiles` is hardcoded `["claude_code","codex"]` whenever `/instances` returns 200, asserting both harnesses exist on the image without verifying. Advertised-but-unverified surfaces. Fix: gate attach flags on `reachable`; derive profiles from the actual image/profile.

### m9 — `exec_signal`/`exec_resize` ignore non-2xx responses
`crates/asylum-daemon/src/substrate/loon.rs:798-822`

Both do `...send().await.context(...)?` and return `Ok(())` without checking `resp.status().is_success()`; a 4xx/5xx from the loon daemon is treated as success. `exec_resize` is best-effort (harmless); `exec_signal` is the SIGTERM in `graceful_and_teardown` whose result is discarded and followed by `teardown_vm`, so impact is low — but the pattern hides real API failures. Fix: check `status().is_success()`, consistent with `exec_pty`.

---

## NIT

### n1 — Operating manual claims "13 events" but omits the real 14th (`node.resumed`)
`crates/asylum-daemon/src/launch_packet.rs:34-40` vs `hooks/mod.rs:97`. `event_catalog()` legitimately fires `node.resumed` (producer at `capability_service.rs:2336`); the agent-facing manual says "one of the 13 events" and lists 13, omitting it. Under-advertises a live hookable event (reverse of the cardinal rule, hence nit). The manual self-test (`launch_packet.rs:214-230`) only checks those 13 are present, not that the list equals `event_catalog()`, so drift is unguarded.

### n2 — Statusline blocks up to 2s before rendering
`crates/asylum-cli/src/harness_event.rs:224-242`. `run_claude_statusline` computes the line, then `await`s `dispatch(...)` (bounded by `REQUEST_TIMEOUT = 2s`) *before* `println!("{line}")`. Claude runs statusline after every assistant message, so a slow/unreachable daemon stalls the visible status bar up to 2s per render. The line is independent of the POST result; print it before dispatching.

### n3 — Empty `ASYLUM_SOCKET_PATH` is treated as a valid socket
`crates/asylum-cli/src/harness_event.rs:97-99`. `env::var` returns `Ok("")` for a set-but-empty var, yielding `Socket(PathBuf::from(""))` and failing to connect rather than falling through to HTTP/default-socket. Minor since the daemon always sets a real path; harden with `.filter(|s| !s.is_empty())` on the env reads.

### n4 — `toml_key` does not escape newlines (loon)
`crates/asylum-daemon/src/substrate/loon.rs:1220-1222`. Not a realistic vector (a workspace path with an embedded newline), noted for completeness.

---

## Checked and cleared (no finding)

- **Inline settings-JSON / hook / statusline injection is safe.** `claude_settings_json` (claude.rs:294-325) builds JSON via `serde_json::json!`; the only shell-interpolated value is the asylum binary path, wrapped by a correct POSIX single-quote `shell_quote` (claude.rs:284-286). The workspace path is never placed in the settings JSON or any hook/statusline command. MCP config is a discrete argv element, not shelled.
- **No dead catalog event.** Every `event_catalog()` entry has a real producer; the injected settings wire `Stop`/`Notification`/`SessionStart`/`SessionEnd`/`PostToolUse`/`statusLine`, so `node.session_end`/`node.tool_call`/etc. are all emittable. Unknown hook types fall through `_ => {}` and are honestly accepted with `event:None`, not misclassified.
- **Terminal-node ingestion is rejected** (`post_harness_event` gates on `is_active_liveness`, returns `accepted:false`).
- **Resume argv is non-contradictory:** `--resume <id>` and `--session-id <id>` never coexist (`resume_args` is used exclusively); `--dangerously-skip-permissions` leads and is not dropped on resume. Codex mirrors this.
- **Loon resume is honest, not advertised-dead:** create overrides `capabilities.resume=false` for Loon and `resume_node` returns an honest error for the Loon substrate.
- **No double-idle:** the quiescence sweep skips harnesses with `native_idle_signal()` (claude), and loon nodes stream PTY output through the same transcript sink, so quiescence does not false-fire on loon.
- **exec argv passthrough is safe:** `sh -lc 'exec "$@"' ...` passes argv verbatim; interpolated paths go through `shell_single_quote` or JSON.
- **Boot reconciliation is race-free:** `reconcile_on_boot` is awaited to completion before background tasks start and before any listener binds; no local runtimes exist at boot.
- **Partial-write durability concern does not apply:** individual SQLite writes remain atomic/durable under the default journal + `synchronous=FULL`.
- **Honesty improvements shipped:** `hooks/mod.rs` deletes three dead catalog entries (`node.permission_requested`, `substrate.unreachable`, `schedule.cron`); `cli.rs` removes the dead `recipe` surface and candidly documents token `--scope` as advisory-only; the ntfy POST-to-root fix is correct and regression-tested.


---

## Resolution (2026-07-07)

Fixes landed on branch `phase-d-fixes` across two commits:
`ba81d86` (predecessor: C1, M1-M7, minors, nits) and `6a4c540`
(this pass: Addenda A/B/C, M3 scope test, M8 doc, m2). Full
`cargo test-asylum` green (cli 69, daemon 178, types 4, integration 7,
cockpit 106); zero new build warnings.

| Finding | Status | Commit | How |
|---|---|---|---|
| C1 loon watch_exit success-on-error + teardown-on-stream-loss | FIXED | ba81d86 | `ExitOutcome.stream_lost`; loon maps lost stream -> `success:false`/`node.errored`(stream_lost) and keeps the VM; only a parsed exit_code tears down. |
| M1 resume/create eternal-Running TOCTOU | FIXED | ba81d86 | `transition_node_liveness` CAS; move to `Starting` before launch, exit sink owns terminal truth. |
| M2 post_harness_event resurrection | FIXED | ba81d86 | liveness write is CAS from active states; terminal-node decision guard. |
| M3 guest token unscoped + never revoked | FIXED | ba81d86 + 6a4c540 | revoke `loon-node-{id}` on stop/archive/exit/reconcile; `scoped_token_authorizes_path` narrows per-node token to its own `/api/nodes/{id}` path and blocks `/api/tokens`. Unit test added (6a4c540). |
| M4 attach WS half-open silent drop | FIXED (structural; live check pending) | ba81d86 | WS keepalive ping + surface death as `node.errored`, keep VM. |
| M5 concurrent send_input interleave | FIXED | ba81d86 | per-node submit mutex across body->gap->CR (local + loon). |
| M6 decision dedup non-atomic | FIXED | ba81d86 | partial unique index `decisions_one_pending_per_node` + atomic upsert; DB-level test. |
| M7 statusline O(n) body scan | FIXED | ba81d86 | ctx_pressure fired-state on node row; no per-post load-all. |
| M8 ntfy topic = fleet control (doc) | FIXED (doc) | 6a4c540 | security note in README ntfy section + `NtfyConfig` doc-comment: topic write-access is fleet control; use a private/unguessable topic + publish ACL; 32-hex correlation token is anti-collision, not a secret. |
| m1 idle downgrades WaitingForInput | FIXED | ba81d86 | idle liveness applied only via allowed-from guard. |
| m2 resume drops per-node launch_args | FIXED | 6a4c540 | persist `launch_args` on node row at create; resume reappends them (argv matches create minus session-id->resume swap). Round-trip test. |
| m3 reconcile assumes local PTYs died | SKIPPED | - | minor; skip-unless-trivial per mission brief (needs PID tracking / process-group kill; not trivial). |
| m4 set_node_liveness_with_reason non-atomic pair | SKIPPED | - | minor audit-completeness gap (per-statement writes stay durable); skip-unless-trivial. |
| m5 correlation token ~20 bits | FIXED | ba81d86 | `CHANNEL_REPLY_TOKEN_LENGTH` raised to 32 hex chars. |
| m6 reply hardcodes approved | FIXED | ba81d86 | reply path records honest `answered` status with the verbatim answer. |
| m7 dead LoonConfig fields | FIXED | ba81d86 | `api_key_file` / `cert_fingerprint_file` deleted. |
| m8 loon caps advertised regardless of reachability | SKIPPED | - | minor; skip-unless-trivial per mission brief. |
| m9 exec_signal/exec_resize ignore non-2xx | FIXED | ba81d86 | status `.is_success()` checked. |
| n1 manual "13 events" omits node.resumed | FIXED | ba81d86 | manual lists 14; drift guard test vs `event_catalog()`. |
| n2 statusline blocks before render | FIXED | ba81d86 | print line before dispatch. |
| n3 empty ASYLUM_SOCKET_PATH treated valid | FIXED | ba81d86 | empty env filtered out. |
| n4 toml_key does not escape newlines | FIXED | ba81d86 | newline escaping + test. |
| Addendum A honest resumable (fs probe) | FIXED | 6a4c540 | reconcile + resume gate on on-disk transcript existence (claude `<cwd-slug>/<session>.jsonl`, codex `rollout-*-<thread-id>`); resume fails fast when absent. Slug + probe tests with fake HOME. |
| Addendum B graceful claude stop | FIXED | 6a4c540 | claude stop submits `/exit` over PTY, bounded 5s wait for clean exit (transcript flush), then SIGKILL fallback; codex documented as staying on kill path. PTY sequencing test. |
| Addendum C send_input event coverage | RESOLVED (no change) | 6a4c540 | programmatic `send_input` already records `InputSent`; the only gap is the raw interactive-attach path, deliberately event-less (per-keystroke events would be spam) - documented in `route_attach_input`. |

### Deferred to a frugal live check (not unit-testable here)
- **C1 stream-loss honesty**: with a real loon VM, drop the daemon->loon SSE mid-session and confirm the node goes `node.errored`(stream_lost) and the VM survives (no teardown).
- **M4 half-open attach**: silently sever the attach WS TCP and confirm the keepalive ping detects it and surfaces death rather than accepting dropped input as delivered.
- **Addendum B graceful-stop -> resume round trip**: stop a live claude node, confirm `/exit` flushed the transcript, then resume and confirm the session restores from that transcript.
