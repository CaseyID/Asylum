# Asylum Current-Spec Coverage Findings

**Status:** complete for the 2026-05-05 repo-vs-current-spec audit - full coverage matrix populated; CLI/MCP/API/Cockpit decisions, Cockpit relationship management, Cockpit notification operator workflow, authenticated inbound remote-command envelopes, channel correlation, local harness decision ingestion, Loon attach/observe honesty, Cockpit populated-state/responsive regressions, Cockpit narrow-viewport shell/table/node-detail responsiveness, CLI status/doctor exposure warnings, Cockpit telemetry estimate labeling, status JSON cache-shape fixes, and hook unsupported-tool honesty fixes added
**Date:** 2026-05-05
**Primary source of truth:** [asylum-current-product-spec.md](../specs/asylum-current-product-spec.md)
**Controlling goal:** [2026-05-05-asylum-spec-audit-goal.md](../context/2026-05-05-asylum-spec-audit-goal.md)
**Release status:** Released as v0.1.7. See [RELEASES.md](../../RELEASES.md).

## PR Review Summary

This is the single canonical record of the 2026-05-05 spec audit and fix pass.
If you only read one doc for PR #9, read this section plus the Follow-On Fix
Queue at the bottom.

What changed in this PR:

- Completed a repo-vs-[current product spec](../specs/asylum-current-product-spec.md) audit and recorded the evidence matrix below.
- Added/filled practical root capability gaps across CLI, MCP, daemon API, and Cockpit:
  decisions, graph relationships, channel CRUD/inbound, hooks, recipes, notifications, workspace/context, remote commands, and node fork.
- Made Cockpit materially more real and operator-facing:
  daemon-backed Decisions screen, relationship create/remove, notification read/open-node workflow, manual channel inbound, improved terminal/session input, event-backed Activity, honest settings errors, responsive fixes, and populated-state regression tests.
- Added daemon-side behavior that was missing or misleading:
  local stdout-line decision ingestion, inbound channel remote-command execution with token validation, ntfy reply correlation, Loon unavailable-operation failures before mutation, Loon attach/observe honesty, exposure warnings, status JSON cache shape, and unsupported hook-tool honesty.
- Updated docs/release tracking:
  README known limits/service examples, old handoff release-status normalization, and this completed findings record.

Final verification passed:

- `cargo fmt --check`
- `cargo test --workspace`
- `npm --prefix cockpit run test -- --run`
- `npm --prefix cockpit run build`

Remaining product gap:

- Loon/substrate-native decision ingestion is still not implemented because there is no real Loon event stream in this repo to consume. The PR stops over-advertising that capability instead of pretending it works.

How to use the rest of this doc:

- Requirement Coverage: full spec-by-spec matrix.
- Fixes Completed During This Audit: human-readable change log for the work.
- Browser Validation Log and Runtime Smoke Evidence: proof of what was actually run.
- Commands Run: reproducibility trail.
- Completion Audit Checkpoint: why the audit goal was marked complete.

## Start Check

- Branch/checkpoint: `main`, `HEAD=f14a7c4`, `origin/main=f14a7c4`.
- Workspace cleanliness: **not clean before audit work** because `docs/context/2026-05-05-asylum-spec-audit-goal.md` was already untracked. No findings report existed yet.
- Findings file created during this audit and updated after each meaningful slice from that point forward.

## Summary

- Implemented: four-crate architecture, daemon-owned API, local socket/HTTP split, SQLite persistence, local Codex node launch/control/observe, browser/native attach issuance, graph/fork relationships, Cockpit relationship management, most hook/channel storage, current release ledger, and Cockpit first-run/settings basics.
- Partial: Loon/substrate-native decision ingestion remains unimplemented because no real Loon event stream is wired.
- Missing/wrong: no remaining known daemon-side gap for explicit ntfy reply correlation or local harness stdout decision ingestion; Loon decision ingestion remains unimplemented because no Loon event stream is wired.
- Blocked/deferred: full Loon runtime behavior was code-inspected but not live-smoked because no Loon endpoint was configured. Fresh desktop browser validation did not reproduce the stale empty-state count mismatch; follow-up narrow viewport validation reproduced and then fixed Cockpit's fixed-left-rail overflow.

Highest-risk gaps:

- Loon/substrate-native decision ingestion needs a real Loon event source before it can be implemented honestly.
- Cockpit populated-state desktop, narrow Decisions, and narrow Node detail validation now pass the checked shell/table/form behavior; durable automated browser coverage is still missing.

## Evidence Rules Used

- Runtime behavior must be real daemon/Cockpit behavior; test mocks and prototype files do not count as implementation.
- Each non-pass row names source evidence, live verification, or a concrete blocker.
- Cockpit visible behavior requires real browser validation before visible workflows are fully closed.

## Requirement Coverage

