# Asylum Current Product Spec

**Status:** canonical current product contract

**Updated:** 2026-07-12

**Release context:** `v0.2.0` is the latest published release. Release truth is
tracked in [RELEASES.md](../../RELEASES.md).

## Purpose

This document defines what Asylum is supposed to be and do as a current deliverable product. It is the contract an agent should audit against when comparing the implementation, Cockpit UI, CLI, daemon, MCP bridge, install path, and docs to the intended product.

This is not an implementation plan and not a gap report. The implementation may fall short of this spec; those shortfalls belong in a separate coverage audit.

## How To Read This Spec

- Requirement IDs are stable handles for audits and implementation planning.
- "Acceptance" describes the observable behavior that proves the requirement.
- Earlier Cockpit prototypes are historical intent evidence only. Their mock
  data, simulations, edit controls, canned responses, fake node IDs, fake
  settings, and other prototype mechanics are not product requirements.
- User-facing runtime behavior must be real. Test fixtures and unit-test mocks are allowed; shipped daemon and Cockpit behavior must not be simulated, mocked, stubbed, hardcoded as fake real data, or demo-only.

## Source Authority

This spec is the consolidated product contract. It replaces older PRDs,
handoffs, reviews, and dated implementation plans as the live source of truth.
Those older artifacts are not kept as live docs on this branch; recover them
from git history only when historical reconstruction is needed.

This spec should stay aligned with:

- Current release truth from [RELEASES.md](../../RELEASES.md).
- Current agent/product principles from [AGENTS.md](../../AGENTS.md).
- The coordination layer model in
  [orchestration-layers.md](../concepts/orchestration-layers.md), which the
  `LAYER-*` and launch-profile requirements below make auditable.
- The session-first interaction invariant in
  [2026-05-09-cockpit-node-session-ux-design.md](../superpowers/specs/2026-05-09-cockpit-node-session-ux-design.md).
- Current implementation surfaces in `crates/`, `cockpit/src/`, and `scripts/`.
- Delivered scenario evidence in the completed
  [Asylum completion mission](../superpowers/specs/2026-07-06-asylum-completion-mission.md).

Completed plans and reviews are evidence, not competing product authority. When
they disagree with this document, this document governs current product intent
and the implementation/release ledger governs what is delivered.

## Product Brief

### Primary User And Job

Asylum is for one owner who already uses capable agent harnesses and wants to
delegate meaningful bodies of work without personally babysitting every
terminal. The owner may work locally or use a Loon host for isolated parallel
capacity, but should not need a different coordination model for each substrate.

The core job is:

> Give a capable coordinator one substantial objective, let it create and
> coordinate the right fleet, stay informed by exception instead of raw output,
> intervene in any live session when useful, and receive a durable result that
> survives process, daemon, and disposable-compute boundaries.

Asylum succeeds when one instruction becomes a visible, controllable fleet and
a trustworthy result. It fails when the owner must manually recreate context,
poll terminal output, translate between unrelated substrate controls, guess
whether a node needs help, or rescue work before an opaque teardown destroys it.

### Product Promise

Asylum provides:

- **Delegation:** launch a direct node or a coordinator with an objective and
  completion criteria.
- **Coordination:** let harnesses spawn and direct peers through daemon-owned
  capabilities and explicit graph relationships.
- **Supervision by exception:** surface meaningful progress, stalls, questions,
  failures, and resource pressure without requiring raw-stream monitoring.
- **Direct intervention:** selecting any node opens its real live session; the
  owner can send input, interrupt a turn, stop, resume, fork, or archive when the
  current state supports that action.
- **Durability:** preserve node identity, work context, events, decisions,
  results, and declared artifacts across daemon restarts and disposable compute.
- **Substrate choice:** keep the operator-level job consistent across local and
  Loon while explaining real capability differences.

Harnesses remain responsible for reasoning, decomposition, coding, review, and
judgment. Asylum does not insert a mandatory planner or workflow state machine.

Asylum coordinates at **session granularity**. Harness-internal parallelism —
subagents, agent teams, and scripted in-harness workflows — stays inside a
node; Asylum neither replaces nor models it. The full layer model, including
what each layer isolates and when work belongs at which layer, is defined in
[orchestration-layers.md](../concepts/orchestration-layers.md).

## Product Operating Loop

| Stage | Owner outcome | Product responsibility |
|---|---|---|
| Ready | Know that local harnesses, optional Loon capacity, auth, channels, and persistence are actually usable. | Diagnose readiness and show actionable unavailable reasons before launch. |
| Start | State the objective once and begin with sensible defaults. | Create a named node with a durable work envelope and open its real session. |
| Delegate | Let the coordinator create parallel peers without manual plumbing. | Inject bounded Asylum context/tools, inherit appropriate work context, and record explicit responsibility edges. |
| Supervise | Understand progress and exceptions without reading every transcript. | Maintain honest runtime/activity/attention state, summaries, monitors, and an actionable inbox. |
| Intervene | Reach the right node immediately and answer with full fidelity. | Focus the existing session, preserve decision types/options, and deliver the exact response. |
| Finish | Know what changed, where the result lives, and whether compute can be stopped safely. | Capture result summaries and artifact provenance, verify work preservation, then stop/archive intentionally. |
| Recover | Restart Asylum or recover a session without fictional state or silent loss. | Reconcile runtime truth, explain resumability, and preserve durable work independently of disposable compute. |

