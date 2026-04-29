# Local Ultrareview — 2026-04-29

Six specialist reviewers ran in parallel against the full main branch (commit
`a489c5f`), each scoped to one subsystem and instructed to verify findings by
re-reading cited code before reporting. 54 unique findings remain after dedup.

| Severity | Count |
|---|---|
| **High** | 9 |
| **Med**  | 20 |
| **Low**  | 25 |

Findings reference `file:line_start-line_end`. "Evidence" is the snippet/line
the reviewer cited; "Suggested fix" is the supervisor's read.

---

## HIGH

### H1. Cached owner-token hashes never re-check expiry/revocation
- **File:** `crates/asylum-daemon/src/capability_service.rs:1472-1489`
- **Category:** security
- **Finding:** `validate_owner_token_value` short-circuits on the in-memory
  `expected_hashes` set (built once in `app.rs serve()` from
  `store.list_active_tokens()` plus the configured owner_token, then moved into
  `AuthMode::OwnerToken` and never refreshed). Tokens later marked
  `revoked = 1` in the DB still authenticate until restart, and the
  `expires_at` filter in `find_token_by_hash` (storage.rs:557-583) is never
  consulted for tokens loaded into the snapshot. `DELETE /api/tokens/{id}`
  appears to succeed but has no effect.
- **Evidence:** `app.rs:68-91`, `capability_service.rs:1472-1489`,
  `storage.rs:525-583`. Confirmed independently by both security and
  architecture reviewers.
- **Suggested fix:** drop the in-memory cache and always go through
  `find_token_by_hash` (which already checks expiry+revoked), OR keep the
  cache but consult the DB row's `expires_at`/`revoked` after an in-memory
  hit, OR rebuild the snapshot on every revoke/issue. The DB-only path is
  simplest and safe given the write rate.

### H2. Hook dispatch task dies silently on broadcast lag
- **File:** `crates/asylum-daemon/src/capability_service.rs:113-121` (consumer
  loop) + `hooks/mod.rs:26` (channel)
- **Category:** concurrency
- **Finding:** the consumer is `while let Ok(event) = rx.recv().await { ... }`.
  `tokio::sync::broadcast::Receiver::recv` returns `Err(Lagged)` when the
  consumer falls behind; the while-let treats it as terminal. Once the 256-slot
  channel saturates while a slow action runs, the task exits and ALL future
  hook processing (channel sends, spawn actions, schedule.5m/30m firings)
  silently stops until daemon restart.
- **Suggested fix:** explicit `loop { match rx.recv().await { ... } }` — log
  on `Lagged(n)` and `continue`; only `break` on `Closed`.

### H3. CLI commands ignore configured listen / `ASYLUM_BIND`
- **File:** `crates/asylum/src/cli.rs:88-154` (Node, Graph, Attach, Token,
  Notify, Mcp arms)
- **Category:** correctness
- **Finding:** `let client = AsylumClient::from_env();` at line 37 only
  consults `ASYLUM_BASE_URL` (default `http://127.0.0.1:7717`). Setup, Cockpit,
  Start, Restart, Status, Doctor, Update each rebind `client` via
  `runtime_client(&paths)` (which honors config `listen` / `ASYLUM_BIND`).
  The data-plane arms do not, so they target `127.0.0.1:7717` regardless of
  config.
- **Suggested fix:** rebind `client = runtime_client(&paths)` in each data
  arm, or hoist the rebind to once at the top of dispatch.

### H4. `CapabilitySnapshot` has no `#[serde(default)]`; persisted JSON has no schema_version
- **File:** `crates/asylum-core/src/node.rs:60-70`
- **Category:** api-contract
- **Finding:** persisted as JSON in `nodes.capabilities_json`
  (storage.rs:1163-1165). Adding any new flag (PRD §10 anticipates several)
  fails to deserialize all existing rows: `failed to decode capabilities`.
- **Suggested fix:** annotate every field with `#[serde(default)]`. Optionally
  derive `Default` for `CapabilitySnapshot`. Add a serde-roundtrip test that
  feeds the struct a JSON object missing each field and asserts `false` for
  the missing flags.

### H5. ntfy toast spawner interval is reset every refresh, never fires
- **File:** `cockpit/src/App.tsx:185-219`
- **Category:** concurrency
- **Finding:** the toast effect has `[tweaks.ntfyEnabled, tweaks.simSpeed,
  channels]` in its deps. `refreshAll()` polls every 6s and calls
  `setChannels(c)` (line 138), producing a new array reference even when
  content is unchanged → effect cleanup → fresh interval. With `simSpeed=slow`
  the interval is 9000ms but the deps churn every 6000ms, so the timer never
  fires before being reset. Inbound ntfy messages never surface as toasts.