| ID | Status | Evidence | Notes / recommended follow-up |
|---|---|---|---|
| ARCH-001 | implemented | `cargo metadata --no-deps` lists exactly `asylum`, `asylum-cli`, `asylum-daemon`, `asylum-types`. | - |
| ARCH-002 | implemented | `cargo metadata` shows only `crates/asylum` has bin target `asylum`; release docs expect one executable. | Archive contents not rebuilt in this audit. |
| ARCH-003 | implemented | `crates/asylum/src/main.rs` only initializes tracing, parses via `asylum-cli`, and dispatches CLI vs daemon run. | - |
| ARCH-004 | implemented | `cargo tree -p asylum-cli --depth 1` has no daemon dependency; daemon tree has no CLI dependency. | - |
| ARCH-005 | implemented | `crates/asylum-types/src/*` is DTO/config/event/node/relationship/security types and pure helpers. | - |
| ARCH-006 | implemented | CLI/MCP/Cockpit/API/hooks/remote commands call daemon service/routes. | CLI breadth gaps tracked under CLI/CAP. |
| ARCH-007 | implemented | Cockpit is top-level `cockpit/`; browser served from daemon; no Rust imports in `cockpit/src`. | - |
| TRANSPORT-001 | implemented | Native attach/status/tests show Unix socket path and `ASYLUM_SOCKET_PATH` support. | - |
| TRANSPORT-002 | implemented | Live daemon served `/`, `/api/health`, `/api/capabilities`, attach URL route, and Cockpit. | Observe WS source-checked; browser validator pending. |
| TRANSPORT-003 | implemented | Current README/spec/CLI use daemon/service terminology; historical serve references are archival/superseded. | - |
| TRANSPORT-004 | implemented | `bind_unix_socket` creates owner-only dir/socket and unlinks stale socket; socket auth test passes. | - |
| TRANSPORT-005 | implemented | HTTP auth middleware is separate from socket auth-bypass router; socket health auth-bypass test passes. | - |
| TRANSPORT-006 | implemented | Localhost defaults and Settings warning exist; `status`, `status --json`, and `doctor` now warn when the effective live bind is non-loopback or all-interfaces. Live smoke with `0.0.0.0:7802` showed the human warning, JSON `network.exposure_warning`, and doctor `warn network exposure`. | Fixed in this audit. |
| LIFE-001 | implemented | Installer/release docs and binary target show one `asylum` product binary. | Release archive not rebuilt. |
| LIFE-002 | implemented | Source/tests cover bare command to Cockpit/bootstrap and health-ready open. | GUI opening not executed. |
| LIFE-003 | implemented | `ASYLUM_HOME=/tmp/asylum-spec-audit.XWSs46 ./target/debug/asylum setup` created runtime state. | - |
| LIFE-004 | implemented | Defaults documented and runtime health/status expose config/db/socket/log paths. | `transcripts_dir` fallback is `~/.asylum/transcripts` unless workspace root configured. |
| LIFE-005 | implemented | Env/CLI overrides work; live status now reports `daemon.bind`, `network.bind`, and `network.port` from health/effective daemon bind when running. | Fixed in this audit. |
| LIFE-006 | implemented | Help/README use `asylum daemon run`; no live command alias for `serve`. | - |
| LIFE-007 | implemented | `asylum --help` lists setup/cockpit/start/stop/restart/status/doctor/logs/update/uninstall. | - |
| LIFE-008 | implemented | Service source/tests cover systemd user/launchd/pid fallback and generated `daemon run --config`. | - |
| LIFE-009 | implemented | `asylum service generate systemd` emits unit text; README now uses service generate. | Fixed in this audit. |
| LIFE-010 | implemented | `status --json` has required top-level shape, uses effective daemon network state when healthy, includes `network.exposure_warning`, and now serializes Cockpit cache state as explicit `cockpit.caches: []` when no separate cache paths exist. | Fixed in this audit. |
| LIFE-011 | implemented | Tests cover update service refresh and post-update doctor through fresh binary. | Live release update not run. |
| LIFE-012 | implemented | Help/source/tests cover uninstall plan/confirm/preserve/purge behavior. | Destructive live uninstall not run. |
| LIFE-013 | implemented | `RELEASES.md` clearly distinguishes manual published releases from main. | - |
| DATA-001 | implemented | SQLite schema creates nodes/events/transcripts/relationships/artifacts/tokens/remote commands/notifications/decisions/channels/messages/hooks/firings. | - |
| DATA-002 | implemented | `NodeRecord` includes ID, harness, substrate, role hint, liveness, workspace, description, timestamps, external ID, capability snapshot, telemetry fields, and idle seconds; daemon storage hydrates telemetry from durable events and tests cover token/tool-call hydration. | Telemetry is derived/estimated and labeled as such under DATA-006. |
| DATA-003 | implemented | Required liveness wire values exist; live node moved through running/stopped. | - |
| DATA-004 | implemented | Live events returned id/node/sequence/kind/body/timestamp/schema in order. | - |
| DATA-005 | implemented | Real Codex PTY produced `output_chunk`; input produced `input_sent`. | - |
| DATA-006 | implemented | Cockpit token/context/tool-call telemetry surfaces now label values as estimates in Inspector, Node detail, Fleet, and session sublines. | Fixed in this audit; `npm --prefix cockpit run test` and `build` pass. |
| DATA-007 | implemented | Tokens hashed/listed as metadata; attach secret not exposed; raw token only issued/rotated. | Scope semantics are explicitly advisory under SEC-004. |
| NODE-001 | implemented | Live `asylum node create --harness codex --substrate local` launched real Codex PTY node. | Claude/Loon not live-smoked. |
| NODE-002 | implemented | CLI/API list/inspect returned durable node records with liveness/caps/workspace/telemetry/substrate/harness. | - |
| NODE-003 | implemented | Live `node send "audit ping"` recorded `input_sent` and reached PTY. | - |
| NODE-004 | implemented | Local interrupt exists; Loon interrupt now requires configured substrate and external ID before mutating liveness. | Fixed in this audit. |
| NODE-005 | implemented | Local stop worked; Loon stop now requires configured substrate and external ID before mutating liveness. | Fixed in this audit. |
| NODE-006 | implemented | Archive exists across API/CLI/MCP/Cockpit/hooks; local archive calls idempotent local runtime stop, marks liveness archived, and preserves durable records; Loon archive now requires configured substrate/external ID before mutating state. | Fixed in this audit. |
| NODE-007 | implemented | Live `/api/nodes/{id}/fork` and `asylum node fork` both launched real nodes and created `spawned_for` relationships. | - |
| NODE-008 | implemented | `/api/nodes/{id}/events` returned history; PTY output stored as events; observe WS source streams history/live; Cockpit session browser validation showed live terminal/session output without fake transcript content. | Add durable WS browser regression coverage. |
| NODE-009 | implemented | Browser attach issued signed 600s URL and event; attach token tests pass. | Attach page not manually opened. |
| NODE-010 | implemented | `asylum attach <node>` returned native attach command/env for same socket/node. | - |
| NODE-011 | implemented | Node capability snapshots visible; harness descriptors now probe configured command executability instead of hardcoding availability. | Fixed in this audit. |
| NODE-012 | implemented | Role hint `command-center` is normal create path/Cockpit first-run launch path. | Live smoke used worker. |
| GRAPH-001 | implemented | Live graph returned only stored `spawned_for` fork relationship; code treats provenance separately. | - |
| GRAPH-002 | implemented | API, MCP, CLI, and Cockpit relationship create/remove/list display now call the daemon relationship endpoints. Desktop browser smoke on `127.0.0.1:7803` created a `user_created` edge from a command-center node to a worker node, refreshed graph state, displayed the child and edge record, then removed it; network showed `POST /api/relationships` 200, `DELETE /api/relationships/{id}` 204, and follow-up `GET /api/graph` 200. | Fixed in this audit. |
| GRAPH-003 | implemented | Graph data/layout derive from explicit relationships, not workspace/substrate correlation. | - |
| HARN-001 | implemented | Codex/Claude descriptors exist, Codex launched, and descriptor availability now reflects configured executable presence. | Fixed in this audit. |
| HARN-002 | implemented | Local launch uses real PTY; Loon adapter invokes real `loon` CLI operations by source inspection. | Loon not live-smoked. |
| HARN-003 | implemented | Launch-packet/context route exists; local PTY launches now receive `ASYLUM_NODE_ID`, role, workspace, base URL, socket path, control transport, graph summary, and capabilities JSON in env. | Fixed and live-smoked in this audit. |
| HARN-004 | implemented | Capability snapshots exist; descriptor availability now probes whether the configured command is executable. | Fixed in this audit. |
| SUB-001 | implemented | Live local Codex launch/output/input/stop/browser attach/native attach worked. | - |
| SUB-002 | implemented | Loon disabled local daemon works; descriptors show local only. | - |
| SUB-003 | implemented | Loon source uses CLI contract; service-layer send/interrupt/stop/archive now fail clearly when Loon is unconfigured or missing external ID. | Fixed in this audit; no live Loon endpoint was available. |
| SUB-004 | implemented | Loon health/capacity descriptor code exists when configured; disabled state is honest. | Not live-smoked. |
| SUB-005 | implemented | Unsupported live observe sentinel exists; Cockpit now renders Loon-specific observe copy, Loon browser attach buttons disclose `loon attach` proxying, and attach API/MCP responses carry `transport`/`note` metadata. | Loon attach/observe copy is source/test-validated; no live Loon endpoint was available. |
| CAP-001 | implemented | `/api/capabilities` lists method/path/description/availability. | Some availability flags need tightening. |
| CAP-002 | implemented | Shared daemon service backs clients; CLI/MCP now cover practical lifecycle, node, graph/relationships, channel, hook, recipe, remote-command, notify, workspace/context, and decision families. | Richer JSON flags remain ergonomic backlog, not root-capability parity. |
| CAP-003 | implemented | Live curl responses and source use `asylum-types` DTOs. | - |
| CAP-004 | implemented | Core node API/MCP/Cockpit source exists; live create/send/events/attach/native/fork/stop worked; CLI `node fork` was added and live-smoked. | - |
| CAP-005 | implemented | Graph/relationship API/MCP/CLI/Cockpit exist; live graph returned explicit relationships and Cockpit relationship create/remove was browser-smoked. | - |
| CAP-006 | implemented | Descriptor routes exist; harness availability now probes configured command executability. | Fixed in this audit. |
| CAP-007 | implemented | System map and launch packet routes exist; launch packet stores artifact. | Launch packet not runtime-smoked. |
| CAP-008 | implemented | Channel CRUD/messages/test/inbound and notify send exist across API/CLI/MCP; manual inbound can persist and explicitly route to a node; unconfigured notify now returns explicit 503. | - |
| CAP-009 | implemented | Hook CRUD/event catalog/test/firings routes exist; event catalog smoked. | - |
| CAP-010 | implemented | Token API/CLI exist, MCP excludes token management, and README/Cockpit now label scopes as advisory. | Fixed in this audit. |
| CAP-011 | implemented | Remote command endpoint/parser supports verbs; CLI helper smoke verified authenticated status and missing-token failure; inbound channel bodies that look like remote-command envelopes and carry `token=` now execute through the same token validation/remote-command executor. Live HTTP smoke on `127.0.0.1:7804` with owner-token auth showed `status token=dev-token` over `/api/channels/webhook-substrate/inbound` created a remote-command notification, plain `status` only recorded inbound text, and `approve decision=<id> token=dev-token` resolved a real decision. | Fixed in this audit. |
| CAP-012 | implemented | Unsupported unconfigured notify returns 503; Loon controls now error before mutating state when Loon is unconfigured or missing external ID. | Fixed in this audit. |
| CLI-001 | implemented | CLI can operate lifecycle/node/graph/attach/token/notify/channel/hook/recipe/remote-command/workspace/context/decision/MCP families. | Richer JSON output flags would still help automation. |
| CLI-002 | implemented | `asylum --help` lists setup/cockpit/start/stop/restart/status/doctor/logs/update/uninstall/daemon/config/service. | - |
| CLI-003 | implemented | `asylum node --help` lists `fork`; live `asylum node fork <id>` created a real forked node. | Fixed in this audit. |
| CLI-004 | implemented | `asylum graph --help` lists `relationships`; `relationships create/list/remove` were live-smoked against daemon relationships. | Fixed in this audit. |
| CLI-005 | implemented | Token issue, notify send, and `channel` list/create/inspect/update/delete/messages/test/inbound commands exist. | Fixed in this audit. |
| CLI-006 | implemented | Recipe list/spawn and remote-command helpers now exist; live smoke returned 6 recipes, authenticated remote status succeeded, missing inline token returned `token required`, and whitespace args are rejected before daemon misparse. | Fixed in this audit. |
| CLI-007 | implemented | Human help/output exists; `status --json` automation output exists. | More JSON flags would improve automation. |
| CLI-008 | implemented | Tests/source show local control uses socket by default unless base URL override. | - |
| MCP-001 | implemented | Stdio JSON-RPC `tools/list` worked; notification handling covered by tests. | - |
| MCP-002 | implemented | Required core node/graph tools exist; `relationship.remove` now advertises and deletes through daemon `DELETE /api/relationships/{id}`. | Fixed in this audit. |
| MCP-003 | implemented | Rebuilt MCP smoke advertises `hook.*`, full channel CRUD/messages/test/inbound, `notify.send/list/read`, workspace/context, recipe, remote-command, decisions, and `health.get`. | Fixed in this audit. |
| MCP-004 | implemented | MCP comments/tool list exclude token management. | - |
| MCP-005 | implemented | Rebuilt MCP smoke shows `notify.send`; `tools/call notify.send` reaches daemon `/api/notify/send`. | Fixed in this audit. |
| COCKPIT-001 | implemented | Browser loaded `http://127.0.0.1:7787/`, title `asylum cockpit`; Playwright and Chrome console checks found no errors. | - |
| COCKPIT-002 | implemented | Browser first view was graph-first shell/first-run with nav counts; source shows graph/session/inspector areas. A later UI subagent saw `0` counts while backend graph already had two stopped nodes, but a fresh desktop smoke on `127.0.0.1:7800` with one live node showed `1 running`, nodes `1`, channels `1`, hooks `0`, and daemon-derived `asylum 0.1.6`/bind footer. `cockpit/src/App.test.tsx` now regression-tests populated graph refresh, nav counts, live count, and daemon footer from mocked daemon responses. | Fixed in this audit. |
| COCKPIT-003 | implemented | Browser zero-node first-run showed product summary and `start a command center`. | - |
| COCKPIT-004 | implemented | Source uses `createNode` daemon API; daemon runtime launch verified via CLI/API. | Browser create click pending. |
| COCKPIT-005 | implemented | `NodeSession` sends via `postNodeInput`, observes events/WS, renders a multiline terminal textarea with send/newline keyboard hints, and has honest attach/native/interrupt session chrome. | Textarea/input browser-validated; final chrome changes were test/build/static-validated. |
| COCKPIT-006 | implemented | Source routes graph/table/chat node selection to real node session/detail data; browser validation opened a real node detail/session view. | Browser-validated in this audit. |
| COCKPIT-007 | implemented | Graph layouts derive from real nodes/relationships and persisted layout preference. | Visual quality pending. |
| COCKPIT-008 | implemented | Fleet screen uses real node records with search/filter. Latest populated desktop browser smoke showed real nav/header counts against a live daemon, so the stale empty-count state did not reproduce there. `cockpit/src/App.test.tsx` now drives the app from a populated graph snapshot and verifies the Fleet screen receives/render counts from that same data. | Fixed in this audit. |
| COCKPIT-009 | implemented | Node detail tabs use daemon-backed session/events/activity/capabilities/relationships/telemetry; the old fake tools placeholder was replaced with an event-backed activity view, and the Relationships tab now creates/removes explicit daemon graph edges. | Relationship tab browser-validated on desktop and narrow viewports. |
| COCKPIT-010 | implemented | Send controls now route to real session input; node session sends daemon input through a multiline terminal input. | Fixed and browser-validated in this audit. |
| COCKPIT-011 | implemented | Logs copy now says daemon notification records, matching real `fetchNotifications`. | Fixed in this audit. |
| COCKPIT-012 | implemented | Channel CRUD/test/history are API-backed; Cockpit now has a manual inbound composer for live inbound/duplex channels and can ask the daemon to route the body to a node input stream by node ID. Authenticated command envelopes submitted through inbound channel bodies now execute daemon remote commands. | Fixed in this audit. |
| COCKPIT-013 | implemented | Hooks screen source uses daemon hook endpoints for CRUD/test/catalog/firings. | Browser pending. |
| COCKPIT-014 | implemented | Settings reads health/descriptors/channels/tokens; app chrome derives version/bind/harness count; settings now surfaces per-panel load errors instead of silently swallowing API failures. | Fixed in this audit. |
| COCKPIT-015 | implemented | Cmd-K source navigates screens, finds nodes, request attach/remote commands. | Browser pending. |
| COCKPIT-016 | implemented | Owner token kept in module memory; URL token is stripped by `hydrateOwnerTokenFromLocation`. | - |
| COCKPIT-017 | implemented | No `Tweaks`/`simSpeed`/`runResponse`; no-op send, hardcoded nav/count, and settings silent-failure paths fixed. `cockpit/src/App.test.tsx` protects populated-state data plumbing and `cockpit/src/responsive-css.test.js` protects the responsive layout guardrails added after browser validation. | Fixed in this audit. |
| COCKPIT-018 | implemented | Browser/source preserve graph-first dense operational layout with real data; node session terminal UI was browser-validated and restart/fake-tools terminal issues were fixed. Fresh desktop Decisions smoke worked with populated daemon state. A 390px validation exposed fixed-rail overflow; CSS follow-up stacked nav above content and contained wide tables with horizontal panel overflow, then browser revalidation confirmed shell/table behavior. Later 390px Node detail validation exposed two-column node-detail overflow; a follow-up CSS patch stacked Node detail/main/side panels and made relationship form/table controls fit inside the node pane. `cockpit/src/responsive-css.test.js` now guards the mobile shell/table/node/log selectors. | Fixed in this audit. |
| CHAN-001 | implemented | ntfy channel and inbound subscriber exist; outbound notify route exists and returns explicit unavailable when unconfigured. | - |
| CHAN-002 | implemented | Manual inbound route persists `channel_messages` rows and fires `channel.inbound`; routed inbound validates channel/node before durable write. | - |
| CHAN-003 | implemented | Inbound records exist; Cockpit/manual API/CLI/MCP inbound can route to an explicit node ID; ntfy reply-marker correlation can resolve token to node ID and route before durable write. | - |
| CHAN-004 | implemented | Remote command parser exists; inbound channel auth/routing now checks command-looking bodies with `token=`, validates the message token, executes the shared remote-command path, and records the inbound envelope. Plain command words without tokens remain normal inbound text. Status and decision approve/deny are covered by regression tests and live HTTP smoke. | Fixed in this audit. |
| CHAN-005 | implemented | Node input routing exists for explicit node IDs and ntfy reply-marker correlation; correlated inbound strips the transport marker before sending text to the node. | Loon live routing not smoked. |
| CHAN-006 | implemented | Non-ntfy adapters appear as `future`/`live:false`; webhook live is real inbound endpoint. | - |
| NOTIFY-001 | implemented | Notifications list/mark-read persist, Cockpit Logs now shows read/unread state, all/unread counts, unread filtering, node context, per-row mark-read action, and node-open action for node-linked notifications. Live browser smoke on `127.0.0.1:7805` marked a real decision notification read through `POST /api/notifications/1/read`, refreshed counts from `1 unread` to `0 unread`, and opened the linked node. | Fixed in this audit. |
| HOOK-001 | implemented | Hooks match event/filter, execute actions, persist firings. | - |
| HOOK-002 | implemented | Filter parse failures fail closed by hook engine tests/source. | - |
| HOOK-003 | implemented | Hook `channel`, `spawn`, `pause_node`, and `archive` actions call daemon behavior; `tool graph.get` is real summary behavior; unsupported `tool transcript.checkpoint` now returns an explicit unsupported error instead of successful `noop`. | Fixed in this audit; targeted test covers unsupported firing. |
| HOOK-004 | implemented | Dry-run sets synthetic payload and stores test firing. | - |
| DECISION-001 | partial | First-class API/CLI/MCP/Cockpit decision create/list/inspect/resolve exists; local harness stdout-line decision ingestion creates node-linked decisions and was live-smoked. Loon no longer advertises `structured_events` capability without an event stream. | Loon/substrate-native decision events remain unimplemented until a real Loon event source exists. |
| DECISION-002 | implemented | CLI/MCP/API can surface pending decisions and resolve them; Cockpit now has a daemon-backed Decisions screen with pending/resolved lists and approve/deny actions. Desktop browser validation showed the pending decision row and approve/deny controls against a live daemon. | Playwright safety review blocked clicking approve in-browser despite disposable dev state; state transitions remain covered by CLI/MCP/API live smokes and tests. |
| DECISION-003 | implemented | Resolve updates durable state and now emits notifications; node-linked decisions also emit node events. | Fixed in this audit. |
| SEC-001 | implemented | Default bind localhost; no team/org/RBAC model found. | - |
| SEC-002 | implemented | HTTP owner-token auth validates config/DB tokens and rejects missing token when enabled; remote command returned token required. | - |
| SEC-003 | implemented | Socket auth boundary separate from HTTP auth; socket bypass test passes. | - |
| SEC-004 | implemented | Token scopes are stored/listed and now labeled as advisory in README and Cockpit settings. | Fixed in this audit. |
| SEC-005 | implemented | Attach token tests reject expired/tampered tokens; live URL was signed and 600s. | - |
| SEC-006 | implemented | Remote commands require token/auth; live unauthenticated status returned `token required`; inbound channel command envelopes also require a valid `token=` inside the message body and do not execute plain command words without one. | Fixed in this audit. |
| SEC-007 | implemented | Non-local exposure warnings now exist in Settings plus CLI `status`, machine-readable `status --json`, and `doctor`, based on the effective live bind after health is applied. | Fixed in this audit. |
| SEC-008 | implemented | Token list/status do not dump raw tokens; ntfy token not shown in normal health/status. | Review channel config redaction before exposing custom secrets broadly. |
| PROTO-001 | implemented | Browser/source graph-first, dense, terminal-aware, node/session-focused layout; terminal/session was browser-validated after textarea fix. | - |
| PROTO-002 | implemented | Most workflows real; decisions, manual channel inbound, explicit ntfy reply correlation, Cockpit relationship management, and authenticated inbound command envelopes are now daemon-backed. | Remaining non-prototype gaps are tracked in their domain rows. |
| PROTO-003 | implemented | Fake nodes/logs/settings mostly gone; hardcoded nav/count fixed; fake attach transcript removed; future adapter stubs are clearly not live; hook unsupported-tool noop fixed; remaining gaps are verification/product-depth backlog rather than fake/demo runtime state. | - |
| PROTO-004 | implemented | Simulation speed/canned response gone; send no-op and fake attach transcript fixed; hook `transcript.checkpoint` no longer returns a successful noop and instead records explicit unsupported failure. | - |
| PROTO-005 | implemented | Theme/nav/layout handled as persisted UI preferences. | - |
| DOC-001 | implemented | README now names `asylum daemon run` and `service generate`; install/run current. | Fixed in this audit. |
| DOC-002 | implemented | Current spec/brief point to canonical truth; older docs labeled historical/superseded where relevant. | - |
| DOC-003 | implemented | README no longer instructs old service install forms; stale `serve`/install references are historical docs/spec text. | Fixed in this audit. |
| DOC-004 | implemented | Ledger is clear; this active audit doc uses the canonical `Doc-only / internal — no release needed. Last release: v0.1.6 (2026-05-05).` phrase, and older delivery docs with stale release-status wording were normalized to canonical ledger links. | - |
| DOC-005 | implemented | README now has a concise Known Limits section for advisory scopes, local-vs-Loon validation, inbound routing/correlation limits, decision workflow limits, and exposure posture. | Fixed in this audit. |

