# Asylum Current Product Spec

**Status:** canonical current product contract
**Date:** 2026-05-05
**Release context:** latest published release is tracked in [RELEASES.md](../../RELEASES.md). At the time this spec was last cleaned up, the latest published release was `v0.1.10`.

## Purpose

This document defines what Asylum is supposed to be and do as a current deliverable product. It is the contract an agent should audit against when comparing the implementation, Cockpit UI, CLI, daemon, MCP bridge, install path, and docs to the intended product.

This is not an implementation plan and not a gap report. The implementation may fall short of this spec; those shortfalls belong in a separate coverage audit.

## How To Read This Spec

- Requirement IDs are stable handles for audits and implementation planning.
- "Acceptance" describes the observable behavior that proves the requirement.
- Prototype material under `cockpit/prototype/` is intent evidence for Cockpit behavior, layout, product feel, and workflows. Mock data, simulations, edit-mode controls, Tweaks panels, canned responses, fake node IDs, fake settings, and other Claude Design prototype mechanics are not product requirements.
- User-facing runtime behavior must be real. Test fixtures and unit-test mocks are allowed; shipped daemon and Cockpit behavior must not be simulated, mocked, stubbed, hardcoded as fake real data, or demo-only.

## Source Authority

This spec is the consolidated product contract. It replaces older PRDs,
handoffs, reviews, and dated implementation plans as the live source of truth.
Those older artifacts are not kept as live docs on this branch; recover them
from git history only when historical reconstruction is needed.

This spec should stay aligned with:

- Current release truth from [RELEASES.md](../../RELEASES.md).
- Current agent/product principles from [AGENTS.md](../../AGENTS.md).
- Cockpit intent from [cockpit/prototype/README.md](../../cockpit/prototype/README.md) where it describes product feel rather than prototype mechanics.
- Current implementation surfaces in `crates/`, `cockpit/src/`, and `scripts/`.

## Product Definition

Asylum is a single-user, always-on control plane for real agent harness sessions. It does not replace Codex, Claude Code, Pi, Hermes, Loon, or any other harness/substrate. It launches harnesses, gives them shared context and capabilities, observes them, lets humans open live node sessions and intervene, and lets harnesses coordinate with each other through a common daemon-owned capability surface.

The core product object is the **Node**: a live or resumable harness session running on a substrate. A node can have a role hint such as `command-center`, `supervisor`, `worker`, `evaluator`, or `assistant`, but role hints are not workflow states.

Asylum provides control, visibility, durable coordination, and reachable interfaces. Harnesses provide intelligence.

## Current Product Boundary

### Goals

- Provide one installable local product command, `asylum`.
- Run a resident Asylum daemon that owns all live behavior.
- Support real Codex and Claude Code harness sessions.
- Support local execution and configured Loon-backed execution.
- Make the graph of nodes the primary mental model.
- Provide Cockpit as the primary first-party UI.
- Provide CLI, HTTP/WebSocket, MCP, hooks, notifications, and remote-command entrypoints over the same root capabilities.
- Let humans open node sessions, inspect, send input, interrupt, stop, archive, fork, and relate nodes.
- Let nodes create and coordinate peer nodes through the same daemon-owned capabilities operators use.
- Let nodes and operators send notifications, receive inbound channel messages, and route explicit remote commands.
- Keep Asylum single-user, localhost-first, and Loon-independent.

### Non-Goals

- No hosted SaaS control plane.
- No multi-tenant organization model.
- No team RBAC.
- No mandatory task/run/workflow state machine.
- No replacement agent brain owned by Asylum.
- No inferred graph edges presented as real responsibility edges.
- No hard dependency on Loon for local use.
- No private powers in one client that bypass the root capability surface.

## Terminology