- **Suggested fix:** drop `channels` from deps; read from a ref. Or memoize
  channels by content. Or move the toast logic into the `refreshAll`
  callback itself rather than reacting to state changes.

### H6. Toast reply lookup uses sender string as node id
- **File:** `cockpit/src/App.tsx:200,500-510`; `cockpit/src/types.ts:122`
- **Category:** correctness
- **Finding:** `t.from = latest.sender` (free-form channel sender string,
  e.g. `"ntfy:user@host"`). The reply handler then does
  `graph.nodes.find(n => n.id === t.from)` which is always undefined. Quick
  reply and free-text reply silently no-op while the UI implies success.
- **Suggested fix:** the toast must remember the node id explicitly. The
  message comes from `latest` which has the originating node id; persist that
  on the toast as `nodeId` and look up by it.

### H7. `resumeNode` hits nonexistent endpoints; failure swallowed
- **File:** `cockpit/src/api.ts:202-205`; `cockpit/src/App.tsx:312`;
  `crates/asylum-daemon/src/app.rs:130-133` (router)
- **Category:** api-contract
- **Finding:** the daemon registers only `/interrupt|stop|archive|input`
  routes. `/resume` does not exist anywhere. `resumeNode` always 404s, and
  the App's `.catch(()=>{})` hides it.
- **Suggested fix (path A — cockpit):** remove `resumeNode` and the call site
  until the daemon implements it; show "resume not yet supported" in UI
  affordances. **Path B (daemon):** add a `/api/nodes/{id}/resume` route
  that maps to a daemon `resume_node` (which currently does not exist either
  — would require a real implementation in `capability_service`). Path A is
  the right fix in scope; path B is a future PR.

### H8. `publish-release` uses `--target HEAD` + `--clobber`
- **File:** `scripts/publish-release.sh:117-124`
- **Category:** correctness
- **Finding:** if the tag does not yet exist on GitHub, the script creates
  the release at `git rev-parse HEAD` regardless of which branch the operator
  is on. There is no check that HEAD matches the version being published, no
  check that a local annotated tag for `$tag` exists or points at this commit.
  Combined with `--clobber` on the upload, an accidental re-run from a
  different branch silently overwrites a tagged release's artifacts with
  binaries built from unrelated source.
- **Suggested fix:** require a local annotated tag for `$tag` to exist; verify
  `git rev-parse $tag` matches HEAD (or at minimum print and refuse on
  mismatch). Use the tag name as `--target` rather than `HEAD`.

### H9. `curl | bash` installer trusts a co-located checksum
- **File:** `scripts/install.sh:308-344` (download + verify path); `README.md`
  (instructions); `install.sh:277-280` (silent skip on no hash tool)
- **Category:** security
- **Finding:** `checksums.txt` is fetched from the same release URL as the
  archive, so a release-asset compromise (stolen GitHub token, malicious
  workflow that re-uploads with `--clobber`) can publish self-consistent
  attack artifacts. There is no GPG/sigstore/cosign signature, no pinned
  public key, and verification is silently "skipped" if the host has neither
  `sha256sum` nor `shasum` (line 277-280).
- **Suggested fix:** (1) make missing-hash-tool a hard error, not a silent
  skip; (2) add support for a detached signature on `checksums.txt` —
  prefer minisign (small, single binary, no agent), fall back to gpg if
  present; ship the public key embedded in the installer or pinned by a
  TOFU file under `~/.config/asylum`. (3) Document the model in README.

---

## MED

### M1. Attach bearer token persisted in plaintext in events table
- **File:** `crates/asylum-daemon/src/capability_service.rs:1088-1100`
- **Finding:** `attach_browser` issues a 10-minute attach token whose `.raw`
  value is the actual bearer used by `/attach/{token}` and
  `/api/attach/{token}/ws`. The handler then writes
  `record_event(node_id, AttachIssued, json!({ "token": token.raw }))`,
  serializing the secret into the events table — exposed via
  `/api/nodes/{id}/events` to anyone with an owner token, kept past the
  10-min TTL. A leaked SQLite file replays as historical attach URLs.