## Findings

### Fixed: CLI/MCP capability surface is much broader

Expected: The CLI is the primary local operator interface and reaches practical root capabilities.

Observed: `node fork`, `graph relationships`, `channel`, `hook`, `recipe`, `remote-command`, `notify list/read`, `workspace recent`, `context system-map/launch-packet`, and `decision` families were added during this audit. MCP parity was also added for channel CRUD/messages/test/inbound, notification list/read, workspace/context, recipe, raw remote-command send, and decisions.

Impact: Operators can now drive the practical root capability families from terminal and MCP surfaces without relying on raw HTTP.

Recommended follow-up: Add richer machine-readable output flags where useful; keep token management out of MCP.

### Fixed/Partial: Decision workflow is now first-class, local ingestion exists

Expected: Harness/substrate events can create decision records; Cockpit/notifications/remote commands surface pending decisions; approve/deny resolves and feeds back.

Observed before fix: Decision storage and remote approve/deny resolution existed in pieces, but no create/list/inspect API, no CLI/MCP decision workflow, no Cockpit pending-decision workflow, and no clear node/event/channel feedback on resolution were found.

Remediation completed: Decision create/list/inspect/resolve now exists through daemon API, CLI, MCP, and Cockpit. Create/resolve writes durable records and notifications; node-linked decisions emit node events. Remote `approve`/`deny` now goes through the normal decision resolution path, so remote-command decisions also emit the same notification/event feedback. Local harness stdout-line decision ingestion now creates pending node-linked decisions with notification/event/liveness feedback.