The scenario-level acceptance test for the current product is this entire loop,
not merely the existence of individual routes or buttons.

## Conceptual Product Model

### Node And Work Envelope

The Node remains the central object. Asylum does not require a separate task,
run, or workflow engine. A node may carry an optional durable **work envelope**
so direct sessions stay lightweight while coordinated work remains intelligible.

| ID | Requirement | Acceptance |
|---|---|---|
| WORK-001 | A node has a human-readable name separate from its UUID and prompt. | Cockpit, CLI, MCP, and API show the name first and keep the UUID available for exact addressing. |
| WORK-002 | A node can preserve an objective, completion criteria, and its assigned part of the work. | These fields survive restart/resume and are not reconstructed from terminal output. |
| WORK-003 | A live node can publish a bounded current-status summary without surrendering control of its reasoning to Asylum. | The harness can update the summary through a root capability; clients show its source and freshness. |
| WORK-004 | A node can publish a completion/result summary and artifact references. | Results name the producing node, objective, creation time, kind, and durable URI/path/commit reference. |
| WORK-005 | Spawned peers inherit relevant context without inheriting accidental control state. | The child receives the parent/root objective plus its explicit assignment; an edge records provenance. |
| WORK-006 | Completion is not inferred from silence or process exit. | A structured result/completion signal is distinct from idle, stopped, exited, or archived runtime state. |

A root supervisor node and its explicit descendants are sufficient to represent
a body of coordinated work. A future optional mission/run grouping may be added
only if real usage demonstrates that the node graph plus work envelopes cannot
express the needed grouping.

### Coordination Layers

Asylum is one layer of a coordination stack, defined in
[orchestration-layers.md](../concepts/orchestration-layers.md): harness tool
calls and harness-internal parallelism (subagents, agent teams, scripted
in-harness workflows) operate inside a node; Asylum operates at session
granularity across nodes; substrates set where a node's blast radius ends.
Nesting is the intended composition — a node's harness keeps every internal
parallelism tool it would have in a bare terminal.

| ID | Requirement | Acceptance |
|---|---|---|
| LAYER-001 | Harness-internal parallelism is node-internal behavior, not nodes. | Subagents, agent teams, or in-harness workflows run by a harness inside its session never auto-create node records, relationships, or graph edges. When a harness reports internal-orchestration telemetry, it may surface only as facts of that node per `HARN-004`. |
| LAYER-002 | Peer nodes are session-granularity delegation. | Product guidance directs work to the cheapest sufficient layer: direct work in-session, fine-grained fan-out via in-harness parallelism, and Asylum peers for work needing independent lifetime, isolation, separate supervision, a different workspace/harness/substrate/launch profile, or survival beyond the parent session. No product guidance forbids or discourages in-harness parallelism as such. |
| LAYER-003 | Injected coordination guidance teaches layer choice and verification etiquette. | The launch packet's coordination guidance includes the layer-choice etiquette and fresh-context verification etiquette from the concepts doc, distinguishes "never simulate a worker" from "never use in-harness subagents", and remains drift-checked against the real tool/event catalogs. |
| LAYER-004 | Verification guidance recommends decorrelated review without enforcing a workflow. | Guidance recommends verifying substantial results in a fresh context with a distinct adversarial framing (evaluator peer or in-harness equivalent) and warns that same-context review is weak; Asylum does not gate completion on any verification step. |

### Role And Command-Center Semantics

Roles describe responsibility; they do not control a mandatory workflow.

- `supervisor` coordinates peers for an objective.
- `worker` executes a delegated portion of work.
- `evaluator` verifies, reviews, or challenges outcomes.
- `assistant` is a direct general-purpose session.
- Custom role hints remain allowed.

**Command center is a Cockpit designation, not a competing responsibility
role.** The owner may pin any live node—commonly a supervisor—as the primary
Cockpit session. Existing `command-center` role values remain readable for
compatibility, but new UI should ask what the node will do and separately which
node is pinned as the command center.

| ID | Requirement | Acceptance |
|---|---|---|
| ROLE-001 | Role hints do not gate root capabilities. | Capability availability comes from current harness, substrate, runtime state, and security context. |
| ROLE-002 | The pinned command center is explicit and changeable. | Cockpit preserves the owner's selection and never silently chooses an archived node as the active command center. |
| ROLE-003 | Supervisor and direct-session starts are understandable choices. | Launch copy explains the outcome of each without requiring the owner to understand internal role enums. |

## Node State And Action Semantics

One liveness label is not enough to describe an agent session honestly. Product
surfaces must represent these independent dimensions even if a transitional wire
contract still carries legacy combined fields:

| Dimension | Values | Meaning |
|---|---|---|
| Runtime | `starting`, `live`, `stopped`, `exited`, `failed`, `archived`, `unknown` | Whether controllable compute/session runtime exists. |
| Activity | `active`, `idle`, `awaiting_input`, `unknown` | What a live harness is doing now. |
| Attention | `none`, `decision_pending`, `warning`, `error` | Whether the owner or coordinator should act. |
| Recovery | `resumable`, `not_resumable`, `unknown` plus reason | Whether the same harness session can be resumed now. |