- **Suggested fix:** redact — store only a short fingerprint
  (`token.raw[..6]`) and the token id, not the full bearer.

### M2. `handle_node_observe_ws` never reads from socket
- **File:** `crates/asylum-daemon/src/app.rs:382-428`
- **Finding:** the function does not split the socket and only awaits
  `output.recv()`. It never polls `socket.recv()`, so client Close frames and
  Pings are not observed. On a quiet node with a closed client, the task
  blocks indefinitely on `output.recv()` while holding the WebSocket and
  broadcast subscriber. `handle_attach_ws` correctly uses `select!`.
- **Suggested fix:** mirror `handle_attach_ws` — `socket.split()` and
  `select!` over `recv.next()` and `output.recv()`.

### M3. `fork_node` swallows relationship-creation error
- **File:** `crates/asylum-daemon/src/capability_service.rs:1833-1838`
- **Finding:** `let _ = self.store.create_relationship(...)` discards the
  Result; the new node is returned 200 OK without a lineage edge if the DB
  write fails. Persistent inconsistency between API response and graph.
- **Suggested fix:** propagate the error with `?`. If atomicity matters
  across both writes, wrap in a SQL transaction.

### M4. `append_transcript_chunk` not transactional — torn writes
- **File:** `crates/asylum-daemon/src/storage.rs:413-427`; same shape on
  `insert_node` (`344-369`)
- **Finding:** two separate INSERTs (events row + transcript_chunks row) with
  no transaction. If the second fails, the OutputChunk event row persists
  with no transcript text; consumers joining events↔transcript_chunks see
  inconsistent state.
- **Suggested fix:** `conn.transaction()` (or `BEGIN IMMEDIATE`/`COMMIT`)
  around the two INSERTs.

### M5. PTY transcript persistence sink swallows every error
- **File:** `crates/asylum-daemon/src/capability_service.rs:80-82`
- **Finding:** `let _ = sink_store.append_transcript_chunk(...)` discards
  every error with no log. A sustained DB-write failure (lock contention,
  poisoned mutex) silently desyncs cockpit history and telemetry from what
  the user sees on the live attach.
- **Suggested fix:** at minimum `tracing::warn!` on the error. Optionally
  retry with exponential backoff for `Database is locked`.

### M6. Event sequence allocation not transactional + no UNIQUE constraint
- **File:** `crates/asylum-daemon/src/storage.rs:77-85,1118-1148`
- **Finding:** `SELECT COALESCE(MAX(sequence), -1) + 1` then a separate INSERT.
  No `BEGIN IMMEDIATE`, no `UNIQUE(node_id, sequence)`. Two daemons (or
  daemon+CLI) on the same DB can produce duplicate sequences.
- **Suggested fix:** add `UNIQUE(node_id, sequence)` to the schema and a
  transaction around SELECT+INSERT. Add a migration step.

### M7. MCP server replies to JSON-RPC notifications
- **File:** `crates/asylum/src/mcp.rs:174-215`
- **Finding:** treats id-less requests as `id=Null` and answers; spec violation
  — `notifications/initialized` (sent after `initialize`) gets an unsolicited
  error reply, which a strict client treats as a protocol abort.
- **Suggested fix:** if `request.id.is_none()`, do not send a response. Only
  log if the method is unknown.

### M8. pid-fallback daemon inherits stdin/tty, no `setsid`, no `Stdio::null()`
- **File:** `crates/asylum/src/service.rs:174-205`
- **Finding:** dies on terminal close (SIGHUP), leaves stale pidfile.
- **Suggested fix:** `.stdin(Stdio::null())` + pre-exec `setsid()` (or use
  `nix::unistd::setsid` in a `pre_exec` closure).

### M9. Owner token in `localStorage` + embedded in WS URL query
- **File:** `cockpit/src/api.ts:54-62, 371-377`
- **Finding:** XSS in same origin = full daemon control; WS query lands in
  proxy/access logs.
- **Suggested fix:** keep token in module-level memory (lost on reload, fine
  for cockpit which re-prompts) instead of localStorage. For WS, send token
  in `Sec-WebSocket-Protocol` header subprotocol (browser-supported) or as
  a first-message frame; do not embed in URL.

### M10. Duplicate user-input transcript line
- **File:** `cockpit/src/components/NodeSession.tsx:174-185, 252-256`
- **Finding:** optimistic local push + server `input_sent` event both append
  a `{kind:'user', text:v}` row, producing visible duplicates.