Remaining impact: Loon/substrate-native event ingestion still has no real event source in current code.

Recommended remediation: Add substrate-native decision event ingestion once the substrate exposes real structured events.

### Fixed: Loon unavailable capability paths could report success too quietly

Expected: Unsupported capabilities fail clearly.

Observed before fix: Some Loon send/interrupt/stop/archive branches did nothing when `loon_substrate` was absent or external ID was missing, then returned success and could update liveness.

Impact: Operators can get false success and node liveness can drift from substrate truth.

Remediation completed: `require_loon_target` now requires a configured Loon substrate and external ID for send/interrupt/stop/archive. The regression test verifies these operations return errors and do not add `input_sent` or change liveness when Loon is unavailable.

### Fixed: Status JSON mixed effective and configured network state

Expected: `status --json` exposes machine-readable effective host/network state.

Observed before fix: With daemon running on `127.0.0.1:7787`, status returned `daemon.bind=127.0.0.1:7787` but `network.bind=127.0.0.1:7717`.

Impact: Automation and operators can inspect the wrong port/exposure state.

Remediation completed: `HostState::apply_daemon_health` now updates daemon and network state together from health. Live smoke now reports `network.bind=127.0.0.1:7787`, `network.port=7787`, and `port_in_use=in_use`.

### Fixed: CLI status/doctor now warn on non-loopback exposure

Expected: Operators get an obvious warning when the daemon is effectively bound beyond loopback.

Observed before fix: Cockpit Settings showed exposure posture, but CLI `status`, `status --json`, and `doctor` did not warn based on the effective bind learned from daemon health.

Impact: A local operator could bind the HTTP daemon to all interfaces and miss the exposure posture in CLI workflows.

Remediation completed: `HostState.network` now includes `exposure_warning` derived from configured or health-reported effective bind. Human `status` prints `Warning: ...`, JSON status includes `network.exposure_warning`, and `doctor` emits `warn network exposure`. A live daemon bound to `0.0.0.0:7802` verified all three surfaces.

### Fixed: Status JSON Cockpit cache state is explicit

Expected: `status --json` exposes machine-readable Cockpit cache state.

Observed before fix: v0.1.x has no separate Cockpit cache paths outside owned runtime state, but JSON reported this as `cockpit.caches: null`.

Impact: Automation had to distinguish "unknown" from "known empty" even though current runtime semantics are known empty.

Remediation completed: `CockpitInfo.caches` is now an explicit empty list when there are no separate Cockpit cache paths. Live `status --json` smoke under `/tmp/asylum-status-json-smoke` returned `"caches": []`.

### Fixed: Unsupported hook tool target no longer succeeds as noop

Expected: Hook actions call real capabilities or return explicit unsupported errors.

Observed before fix: `tool` action target `transcript.checkpoint` returned success text `transcript.checkpoint:noop`, which made unsupported behavior look successful at runtime.

Impact: Hook firings could report `ok=true` while doing no real daemon work.

Remediation completed: `transcript.checkpoint` now returns `tool target 'transcript.checkpoint' is not supported yet`; hook test/firing records this as `ok=false`. Regression test `transcript_checkpoint_hook_tool_reports_unsupported` covers the behavior.

### Fixed: Harness descriptors overstated availability

Expected: Harness descriptors show real availability and capability flags.

Observed before fix: `list_harness_descriptors` hardcoded `available: true` for Codex and Claude Code without probing command availability.

Impact: Cockpit/API can advertise launch support that fails only at create time.

Remediation completed: descriptors now probe whether the configured command resolves to an executable file. A live daemon started with missing harness command paths returned `available:false` for both Codex and Claude Code.

### Fixed: Launch context was weak for local harnesses

Expected: New nodes receive node ID, role hint, graph/capability context, and attach/control instructions.

Observed before fix: Local `SubstrateContext.env` was empty at launch; launch packets existed as a separate route/artifact, but local harness startup did not clearly receive them.

Impact: Harnesses are less Asylum-aware than the spec requires.

Remediation completed: Local launch context now injects node ID, role, workspace, base URL, socket path, control transport, graph summary, and capabilities JSON through environment variables. A fake-harness live smoke verified the variables arrived in the spawned PTY process.

### Fixed: Token scopes were advisory but not labeled

Expected: Token scopes are represented honestly.

Observed before fix: Token issuance stored scopes; validation only checked hash/revocation/expiry and did not enforce scope on routes or remote command dispatch. README/Cockpit did not clearly say scopes were advisory.

Impact: Scope display can create a false least-privilege impression.

Remediation completed: README and Cockpit settings now state that scopes are advisory labels and owner-token auth is enforced at token level.

### Fixed: Cockpit/channel inbound workflow now has manual and correlated paths

Expected: Cockpit channels screen supports inbound webhook/manual messages and subscribe details using daemon endpoints.

Observed before fix: API client had `inboundChannel`, and live API persisted manual inbound, but Cockpit screen did not expose a manual inbound composer.

Remediation completed: Cockpit now shows `record inbound` for live inbound/duplex channels. The modal records sender/subject/body/replies through the daemon inbound endpoint and can optionally ask the daemon to route the body to an explicit node ID. The daemon validates live/inbound channel status and target node delivery before durable write, preventing partial channel records for failed routes. For real ntfy replies, hook-driven outbound channel sends with node context now mint a short reply token, store a durable correlation, append a rigid ntfy marker, and let the ntfy inbound subscriber resolve the token back to the node before durable inbound write.

Additional remediation completed: Authenticated remote-command envelopes now execute through the channel inbound path. Command-looking bodies with `token=` use the same token validation and remote-command executor as direct `/api/remote-commands`; plain command words without tokens remain ordinary inbound text. The ntfy subscriber now funnels inbound messages through the same service path while preserving correlated raw reply routing before durable write.

### Fixed/Partial: Local harness decision ingestion now exists

Expected: Harness/substrate events can create durable decision records tied to nodes.

Observed before fix: Decision workflows existed through API/CLI/MCP/Cockpit, but there was no path for a real harness event/output stream to create a pending decision.

Remediation completed: Local PTY output now supports an explicit reserved stdout-line protocol, `@@asylum:decision.request {...}`, advertised to local harnesses through `ASYLUM_DECISION_PROTOCOL=stdout-line-v1`. Complete control lines are suppressed from transcript/live output, malformed marker lines are ignored, and ordinary prose is passed through. Valid requests create a pending node-linked decision, notification, `human_input_requested` event, `node.permission_requested` hook event, and move the node to `waiting_for_input`. Resolving the decision restores the node to `running` only when it was waiting for input.

Remaining impact: This is local-substrate stdout ingestion. Loon/substrate-native structured decision events still need a real event source before implementation.

### Medium: Cockpit terminal/session UX still needs deeper prototype alignment

Expected: Node interaction should feel like supervising live harness TUIs, with minimal friction between operator and node. The prototype treats chat/session as a unified live TUI surface with node rail, terminal transcript, inline attach artifacts, and session-adjacent controls.

Observed: Current `NodeSession` is real and no longer simulated, but before this audit its input was a one-line `<input>`, attach preview contained canned fake terminal output, restart controls actually stopped nodes, and the Tools tab was a placeholder. This audit changed the input to a multiline terminal `textarea`, added send/newline keyboard hints, removed fake attach transcript content, moved attach/native attach/interrupt into shared session chrome, replaced restart labels with stop semantics, removed dead streaming/esc copy, and replaced the fake Tools tab with an event-backed Activity view. Remaining deltas: the command-center orchestration feel is still weaker than the prototype, activity only shows explicit tool-like events, and rendered populated-graph/session workflows need broader browser validation.

Impact: The core Asylum value proposition depends on low-friction supervision and interaction with multiple live nodes. A cramped or split session UI makes Cockpit feel like an administrative wrapper instead of an operational terminal.

Recommended remediation: Continue treating NodeSession/ChatScreen as a first-class terminal workbench: tighten rail-to-session workflows, enrich event-backed activity when daemon emits dedicated tool events, and validate against the prototype with real browser screenshots.

### Fixed: Cockpit settings swallowed some daemon API failures

Expected: Cockpit settings displays daemon-backed values or honest errors.

Observed before fix: Settings loaded real APIs, but several `.catch(() => {})` paths could leave empty panels without visible error.

Impact: API/auth/backend failures can look like empty configuration.

Remediation completed: Settings now records and renders panel-level errors for ntfy channels, tokens, harnesses, substrates, and health-backed network/storage panels.

### Fixed: Cockpit populated-state browser validation improved; responsive shell fixed

Expected: Once the backend graph contains persisted nodes, Cockpit first-run/nav counts agree with `/api/graph` and descriptor routes after refresh; Cockpit remains usable on narrow screens.