| ID | Requirement | Acceptance |
|---|---|---|
| STATE-001 | Normal process exit is not rendered as idle. | `exited` remains a terminal runtime outcome; idle is activity of a live node. |
| STATE-002 | Interrupt cancels the current turn rather than claiming to stop or pause the node. | After interrupt, runtime remains live unless the harness actually exits. |
| STATE-003 | Stop is graceful where supported and reports escalation to hard termination. | State, events, and UI distinguish clean stop, forced stop, and unexpected exit. |
| STATE-004 | Archive is historical/read-only and preserves durable records. | Live-only controls are unavailable; archive never advertises attach/send/interrupt as currently usable. |
| STATE-005 | Actions are derived from current state and capabilities. | Every client disables or omits impossible actions and gives the same unavailable reason. |
| STATE-006 | Time labels name their semantics. | Live runtime age, time since last activity, ended duration, and record age are never all labeled `uptime`. |
| STATE-007 | Telemetry names its source and freshness. | Missing or estimated counters render as unavailable/estimated, never as authoritative zeroes. |
| STATE-008 | Observation loss is explicit. | A broken observe/control stream cannot leave a node looking healthy solely because its last stored state was live. |

## Loon Product Boundary And Work Durability

Loon is Asylum's disposable-compute substrate, not a second agent orchestration
model. Loon owns microVM lifecycle, guest execution, networking, volumes, and
low-level host health. Asylum owns node identity, harness context, coordination,
relationships, decisions, summaries, and the operator experience.

| ID | Requirement | Acceptance |
|---|---|---|
| LOON-001 | The installed Asylum product can declare Loon ready or explain exactly why not. | Readiness covers client profile, daemon reachability, guest image, harness binaries/auth, guest control path, resource baseline, and work persistence. |
| LOON-002 | Guest-to-Asylum control does not require exposing the whole unauthenticated Cockpit/API to the LAN. | Events and MCP use a scoped authenticated listener reachable only from intended guests, such as a Loon-gateway listener or an equivalently isolated transport. |
| LOON-003 | Disposable VM lifetime is independent of work lifetime. | A Loon coding workspace is durable on a volume or synchronized to an explicit external destination before VM deletion. |
| LOON-004 | Stop/archive cannot silently discard unreturned work. | Cockpit/CLI/MCP show preservation status and require explicit discard when durable return has not been established. |
| LOON-005 | Environment profiles are product objects, not hidden image folklore. | Launch can choose a validated profile that names image, harness, auth readiness, resources, persistence mode, and supported capabilities. |
| LOON-006 | Local/Loon differences are outcome-oriented. | Users see `resume unavailable because the VM workspace was ephemeral`, not internal adapter guesses or false parity. |
| LOON-007 | Asylum remains useful with Loon disabled or unhealthy. | Local creation/control and the rest of Cockpit continue without Loon-specific errors dominating the product. |

## Cockpit Information Architecture

Cockpit should be an operating surface for the loop above, not a collection of
thin API administration screens.

| Surface | Product purpose |
|---|---|
| Home | Active work, active graph/table toggle, selected node's real session, coordinator summary, and current exceptions. Archived history is excluded by default. |
| Inbox | One actionable queue for decisions, failures, warnings, and meaningful notifications, with node/work context and exact response controls. |
| Automations | Recommended supervision recipes first; advanced event/filter/action hook editing second. |
| History | Searchable stopped/exited/archived nodes, results, artifacts, and audit evidence. |
| Settings | Harness/substrate readiness, Loon profiles, channels/integrations, auth/exposure, storage, retention, and diagnostics. |

Launch remains a global action. A node detail/session view is reached by selecting
a node; a separate Chat concept must not imply a second kind of session.

| ID | Requirement | Acceptance |
|---|---|---|
| UX-001 | Cockpit opens on active work and attention. | The owner can tell what is running, what needs attention, and what the selected node is doing without changing screens. |
| UX-002 | Selecting a node focuses the same session everywhere. | Graph, table, inbox, search, and history all lead to one node/session model with no attach workflow. |
| UX-003 | Graph is one view of explicit relationships, not the whole product. | All explicit relevant edges can render; arbitrary `first relationship = parent` hierarchy is not presented as truth. |
| UX-004 | History does not overwhelm current work. | Archived nodes are hidden from the active graph by default and remain discoverable through history/filtering. |
| UX-005 | Launch is goal-first and substrate-aware. | The primary fields are name/objective/desired outcome; advanced harness, launch-profile (model/effort/posture), role, workspace, persistence, and resource controls adapt to the selected harness and substrate. |
| UX-006 | Supervision works without building raw hooks from scratch. | The owner can enable clear recipes for idle, awaiting-input, error, context pressure, completion, and escalation, then inspect/edit their underlying rules. |
| UX-007 | Destructive actions are deliberate. | Stop/archive/discard explain effects, protect unreturned work, prevent double submission, and request confirmation proportional to risk. |
| UX-008 | Auth gates block the application semantically and interactively. | The dialog traps focus, prevents underlying actions/shortcuts, avoids unauthenticated false-zero fleet data, and does not create request-error storms. |
| UX-009 | Core Cockpit flows work at narrow/mobile widths. | Status, inbox decisions, node selection, and session intervention remain usable at 390 CSS pixels without clipped or unreachable content. |
| UX-010 | Cockpit uses direct capabilities for local actions. | It never asks the owner to type raw remote-command syntax or paste an owner token into commands that Cockpit can invoke directly. |

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
- Preserve objectives, current summaries, results, and artifact provenance without imposing a workflow engine.
- Make attention and recovery state understandable without requiring transcript inspection.
- Make Loon-backed work safe to run from the installed product and durable beyond a disposable VM.
- Keep Asylum single-user, localhost-first, and Loon-independent.

### Non-Goals

