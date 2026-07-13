# Orchestration Layers

**Status:** current product concept doc. Companion to the
[current product spec](../specs/asylum-current-product-spec.md); the spec's
`LAYER-*` and `HARN-005`..`HARN-007` requirements are the auditable form of
this document.

**Updated:** 2026-07-12

## Why this document exists

Asylum is one layer in a stack of coordination mechanisms, not a replacement
for any of them. Modern harnesses already parallelize internally: Claude Code
has subagents, agent teams, and scripted multi-agent workflows; Codex has its
own delegation mechanisms; future harnesses will have more. Asylum must be
designed, documented, and operated with a precise answer to "when does work
belong inside one node, and when does it become another node?" Getting this
wrong in either direction produces a worse product:

- If Asylum tries to own fine-grained fan-out, it reimplements — badly, over a
  PTY — orchestration the harness vendors already ship and tune.
- If coordinators are taught to spawn a peer node for every parallel thought,
  they pay session-granularity coordination cost (launch, auth, workspace,
  supervision) for work that needed a subagent.

## The layer model

Each layer has a different unit of work, and the unit determines everything
else: what is isolated, what coordination costs, and how long the unit lives.

| Layer | Unit of work | Isolation | Coordination cost | Typical lifetime | Owned by |
|---|---|---|---|---|---|
| Harness tool call | One function call | None | ~Free | Milliseconds | Harness |
| Harness-internal parallelism (subagents, agent teams, scripted workflows) | A context window | Context only — shares the node's process, filesystem, credentials | Cheap (in-process) | Seconds to minutes | Harness |
| Asylum node | A whole harness session | Process/session; kernel-level on Loon | Expensive (launch, auth, workspace, supervision) | Minutes to days | Asylum |
| Substrate | Where a node's blast radius ends | Local: shared machine. Loon: microVM boundary | Chosen at launch | Node lifetime | Asylum + Loon |

**Nesting is the intended composition.** An Asylum node runs a real harness;
that harness keeps every internal parallelism tool it would have in a bare
terminal. A supervisor node coordinating three worker nodes, where each worker
internally fans out subagents for its own sub-tasks, is the system working as
designed — not a conflict between two orchestration models.

```text
Asylum fleet            long-lived, isolated, separately supervisable sessions
  └── node              a real harness session (Claude Code, Codex, ...)
        └── harness-internal parallelism   subagents / teams / workflows
              └── tool calls               the only primitive that exists
```

Anti-patterns, both directions:

- Do not spawn an Asylum peer node to do what one `grep`, one file read, or
  one in-harness subagent would do. The coordination cost is orders of
  magnitude too high for the unit of work.
- Do not run a long-horizon, unattended, or untrusted body of work as an
  in-harness subagent when it needs independent lifetime, its own supervision,
  durability across the parent's death, or isolation from the host. That is
  what a node is for.

## What each layer isolates

Harness-internal parallelism isolates **context**: a subagent gets a fresh
context window and returns a bounded summary; the parent pays for the
conclusion, not the exploration. It isolates nothing else — subagents share
the node's filesystem, process tree, credentials, and machine.

Asylum isolates **blast radius**: a node is a separate session whose failure,
runaway behavior, or destructive mistake is contained to its own workspace —
and, on Loon, to a disposable microVM whose deletion cannot touch the host. A
coordinator mistake inside a Loon node destroys a VM, not a workstation.

These are different guarantees, and product surfaces should never imply one
provides the other. Local nodes share the owner's machine; the spec's
execution-posture requirements (`SEC-009`) exist because that fact must be
visible, not smoothed over.

## Context economy

Delegation at every layer is fundamentally a context-management technique:
fresh context in, bounded conclusion out. Asylum embodies this at session
granularity through mechanisms that already exist in the product contract:

- The **work envelope** (objective, assignment, completion criteria) gives a
  spawned node exactly the context it needs, not the parent's accumulated
  state (`WORK-005`).
- **Status summaries and results/artifacts** (`WORK-003`, `WORK-004`) are the
  bounded return channel — a coordinator or owner reads the envelope, not the
  transcript.
- **Handles and cursors, never unbounded dumps** (`HARN-003`, `MCP-006`) keep
  injected context and MCP views bounded.
- **Hooks over polling**: supervision by exception is the fleet-level form of
  "don't spend context watching."

The cost of every bounded return is that it is lossy and unauditable. Asylum's
answer is the two-channel design below, plus verification etiquette — not
bigger summaries.