| Term | Meaning |
|---|---|
| Asylum | Product name. |
| `asylum` | The one installed executable and terminal command. |
| Asylum daemon | Long-running resident process that owns nodes, storage, transports, Cockpit service, hooks, channels, and harness/substrate control. |
| Asylum service | The systemd/launchd/pid-fallback managed instance of the daemon. |
| Asylum CLI | Terminal interface implemented by `asylum-cli` and shipped through the `asylum` binary. |
| Cockpit | Browser UI client served by the daemon. |
| Node | Durable live/resumable harness session record. |
| Harness | Agent runtime such as Codex or Claude Code. |
| Substrate | Place where a node runs: local or Loon-backed. |
| Root capability | A daemon-owned operation exposed consistently to clients. |
| Channel | A notification or command transport such as ntfy or webhook. |
| Hook | Declarative event trigger with daemon-executed actions. |

"Server" is not the primary product term. HTTP is a Cockpit and remote transport detail of the daemon.

## Architecture

### Component Model

```text
                                 one installed executable: asylum

  CLI / MCP / lifecycle  ---------------------+
                                              |
  Cockpit browser UI  --------------------+   |
                                           |   |
  ntfy / hooks / remote commands  -----+   |   |
                                       |   |   |
                                       v   v   v
                              Asylum daemon capabilities
                       nodes / graph / session transport / channels / hooks
                                      / storage / auth
                                       |
                                       v
                         harness adapters and substrate adapters
                         Codex / Claude Code on local / Loon
```

### Crate Layout Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| ARCH-001 | The workspace has four Rust crates: `asylum`, `asylum-cli`, `asylum-daemon`, and `asylum-types`. | `cargo metadata` and `Cargo.toml` show exactly this product crate split. |
| ARCH-002 | `crates/asylum` is the only binary crate and builds the installed executable named `asylum`. | Release archives contain one executable named `asylum`; no separate `asylum-daemon` binary ships. |
| ARCH-003 | `crates/asylum` is a tiny composition crate. | `main.rs` only initializes process-level concerns, parses top-level action through `asylum-cli`, dispatches CLI actions to `asylum-cli`, and dispatches `asylum daemon run` to `asylum-daemon`. |
| ARCH-004 | `asylum-cli` and `asylum-daemon` do not depend on each other. | `cargo tree -p asylum-cli` has no `asylum-daemon`; `cargo tree -p asylum-daemon` has no `asylum-cli`. |
| ARCH-005 | `asylum-types` contains shared data contracts only. | It contains request/response DTOs, node/event/relationship/config/security/capability shapes, and pure helpers; it does not perform filesystem, network, SQLite, process, async runtime, or service-manager work. |
| ARCH-006 | The daemon owns all live Asylum behavior. | CLI, Cockpit, MCP, hooks, notifications, and remote commands call daemon capabilities; they do not reimplement node launch/control/storage semantics. |
| ARCH-007 | Cockpit stays a top-level TypeScript/React app under `cockpit/`. | Cockpit talks to daemon HTTP/WebSocket routes and does not import Rust crates. |

### Transport Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| TRANSPORT-001 | CLI and MCP use the local Unix socket for ordinary local daemon control on Unix platforms. | Default local control path is `~/.asylum/run/asylum.sock`; `ASYLUM_SOCKET_PATH` overrides it. |
| TRANSPORT-002 | Cockpit uses daemon HTTP/WebSocket routes. | The daemon serves `/`, `/assets/*`, `/api/...`, `/api/nodes/{id}/observe/ws`, and `/api/attach/{token}/ws`. |
| TRANSPORT-003 | HTTP is not the conceptual center of the product. | User-facing terminology and docs call the resident process the daemon/service, not the server, except when describing HTTP transport mechanics. |
| TRANSPORT-004 | Socket access is local and filesystem-protected. | The daemon creates the run directory with owner-only permissions where supported, unlinks stale socket files, and binds a socket file with owner-only permissions where supported. |
| TRANSPORT-005 | HTTP auth and socket auth boundaries are explicit and separate. | Socket requests may bypass bearer auth because filesystem permissions are the local boundary; HTTP routes still enforce owner-token auth when auth is enabled. |
| TRANSPORT-006 | Remote exposure is explicit. | Binding beyond localhost, exposing Cockpit, or using remote channels requires explicit config and must surface a visible warning in operator UX/docs. |