Observed: Backend validation showed two stopped audit nodes and one explicit relationship. A delegated UI validation subagent later captured the first-run browser state as `0 harnesses ready · 0 substrates configured · 0 nodes alive`; Playwright/Chrome console checks found no errors and Playwright network inspection showed repeated `200 OK` API polls. A fresh main-agent desktop smoke on `127.0.0.1:7800` did not reproduce the stale count state: the header/nav/footer reflected one running node, one channel, zero hooks, daemon version, and daemon bind. However, a 390px viewport on the Decisions screen kept the 220px side rail and pushed the main content/table off to the right.

Remediation completed: A narrow-viewport CSS media query now stacks the shell into one column, makes the nav full-width above the main content, wraps page chrome/toolbars, and lets table-heavy panels scroll horizontally inside the panel. Follow-up browser validation on `127.0.0.1:7801` at `390x844` showed full-width nav above Decisions content, visible daemon-derived counts/footer, contained wide decision tables, and zero fresh console warnings/errors.

Additional remediation completed: `cockpit/src/App.test.tsx` now exercises populated graph refresh, nav counts, daemon footer, and Fleet screen data flow from daemon-shaped responses. `cockpit/src/responsive-css.test.js` guards the mobile shell/table/node/log CSS selectors added after browser validation.

Remaining impact: These are Vitest/jsdom/static CSS guardrails, not a full Playwright CI suite. Add Playwright e2e coverage later only if the project chooses to carry that dependency.

### Fixed: Documentation limitations were scattered

Expected: Known product limitations are explicit.

Observed before fix: README documented some posture but did not consolidate unsupported adapters, local-vs-Loon differences, advisory scopes, decision limitations, channel correlation limits, and exposure warnings.

Impact: Operators can miss important product boundaries.

Remediation completed: README now includes a concise Known Limits section near the product path.

### Low: Release-status phrasing is uneven across delivery docs

Expected: Delivery docs use clear release status sections.

Observed: Ledger is clear, but some handoff/review docs use historical wording rather than the exact current phrase set.

Impact: Mostly legibility, not runtime behavior.

Remediation completed for this active audit: release status now uses the canonical `Doc-only / internal — no release needed. Last release: v0.1.6 (2026-05-05).` wording and links to `RELEASES.md`. Older release-status sections with stale platform or ahead-of-tag wording were also normalized to either `Released as` or `Doc-only / internal` plus a canonical ledger link.

## Fixes Completed During This Audit

- README service examples changed from old `asylum install launchd|systemd` forms to `asylum service generate launchd|systemd`.
- MCP tool surface changed from off-spec `channel.send` to `notify.send`, routed to `/api/notify/send`; rebuilt smoke verifies `notify.send` appears and calls the daemon.
- Cockpit send-input side controls now route to the real session input surface instead of only selecting/flashing locally.
- Cockpit Logs screen copy now describes daemon notification records instead of claiming a unified event stream.
- Cockpit nav/version and first-run harness count now derive from daemon health/harness descriptor APIs; stale fallback `asylum 0.1.0` removed.
- `status --json` now reports effective live daemon bind/port in `network` when health is available instead of stale configured bind.
- MCP `relationship.remove` now exists and was live-smoked by deleting the audit fork relationship.
- CLI `asylum node fork` now calls daemon fork and was live-smoked with a real local Codex fork.
- CLI `asylum graph relationships create/list/remove` now calls the daemon relationship API and was live-smoked.
- Harness descriptors now report missing configured commands as unavailable instead of hardcoded available.
- Unconfigured `/api/notify/send` now returns HTTP 503 with a clear error instead of `{"sent":false}`.
- Loon send/interrupt/stop/archive now fail clearly before mutating state when Loon is unconfigured or missing an external ID.
- README and Cockpit settings now label owner-token scopes as advisory rather than enforced.
- CLI `asylum channel` now exposes list/create/inspect/update/delete/messages/test/inbound and was live-smoked against temp daemons.
- CLI `asylum hook` now exposes list/create/delete/firings/catalog/test and was live-smoked against a temp daemon.
- CLI `asylum recipe list/spawn` and `asylum remote-command status/attach/send/start/interrupt/stop/approve/deny` now call daemon recipe/remote-command routes; live smoke verified recipe listing, authenticated remote status, missing-token failure, and whitespace guardrails.
- CLI `asylum notify list/read`, `asylum workspace recent`, and `asylum context system-map/launch-packet` now call the daemon notification/workspace/context routes; live smoke verified notification read, system-map output, and a launch packet for a recipe-spawned node.
- MCP now advertises and handles `notify.list`, `notify.read`, `workspace.recent`, `context.system_map`, `context.launch_packet`, `recipe.list`, `recipe.spawn`, and `remote_command.send`; live JSON-RPC smoke verified tool listing, recipe list/spawn, remote status, launch packet, notification read, and node stop cleanup.
- MCP now also advertises and handles `channel.inspect`, `channel.create`, `channel.update`, `channel.delete`, `channel.messages`, `channel.test`, and `channel.inbound`, closing the channel surface parity gap with API/CLI.
- Decisions now have first-class daemon API routes plus CLI and MCP tools for create/list/inspect/resolve; live smoke verified CLI pending/approved transitions and MCP create/denied/list behavior.
- Cockpit now has a daemon-backed Decisions screen with pending/resolved tables, create, refresh, approve, and deny actions.
- Remote `approve`/`deny` commands now call the normal decision resolution path so node-linked decisions get the same resolved notification and node event as API/CLI/MCP resolution.
- ntfy reply correlation now uses a durable `channel_reply_correlations` table and rigid `[asylum-reply:<token>]` marker; correlated inbound strips the marker before routing to the node and does not persist if route delivery fails.
- Local harness stdout decision ingestion now supports `@@asylum:decision.request` protocol lines, suppresses control lines from visible transcript/output, and creates pending node-linked decisions with notification/event/liveness feedback.
- Local harness launch context now injects `ASYLUM_NODE_ID`, `ASYLUM_NODE_ROLE`, `ASYLUM_WORKSPACE`, `ASYLUM_BASE_URL`, `ASYLUM_SOCKET_PATH`, `ASYLUM_CONTROL_TRANSPORT`, `ASYLUM_GRAPH_SUMMARY`, and `ASYLUM_CAPABILITIES_JSON`; a fake-harness live smoke verified the env reached the PTY process.
- Cockpit Channels now includes a manual inbound composer for live inbound/duplex channels; it records inbound messages and can route the body to an explicit node input stream through the daemon. The daemon rejects non-live channels and missing/unroutable nodes before storing the inbound row.
- Authenticated remote-command envelopes now execute through channel inbound: `status`, `approve`, `deny`, and other remote-command verbs with `token=` are parsed, token-validated, executed through the shared remote-command path, and still recorded as inbound channel messages. Plain command-looking text without `token=` remains normal inbound text.
- Cockpit `NodeSession` now uses a multiline terminal textarea with `Enter` send and `Shift+Enter` newline; fake attach-preview transcript content was removed.
- Cockpit terminal/session chrome now exposes browser attach, native attach, and interrupt in shared sessions; restart controls were relabeled to real stop semantics; dead streaming/ESC affordances were removed; additional daemon event kinds render with useful session text; fake Tools placeholder was replaced with an event-backed Activity tab.
- Cockpit Node detail Relationships tab now creates and removes explicit graph relationships through daemon APIs, displays edge records, refreshes graph state after mutations, and surfaces API errors.
- Cockpit Logs now treats daemon notifications as operator records: unread/read state, all/unread counts and filtering, node context, real `mark read` action, and an `open` action for node-linked notifications.
- Cockpit derived token/context/tool-call telemetry labels now say `est.` / `telemetry estimates` in Inspector, Node detail, Fleet, and session sublines.
- Cockpit Settings now renders explicit per-panel load errors for channels/tokens/descriptors/health instead of silently presenting empty panels on API/auth failures.
- README now includes a concise Known Limits section covering advisory scopes, Loon optionality, inbound routing limits, decision workflow limits, and exposure posture.
- CLI `status`, `status --json`, and `doctor` now surface non-loopback/all-interface exposure warnings from effective live daemon bind.
- `status --json` now reports Cockpit cache state as an explicit `[]` when no separate cache paths exist.
- Hook `tool` target `transcript.checkpoint` now reports explicit unsupported failure instead of successful `noop`.
- Loon capability snapshots no longer advertise `structured_events` before a real Loon event stream exists.
- Browser attach API/MCP responses now carry `transport` and `note`; Loon browser attach discloses `loon_attach_proxy`, and Cockpit surfaces the Loon attach/observe limitation in operator-facing copy.
- Cockpit populated-state regression coverage now verifies daemon-derived nav counts/footer and Fleet data flow; responsive CSS guard coverage protects the narrow shell/table/node/log fixes.

## Browser Validation Log