- No hosted SaaS control plane.
- No multi-tenant organization model.
- No team RBAC.
- No mandatory task/run/workflow state machine.
- No Asylum-owned scripted orchestration engine, workflow DSL, or subagent
  layer; scripted orchestration and fine-grained fan-out belong to harnesses
  inside a node.
- No modeling of harness-internal subagents/teams/workflows as nodes or edges.
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
| Work envelope | Optional node metadata containing name, objective, completion criteria, assignment, current summary, result, and artifact references. |
| Session | The real interactive harness process/control stream associated with a node. It is not a separate Cockpit object. |
| Harness | Agent runtime such as Codex or Claude Code. |
| Substrate | Place where a node runs: local or Loon-backed. |
| Root capability | A daemon-owned operation exposed consistently to clients. |
| Command center | The node currently pinned as the primary Cockpit session; usually a supervisor, but not a role or state. |
| Attention item | A decision, warning, failure, or meaningful notification that may require owner/coordinator action. |
| Result | Structured completion/outcome summary published by a node. |
| Artifact | Durable reference to a file, patch, commit, branch, report, URL, or other work product with node provenance. |
| Channel | A notification or command transport such as ntfy or webhook. |
| Hook | Declarative event trigger with daemon-executed actions. |
| Coordination layer | Which mechanism owns a unit of parallel work: harness tool calls, harness-internal parallelism inside one node, Asylum peer nodes, or substrate isolation. See [orchestration-layers.md](../concepts/orchestration-layers.md). |
| Harness-internal parallelism | Subagents, agent teams, or scripted workflows a harness runs inside its own session. Node-internal behavior; never nodes or graph edges. |
| Launch profile | The harness-level launch choices recorded for a node: model, reasoning effort, execution posture, and harness-specific options actually applied at launch. |

"Server" is not the primary product term. HTTP is a Cockpit and remote transport detail of the daemon.

## Technical Architecture Contract

The following sections preserve engineering invariants that keep all product
surfaces coherent. They support the product loop above; passing them alone does
not prove product acceptance.

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
| DATA-002 | Node records are the central durable objects. | A node has ID, name, harness, substrate, role hint, work envelope, runtime/activity/attention/recovery state, workspace, timestamps, external substrate ID, current capability view, and sourced telemetry fields. |
| DATA-003 | Runtime, activity, attention, and recovery are separate product facts. | Wire contracts expose the dimensions defined by `STATE-001` through `STATE-008`; legacy combined liveness remains compatibility data, not the only UI truth. |
| DATA-004 | Events are durable and ordered per node. | Node events carry ID, node ID, sequence, kind, body, timestamp, and schema version. |
| DATA-005 | Transcript/output storage records real harness output. | Output chunks come from harness/substrate output or explicit input/control events, not from canned UI sequences. |
| DATA-006 | Telemetry shown in UI is honest about source. | Native metrics may be shown when available; estimates derived from event text must be treated as estimates, not claimed as harness-native truth. |
| DATA-007 | Secrets are not stored or rendered casually. | Tokens and attach secrets are stored hashed or redacted where practical; raw issued tokens are shown only at issuance/rotation moments. |
| DATA-008 | Result and artifact records are durable and queryable. | API, CLI, MCP, and Cockpit can list a node's result/artifacts without parsing ANSI/TUI transcript text. |
| DATA-009 | Automation audit evidence survives rule lifecycle. | Deleting or changing a hook does not delete its prior firing records; records retain the executed rule/action snapshot. |

## Nodes, Graph, And Relationships