## Runtime, Install, And Lifecycle

| ID | Requirement | Acceptance |
|---|---|---|
| LIFE-001 | Asylum ships as a single-binary install. | The installer places one `asylum` binary in the install dir. |
| LIFE-002 | Bare `asylum` is the product bootstrap path. | If runtime state is missing, it explains setup and next steps; once set up, it starts or verifies the service and opens Cockpit. |
| LIFE-003 | `asylum setup` is idempotent. | It creates `~/.asylum/`, `config.toml`, `logs/`, `run/`, and default config when needed; it preserves existing user config/state. |
| LIFE-004 | Runtime state lives under `~/.asylum/` by default. | Default paths are `~/.asylum/config.toml`, `~/.asylum/asylum.sqlite3`, `~/.asylum/run/asylum.sock`, `~/.asylum/run/asylum.pid`, and `~/.asylum/logs/asylum.log`. |
| LIFE-005 | `ASYLUM_HOME`, `ASYLUM_CONFIG`, `ASYLUM_DATABASE`, `ASYLUM_SOCKET_PATH`, and daemon-specific env overrides are respected. | CLI and daemon compute consistent runtime paths and expose effective values through health/status. |
| LIFE-006 | `asylum daemon run` is the foreground daemon entrypoint. | Service templates, pid fallback, dev commands, and docs use `asylum daemon run`; `asylum serve` does not exist. |
| LIFE-007 | `asylum start`, `stop`, `restart`, `status`, `doctor`, `logs`, `update`, and `uninstall` are first-class CLI lifecycle commands. | Each command is discoverable in `asylum --help`, uses shared host/runtime inspection where applicable, and reports actionable state. |
| LIFE-008 | Service management uses launchd on macOS, systemd user services on Linux when available, and pid fallback otherwise. | `asylum start` selects the best backend; generated/installed units invoke `asylum daemon run --config <path>`. |
| LIFE-009 | `asylum service generate systemd|launchd` emits service definitions without installing them. | The command name does not claim to install; old `asylum install systemd|launchd` forms are absent. |
| LIFE-010 | `asylum status --json` exposes machine-readable host state. | Output has a schema version and includes binary, runtime dir, config dir, daemon, service unit, Cockpit cache, and network state. |
| LIFE-011 | `asylum update` refreshes the installed binary and installed service definition. | After update, service units do not retain stale command shapes, and post-update doctor runs through the newly installed binary. |
| LIFE-012 | `asylum uninstall` has plan/confirm/execute semantics. | It can dry-run, preserve state/config/logs, purge config only with typed confirmation, stop the daemon, remove service unit, remove the installed binary, and report what remains. |
| LIFE-013 | Release state is explicit and manual. | `RELEASES.md` distinguishes "on main" from "published"; docs-only changes normally do not require a release. |

## Data Model And Persistence

| ID | Requirement | Acceptance |
|---|---|---|
| DATA-001 | SQLite is the durable local store. | Nodes, events, transcript chunks, relationships, artifacts, tokens, remote commands, notifications, decisions, channels, channel messages, hooks, and hook firings persist in the database. |
| DATA-002 | Node records are the central durable objects. | A node has ID, harness, substrate, role hint, liveness, workspace, description, timestamps, external substrate ID, capability snapshot, and telemetry fields. |
| DATA-003 | Node liveness values are explicit. | Wire values include `starting`, `running`, `waiting_for_input`, `exited`, `stopped`, `failed`, and `archived`. |
| DATA-004 | Events are durable and ordered per node. | Node events carry ID, node ID, sequence, kind, body, timestamp, and schema version. |
| DATA-005 | Transcript/output storage records real harness output. | Output chunks come from harness/substrate output or explicit input/control events, not from canned UI sequences. |
| DATA-006 | Telemetry shown in UI is honest about source. | Native metrics may be shown when available; estimates derived from event text must be treated as estimates, not claimed as harness-native truth. |
| DATA-007 | Secrets are not stored or rendered casually. | Tokens and attach secrets are stored hashed or redacted where practical; raw issued tokens are shown only at issuance/rotation moments. |