- Main-agent browser smoke opened `http://127.0.0.1:7787/`: title `asylum cockpit`; accessibility snapshot captured; first-run shell visible; nav showed `asylum 0.1.6`, `127.0.0.1:7787`, channels count `1`, hooks count `0`; first-run showed `2 harnesses ready`, `1 substrates configured`, `0 nodes alive`.
- Settings screen browser smoke opened from nav: visible title `settings`; subcopy showed `single-user · bound to 127.0.0.1:7787` and `v0.1.6`; substrates panel showed local healthy.
- A delegated UI validation subagent opened the same URL and confirmed title/topbar/nav/first-run rendering, but observed `0 harnesses ready`, `0 substrates configured`, and `0 nodes alive` after the backend graph had two stopped nodes. It could not capture console/network evidence because of browser tooling limitations in its worker context.
- Main-agent read-only console/network fill-in found no Playwright warning messages, no Chrome console messages, and no failed API requests; repeated health/graph/notifications/channel/hook/substrate/harness API polls returned `200 OK`.
- Cheap browser validation for the node session opened `http://127.0.0.1:7788/` on node `f1a3c615-4170-4919-b6b8-93586c73be98`: title `asylum cockpit`; visible session controls included attach, native attach, session tabs, `tui`/`struct`, and side actions; input rendered as a `textarea rows=3` with `Enter send` and `Shift+Enter newline`; no fake/demo transcript content was visible; no console errors or failed network requests.
- Follow-up delegated validation after session-header controls used temp daemon `127.0.0.1:34990` and real node `a509fed4-188b-49a1-8ce2-e910cac67d4f`; it confirmed backend/static evidence for session chrome controls `browser attach`, `native attach`, `interrupt`, `tui`/`struct`, and the multiline textarea/hints. That worker lacked browser tooling, so it did not capture screenshots/console traces.
- Final delegated terminal/session validation after the stop/activity/session-chrome patch was static-first because Playwright MCP localhost navigation was blocked in the worker environment. It confirmed source evidence for textarea rows/hints, shared attach/native/interrupt handlers in Cockpit/Chat/Node screens, Activity replacing Tools, no hardcoded fake attach transcript, and app-level attach/native/interrupt handlers. No fresh browser screenshot was captured for this final patch.
- Main-agent desktop browser smoke opened `http://127.0.0.1:7800/` against a temp daemon with live node `364c0ea2-5155-4ff6-a5e2-b1b74c55664f` and pending decision `0888a04a-1424-4157-9c7e-800a94b88e87`: title `asylum cockpit`; header showed `1 running`; nav showed nodes `1`, channels `1`, hooks `0`; footer showed daemon-derived `asylum 0.1.6` and `127.0.0.1:7800`; fresh console check returned zero errors/warnings.
- Decisions screen desktop smoke showed `pending 1 / resolved 0`, the `browser decision smoke` pending row, and visible `approve`/`deny` controls. The browser safety reviewer blocked clicking `approve`, so in-browser state mutation was not completed; resolution behavior remains covered by CLI/MCP/API live smoke and regression tests.
- Narrow viewport validation at `390x844` kept the 220px left rail visible and pushed Decisions content/tables off-screen to the right. Console still had zero fresh errors/warnings; this is a layout/responsiveness gap, not a runtime API failure.
- After the responsive CSS patch, main-agent browser smoke opened `http://127.0.0.1:7801/` at `390x844` with live node `8779b8b9-ac36-4cb2-970a-242a856d65c1` and pending decision `d8feac3a-de86-4c78-9e76-a0c856900e7b`: nav stacked full-width above Decisions content, footer showed `asylum 0.1.6` and `127.0.0.1:7801`, pending count was visible, the wide pending/resolved tables were contained as panel overflow instead of pushing the whole main pane offscreen, a viewport screenshot was inspected, and fresh console check returned zero errors/warnings.
- Relationship-management desktop browser smoke opened `http://127.0.0.1:7803/` against an isolated temp daemon with two real local Codex nodes. The Node detail Relationships tab showed explicit graph relationship controls, created a `user_created` edge from command-center `d7516f05-87c4-49d4-9b3f-1362b27866ca` to worker `8c06c267-89ab-453b-a5ef-bc5eee418e53`, displayed the child and edge record, then removed the edge. Playwright network inspection showed `POST /api/relationships` 200, `DELETE /api/relationships/{id}` 204, follow-up `GET /api/graph` 200, and zero fresh console warnings/errors.
- Relationship-management narrow browser validation at `390x844` initially exposed Node detail overflow because the node page still used a desktop two-column layout. A follow-up CSS patch stacked Node main/side panels, wrapped header/meta controls, made tabs horizontally scrollable, and laid out the relationship create form as two columns inside the pane; revalidation showed `nodePageScrollWidth=390`, `nodePageClientWidth=390`, relationship controls fit in the node pane, and zero fresh console warnings/errors.
- Notification workflow browser smoke opened `http://127.0.0.1:7805/` against an isolated temp daemon with one real local Codex node and one node-linked pending-decision notification. Logs showed `all (1)`, `unread (1)`, `1 / 1 events (1 unread)`, state `unread`, node short ID, `open`, and `mark read`. Clicking `mark read` issued `POST /api/notifications/1/read` 204 and refreshed the row/counts to `read` and `0 unread`; clicking `open` navigated to node detail for `ef7db464-badc-44fa-ba03-27171e5f71fa`; fresh console check returned zero errors/warnings.
- Notification workflow narrow validation at `390x844` showed the Logs toolbar wrapping within the viewport, the count line contained in the main pane, and the wider log table contained inside the log frame (`logClientWidth=364`, `logScrollWidth=812`) instead of pushing the page horizontally; fresh console check returned zero errors/warnings.

## Runtime Smoke Evidence

- `curl http://127.0.0.1:7787/api/health` returned daemon `0.1.6`, bind/base URL `127.0.0.1:7787`, temp socket/database paths.
- `curl /api/capabilities` returned node, graph, context, token, channel, hook, recipe, fork, and remote-command descriptors.
- `asylum node create --harness codex --substrate local --role worker --workspace /home/casey/Projects/Asylum` created node `9a60698f-7788-43d0-bd63-ed0d9f0ac9df`.
- `asylum node list` and `/api/nodes/{id}` returned the real Codex local node with `running` liveness and capabilities.
- `/api/nodes/{id}/events` returned ordered `node_started`, `liveness_changed`, real Codex PTY `output_chunk`, `attach_issued`, and `input_sent`.
- Browser attach route returned signed URL with `expires_in_seconds: 600`.
- Native attach command returned `ASYLUM_SOCKET_PATH=/tmp/asylum-spec-audit.XWSs46/run/asylum.sock asylum attach <node>`.
- `asylum node send <node> "audit ping"` recorded `input_sent`.
- `/api/nodes/{id}/fork` created a second real node and explicit `spawned_for` relationship.
- MCP `relationship.remove` deleted the live audit `spawned_for` relationship; `/api/relationships` returned an empty list afterward.
- `asylum node fork 9a60698f-7788-43d0-bd63-ed0d9f0ac9df --description cli-fork-audit` created node `006d70e3-b48a-4220-8c10-182cd639dc26` and relationship `f6374e1f-dde6-428b-a23b-d42caa080ebf`; the forked node was stopped after smoke.
- `asylum graph relationships list` returned the live fork relationship; `remove` deleted it; `create` created relationship `f568b6f2-fcaf-4cfc-97ea-d2f4713e1cab`; `remove` deleted that relationship too.
- Cockpit Relationship tab against temp daemon `127.0.0.1:7803` created and removed a `user_created` relationship between two real local Codex nodes; final `/api/graph` returned `"relationships":[]`.
- Cockpit Logs against temp daemon `127.0.0.1:7805` rendered a real node-linked decision notification, marked it read through `POST /api/notifications/1/read`, and opened the linked node detail.
- Both audit-created nodes were stopped after smoke.
- Manual inbound channel POST to `/api/channels/ntfy-default/inbound` persisted an inbound `channel_messages` row.
- Unauthenticated remote command POST returned `400 {"message":"token required"}`.
- Rebuilt MCP `tools/list` advertised `notify.send`; after the notify fix, MCP `tools/call notify.send` against an unconfigured daemon returns an explicit 503-derived JSON-RPC error.
- A short-lived daemon on `127.0.0.1:7788` with missing `--harness-codex-command` and `--harness-claude-command` returned both harness descriptors with `available:false`.
- A short-lived unconfigured daemon returned `503 {"message":"ntfy notification channel is not configured"}` for `/api/notify/send`.
- `asylum channel list`, `inbound`, and `messages` were live-smoked against `ntfy-default`; `channel create/update/inspect/delete` was live-smoked against a custom webhook channel.
- `asylum hook catalog/create/list/test/firings/delete` was live-smoked against a temp daemon and persisted a synthetic firing.
- `asylum recipe list` against a temp authenticated daemon returned 6 starter recipes; `asylum remote-command status` with `ASYLUM_REMOTE_TOKEN=dev-token` returned `"status":"success"`; without inline remote token it returned `400 token required`; whitespace-bearing `--text` was rejected client-side because the daemon parser is space-delimited.
- `asylum notify list/read`, `workspace recent`, `context system-map`, `recipe spawn start-command-center`, and `context launch-packet <node>` were live-smoked against a temp authenticated daemon; the spawned node was stopped before cleanup.
- MCP `tools/list` and `tools/call` were live-smoked against temp authenticated daemons for `recipe.list`, `recipe.spawn`, `remote_command.send`, `notify.list`, `notify.read`, `workspace.recent`, `context.system_map`, `context.launch_packet`, and `node.stop`.
- Decision API/CLI/MCP live smoke used a temp authenticated daemon: `asylum decision create/list/inspect/approve` moved one decision from `pending` to `approved`; MCP `decision.create/resolve/list` moved another to `denied`.
- Regression tests now cover remote-command decision resolution feedback (`remote_decision_resolution_emits_feedback_events`) and routed inbound failure ordering (`routed_channel_inbound_fails_before_recording_when_node_delivery_fails`).
- Channel routing live smoke against a temp daemon verified explicit `node_id`/`correlation_token` persisted on a `webhook-substrate` inbound message, missing target node returned `400 {"message":"node not found"}` without storing the message, and non-live `discord` inbound returned `400 {"message":"channel 'discord' is not live"}` without storing the message. The fake PTY showed `input_sent` and terminal echo for routed body; canonical route delivery remains covered by the service regression test.
- Channel correlation regression tests cover durable token round-trip/expiry, ntfy marker parsing, and correlated ntfy routing-failure no-persist behavior.
- Authenticated channel remote-command regression tests cover inbound `status token=<valid>` execution plus durable inbound recording, plain `status` without token staying plain with no remote-command fanout, and inbound `approve`/`deny decision=<id> token=<valid>` resolving decisions.
- Live authenticated channel remote-command smoke on `127.0.0.1:7804` used `--owner-tokens-enabled --owner-token dev-token`: `POST /api/channels/webhook-substrate/inbound` with `body="status token=dev-token"` returned 204, `/api/notifications` showed `Remote command received` / `status requested`, a second inbound `body="status"` returned 204 and only appeared as a channel message, and `body="approve decision=c057b701-5a66-433f-8161-46205db6d222 token=dev-token"` resolved a real pending decision to `approved`.
- Loon truthfulness regression tests cover supported Loon harness snapshots not advertising `structured_events` without a real event stream and browser attach responses disclosing `loon_attach_proxy`.
- Local harness decision-ingestion regression tests cover parser chunking/malformed/prose behavior, pending decision/event/notification/waiting liveness creation, resolve-to-running liveness restoration, and manual create not mutating liveness.
- Local harness decision-ingestion live smoke used a fake Codex harness that printed `visible-before`, a `@@asylum:decision.request` marker, and `visible-after`; Asylum created pending decision `6b80cf8d-dca8-4328-91b5-15d0b4eb3335` for node `c7200384-8d25-4a33-bffa-1cff32b1147b`, set liveness to `waiting_for_input`, emitted a `human_input_requested` event and `Decision requested` notification, preserved visible output, and did not leak the control marker into `output_chunk`.
- Launch-context live smoke used a fake Codex harness script under a temp daemon and verified the spawned local PTY process printed the injected `ASYLUM_*` env values, including node ID, role, base URL, socket path, control transport, graph summary, and capabilities JSON.
- Browser validation of the node terminal/session UI used a temp daemon on `127.0.0.1:7788` with real node `f1a3c615-4170-4919-b6b8-93586c73be98`; the temp daemon/state were stopped and removed afterward.
- A follow-up session-header validation used temp daemon `127.0.0.1:34990` and real node `a509fed4-188b-49a1-8ce2-e910cac67d4f`; the temp state was removed by the worker.
- Browser validation of populated Cockpit/Decisions UI used temp daemon `127.0.0.1:7800`, live node `364c0ea2-5155-4ff6-a5e2-b1b74c55664f`, and pending decision `0888a04a-1424-4157-9c7e-800a94b88e87`; desktop rendered real counts and controls with zero fresh console warnings/errors, while 390px viewport exposed fixed-rail overflow.
- Browser validation of the responsive Cockpit follow-up used temp daemon `127.0.0.1:7801`, live node `8779b8b9-ac36-4cb2-970a-242a856d65c1`, and pending decision `d8feac3a-de86-4c78-9e76-a0c856900e7b`; 390px viewport showed stacked nav/main layout and contained table overflow with zero fresh console warnings/errors. The temp daemon/state and Playwright artifacts were removed afterward.
- Cockpit populated-state regression tests cover daemon-derived nav counts/footer and Fleet data flow; responsive CSS guard tests cover the mobile shell, wide table containment, node relationship controls, and notification toolbar selectors.
- Exposure-warning live smoke used a temp daemon bound to `0.0.0.0:7802` with base URL `http://127.0.0.1:7802`; human `status` printed `Warning: HTTP bind 0.0.0.0:7802 listens on all interfaces...`, `status --json` included `network.exposure_warning`, and `doctor` printed `warn network exposure ...`. The temp daemon/state were removed afterward.
- Status JSON cache-shape smoke used `ASYLUM_HOME=/tmp/asylum-status-json-smoke ./target/debug/asylum status --json`; output included `"cockpit": { "caches": [] }`. The temp state was removed afterward.