| ID | Requirement | Surfaces | Acceptance |
|---|---|---|---|
| NODE-001 | Operators can create real nodes. | CLI, API, MCP, Cockpit | Creating a node launches a real configured harness on a supported substrate or returns a clear error. |
| NODE-002 | Operators can list and inspect nodes. | CLI, API, MCP, Cockpit | List/inspect return named node records with work context, multidimensional state, current action availability/reasons, workspace/persistence, sourced telemetry, and substrate/harness identity. |
| NODE-003 | Operators can send input to running nodes. | CLI, API, MCP, Cockpit, remote command | Input reaches the harness stdin/control path and records an input event. |
| NODE-004 | Operators can interrupt nodes. | CLI, API, MCP, Cockpit, remote command, hooks | Interrupt reaches the substrate when supported and records a liveness/control event. Unsupported cases return honest errors. |
| NODE-005 | Operators can stop nodes. | CLI, API, MCP, Cockpit, remote command | Stop terminates or requests termination through the substrate and updates liveness. |
| NODE-006 | Operators can archive nodes. | CLI, API, MCP, Cockpit, hooks | Archive stops active runtime when possible, marks the node archived, and preserves durable record/transcript references. |
| NODE-007 | Operators can fork nodes. | CLI, API, MCP, Cockpit | Fork creates a real node inheriting harness/substrate/workspace defaults and creates an explicit relationship to the source node. |
| NODE-008 | Operators can observe node events and live output. | API, WebSocket, Cockpit | Historical events stream first; live output streams where substrate supports it; unsupported live stream paths say so clearly. |
| NODE-009 | Signed browser session transport works for compatible clients. | API, MCP, remote command | Issued session URLs are signed, time-limited, verify before opening, and stream real I/O over the daemon attach transport. |
| NODE-010 | Native attach is compatibility-only and must be real or absent. | CLI, API | If retained, the command opens the same real node session and cannot recursively point back to itself. It may be removed when no supported consumer needs it. Cockpit never exposes it as a normal workflow. |
| NODE-011 | Node capabilities are visible, current, and explained. | API, Cockpit, MCP | Availability combines adapter support with runtime/security/readiness state and includes a reason when unavailable; archived snapshots are not presented as actions that work now. |
| NODE-012 | A command center is a pinned real node, not a special runtime. | Cockpit, CLI, API, MCP | Pinning a supervisor/assistant changes primary Cockpit focus only; it does not create private powers or a competing role state. |
| NODE-013 | Nodes can spawn peer nodes through Asylum. | API, CLI, MCP | A running node with delegated spawn authority can create a real peer, inheriting explicitly documented harness/substrate/profile/work context fields, overriding its assignment, and atomically recording `spawned_for` provenance. |
| NODE-014 | Node creation and relationship creation are consistent. | API, CLI, MCP, Cockpit | A failed spawn cannot leave an unintended unlinked child; creation either commits node+edge together or reports/reconciles the partial result explicitly. |
| NODE-015 | Nodes can publish status and results. | API, CLI, MCP, Cockpit | Bounded status/result updates carry source/freshness and remain available after the session stops. |
| NODE-016 | Inspection operations are side-effect free. | API, CLI, MCP | `GET`, list, inspect, context preview, and dry-run operations do not persist artifacts or execute actions; materialization uses an explicitly named mutation. |
| GRAPH-001 | The graph shows explicit relationships only. | API, Cockpit, MCP | Relationships in graph responses come from stored relationship records, not inferred workspace/substrate correlation. |
| GRAPH-002 | Operators can create, list, and remove relationships. | CLI, API, MCP, Cockpit | Relationship kinds include `supervises`, `spawned_for`, `user_created`, and `platform_responsibility`; invalid kinds are rejected. |
| GRAPH-003 | Correlations are distinct from edges. | Cockpit, API | Same workspace, same substrate, same harness, and similar metadata may be filters/groups/facts but not graph edges. |
| GRAPH-004 | Clients preserve graph multiplicity and meaning. | Cockpit, API, MCP | All relevant explicit edges can be returned/rendered; clients do not collapse the first arbitrary incoming edge into a fictional parent hierarchy. |

## Harnesses And Substrates

| ID | Requirement | Acceptance |
|---|---|---|
| HARN-001 | Codex and Claude Code are supported harnesses. | Config exposes the command for each; descriptors show availability/capabilities; create-node can launch both when commands exist. |
| HARN-002 | Harness adapters are real process/control adapters, not simulations. | Local launch starts the configured CLI process in a PTY; Loon launch uses Loon control contracts. |
| HARN-003 | Launch context is Asylum-aware and bounded. | New nodes receive node ID, work envelope/assignment, role hint, relevant graph/status summary, authority/capability guidance, and workspace/substrate facts through the harness's supported launch mechanism. Local Codex and Claude Code launches receive per-process Asylum MCP configuration. Context uses handles/cursors rather than unbounded fleet/transcript dumps. |
| HARN-004 | Optional harness capabilities are advertised per adapter. | Structured events, tool-call telemetry, transcript export, native resume, subagent visibility, permission prompts, and context telemetry are shown only when actually supported. |
| HARN-005 | Launch profile options are first-class where the harness supports them. | Node creation and peer spawn accept harness-supported model, reasoning-effort, and execution-posture options; harness descriptors advertise which profile options each adapter supports; unsupported options return honest errors rather than silent no-ops. Asylum passes profile values through as plumbing and does not maintain its own model/effort catalogs — the harness is authoritative and its rejection errors are surfaced. |
| HARN-006 | Coordinators choose per-peer launch profiles. | `node.spawn_peer` carries the same launch-profile options as node creation, so a coordinator can put mechanical work on cheaper configurations and judgment work on stronger ones without owner intervention. |
| HARN-007 | The effective launch profile is durable and visible. | Node records preserve the harness, model, effort, and posture the node was actually launched with (or an explicit harness-default marker); inspect, Cockpit, CLI, and MCP display it for live and historical nodes. |
| SUB-001 | Local substrate supports real PTY launch/control. | Local nodes can launch, stream output, receive input, interrupt, stop, and connect through browser/native compatibility paths. |
| SUB-002 | Loon is optional and independent. | Asylum works without Loon; enabling Loon uses configured endpoint/CLI/auth/cert settings without coupling Asylum core to Loon internals. |
| SUB-003 | Loon-backed nodes use the documented Loon CLI/control contract. | Launch/input/interrupt/stop/archive/session relay use `loon` operations or return clear errors when unavailable. |
| SUB-004 | Loon health/capacity/readiness is visible. | Cockpit/API distinguish host reachability, real utilization/limits, guest profile/image readiness, harness auth, guest control reachability, persistence, and unsupported capabilities. |
| SUB-005 | Loon-backed session relay and observe semantics are honest. | If browser session relay differs from local PTY output, UI/API say so rather than pretending parity. |
| SUB-006 | Loon workspaces have an explicit persistence/return contract. | Launch identifies durable volume or synchronization destination; stop/archive/VM deletion cannot silently discard work. |
| SUB-007 | Harness credentials are provisioned by need and with lifecycle visibility. | A Claude node does not receive Codex credentials or vice versa; readiness/expiry/revocation failures are actionable and do not corrupt host auth. |

## Root Capabilities And API