## Nodes, Graph, And Relationships

| ID | Requirement | Surfaces | Acceptance |
|---|---|---|---|
| NODE-001 | Operators can create real nodes. | CLI, API, MCP, Cockpit | Creating a node launches a real configured harness on a supported substrate or returns a clear error. |
| NODE-002 | Operators can list and inspect nodes. | CLI, API, MCP, Cockpit | List/inspect return durable node records with liveness, capabilities, workspace, telemetry, and substrate/harness identity. |
| NODE-003 | Operators can send input to running nodes. | CLI, API, MCP, Cockpit, remote command | Input reaches the harness stdin/control path and records an input event. |
| NODE-004 | Operators can interrupt nodes. | CLI, API, MCP, Cockpit, remote command, hooks | Interrupt reaches the substrate when supported and records a liveness/control event. Unsupported cases return honest errors. |
| NODE-005 | Operators can stop nodes. | CLI, API, MCP, Cockpit, remote command | Stop terminates or requests termination through the substrate and updates liveness. |
| NODE-006 | Operators can archive nodes. | CLI, API, MCP, Cockpit, hooks | Archive stops active runtime when possible, marks the node archived, and preserves durable record/transcript references. |
| NODE-007 | Operators can fork nodes. | CLI, API, MCP, Cockpit | Fork creates a real node inheriting harness/substrate/workspace defaults and creates an explicit relationship to the source node. |
| NODE-008 | Operators can observe node events and live output. | API, WebSocket, Cockpit | Historical events stream first; live output streams where substrate supports it; unsupported live stream paths say so clearly. |
| NODE-009 | Signed browser session transport works for compatible clients. | API, MCP, remote command | Issued session URLs are signed, time-limited, verify before opening, and stream real I/O over the daemon attach transport. |
| NODE-010 | Native attach target remains available for CLI/API compatibility. | CLI, API | Native target returns command, args, and env needed to connect to the same daemon/node. Cockpit does not expose this as a normal workflow. |
| NODE-011 | Node capabilities are visible and honest. | API, Cockpit, MCP | Capability flags describe what this harness/substrate can do now. Missing optional capabilities degrade gracefully. |
| NODE-012 | Command-center is a normal real node with a role hint. | Cockpit, CLI, API, MCP | Launching a command center creates a real Codex/Claude node with `role_hint=command-center`, appears in the graph, and uses the same input/observe/session controls as other nodes. |
| NODE-013 | Nodes can spawn peer nodes through Asylum. | API, CLI, MCP | A running node with Asylum MCP access can create a real peer node, inherit sane defaults from the source node, and record an explicit relationship such as `spawned_for`. |
| GRAPH-001 | The graph shows explicit relationships only. | API, Cockpit, MCP | Relationships in graph responses come from stored relationship records, not inferred workspace/substrate correlation. |
| GRAPH-002 | Operators can create, list, and remove relationships. | CLI, API, MCP, Cockpit | Relationship kinds include `supervises`, `spawned_for`, `user_created`, and `platform_responsibility`; invalid kinds are rejected. |
| GRAPH-003 | Correlations are distinct from edges. | Cockpit, API | Same workspace, same substrate, same harness, and similar metadata may be filters/groups/facts but not graph edges. |

## Harnesses And Substrates