## Commands Run

```text
git status --short --branch
git rev-parse --short HEAD
git rev-parse --short origin/main
cargo metadata --no-deps --format-version 1
cargo tree -p asylum-cli --depth 1
cargo tree -p asylum-daemon --depth 1
./target/debug/asylum --help
./target/debug/asylum node --help
./target/debug/asylum graph --help
./target/debug/asylum service generate systemd
./target/debug/asylum config show
npm --prefix cockpit run test
npm --prefix cockpit run build
cargo test --workspace
cargo build --workspace
cargo test -p asylum-cli host_state_health_updates_effective_network_bind
cargo test -p asylum-cli
cargo test -p asylum-daemon command_available_reflects_launchable_executable
cargo test -p asylum-daemon harness_descriptors_report_missing_commands_unavailable
cargo test -p asylum-daemon notify_send_errors_when_ntfy_is_unconfigured
cargo test -p asylum-daemon loon_controls_fail_without_configured_target_before_mutating_state
cargo test -p asylum-daemon remote_decision_resolution_emits_feedback_events
cargo test -p asylum-daemon routed_channel_inbound_fails_before_recording_when_node_delivery_fails
cargo test -p asylum-daemon channel_reply_correlation_
cargo test -p asylum-daemon parse_ntfy_reply_marker_
cargo test -p asylum-daemon parser_
cargo test -p asylum-daemon harness_decision_protocol_ingest_records_pending_decision_event_notification_and_waiting_liveness
cargo test -p asylum-daemon resolve_decision_restores_running_from_waiting_for_input
cargo test -p asylum-daemon manual_create_decision_does_not_mutate_liveness
cargo test -p asylum-cli host_state_reports_non_loopback_exposure_warning
cargo test -p asylum-cli host_state_collects_for_empty_runtime
cargo test -p asylum-cli
cargo fmt --check
cargo test -p asylum-daemon transcript_checkpoint_hook_tool_reports_unsupported
npm --prefix cockpit run test
npm --prefix cockpit run build
cargo test -p asylum-cli mcp::tests::tool_definitions_include_expected_names
cargo check -p asylum-cli
cargo build -p asylum
npm --prefix cockpit run test
npm --prefix cockpit run build
cargo test --workspace
cargo test -p asylum-daemon loon_capabilities_do_not_advertise_structured_events_without_event_stream
cargo test -p asylum-daemon loon_browser_attach_response_discloses_transport
npm --prefix cockpit run test -- --run App.test.tsx responsive-css.test.js api.test.ts
cargo fmt --check
cargo test --workspace
npm --prefix cockpit run test -- --run
npm --prefix cockpit run build
ASYLUM_HOME=/tmp/asylum-spec-audit.XWSs46 ./target/debug/asylum setup
ASYLUM_HOME=/tmp/asylum-spec-audit.XWSs46 ./target/debug/asylum daemon run --bind 127.0.0.1:7787 --database /tmp/asylum-spec-audit.XWSs46/asylum.sqlite3 --base-url http://127.0.0.1:7787
curl -fsS http://127.0.0.1:7787/api/health
curl -fsS http://127.0.0.1:7787/api/capabilities
curl -fsS http://127.0.0.1:7787/api/harness-descriptors
curl -fsS http://127.0.0.1:7787/api/substrate-descriptors
curl -fsS http://127.0.0.1:7787/api/channels
curl -fsS http://127.0.0.1:7787/api/hooks/events
curl -fsS -X POST http://127.0.0.1:7787/api/channels/ntfy-default/inbound ...
curl -fsS -X POST http://127.0.0.1:7787/api/notify/send ...
curl -sS -X POST http://127.0.0.1:7787/api/remote-commands ...
printf ... | ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum mcp
printf ... relationship.remove ... | ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum mcp
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum node create ...
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum node list
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum node send ...
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum attach ...
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum node fork ...
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum graph relationships list
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum graph relationships create ...
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum graph relationships remove ...
ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum node stop ...
ASYLUM_HOME=/tmp/asylum-spec-audit.XWSs46 ASYLUM_BASE_URL=http://127.0.0.1:7787 ./target/debug/asylum status --json
ASYLUM_HOME=/tmp/asylum-harness-audit... ./target/debug/asylum daemon run --bind 127.0.0.1:7788 --harness-codex-command ... --harness-claude-command ...
ASYLUM_HOME=/tmp/asylum-notify-audit... ./target/debug/asylum daemon run --bind 127.0.0.1:7788
printf ... notify.send ... | ASYLUM_BASE_URL=http://127.0.0.1:7788 ./target/debug/asylum mcp
ASYLUM_HOME=/tmp/asylum-channel-audit... ./target/debug/asylum daemon run --bind 127.0.0.1:7788
ASYLUM_BASE_URL=http://127.0.0.1:7788 ./target/debug/asylum channel list
ASYLUM_BASE_URL=http://127.0.0.1:7788 ./target/debug/asylum channel inbound ...
ASYLUM_BASE_URL=http://127.0.0.1:7788 ./target/debug/asylum channel messages ...
ASYLUM_BASE_URL=http://127.0.0.1:7788 ./target/debug/asylum channel create/update/inspect/delete ...
ASYLUM_HOME=/tmp/asylum-hook-audit... ./target/debug/asylum daemon run --bind 127.0.0.1:7788
ASYLUM_BASE_URL=http://127.0.0.1:7788 ./target/debug/asylum hook catalog/create/list/test/firings/delete
ASYLUM_HOME=/tmp/asylum-recipe-remote... ./target/debug/asylum daemon run --bind 127.0.0.1:7791 --owner-token dev-token
ASYLUM_BASE_URL=http://127.0.0.1:7791 ASYLUM_TOKEN=dev-token ./target/debug/asylum recipe list
ASYLUM_BASE_URL=http://127.0.0.1:7791 ASYLUM_TOKEN=dev-token ASYLUM_REMOTE_TOKEN=dev-token ./target/debug/asylum remote-command status
ASYLUM_BASE_URL=http://127.0.0.1:7791 ASYLUM_TOKEN=dev-token ./target/debug/asylum remote-command status
ASYLUM_BASE_URL=http://127.0.0.1:7791 ASYLUM_TOKEN=dev-token ./target/debug/asylum remote-command send --node ... --text "hello world"
ASYLUM_HOME=/tmp/asylum-cli-context... ./target/debug/asylum daemon run --bind 127.0.0.1:7792 --owner-token dev-token
ASYLUM_BASE_URL=http://127.0.0.1:7792 ASYLUM_TOKEN=dev-token ./target/debug/asylum notify list
ASYLUM_BASE_URL=http://127.0.0.1:7792 ASYLUM_TOKEN=dev-token ./target/debug/asylum notify read ...
ASYLUM_BASE_URL=http://127.0.0.1:7792 ASYLUM_TOKEN=dev-token ./target/debug/asylum workspace recent
ASYLUM_BASE_URL=http://127.0.0.1:7792 ASYLUM_TOKEN=dev-token ./target/debug/asylum context system-map
ASYLUM_BASE_URL=http://127.0.0.1:7792 ASYLUM_TOKEN=dev-token ./target/debug/asylum context launch-packet ...
ASYLUM_HOME=/tmp/asylum-mcp-parity... ./target/debug/asylum daemon run --bind 127.0.0.1:7793 --owner-token dev-token
printf ... tools/list, recipe.list, remote_command.send, recipe.spawn, context.launch_packet, node.stop ... | ASYLUM_BASE_URL=http://127.0.0.1:7793 ASYLUM_TOKEN=dev-token ./target/debug/asylum mcp
ASYLUM_HOME=/tmp/asylum-mcp-notify... ./target/debug/asylum daemon run --bind 127.0.0.1:7794 --owner-token dev-token
printf ... remote_command.send, notify.list, notify.read ... | ASYLUM_BASE_URL=http://127.0.0.1:7794 ASYLUM_TOKEN=dev-token ./target/debug/asylum mcp
ASYLUM_HOME=/tmp/asylum-decision-smoke... ./target/debug/asylum daemon run --bind 127.0.0.1:7795 --owner-token dev-token
ASYLUM_BASE_URL=http://127.0.0.1:7795 ASYLUM_TOKEN=dev-token ./target/debug/asylum decision create/list/inspect/approve ...
printf ... decision.create, decision.resolve, decision.list ... | ASYLUM_BASE_URL=http://127.0.0.1:7795 ASYLUM_TOKEN=dev-token ./target/debug/asylum mcp
curl -fsS -X POST http://127.0.0.1:7797/api/channels/webhook-substrate/inbound ... node_id/correlation_token ...
curl -sS -X POST http://127.0.0.1:7797/api/channels/webhook-substrate/inbound ... missing node ...
curl -sS -X POST http://127.0.0.1:7797/api/channels/discord/inbound ...
ASYLUM_HOME=/tmp/asylum-decision-ingest... ./target/debug/asylum daemon run --bind 127.0.0.1:7799 --harness-codex-command /tmp/asylum-decision-ingest-harness.sh
curl -fsS -X POST http://127.0.0.1:7799/api/nodes ...
curl -fsS http://127.0.0.1:7799/api/decisions
curl -fsS http://127.0.0.1:7799/api/nodes/<node>/events
ASYLUM_HOME=/tmp/asylum-launch-env... ./target/debug/asylum daemon run --bind 127.0.0.1:7796 --harness-codex-command /tmp/asylum-launch-env.../fake-harness.sh
ASYLUM_BASE_URL=http://127.0.0.1:7796 ./target/debug/asylum node create --harness codex --substrate local --role worker --workspace /home/casey/Projects/Asylum
ASYLUM_HOME=/tmp/asylum-terminal-ui... ./target/debug/asylum daemon run --bind 127.0.0.1:7788
ASYLUM_BASE_URL=http://127.0.0.1:7788 ./target/debug/asylum node create --harness codex --substrate local --role command-center ...
ASYLUM_HOME=/tmp/asylum-ui-... ./target/debug/asylum daemon run --bind 127.0.0.1:34990
ASYLUM_BASE_URL=http://127.0.0.1:34990 ./target/debug/asylum node create --harness codex --substrate local --role worker ...
ASYLUM_HOME=/tmp/asylum-browser-smoke-dir ./target/debug/asylum daemon run --bind 127.0.0.1:7800 --database /tmp/asylum-browser-smoke-dir/asylum.sqlite3 --socket-path /tmp/asylum-browser-smoke-dir/asylum.sock --base-url http://127.0.0.1:7800 --harness-codex-command /tmp/asylum-browser-smoke-harness.sh
ASYLUM_BASE_URL=http://127.0.0.1:7800 ./target/debug/asylum node create --harness codex --substrate local --role worker --workspace /home/casey/Projects/Asylum
curl -fsS -X POST http://127.0.0.1:7800/api/decisions ...
Playwright: navigate http://127.0.0.1:7800/, inspect desktop snapshot, click Decisions nav, inspect pending decision, check fresh console warnings/errors, resize to 390x844 and inspect narrow layout.
ASYLUM_HOME=/tmp/asylum-responsive-smoke ./target/debug/asylum setup
ASYLUM_HOME=/tmp/asylum-responsive-smoke ./target/debug/asylum daemon run --bind 127.0.0.1:7801 --database /tmp/asylum-responsive-smoke/asylum.sqlite3 --socket-path /tmp/asylum-responsive-smoke/asylum.sock --base-url http://127.0.0.1:7801
ASYLUM_BASE_URL=http://127.0.0.1:7801 ./target/debug/asylum node create --harness codex --substrate local --role worker --workspace /home/casey/Projects/Asylum
curl -fsS -X POST http://127.0.0.1:7801/api/decisions ...
Playwright: navigate http://127.0.0.1:7801/, resize to 390x844, click Decisions nav, inspect snapshot/screenshot, check fresh console warnings/errors.
ASYLUM_HOME=/tmp/asylum-exposure-smoke ./target/debug/asylum setup
ASYLUM_HOME=/tmp/asylum-exposure-smoke ./target/debug/asylum daemon run --bind 0.0.0.0:7802 --database /tmp/asylum-exposure-smoke/asylum.sqlite3 --socket-path /tmp/asylum-exposure-smoke/asylum.sock --base-url http://127.0.0.1:7802
ASYLUM_HOME=/tmp/asylum-exposure-smoke ASYLUM_BASE_URL=http://127.0.0.1:7802 ./target/debug/asylum status
ASYLUM_HOME=/tmp/asylum-exposure-smoke ASYLUM_BASE_URL=http://127.0.0.1:7802 ./target/debug/asylum status --json
ASYLUM_HOME=/tmp/asylum-exposure-smoke ASYLUM_BASE_URL=http://127.0.0.1:7802 ./target/debug/asylum doctor
ASYLUM_HOME=/tmp/asylum-status-json-smoke ./target/debug/asylum setup
ASYLUM_HOME=/tmp/asylum-status-json-smoke ./target/debug/asylum status --json
```