| ID | Requirement | Acceptance |
|---|---|---|
| CAP-001 | Every product affordance maps to a daemon-owned root capability. | Capability descriptors list the endpoint, method/transport, description, and availability. |
| CAP-002 | Capability semantics are shared across clients. | CLI, MCP, Cockpit, hooks, notifications, and remote commands call the same daemon behavior rather than parallel implementations. |
| CAP-003 | The HTTP API exposes typed JSON contracts. | Routes return stable request/response shapes from `asylum-types` or Cockpit-mirrored equivalents. |
| CAP-004 | Core node capabilities exist. | `node.create`, `node.spawn_peer`, `node.list`, `node.inspect`, `node.observe`, `node.events`, `node.send_input`, `node.interrupt`, `node.stop`, `node.resume`, `node.archive`, `node.fork`, `node.status.update`, `node.result.publish`, `node.attach.browser`, and `node.attach.native_target` are implemented or explicitly unavailable per node. |
| CAP-005 | Graph capabilities exist. | `graph.get`, `relationship.create`, `relationship.list`, and `relationship.remove` are available. |
| CAP-006 | Harness/substrate descriptor capabilities exist. | Clients can list harnesses, substrates, descriptors, health, and capability flags. |
| CAP-007 | Context capabilities exist. | Clients can read current system map/graph and generate launch packets for nodes. |
| CAP-008 | Notification/channel capabilities exist. | Clients can list/create/update/delete channels, list messages, send test messages, record inbound messages, and send notifications. |
| CAP-009 | Hook capabilities exist. | Clients can list/create/update/delete hooks, list event catalog, dry-run hooks, and inspect firings. |
| CAP-010 | Token capabilities exist where safe. | API/CLI can issue, list, revoke, and rotate owner tokens; MCP does not expose token management. |
| CAP-011 | Remote command capabilities exist. | Authenticated remote commands can request status, issue signed session URLs, send input, start nodes, interrupt, stop, approve, and deny decisions. |
| CAP-012 | Unsupported capabilities fail clearly. | A missing harness/substrate/channel feature returns an explicit unsupported/unavailable error and is not hidden behind a successful no-op. |
| CAP-013 | Result/artifact capabilities exist. | Clients can publish, list, inspect, and retrieve durable result/artifact references within the caller's authority. |
| CAP-014 | Capability discovery has one authoritative registry. | IDs are unique; API, CLI, MCP, and Cockpit descriptions/availability are derived or contract-tested against the same semantics. |
| CAP-015 | Agent authority is explicit and least-necessary. | Injected node credentials distinguish self-node operations, delegated peer creation/control, fleet-read, and owner operations; no node silently receives owner-equivalent fleet control. |

## CLI Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| CLI-001 | The CLI is the primary local operator interface. | A user can operate Asylum from `asylum` without needing Cockpit. |
| CLI-002 | CLI lifecycle commands are complete. | `setup`, `cockpit`, `start`, `stop`, `restart`, `status`, `doctor`, `logs`, `update`, `uninstall`, `daemon run`, `config init`, `config show`, and `service generate` work and are documented in help. |
| CLI-003 | CLI node commands cover core node operations. | Create/spawn/fork/list/inspect/events/send/interrupt/stop/resume/archive/status/result and session compatibility commands work through daemon capabilities. |
| CLI-004 | CLI graph commands expose graph state and relationship management. | `graph get` and relationship create/list/remove commands work through daemon capabilities. |
| CLI-005 | CLI token and notification commands exist. | Operators can issue tokens and send notifications from terminal. |
| CLI-006 | CLI can reach all root capabilities practical for a terminal. | Terminal commands exist for channels, hooks, decisions, relationships, fork, and remote commands, or the capability has a clear terminal-inapplicable rationale in this spec. |
| CLI-007 | CLI output is useful for both humans and automation. | Human-readable tables/summaries are the interactive default; a consistent `--json` mode exposes stable machine output without changing semantics. |
| CLI-008 | CLI uses socket transport for local daemon control by default. | Local commands do not require HTTP bearer tokens unless explicitly configured for HTTP remote control. |
| CLI-009 | CLI supports bounded live/retained inspection. | Operators can filter active/history nodes, follow events, inspect attention, search/export transcripts where available, and list results/artifacts without dumping unbounded JSON. |
| CLI-010 | Config and auth output are safe. | Normal config/status commands redact secrets and show effective auth/exposure state; destructive config replacement is explicit. |

## MCP Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| MCP-001 | `asylum mcp` is a stdio JSON-RPC MCP bridge into the daemon. | It initializes, lists tools, handles notifications correctly, and calls daemon capabilities. |
| MCP-002 | MCP exposes core node and graph capabilities. | Required tools include `node.create`, `node.spawn_peer`, `node.list`, `node.inspect`, `node.status.update`, `node.result.publish`, `node.send_input`, `node.interrupt`, `node.stop`, `node.resume`, `node.archive`, `node.events`, `node.fork`, `attach_url.issue`, `graph.get`, `relationship.create`, and `relationship.list`. |
| MCP-003 | MCP exposes safe automation capabilities. | Required tools include `hook.list`, `hook.create`, `hook.delete`, `hook.firings`, `channel.list`, `notify.send`, and `health.get`. |
| MCP-004 | MCP does not expose token management by default. | Token issuance/revocation stays out of MCP unless a separate security review changes this spec. |
| MCP-005 | MCP tool names and routes match daemon capabilities. | Tool handlers call real daemon routes; no MCP tool may point at a non-existent or wrong endpoint. |
| MCP-006 | MCP gives agents bounded coordination views. | Tools can list attention/current status/results and page events with cursors; normal coordination never requires full transcripts or an unbounded fleet dump. |
| MCP-007 | MCP discovery includes harness/substrate readiness. | A coordinator can choose a viable harness/profile/substrate without guessing from failed launches. |