| ID | Requirement | Acceptance |
|---|---|---|
| HARN-001 | Codex and Claude Code are supported harnesses. | Config exposes the command for each; descriptors show availability/capabilities; create-node can launch both when commands exist. |
| HARN-002 | Harness adapters are real process/control adapters, not simulations. | Local launch starts the configured CLI process in a PTY; Loon launch uses Loon control contracts. |
| HARN-003 | Launch context is Asylum-aware. | New nodes receive node ID, role hint, graph/capability context, session/control instructions, and relevant workspace/substrate metadata in the best supported way for that harness/substrate. Local Codex and Claude Code launches also receive per-process Asylum MCP configuration so they can call daemon capabilities directly. |
| HARN-004 | Optional harness capabilities are advertised per adapter. | Structured events, tool-call telemetry, transcript export, native resume, subagent visibility, permission prompts, and context telemetry are shown only when actually supported. |
| SUB-001 | Local substrate supports real PTY launch/control. | Local nodes can launch, stream output, receive input, interrupt, stop, and connect through browser/native compatibility paths. |
| SUB-002 | Loon is optional and independent. | Asylum works without Loon; enabling Loon uses configured endpoint/CLI/auth/cert settings without coupling Asylum core to Loon internals. |
| SUB-003 | Loon-backed nodes use the documented Loon CLI/control contract. | Launch/input/interrupt/stop/archive/session relay use `loon` operations or return clear errors when unavailable. |
| SUB-004 | Loon health/capacity is visible. | Cockpit/API show Loon status, running count, supported harness profiles, and honest unsupported capability flags. |
| SUB-005 | Loon-backed session relay and observe semantics are honest. | If browser session relay differs from local PTY output, UI/API say so rather than pretending parity. |

## Root Capabilities And API

| ID | Requirement | Acceptance |
|---|---|---|
| CAP-001 | Every product affordance maps to a daemon-owned root capability. | Capability descriptors list the endpoint, method/transport, description, and availability. |
| CAP-002 | Capability semantics are shared across clients. | CLI, MCP, Cockpit, hooks, notifications, and remote commands call the same daemon behavior rather than parallel implementations. |
| CAP-003 | The HTTP API exposes typed JSON contracts. | Routes return stable request/response shapes from `asylum-types` or Cockpit-mirrored equivalents. |
| CAP-004 | Core node capabilities exist. | `node.create`, `node.spawn_peer`, `node.list`, `node.inspect`, `node.observe`, `node.events`, `node.send_input`, `node.interrupt`, `node.stop`, `node.archive`, `node.fork`, `node.attach.browser`, and `node.attach.native_target` are implemented or explicitly unavailable per node. |
| CAP-005 | Graph capabilities exist. | `graph.get`, `relationship.create`, `relationship.list`, and `relationship.remove` are available. |
| CAP-006 | Harness/substrate descriptor capabilities exist. | Clients can list harnesses, substrates, descriptors, health, and capability flags. |
| CAP-007 | Context capabilities exist. | Clients can read current system map/graph and generate launch packets for nodes. |
| CAP-008 | Notification/channel capabilities exist. | Clients can list/create/update/delete channels, list messages, send test messages, record inbound messages, and send notifications. |
| CAP-009 | Hook capabilities exist. | Clients can list/create/update/delete hooks, list event catalog, dry-run hooks, and inspect firings. |
| CAP-010 | Token capabilities exist where safe. | API/CLI can issue, list, revoke, and rotate owner tokens; MCP does not expose token management. |
| CAP-011 | Remote command capabilities exist. | Authenticated remote commands can request status, issue signed session URLs, send input, start nodes, interrupt, stop, approve, and deny decisions. |
| CAP-012 | Unsupported capabilities fail clearly. | A missing harness/substrate/channel feature returns an explicit unsupported/unavailable error and is not hidden behind a successful no-op. |