- **Suggested fix:** drop the optimistic push and rely on the server event,
  OR de-dupe the server event when text matches the most recent optimistic
  entry.

### M11. `useEffect` with no deps array rebinds bus closures every render
- **File:** `cockpit/src/components/NodeSession.tsx:163-172`
- **Finding:** wasteful and creates closure-identity churn that can interleave
  in-flight runResponse with newer state.
- **Suggested fix:** add the appropriate deps (the captured setters and
  simSpeed) or use `useCallback` for the handlers + `useEffect(..., [bus])`.

### M12. Several referenced icons missing from registry
- **File:** `cockpit/src/lib/icons.tsx:38-71`; `Nav.tsx:35-49`,
  `CmdK.tsx:51,57,86,78`, `Topbar.tsx:46`
- **Finding:** missing names: `layout-grid`, `list`, `activity`, `zap`,
  `sun`, `moon`. Render as blank spans.
- **Suggested fix:** add them to the registry or replace usages with
  existing names.

### M13. publish-release never recomputes archive sha256 against checksums.txt
- **File:** `scripts/publish-release.sh:90-105`
- **Finding:** `validate_archive` only inspects tar contents. If
  `checksums.txt` is stale (e.g. only `--skip-cockpit-build` re-run), publish
  proceeds with mismatched files; installer rejects with confusing message.
- **Suggested fix:** before upload, recompute `sha256sum` for each archive
  in `ARTIFACT_DIR` and verify it matches the entry in `checksums.txt`.

### M14. Checksum verifier falls back to first hash in file when entry not listed
- **File:** `scripts/install.sh:282-289`
- **Finding:** if the named archive isn't present in `checksums.txt`, awk
  yields nothing; the fallback uses the first hash regardless of which
  archive it belongs to. Conceals misconfiguration; opens a renamed-asset
  attack window.
- **Suggested fix:** hard-fail when the named entry is absent.

### M15. Hardlink rejection in `extract_binary` is order-dependent
- **File:** `scripts/install.sh:385-393`
- **Finding:** tar reports the first hardlink as `-`, subsequent links as `h`.
  An archive with `asylum` as the first hardlink defeats the check.
- **Suggested fix:** also reject when `asylum` is a hardlink (verify with
  `tar -tvf` and a stricter classifier, or extract to a sandbox dir and
  validate `lstat` after extraction).