## Cockpit Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| COCKPIT-001 | Cockpit is the primary first-party UI. | Bare `asylum`/`asylum cockpit` opens it after daemon health is ready. |
| COCKPIT-002 | Cockpit opens active-work-first. | Home shows active graph/table, inline selected session, coordinator/status summary, attention, and real counts; graph remains a primary view without hiding the operating loop. |
| COCKPIT-003 | Cockpit supports a first-success empty state. | With zero nodes, it checks readiness, explains direct versus supervised work succinctly, and offers launching a real named supervisor or assistant from an objective. |
| COCKPIT-004 | Cockpit launch flow creates real nodes from a goal-first form. | Name/objective/outcome are primary; harness/substrate/role/workspace/persistence/resources are validated advanced choices with substrate-specific help. |
| COCKPIT-005 | Cockpit's pinned command-center panel is a real node session. | The inline panel sends input to the pinned/selected node and observes its real output/events; it is not a custom chatbot or a second session object. |
| COCKPIT-006 | Cockpit can focus any node session. | Selecting a node from Home, search, Inbox, or History focuses its one real session and can show metadata, events, capabilities, and relationships without a separate attach action. |
| COCKPIT-007 | Cockpit graph layouts are usable and truthful. | Layouts derive from all applicable explicit relationships, current nodes, and real substrate facts; decorative motion does not imply real work flow. |
| COCKPIT-008 | Home and History provide complementary dense node views. | Home's table covers active work; History covers stopped/exited/archived work, results, and artifacts. Both support name/objective/state/substrate/attention filters, sorting, and pagination without creating a second node model. |
| COCKPIT-009 | Cockpit node detail has real tabs. | Session, events, capabilities, relationships, and telemetry tabs display daemon-backed data or honest empty/unsupported states. |
| COCKPIT-010 | Cockpit controls call real currently available capabilities. | Send input, interrupt, stop, resume, fork, archive, and relationship actions share availability reasons, prevent double submit, confirm destructive effects, and surface delivery errors. Cockpit does not expose attach as a normal workflow. |
| COCKPIT-011 | Inbox and node history show real evidence. | Inbox uses real decisions/notifications/failures; node history uses real events. Neither claims a unified stream or result unless backed by one. |
| COCKPIT-012 | Channel/integration settings are real. | Settings exposes channel CRUD, readiness, message history, test send, inbound webhook/manual messages, and subscribe details through daemon endpoints. |
| COCKPIT-013 | Automations are real. | Recipes and advanced hook CRUD, enable/disable, safe preview, event catalog, actions, and durable firing history use daemon endpoints. |
| COCKPIT-014 | Cockpit settings are real. | Settings display daemon health, version, bind/base URL, database/storage paths and sizes, harness/substrate descriptors, ntfy channels, and token state from APIs. |
| COCKPIT-015 | Cockpit command palette uses direct real navigation/actions. | Cmd-K can navigate, find/focus nodes, and launch/invoke direct capabilities. It never asks for raw remote-command strings or pasted owner tokens when Cockpit already has the capability. |
| COCKPIT-016 | Cockpit auth token handling is not persistent browser storage. | Owner token can be hydrated from URL or prompt, held in memory, and stripped from URL after hydration. |
| COCKPIT-017 | Cockpit contains no prototype mechanics. | No Tweaks panel, `simSpeed`, canned `runResponse`, hardcoded demo nodes, fake settings, fake logs, fake session preview output, visible attach workflow, or no-op buttons ship in `cockpit/src`. |
| COCKPIT-018 | Cockpit visual design follows the prototype intent without inheriting prototype data. | It preserves the graph-first layout, compact operational style, mono terminal feel, node inspector, command-center/session focus, and channels/hooks concepts using real data. |
| COCKPIT-019 | Cockpit has one attention inbox. | Decisions, warnings, failures, and meaningful notifications are contextual, actionable, filterable, and link directly to the affected node/work. |
| COCKPIT-020 | Cockpit offers supervision recipes before raw hook construction. | Recommended idle/awaiting/error/context/completion/escalation policies can be enabled and inspected without hiding their real rules/actions. |
| COCKPIT-021 | Cockpit auth failure cannot masquerade as an empty fleet. | Protected requests render one blocking auth state, suppress underlying actions/poll storms, and preserve the distinction between unknown and zero. |
| COCKPIT-022 | Cockpit is usable at narrow widths. | Core home/status/inbox/session flows work at 390 CSS pixels with semantic keyboard-accessible controls. |

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
| HOOK-003 | Hook actions call real capabilities. | `channel`, `send_input`, `spawn`, `tool`, `pause_node`, and `archive` actions call daemon behavior or return explicit unsupported errors. |
| HOOK-004 | Hook preview is side-effect free and visibly synthetic. | Preview/dry-run evaluates matching and the proposed action plan without executing actions or changing production state. Synthetic payloads and any retained test evidence are clearly marked. |
| DECISION-001 | Human decision requests are typed first-class product behavior. | Records distinguish confirmation, permission, free text, single select, and multi-select where supported, preserving prompt/options/default/source. |
| DECISION-002 | Decisions are surfaced to operators. | Cockpit, notifications, and remote commands can show pending decisions with context and allowed actions. |
| DECISION-003 | Decisions can be resolved remotely or locally with delivery truth. | The exact answer, source, attempt, delivery outcome, and error are durable; waiting state is not cleared merely because persistence succeeded. |
| DECISION-004 | Harness-specific delivery preserves meaning. | Acceptance includes selecting a non-default menu option and proving the harness acted on that exact choice. |

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
| SEC-009 | Autonomous execution posture is explicit per environment/profile. | Permission/sandbox bypass, host reach, network, credentials, and delegated Asylum authority are visible before launch and inspectable afterward. |
| SEC-010 | Guest credentials are scoped and minimal. | Loon guests receive only the selected harness credential and a revocable Asylum token limited to declared node/delegation capabilities. |
| SEC-011 | Loon control ingress is isolated. | Guest reachability does not force all-interface unauthenticated exposure of Cockpit or owner APIs. |