## CLI Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| CLI-001 | The CLI is the primary local operator interface. | A user can operate Asylum from `asylum` without needing Cockpit. |
| CLI-002 | CLI lifecycle commands are complete. | `setup`, `cockpit`, `start`, `stop`, `restart`, `status`, `doctor`, `logs`, `update`, `uninstall`, `daemon run`, `config init`, `config show`, and `service generate` work and are documented in help. |
| CLI-003 | CLI node commands cover core node operations. | Create/spawn/fork/list/inspect/send/interrupt/stop/archive and session compatibility commands work through daemon capabilities. |
| CLI-004 | CLI graph commands expose graph state and relationship management. | `graph get` and relationship create/list/remove commands work through daemon capabilities. |
| CLI-005 | CLI token and notification commands exist. | Operators can issue tokens and send notifications from terminal. |
| CLI-006 | CLI can reach all root capabilities practical for a terminal. | Terminal commands exist for channels, hooks, recipes, relationships, fork, and remote commands, or the capability has a clear terminal-inapplicable rationale in this spec. |
| CLI-007 | CLI output is useful for both humans and automation. | Human output is concise; JSON output exists for status and other automation-sensitive commands where practical. |
| CLI-008 | CLI uses socket transport for local daemon control by default. | Local commands do not require HTTP bearer tokens unless explicitly configured for HTTP remote control. |

## MCP Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| MCP-001 | `asylum mcp` is a stdio JSON-RPC MCP bridge into the daemon. | It initializes, lists tools, handles notifications correctly, and calls daemon capabilities. |
| MCP-002 | MCP exposes core node and graph capabilities. | Required tools include `node.create`, `node.spawn_peer`, `node.list`, `node.inspect`, `node.send_input`, `node.interrupt`, `node.stop`, `node.archive`, `node.events`, `node.fork`, `attach_url.issue`, `graph.get`, `relationship.create`, and `relationship.list`. |
| MCP-003 | MCP exposes safe automation capabilities. | Required tools include `hook.list`, `hook.create`, `hook.delete`, `hook.firings`, `channel.list`, `notify.send`, and `health.get`. |
| MCP-004 | MCP does not expose token management by default. | Token issuance/revocation stays out of MCP unless a separate security review changes this spec. |
| MCP-005 | MCP tool names and routes match daemon capabilities. | Tool handlers call real daemon routes; no MCP tool may point at a non-existent or wrong endpoint. |

## Cockpit Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| COCKPIT-001 | Cockpit is the primary first-party UI. | Bare `asylum`/`asylum cockpit` opens it after daemon health is ready. |
| COCKPIT-002 | Cockpit opens graph-first. | Default view shows graph, inline session/command-center area, selected-node inspector, graph controls, and real live counts. |
| COCKPIT-003 | Cockpit supports first-run empty state. | With zero nodes, it explains the product succinctly and offers launching a real command-center node. |
| COCKPIT-004 | Cockpit launch flow creates real nodes. | Harness/substrate/role/workspace/prompt/recipe choices call daemon APIs and display errors honestly. |
| COCKPIT-005 | Cockpit command-center chat is a real node session. | The inline panel sends input to the selected command-center node and observes its output/events; it is not a custom chatbot. |
| COCKPIT-006 | Cockpit can focus any node session. | Selecting a graph/table/chat rail node focuses its real session and can show metadata, events, capabilities, and relationships without a separate attach action. |
| COCKPIT-007 | Cockpit graph layouts are usable and truthful. | Tree/free/force/swimlane layouts are derived from real node/relationship/substrate data and support pan/zoom. |
| COCKPIT-008 | Cockpit fleet table is a secondary dense node view. | It supports search/filtering/sorting-style inspection over real node records. |
| COCKPIT-009 | Cockpit node detail has real tabs. | Session, events, capabilities, relationships, and telemetry tabs display daemon-backed data or honest empty/unsupported states. |
| COCKPIT-010 | Cockpit controls call real capabilities. | Send input, interrupt, stop, fork, archive, relationship actions, and recipe actions call daemon APIs and surface errors. Cockpit does not expose attach as a normal user workflow. |
| COCKPIT-011 | Cockpit logs show real events/notifications. | Logs/telemetry view uses daemon notifications/events and does not claim a unified stream unless backed by one. |
| COCKPIT-012 | Cockpit channels screen is real. | Channel CRUD, message history, test send, inbound webhook/manual messages, and subscribe details use daemon endpoints. |
| COCKPIT-013 | Cockpit hooks screen is real. | Hook CRUD, enable/disable, dry run, event catalog, actions, and firing history use daemon endpoints. |
| COCKPIT-014 | Cockpit settings are real. | Settings display daemon health, version, bind/base URL, database/storage paths and sizes, harness/substrate descriptors, ntfy channels, and token state from APIs. |
| COCKPIT-015 | Cockpit command palette uses real navigation/actions. | Cmd-K can navigate screens, find nodes, launch nodes, and send remote commands without fake action paths. |
| COCKPIT-016 | Cockpit auth token handling is not persistent browser storage. | Owner token can be hydrated from URL or prompt, held in memory, and stripped from URL after hydration. |
| COCKPIT-017 | Cockpit contains no prototype mechanics. | No Tweaks panel, `simSpeed`, canned `runResponse`, hardcoded demo nodes, fake settings, fake logs, fake session preview output, visible attach workflow, or no-op buttons ship in `cockpit/src`. |
| COCKPIT-018 | Cockpit visual design follows the prototype intent without inheriting prototype data. | It preserves the graph-first layout, compact operational style, mono terminal feel, node inspector, command-center/session focus, and channels/hooks concepts using real data. |

