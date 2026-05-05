---
title: Asylum Live v2 Product Requirements And Design Spec
status: Approved for implementation planning
date: 2026-04-27
---

# Asylum Live v2 Product Requirements And Design Spec

## 1. Summary

Asylum is a single-user, always-on control plane for real agent harness sessions. It does not replace Codex, Claude Code, Pi, Hermes, or future harnesses. It launches them, gives them shared tools and context, observes them, lets humans attach or intervene, and lets harnesses coordinate other harnesses across local and Loon-backed substrates.

The core product object is the **Node**: a live or resumable harness session running somewhere. A node may be a command center, supervisor, worker, evaluator, plain assistant, or custom role, but those are role hints, not mandatory workflow states.

Asylum provides capabilities, visibility, substrates, and durable coordination. Harnesses provide intelligence.

## 2. Product Thesis

Asylum should make long-running agent work visible, controllable, and reachable without forcing every model or harness into one workflow engine.

The user should be able to:

- open Asylum Cockpit and see the living graph of agent sessions,
- launch a Codex or Claude Code command-center node inline,
- ask that command center to spawn and supervise worker nodes,
- run nodes locally or on Loon,
- attach to any node in browser or native harness UI,
- send input, interrupt, stop, and inspect output,
- access the same capabilities from CLI, MCP, dashboard, command-center chat, or third-party chatbot clients.

Asylum is useful even when no nodes are alive. It persists as a service and can start new nodes later through dashboard, CLI, API, MCP, or remote command channel.

## 3. Goals

- Provide a real installable v1 product, not a partial scaffold.
- Support Codex and Claude Code out of the box.
- Support local nodes and Loon-backed nodes in v1.
- Keep Loon independent and usable outside Asylum.
- Make node graph visibility the default product experience.
- Provide a command-center chat powered by a selected real harness.
- Expose one root capability surface used by every client.
- Support remote notifications and command replies through ntfy in v1.
- Avoid hardcoded workflow assumptions where model or harness intelligence should decide.
- Make it easy for any supported harness to act as a supervisor.

## 4. Non-Goals For V1

- No custom chatbot brain owned by Asylum.
- No mandatory run/task/workflow state machine.
- No requirement that nodes belong to a "run."
- No Linear/task-contract workflow engine in v1.
- No inferred graph edges.
- No multi-user or team RBAC.
- No hosted SaaS or public relay service.
- No assumption that Loon is required to use Asylum.

## 5. Design Principles

### P1. Capability-first architecture

If Asylum can do something, it exists once as a root capability. Dashboard, CLI, MCP, command-center chat, API clients, and third-party chatbot integrations all compose the same root capability surface.

No client gets private powers.

### P2. Harnesses provide intelligence

Asylum does not implement its own agent reasoning loop. It launches real harness nodes, gives them context and tools, and lets their model/harness intelligence decide how to supervise or work.

### P3. Node-first, not run-first

V1 centers on nodes. A node is a real harness session running on a substrate. Runs, work items, recipes, and task lifecycles can be later layers, but the core platform does not require them.

### P4. Role hints are not workflow states

A node may be labeled `command-center`, `supervisor`, `worker`, `evaluator`, or custom, but Asylum does not force nodes through planning, implementing, verifying, blocked, or done states.

### P5. Explicit relationships only

The graph shows intentional platform-known relationships. Correlations such as same repo or same substrate may appear as grouping, filters, or inspector facts, but not as edges.

### P6. Loon is independent

Loon is a Firecracker microVM fabric. Asylum integrates with Loon as a first-class substrate, but Loon remains independently useful.

### P7. Remote control is a product surface

Notifications are not only alerts. They are a remote control path. Users should be able to receive status, reply with commands, approve/deny, request attach links, and send input to nodes.

## 6. Core Concepts

### Asylum

The always-on single-user control plane. It owns the node registry, capability service, dashboard, API, CLI, MCP server, auth, and notification channels.

### Node

A live or resumable harness session.

Node fields include:

- node id,
- harness adapter,
- substrate,
- role hint,
- liveness,
- workspace or working directory,
- current output preview,
- attach targets,
- capability snapshot,
- provenance,
- explicit relationships,
- transcript and event references,
- human-readable description.

### Harness

The agent runtime or UI that provides intelligence. V1 requires:

- Codex,
- Claude Code.

Future harnesses may include Pi, Hermes, shell/PTY, browser-native agents, or other systems.

### Substrate

Where a node runs. V1 supports:

- local,
- Loon.

Future substrates may include SSH hosts, containers, cloud runners, or other VM fabrics.

### Command Center

An inline persistent node launched from Cockpit. It is powered by a selected harness and receives Asylum-aware context, tools, and instructions. It appears in the graph like any other node.

### Cockpit

The graph-first dashboard. It is the primary first-party UI for seeing and controlling nodes.

## 7. System Architecture