## Cockpit Prototype Interpretation Rules

| ID | Requirement | Acceptance |
|---|---|---|
| PROTO-001 | Preserve the useful visual intent, not the old information architecture. | Cockpit remains dense, operational, terminal-aware, and node/session-focused while the refined active-work loop governs navigation. |
| PROTO-002 | Preserve useful workflow intent under the refined model. | Launch/pin a real node, inspect active graph/table, open the same node session, handle Inbox attention, configure Automations/integrations, and inspect History/Settings through real capabilities. |
| PROTO-003 | Reject prototype data mechanics. | `ASYLUM_DATA`, fake nodes, fake Loon regions, fake logs, fake transcripts, fake settings, fake version strings, fake pairing codes, fake OpenAPI/SDK panels, and no-op buttons are not allowed in runtime code. |
| PROTO-004 | Reject prototype control mechanics. | Tweaks/edit-mode panels, simulation speed, timer-generated fake toasts, canned response animations, and demo-only command parsing are not allowed in runtime code. |
| PROTO-005 | Prototype-only visual controls must become real preferences or disappear. | Theme, nav collapse, graph layout, and similar UI state are persisted/handled as product UI preferences if retained. |

## Documentation Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| DOC-001 | README describes current install/run behavior. | It names `asylum daemon run`, `asylum service generate`, latest command shape, socket/HTTP split, and current release/build expectations. |
| DOC-002 | Current docs point to this spec as product truth. | Completed missions/plans are clearly labeled evidence, not active work or competing authority. |
| DOC-003 | User-facing docs do not preserve stale command names. | No live docs instruct users to run `asylum serve` or `asylum install systemd|launchd`. |
| DOC-004 | Release docs distinguish main from published. | Delivery docs include release status when they represent a delivery cycle; doc-only specs state no release needed. |
| DOC-005 | Known product limitations are explicit. | Unsupported adapters, local-vs-Loon observe differences, advisory token scopes, and auth exposure posture are not hidden. |
| DOC-006 | Installed-product docs lead with first success. | README shows setup/readiness, launch objective, supervise/intervene, result collection, and safe finish before source-development/release internals. |
| DOC-007 | Docs teach the coordination layer model. | README positions Asylum relative to harness-internal parallelism; [orchestration-layers.md](../concepts/orchestration-layers.md) is a listed current product source; coordination guidance surfaces reference the same layer-choice etiquette rather than contradicting it. |

## Scenario-Level Acceptance

Individual route, unit, and component checks are necessary but do not prove the
product. A release that claims this contract passes the following real scenarios
using the installed artifact and actual harnesses/substrates. Frugal prompts are
fine; mocked runtime behavior is not.

### A. First Success

From a clean Asylum state, the owner runs setup/doctor, opens Cockpit, sees honest
local/Loon readiness, launches a named local supervisor from an objective, and
interacts with its real session. Failure paths identify the missing binary,
credential, workspace, auth, or substrate prerequisite precisely.

### B. Delegated Fleet

The supervisor uses injected Asylum capabilities to spawn at least two peers in
parallel, including one Loon peer when Loon is configured. Each peer receives a
distinct assignment and explicit edge. The owner sees active work and bounded
status without opening raw output, while retaining the ability to open and
intervene in every session.

### C. Attention And Typed Decision

A real node becomes idle, errors, and asks a non-default single-select question
in separate checks. Recommended monitoring surfaces each condition once. The
owner answers locally and through the configured remote channel; the exact answer
and delivery outcome are durable, and the harness acts on the selected non-default
choice.

### D. Result And Safe Finish

Workers publish results/artifacts, the supervisor publishes a final summary, and
Cockpit/CLI/MCP can retrieve them without transcript scraping. Stopping and
archiving the fleet preserves declared work. A Loon VM can be deleted without
losing its durable workspace/result, or the product explicitly blocks for a
discard decision.

### E. Failure And Recovery

The daemon is killed during local and Loon work. Restart reconciles runtime,
activity, attention, and recovery honestly; it never leaves eternal-live nodes.
Resumable local sessions resume with the same work envelope. Unsupported Loon
resume explains why while preserved work remains available.

### F. History, Security, And Narrow UI

Archived history is absent from the active graph by default but searchable with
results/audit evidence intact. Auth failure renders a blocking gate rather than a
zero fleet. Loon guest control works without LAN-wide unauthenticated owner API
exposure. Home, inbox, node selection, and intervention remain usable at 390 CSS
pixels and by keyboard.

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