### M16. Shell-rc PATH block injects unescaped `$install_dir`
- **File:** `scripts/install.sh:529-539`; final write at `590,614`
- **Finding:** a path with `"`, `` ` ``, `$`, or `\` corrupts the rc file or
  executes attacker-controlled commands at every shell start. Final write
  is non-atomic.
- **Suggested fix:** shell-quote `$install_dir` (single-quote escape) into
  the rc block; use `printf '%q'` or a hardcoded escaper. Atomic
  temp-write+rename for the rc file.

### M17. Linux release builds run docker as root
- **File:** `scripts/build-release-artifacts.sh:142-167`
- **Finding:** repo bind-mounted at `/work`; any dependency `build.rs` runs as
  root with rw on the host repo (and `.git`).
- **Suggested fix:** `--user $(id -u):$(id -g)` and pre-create the home dir
  for cargo cache; or drop privileges inside the container script.

### M18. ntfy inbound polling not implemented
- **File:** `crates/asylum-core/src/config.rs:86-103`;
  `crates/asylum-daemon/src/capability_service.rs:113-150` (start_background_tasks)
- **Finding:** PRD §16 lists ntfy inbound as a v1 completion-bar item.
  `NtfyConfig.poll_interval_seconds` is dead config — no consumers anywhere.
  start_background_tasks runs only hooks + 5m/30m schedulers.
- **Suggested fix:** new background task that subscribes to ntfy via the
  ntfy SSE/JSON-stream endpoint and dispatches inbound messages to the
  remote-commands handler. Out of scope for the HIGH cleanup; track as a
  follow-up.

### M19. MCP exposes ~8 of ~60 root capabilities
- **File:** `crates/asylum/src/mcp.rs:95-172`
- **Finding:** PRD §9: every root capability should be exposed in CLI **and**
  MCP. Missing many.
- **Suggested fix:** generator that walks `CapabilityName` and emits a
  ToolSpec for each, mapped to the existing HTTP route. Out of scope for
  HIGH cleanup; track as a follow-up.

### M20. NodeEvent has no schema_version, body untyped
- **File:** `crates/asylum-core/src/event.rs:5-14`; `storage.rs:73`
- **Finding:** events kept forever and re-replayed on every WS attach.
  Format is effectively append-only with no detection on rename.
- **Suggested fix:** add a per-kind body type via tagged enum. Out of scope
  for HIGH cleanup.

### M21 (architecture). `TokenIssueRequest` published but never accepted; `TokenScope` enum unused
- **Files:** `crates/asylum-core/src/api.rs:199-212`;
  `crates/asylum-core/src/security.rs:15-34`; `auth.rs:75,87`
- **Finding:** two parallel request types invite drift; `TokenScope` enum is
  published as if authoritative but the daemon stores/checks raw strings and
  always grants `["*"]`.
- **Suggested fix:** delete `TokenIssueRequest`. Either delete `TokenScope`
  or wire it into `auth.rs` validation. Out of scope for HIGH cleanup.

---

## LOW

### L1. Attach signature compared with non-CT String `!=`
- **File:** `crates/asylum-daemon/src/attach.rs:65-74`
- **Suggested fix:** `subtle::ConstantTimeEq` or fixed-time bytes compare.

### L2. Hook filter parse failure fails open
- **File:** `crates/asylum-daemon/src/hooks/mod.rs:100-109`
- **Suggested fix:** fail closed; log the parse error.

### L3. PTY reader started before runtime registered
- **File:** `crates/asylum-daemon/src/substrate/local.rs:60-92`
- **Suggested fix:** insert into `runtimes` before `spawn_blocking`, or
  buffer the first chunks until the entry is registered.

### L4. Telemetry token math counts bytes, not codepoints
- **File:** `crates/asylum-daemon/src/storage.rs:1207-1247`
- **Suggested fix:** use `text.chars().count() / 4`, or pull tiktoken-like
  estimator if accuracy matters.

### L5. Builtin channel seeding error swallowed at startup
- **File:** `crates/asylum-daemon/src/capability_service.rs:94-99`
- **Suggested fix:** propagate or `tracing::error!` and abort.

### L6. MCP `_jsonrpc` field name doesn't match `jsonrpc`
- **File:** `crates/asylum/src/mcp.rs:14`
- **Suggested fix:** rename the field to `jsonrpc` (with `_` only meaningful
  for unused-field lint suppression — unnecessary here) and add
  `#[serde(rename = "jsonrpc")]` if the lint warning is the issue.

### L7. `run_logs` slurps entire log file
- **File:** `crates/asylum/src/cli.rs:1014-1038`
- **Suggested fix:** `Seek::end` then read backwards in chunks, or shell out
  to `tail -n 80`.

### L8. `run_update` drops restart_error when doctor also fails
- **File:** `crates/asylum/src/cli.rs:1102-1118`
- **Suggested fix:** prepend the restart_error context to the doctor result.

### L9. Native-attach command rendering doesn't shell-quote args/env
- **File:** `crates/asylum/src/native_attach.rs:5-50`
- **Suggested fix:** `shell-escape` crate or hand-roll single-quote escape.

### L10. MCP `node.archive` not exposed
- **File:** `crates/asylum/src/mcp.rs:95-172`
- **Suggested fix:** add the ToolSpec; route through `client.archive_node`.

### L11. `download_update_installer` no integrity check before `bash <path>`
- **File:** `crates/asylum/src/cli.rs:1179-1203`
- **Suggested fix:** ship an embedded sha256 of the canonical installer
  script; compare before exec. Same trust path as H9.

### L12. MCP stdio loop blocks tokio worker
- **File:** `crates/asylum/src/mcp.rs:52-93`
- **Suggested fix:** `tokio::io::stdin/stdout` with `AsyncBufReadExt`, or
  `spawn_blocking` for the loop.

### L13. `fire()` setTimeout not cleared on unmount
- **File:** `cockpit/src/screens/NodeScreen.tsx:95-102`
- **Suggested fix:** store the handle in a ref, clear on cleanup.

### L14. Toast spawner replaces all toasts with latest
- **File:** `cockpit/src/App.tsx:199-208`
- **Suggested fix:** `setToasts(prev => [...prev, latest])` with a max length.

### L15. Cmd-K no-op actions
- **File:** `cockpit/src/components/CmdK.tsx:31-45`
- **Suggested fix:** wire to real handlers or remove the items.

### L16. `NotificationRecord.id` typed as string but daemon emits i64
- **File:** `cockpit/src/api.ts:117-134`; `cockpit/src/types.ts:69`;
  `crates/asylum-core/src/api.rs:153-162`