## Channels, Notifications, Remote Commands, And Hooks

| ID | Requirement | Acceptance |
|---|---|---|
| CHAN-001 | ntfy is the baseline notification channel. | Configured ntfy can send outbound notifications and subscribe to inbound JSON stream messages. |
| CHAN-002 | Inbound channel messages are durable. | Inbound ntfy/webhook/manual messages create `channel_messages` records with direction `in` and fire `channel.inbound` hook events. |
| CHAN-003 | Channel messages can target nodes when appropriate. | Reply correlation or explicit node addressing can associate inbound messages with the intended node. |
| CHAN-004 | Remote replies can control Asylum. | Authenticated inbound/remote commands can request status, request signed session links, send input, start nodes, interrupt, stop, approve, and deny decisions. |
| CHAN-005 | Raw node replies are routed without Rust-side intelligence. | When a message is correlated to a node as input, Asylum routes bytes/text to the node and lets the harness interpret meaning. |
| CHAN-006 | Channel screens are honest about unsupported adapters. | SMS, Discord, Slack, email, or other adapters may appear only as clearly non-live/not-configured entries unless backed by real daemon behavior. |
| NOTIFY-001 | Notifications are durable and readable. | Notifications can be listed, marked read, displayed in Cockpit, and associated with nodes when applicable. |
| HOOK-001 | Hooks are daemon-executed event rules. | Hooks match event name plus filter, then execute supported actions and record firings. |
| HOOK-002 | Hook filters fail closed. | Parse errors block the event and record/log the failure. |
| HOOK-003 | Hook actions call real capabilities. | `channel`, `spawn`, `tool`, `pause_node`, and `archive` actions call daemon behavior or return explicit unsupported errors. |
| HOOK-004 | Hook dry-run is visibly synthetic. | Dry-run/test behavior may use synthetic payloads only when clearly marked and stored as test firings. |
| DECISION-001 | Human decision requests are first-class current product behavior. | Harness/substrate events can create decision records tied to nodes and optionally notifications/channels. |
| DECISION-002 | Decisions are surfaced to operators. | Cockpit, notifications, and remote commands can show pending decisions with context and allowed actions. |
| DECISION-003 | Decisions can be resolved remotely or locally. | Approve/deny commands update durable decision state and produce node/event/channel feedback. |

## Security And Auth