```text
Surfaces and clients
  - Cockpit dashboard
  - Command-center chat
  - CLI
  - MCP server
  - API/SDK clients
  - Third-party chatbot clients
  - Remote notification channels

        use

Core capability service
  - node capabilities
  - harness capabilities
  - substrate capabilities
  - relationship capabilities
  - attach capabilities
  - notification and remote command capabilities
  - auth and connection capabilities

        drives

Adapters
  - Codex adapter
  - Claude Code adapter
  - local substrate adapter
  - Loon substrate adapter

        launch/control

Nodes
  - real Codex sessions
  - real Claude Code sessions
  - future real harness sessions

        running on

Substrates
  - local machine
  - Loon Firecracker VM host
  - future substrates
```

## 8. V1 User Experience

### Cockpit default view

Cockpit opens graph-first.

Primary regions:

- node graph,
- inline command-center chat,
- selected-node inspector,
- create-node controls,
- secondary table view.

The table view exists for sorting, filtering, and bulk operations, but the graph is the default mental model.

### Command-center flow

1. User opens Cockpit.
2. User clicks **New Command Center**.
3. User chooses harness: Codex or Claude Code.
4. User chooses substrate: local or Loon if configured.
5. Asylum launches a real harness node with Asylum-aware context.
6. Inline chat appears immediately.
7. The command-center node appears in the graph.
8. User asks it to inspect the system, start work, spawn workers, or answer normal questions.
9. Any spawned nodes appear in the graph.
10. User can inspect, attach to, message, interrupt, or stop any node.

Command-center chat is not a custom chatbot. It is a real harness session.

### Node attach

V1 supports both:

- browser attach as the portable default,
- native attach where available.

Browser attach should work from Cockpit and remote clients that can open a URL. Native attach should open the best local harness experience when possible.

### Graph semantics

Edges represent intentional responsibility:

- `supervises`,
- `spawned_for`,
- user-created relationship,
- platform-created responsibility relationship.

`created_by` is provenance and does not automatically create an edge.

## 9. Root Capabilities

Every v1 product affordance maps to a documented root capability. Every root capability is available through the typed API and should be exposed through CLI and MCP unless the client physically cannot support it, in which case the client returns a link, token, or explanation.

**MCP catalog parity (as of PR 6):** `node.create`, `node.list`, `node.inspect`, `node.send_input`, `node.interrupt`, `node.stop`, `node.archive`, `node.events`, `node.fork`, `node.attach_url`, `graph.get`, `relationship.create`, `notify.send`, `attach_url.issue` — all wired in `crates/asylum-cli/src/mcp.rs`.

### Node capabilities

- `node.create`
- `node.list`
- `node.inspect`
- `node.observe`
- `node.send_input`
- `node.interrupt`
- `node.stop`
- `node.terminate`
- `node.archive`
- `node.attach.browser`
- `node.attach.native_target`
- `node.restart` where supported
- `node.resume` where supported

### Relationship capabilities

- `relationship.create`
- `relationship.remove`
- `relationship.list`
- `graph.get`

### Harness capabilities

- `harness.list`
- `harness.inspect`
- `harness.configure`
- `harness.capabilities`
- `harness.launch_context`

### Substrate capabilities

- `substrate.list`
- `substrate.inspect`
- `substrate.health`
- `substrate.launch_node`
- `substrate.stop_node`
- `substrate.diagnostics`

### Workspace and context capabilities

- `workspace.list_recent`
- `workspace.inspect`
- `context.current_system_map`
- `context.launch_packet`
- `artifact.list_refs`
- `artifact.add_ref`

### Notification and remote command capabilities

- `notify.channels.list`
- `notify.send`
- `remote_command.receive`
- `remote_command.reply`
- `decision.request`
- `decision.resolve`

### Connection capabilities

- `client.config`
- `token.issue`
- `token.revoke`
- `base_url.inspect`
- `attach_url.issue`

## 10. Harness Adapters

V1 requires Codex and Claude Code adapters. The adapters should use the best available integration for each harness without pretending all harnesses have identical capabilities.

### Required baseline

Both v1 adapters must support:

- launch node,
- observe output,
- send input,
- browser attach,
- native attach target where possible,
- interrupt,
- stop,
- liveness reporting,
- local substrate execution,
- Loon substrate execution where configured,
- Asylum-aware launch context.

### Optional capability flags

Adapters may declare:

- structured event stream,
- context usage telemetry,
- native resume,
- native approval telemetry,
- subagent visibility,
- tool-call telemetry,
- transcript export,
- permission prompt classification,
- auto-compaction awareness,
- checkpoint/handoff support.

The UI and API must expose optional capabilities honestly. Missing optional features should degrade gracefully.

## 11. Substrate Adapters

### Local substrate

The local substrate launches harness nodes on the same machine as Asylum or another configured local execution environment.

It must support:

- launch,
- liveness,
- output observation,
- input delivery,
- browser attach where possible,
- native attach where possible,
- stop/interruption.

### Loon substrate

Loon is optional to configure but implemented in v1.

Asylum must support:

- detect configured Loon hosts,
- show Loon health,
- show enough capacity/status to make node placement understandable,
- create Codex and Claude Code nodes on Loon where supported,
- attach to Loon-backed nodes,
- observe output/events,
- send input,
- interrupt,
- stop,
- surface Loon-specific failure modes,
- let command-center or supervisor nodes create Loon-backed worker nodes through the same capability API.

## 12. Notifications And Remote Commands

V1 includes ntfy as the baseline personal notification and remote command channel.

Outbound notifications include:

- node started,
- node stopped or exited,
- node asks for human input,
- node or harness failure,
- substrate failure,
- long-running work reaches a checkpoint,
- explicit message from a node to the user,
- command-center summary,
- attach link delivery.

Inbound remote commands include:

- request status,
- request attach link,
- send input to a node,
- start a command center or node,
- interrupt,
- stop,
- approve or deny a decision prompt,
- reply to a node/harness question.

Dashboard notifications also exist. Richer channels such as Signal, Telegram, Discord, Slack, and email can be future adapters.

## 13. API, CLI, MCP, And Third-Party Clients

Asylum exposes one typed capability API. CLI, MCP, dashboard, command-center chat, and third-party chatbot integrations all use that same API.

There is one installed binary, `asylum`. Local CLI/MCP control talks to the daemon through `~/.asylum/run/asylum.sock`; Cockpit remains on daemon HTTP/WebSocket routes.

Third-party clients can do whatever their host environment permits:

- inspect nodes,
- ask what is happening,
- start nodes,
- send input,
- interrupt or stop,
- request attach URLs,
- resolve decisions.

If the third-party client cannot render a live terminal, it can still return an attach URL.

## 14. Security And Exposure

Asylum v1 is single-user.

Security requirements:

- bind to localhost by default,
- explicit network exposure configuration,
- Unix-socket local control for CLI/MCP,
- owner token or pairing credential for API, MCP, CLI, and remote channels,
- dashboard authentication,
- attach URLs protected by auth/session tokens,
- visible warning when exposed beyond localhost,
- secure storage of channel secrets,
- practical secret/output redaction in UI and logs,
- no multi-user RBAC in v1.

Remote access should be Tailscale/local-network/reverse-proxy friendly. V1 should not build a hosted relay or SaaS identity layer.

## 15. Optional Recipes

Recipes are reusable prompts or profiles that use root capabilities. They are not kernel workflows.

V1 ships lightweight starter recipes:

- start a command center,
- spawn worker nodes,
- observe and summarize current system,
- run a plan to completion,
- checkpoint or hand off a node,
- parallel exploration.

Users and harnesses can ignore, modify, or replace recipes.

## 16. V1 Completion Bar

V1 is complete only when all of the following work end to end:

- [x] persistent Asylum service starts and remains useful with zero nodes,
- [x] graph-first Cockpit is available,
- [x] table view exists as secondary view,
- inline persistent command-center chat launches a real Codex or Claude Code node,
- [x] Codex adapter works,
- [x] Claude Code adapter works,
- [x] local substrate works,
- Loon substrate works,
- [x] browser attach works,
- [x] native attach target works where available,
- [x] CLI uses root capabilities,
- [x] MCP server uses root capabilities (PR 6 — node.archive, node.fork, relationship.create, notify.send, attach_url.issue all wired),
- [x] API or typed SDK contract exists,
- [x] ntfy outbound notifications work,
- [~] ntfy inbound transport works (PR 3 — daemon subscribes to ntfy.sh JSON stream, cockpit polls for direction=in, channel.inbound hook fires). Auto-routing of inbound messages into a target node's input stream is the remaining piece (needs a node_id column on channel_messages and a small outbound→reply correlation table); user can act on inbound today via hooks,
- [x] dashboard notification center exists,
- basic remote connection setup exists,
- [x] capability flags are visible per harness/substrate,
- optional starter recipes are available.

## 17. V1 Wow Sequence

The product should demonstrate value in this order:

1. Open Cockpit.
2. Start a Codex command-center inline.
3. Ask it to spawn Claude Code and Codex worker nodes.
4. See nodes appear in the graph.
5. Inspect each node's output and substrate.
6. Attach to any node in browser.
7. Open native attach where available.
8. Receive ntfy notification from a node.
9. Reply through ntfy to send input or request an attach link.
10. Use an MCP-capable client to ask what Asylum is doing and start or message a node.
11. From Codex or Claude Code locally, use Asylum tools to launch Loon-backed workers.

## 18. Open Questions For Implementation Planning

- Exact API transport and schema format.
- Best browser attach implementation for Codex and Claude Code PTY/TUI.
- Native attach strategy per OS.
- How much structured telemetry each harness can expose in v1.
- Loon integration details for Codex if Codex subscription/session setup differs from Claude Code.
- Whether node output storage keeps full transcripts, references, summaries, or all three.
- How ntfy inbound commands are authenticated and correlated to nodes.
- Minimal packaging strategy for Asylum service plus clients.

These are planning questions, not product-shape blockers.