## Completion Audit Checkpoint

Objective restated: complete an evidence-backed repo-vs-current-spec audit, keep this findings file current, validate real behavior including Cockpit browser paths, begin/complete well-scoped fixes, and leave unresolved product gaps as explicit backlog instead of marking unsupported behavior as shipped.

Prompt-to-artifact checklist:

| Requirement | Evidence | Status |
|---|---|---|
| Verify starting checkpoint | Start Check records `main`, `HEAD=f14a7c4`, `origin/main=f14a7c4`, and pre-existing untracked goal doc. | complete |
| Create and continuously update findings report | This file now contains the coverage matrix, findings, fix log, browser log, runtime evidence, commands, and follow-on queue. | complete |
| Cover major spec areas | Matrix covers architecture, transport/lifecycle/data/nodes/graph/harness/substrate/capability/CLI/MCP/Cockpit/channels/notify/hooks/decisions/security/prototype/docs. | complete |
| Validate real behavior, not mocks | Runtime Smoke Evidence lists live daemons, CLI/MCP/API calls, browser smokes, and targeted regression tests. Remaining Loon-native decision ingestion is explicitly not counted as implemented. | complete |
| Cockpit browser validation | Browser Validation Log records desktop, node-session, Decisions, Cockpit relationship-management create/remove, notification mark-read/open-node workflow, and 390px responsive checks plus console status and blocker where browser state mutation was blocked. Populated-state and responsive guard tests now backstop the browser findings. | complete for audit |
| Start/complete well-scoped fixes | Fixes Completed section lists CLI/MCP/API/Cockpit decisions, Cockpit relationship management, Cockpit notification operator workflow, authenticated inbound remote-command envelopes, channel routing/correlation, local decision ingestion, terminal/session UX, status/doctor exposure warnings, status JSON cache shape, telemetry estimate labels, and hook unsupported-tool honesty. | complete |
| Explicit unresolved backlog | Follow-On Fix Queue plus the remaining `partial` row list Loon/substrate-native decision ingestion as blocked on a real event source. | complete |
| Release handling | Release status says doc-only/internal with latest published v0.1.6; no release was cut. | complete |
| Final verification | `cargo fmt --check`, `cargo test --workspace`, `npm --prefix cockpit run test -- --run`, and `npm --prefix cockpit run build` passed after the final fixes. | complete |

Completion decision: **achieved for this audit goal**. The current spec coverage matrix is complete and evidence-backed, Cockpit has live browser validation recorded, well-scoped fixes were completed across daemon/CLI/MCP/Cockpit/docs/tests, and the only remaining partial row is an explicit product backlog item blocked on a real Loon event source rather than an in-repo follow-on fix.

## Follow-On Fix Queue

1. Add Loon/substrate-native decision ingestion once a real event source exists.
2. Continue Cockpit terminal/session prototype alignment: rail workflows and richer event-backed activity when daemon emits tool events.
3. Consider a Playwright e2e suite for populated desktop and responsive Cockpit workflows if the project chooses to carry that dependency.