| ID | Requirement | Acceptance |
|---|---|---|
| SEC-001 | Asylum is single-user and localhost-first. | Default bind is localhost; no team/org/account model exists. |
| SEC-002 | Owner-token auth protects HTTP when enabled. | Protected HTTP routes accept bearer tokens or query-token for browser/WebSocket constraints and reject invalid/revoked/expired DB tokens. |
| SEC-003 | Local socket auth uses filesystem boundary. | Socket routes do not accidentally disable HTTP auth. |
| SEC-004 | Token scopes are represented honestly. | If scopes are advisory rather than enforced, UI/docs must say so or implementation must enforce them. |
| SEC-005 | Attach URLs are signed and time-limited. | Attach token verification rejects expired/tampered tokens and does not expose raw signing secret. |
| SEC-006 | Remote commands require authentication. | Remote-command text or channel envelope includes a valid token or equivalent pairing credential before executing control actions. |
| SEC-007 | Exposed beyond localhost requires warning and configuration. | Cockpit/settings/status make non-local bind/base URL state visible. |
| SEC-008 | Secrets in config and channel settings are protected. | Tokens/API keys are not dumped into normal logs, UI panels, or JSON status output. |

## Cockpit Prototype Interpretation Rules

| ID | Requirement | Acceptance |
|---|---|---|
| PROTO-001 | Preserve prototype product intent. | Cockpit remains graph-first, dense, operational, terminal-aware, and node/session-focused. |
| PROTO-002 | Preserve prototype workflow intent. | Launch command center, observe graph, inspect nodes, open node sessions, remote notifications, channels, hooks, fleet table, and settings are real workflows. |
| PROTO-003 | Reject prototype data mechanics. | `ASYLUM_DATA`, fake nodes, fake Loon regions, fake logs, fake transcripts, fake settings, fake version strings, fake pairing codes, fake OpenAPI/SDK panels, and no-op buttons are not allowed in runtime code. |
| PROTO-004 | Reject prototype control mechanics. | Tweaks/edit-mode panels, simulation speed, timer-generated fake toasts, canned response animations, and demo-only command parsing are not allowed in runtime code. |
| PROTO-005 | Prototype-only visual controls must become real preferences or disappear. | Theme, nav collapse, graph layout, and similar UI state are persisted/handled as product UI preferences if retained. |

## Documentation Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| DOC-001 | README describes current install/run behavior. | It names `asylum daemon run`, `asylum service generate`, latest command shape, socket/HTTP split, and current release/build expectations. |
| DOC-002 | Current docs point to this spec as product truth. | Live docs do not require agents to reconcile stale PRDs, handoffs, reviews, or dated plans before acting. |
| DOC-003 | User-facing docs do not preserve stale command names. | No live docs instruct users to run `asylum serve` or `asylum install systemd|launchd`. |
| DOC-004 | Release docs distinguish main from published. | Delivery docs include release status when they represent a delivery cycle; doc-only specs state no release needed. |
| DOC-005 | Known product limitations are explicit. | Unsupported adapters, local-vs-Loon observe differences, advisory token scopes, and auth exposure posture are not hidden. |

## Audit Acceptance Model

An audit against this spec should classify each requirement as:

- **Pass** - observable behavior matches the requirement.
- **Partial** - real behavior exists but misses part of the requirement.
- **Fail** - requirement is absent, fake, broken, or contradicted.
- **Blocked** - cannot be verified in the available environment; the blocker is concrete.
- **Not Applicable** - the requirement is outside the audited surface.

Each non-pass result must include evidence: file path/line, command output, API response, screenshot/manual step, or test result.

## Reference-Only Product Extensions

The following concepts are allowed as product direction but are not part of the current deliverability contract unless a requirement above names them:

- Multi-user/team/org semantics.
- Hosted relay/SaaS identity.
- Non-ntfy rich messaging adapters such as SMS, Discord, Slack, Telegram, Signal, or email as live adapters.
- Additional harnesses beyond Codex and Claude Code.
- Additional substrates beyond local and Loon.
- A mandatory run/task/workflow engine above nodes.
- A public SDK package or generated OpenAPI surface.