- **Suggested fix:** type as `number` and pass through unchanged.

### L17. `package_binary` leaks tmpdir on failure
- **File:** `scripts/build-release-artifacts.sh:106-124`
- **Suggested fix:** `trap 'rm -rf "$tmpdir"' EXIT` after `mktemp -d`.

### L18. `asylum_fetch_latest_release` uses unauth GH API + sed JSON parse
- **File:** `scripts/install.sh:248-258`
- **Suggested fix:** use the redirect from
  `https://github.com/$REPO_SLUG/releases/latest` (no API call), parse the
  resolved URL.

### L19. `REPO_SLUG` hardcoded
- **File:** `scripts/install.sh:5-7`
- **Suggested fix:** `REPO_SLUG="${ASYLUM_REPO_SLUG:-CaseyID/Asylum}"`.

### L20. `asylum_path_contains` doesn't normalize trailing slashes on PATH segments
- **File:** `scripts/install.sh:92-102`
- **Suggested fix:** strip trailing `/` from each PATH segment before compare.

### L21. Cockpit handles `tool_call` NodeEventKind that doesn't exist
- **File:** `cockpit/src/components/NodeSession.tsx:272-280`;
  `crates/asylum-core/src/event.rs:18-29`
- **Suggested fix:** drop the case (dead code), or define and emit it on the
  daemon side.

### L22. `is_terminal` classifies `Stopped` as terminal
- **File:** `crates/asylum-core/src/api.rs:457-470`
- **Suggested fix:** PRD treats Stopped as resumable (`node.resume`); split
  `is_terminal` (Failed, Archived) from `is_done_for_now` (+ Stopped, Exited).

### L23. App.tsx: "live mode wins the race only sometimes"
- See H5 — same root cause; same fix.

### L24. NodeEventKind::AttachIssued payload shape isn't documented
- **File:** `cockpit/src/components/NodeSession.tsx:262`
- **Suggested fix:** establish a per-kind body type (see M20).

### L25. NodeSession comment "matches NodeEvent on the daemon side (asylum-core::node)"
- **File:** `cockpit/src/components/NodeSession.tsx:65`
- **Finding:** the file is `asylum-core::event`, not `asylum-core::node`.
- **Suggested fix:** correct the comment.

---

## Workstreams (recommended grouping for execution)

1. **Auth/security daemon-side** (H1, M1, L1, L2): consolidate token validation
   into `find_token_by_hash`, redact attach token in events, constant-time
   compare.
2. **Hook engine reliability** (H2, L5): broadcast Lagged → continue, log
   seed errors.
3. **Daemon transactionality + WS hygiene** (M2, M3, M4, M5, M6): one PR.
4. **CLI client wiring** (H3, L7, L8): rebind `client` consistently.
5. **MCP correctness** (M7, L6, L10, L12).
6. **Cockpit reliability** (H5, H6, H7, M9, M10, M11, M12, L13, L14, L15, L16,
   L21).
7. **Installer/release supply chain** (H8, H9, M13, M14, M15, M16, M17, L11,
   L17, L18, L19, L20).
8. **Schema versioning** (H4, M20): add `#[serde(default)]` everywhere
   persisted; consider a `schema_version` column.
9. **PRD parity follow-ups** (M18 ntfy inbound, M19 MCP catalog, M21 token
   surface): track as separate PRs.

---

## Provenance

This report was produced by 6 parallel general-purpose agents acting as a
local imitation of `/ultrareview`, after the cloud version crashed on a
full-codebase orphan-base PR shape (PR #7). Each agent was scoped to a
domain, instructed to produce only verified findings, and to return
structured JSON. Agents and their scopes:

- Security & sandboxing: auth.rs, capability_service.rs, hooks/, substrate/,
  harness/
- Correctness & concurrency: app.rs, attach.rs, channels/, storage.rs,
  notifications/, recipes.rs, asylum-core/event|node|relationship
- CLI & MCP: crates/asylum/*
- Cockpit frontend: cockpit/**
- Installer & release scripts: scripts/*, README.md, Cargo.toml
- Architecture & API contracts: asylum-core/api|capabilities|config|security
  (cross-checked against PRD)

Each finding lists the cited file/line. Severity scale: high (data loss,
silent compromise, broken core feature), med (real bug, may not surface
immediately), low (correctness/UX papercut).
