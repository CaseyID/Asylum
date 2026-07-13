# Orchestration Alignment Implementation Plan

**Status:** planned, not started. The product-level alignment (spec `LAYER-*`,
`HARN-005`..`HARN-007`, `UX-005`, `DOC-007`, plus
[docs/concepts/orchestration-layers.md](../../concepts/orchestration-layers.md))
landed as docs on 2026-07-12; this plan is the code-level follow-through so it
can be executed quickly in one later delivery step.

**Date:** 2026-07-12

**Owner:** Casey. Execution: one delivery cycle, branch per workstream.

Mirror these workstreams into the Linear `Asylum` project when the connector
is available (milestone: `Agent and operator coordination`); this doc is the
execution plan, Linear remains the canonical backlog.

## What this delivers

Two spec gaps are implementation gaps today:

1. The injected coordination guidance (launch packet) has no layer-choice or
   verification etiquette, and its closing rule ("Do not simulate worker
   nodes... spawn a real node for real parallel work") reads as banning
   in-harness subagents — the wrong layer for fine-grained fan-out.
   (`LAYER-003`, `LAYER-004`)
2. Launch profiles do not exist: `CreateNodeRequest`/`SpawnPeerRequest` carry
   no model/effort options, adapters never pass them, node records never store
   them, and the Cockpit launch form cannot set them. (`HARN-005`..`HARN-007`,
   `UX-005`)

## Workstream 1 — Launch-packet etiquette (small)

Branch: `launch-packet-layer-etiquette`. Touches
`crates/asylum-daemon/src/launch_packet.rs` and its tests only.

- [ ] Add a short "Choosing the right layer" etiquette block to
  `FLEET_OPERATING_MANUAL`, in the concepts doc's preference order: direct
  work in-session; in-harness subagents/workflows for fine-grained fan-out
  inside one body of work; `node.spawn_peer` for work needing independent
  lifetime, isolation, separate supervision, or a different
  workspace/harness/substrate/launch profile.
- [ ] Add verification etiquette: verify substantial results in a fresh
  context with a distinct adversarial framing (evaluator peer or in-harness
  equivalent); same-context self-review is weak. Recommendation, not a gate.
- [ ] Reword the final etiquette line so it bans fiction, not in-harness
  parallelism: "Never simulate a worker in your own transcript. Real fan-out
  is either a real in-harness subagent or a real node."
- [ ] Extend the drift-guard test so the manual must contain the layer-choice
  and verification etiquette markers (same pattern as the existing
  catalog-event guard), keeping `LAYER-003`'s "drift-checked" acceptance true.

Keep the manual terse — it is a reference sheet a harness reads once. Do not
inline the whole concepts doc.

## Workstream 2 — Launch profile end to end (medium)

Branch: `launch-profile`. The dumb-plumbing rule governs everything here:
Asylum passes profile values through verbatim, maintains no model/effort
catalogs, and surfaces harness rejections honestly (`HARN-005`).

- [ ] **Verify harness mechanisms first** (the harness-contract-notes
  practice: check live `--help` on this machine before coding):
  - Claude Code: `--model <value>` per-launch; confirm the current mechanism
    for per-launch reasoning effort (expected: a settings key via the
    already-used `--settings '<inline JSON>'` injection; confirm exact key and
    accepted values against the installed version).
  - Codex: `-c model=<value>`; confirm the reasoning-effort config key
    (expected: `-c model_reasoning_effort=<value>`) against the installed
    version.
  - Record findings in `docs/superpowers/specs/2026-07-06-harness-contract-notes.md`
    (append a dated section).
- [ ] **Types** (`crates/asylum-types`): optional `model: Option<String>` and
  `effort: Option<String>` on `CreateNodeRequest` and `SpawnPeerRequest`
  (spawn inherits none from the parent unless explicitly set — profile is not
  accidental control state, per `WORK-005`'s spirit). Persist the effective
  profile on the node record with an explicit harness-default marker when
  unset (`HARN-007`).
- [ ] **Adapters** (`crates/asylum-daemon` harness adapters): translate
  profile fields into the verified per-launch flags/config for local claude
  and codex launches; Loon launches carry the same fields through the guest
  launch contract. An option the adapter cannot express returns an honest
  unsupported error (`CAP-012` shape), never a silent no-op.
- [ ] **Descriptors**: harness descriptors advertise which profile options the
  adapter supports (`supports_model`, `supports_effort`), so Cockpit/CLI/MCP
  can offer only real controls (`HARN-005`). No hardcoded model lists.
- [ ] **CLI**: `asylum node create --model ... --effort ...` and the same on
  `node spawn`; inspect output shows the recorded profile.
- [ ] **MCP**: `node.create`/`node.spawn_peer` tool schemas gain the optional
  params; the launch-packet manual's spawn documentation is updated in the
  same PR (its drift test will force this).
- [ ] **Storage**: profile fields on the node row + wire contracts; recorded
  at launch time from what was actually applied, surviving restart/resume.
- [ ] Unit/integration tests across types, adapters (arg construction),
  descriptors, CLI parse, MCP schema.

## Workstream 3 — Cockpit surfacing (small)

Branch: `cockpit-launch-profile`. Depends on workstream 2's API.

- [ ] `CreateScreen`: advanced launch-profile controls (model, effort),
  rendered only when the selected harness descriptor advertises support;
  free-text with harness-default placeholder, not an Asylum-owned dropdown
  catalog (`UX-005`).
- [ ] Node detail/inspect surfaces: show the recorded profile or "harness
  default" for live and historical nodes (`HARN-007`).
- [ ] Vitest coverage for the conditional controls and the detail display.

## Verification

- Full suite green (`cargo test-asylum`), including the extended launch-packet
  drift guards.
- One frugal live check (Casey's subscription, trivial prompt, stopped
  promptly): create a local claude node with an explicit non-default model and
  confirm via the statusline/hook payload `model` field that the harness
  actually launched with it; inspect shows the recorded profile. This is the
  `HARN-007` "reflects what was actually launched" acceptance, not an
  Asylum-side echo.
- Spot-check a spawn_peer with `effort` set from a supervisor session (can be
  same live check).

## Workstream 4 — Outstanding v0.2.0 follow-ups (close them, small)

The two loose ends left open at mission close ride this delivery so nothing
stays unresolved:

- [ ] **Menu-dialog answer fidelity** (`DECISION-004`): a decision resolution
  answering a menu-style harness question must select the named non-default
  option, not deliver Enter-takes-default. Investigate the harness's menu
  input contract (arrow-key/number sequences over the PTY), implement typed
  delivery for claude AskUserQuestion-style menus, and prove a non-default
  selection live. Remove the corresponding README known-limit line when done.
- [ ] **Claude local PTY crash follow-up**: the one-time claude 2.1.202
  local-`portable_pty` "output.write assertion" crash from the final live
  check. Reproduce against the currently installed claude version first; if
  unreproducible on current versions, record that and close it; if
  reproducible, pin/document the failing version range and fix or guard the
  local launch path.

## Explicitly out of scope (backlog, not this plan)

- Surfacing harness-internal orchestration telemetry (`HARN-004` subagent
  visibility) as node facts in Cockpit — record in Linear; needs harness-side
  signal research first.
- Node naming, completion criteria, and monitoring-policy launch fields —
  already tracked as the work-envelope launch gap (`WORK-001`+, README known
  limits); this plan does not absorb it.
- Any Asylum-side orchestration engine or workflow DSL — permanent non-goal.

## Release status

Doc-level alignment is on main, not released — doc-only, no release needed.
Implementation (workstreams 1-3) not started; when delivered it will be
user-facing and should ship in the next cut release. Last release: v0.2.0
(2026-07-07). See [RELEASES.md](../../../RELEASES.md).