## Two channels per node: typed envelope and live session

Every node exposes two complementary interaction surfaces, and they have
different jobs:

- The **typed coordination surface** — work envelope, status, results,
  artifacts, decisions, events, hooks — is the composition channel. It is what
  coordinators and automation consume, and it is structured because
  agent-to-agent handoff only composes over structured data.
- The **live session (PTY)** is the full-fidelity intervention channel. It is
  bytes because the harness owns its own interior; the owner opens it to see
  and steer everything, exactly as if at the terminal.

Asylum's structured surface is deliberately populated by **harness-native
reporting** (hooks, statuslines, explicit status/result publication), never by
Rust-side parsing of PTY output. This is the dumb-plumbing principle in layer
terms: harness models are post-trained on their own vendors' tool surfaces and
agent loops. Routing bytes to real harnesses and letting them self-report
inherits all of that tuning for free; interpreting bytes in Asylum would
discard it and create a second, worse brain.

## Verification: decorrelated errors, not smarter judges

The `evaluator` role exists because of a statistical fact, not an
organizational one: an agent that just spent a long session convincing itself
its work is correct is an anchored judge of that work. A fresh context with a
distinct framing ("try to refute this") has decorrelated errors, and
decorrelation — not extra intelligence — is what independent verification
buys.

Consequences for the product:

- Verification guidance (in injected coordination etiquette and docs) should
  recommend fresh-context review of substantial work: an evaluator peer node,
  or an in-harness subagent with a distinct adversarial framing — layer choice
  follows the layer model above.
- N identical reviewers sharing the worker's context and framing are
  confidence theater; guidance should say so.
- Asylum does not enforce a verification workflow. Like all coordination
  intelligence, whether and how to verify belongs to the harnesses and the
  owner. Asylum's job is to make the pattern cheap: spawnable evaluator peers,
  explicit edges, results that can be read without transcript scraping.

## Model and effort are fleet economics

Harnesses expose per-session model choice (e.g. Opus/Sonnet/Haiku tiers) and
reasoning-effort levels. At fleet scale these are the cost/quality dials:
mechanical fan-out belongs on cheaper, faster configurations; judgment stages
(synthesis, verification, final review) justify expensive ones. Two facts of
practice worth encoding in guidance:

- Effort is non-monotonic on total cost: higher effort often plans better and
  finishes in fewer turns. "Cheapest per token" is not "cheapest per outcome."
- Reflexive downgrading costs more than it saves when wrong answers survive
  into synthesis. Default to the harness's default; downgrade deliberately.

Asylum's responsibility is to make these **launch-time, per-node, recorded,
and visible** choices (`HARN-005`..`HARN-007`) — launch profile is part of a
node's identity, both for coordinators choosing worker configurations and for
the owner auditing what actually ran. Asylum does not pick models or efforts
itself, does not validate vendor model names, and passes profile options
through as dumb plumbing; the harness is authoritative and its errors are
reported honestly.

## Layer-choice etiquette (what coordinators are taught)

The injected launch packet is the delivery vehicle for this doctrine — it is
the one document every spawned coordinator actually reads. Its etiquette
should teach, in this order of preference:

1. Work you can do directly in your own session: do it. No delegation.
2. Fine-grained parallel fan-out inside one body of work (read many files,
   run many checks, draft alternatives): use your harness's own subagents or
   scripted workflows. They are cheap, fast, and share your workspace.
3. Work needing independent lifetime, separate supervision, isolation, a
   different workspace/harness/substrate, or survival beyond your own session:
   spawn an Asylum peer node with a concrete assignment and completion
   criteria.
4. Substantial results: have them verified in a fresh context with an
   adversarial framing before treating them as done.
5. Never simulate a worker — do not roleplay parallel sessions in your own
   transcript. Real fan-out is either a real in-harness subagent or a real
   node.

Point 5 is the honest scope of the older "do not simulate worker nodes" rule:
it bans fiction, not in-harness parallelism.

## What Asylum deliberately does not do

- No Asylum-owned workflow engine, task DSL, or scripted orchestration layer
  above or below the node graph. Scripted orchestration exists — inside
  harnesses, where the vendors tune it.
- No modeling of harness-internal subagents as nodes. If a harness reports
  internal orchestration telemetry, it may surface as node facts
  (`HARN-004`), never as graph objects.
- No Rust-side interpretation of harness output to infer coordination state.
- No hiding of layer differences: local vs Loon isolation, in-harness vs peer
  delegation, and per-node launch profiles are visible product facts.
