# Orchestration Alignment Implementation Plan

**Status:** DELIVERED 2026-07-13 on main. All four workstreams implemented,
adversarially reviewed, and merged; full suite green (200 daemon + 71 cli +
11 types Rust tests, 112 cockpit vitest); live checks recorded in the
Delivery record below. The product-level alignment (spec `LAYER-*`,
`HARN-005`..`HARN-007`, `UX-005`, `DOC-007`, plus
[docs/concepts/orchestration-layers.md](../../concepts/orchestration-layers.md))
landed as docs on 2026-07-12; this plan was the code-level follow-through.

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

- [x] Add a short "Choosing the right layer" etiquette block to
  `FLEET_OPERATING_MANUAL`, in the concepts doc's preference order: direct
  work in-session; in-harness subagents/workflows for fine-grained fan-out
  inside one body of work; `node.spawn_peer` for work needing independent
  lifetime, isolation, separate supervision, or a different
  workspace/harness/substrate/launch profile.
- [x] Add verification etiquette: verify substantial results in a fresh
  context with a distinct adversarial framing (evaluator peer or in-harness
  equivalent); same-context self-review is weak. Recommendation, not a gate.
- [x] Reword the final etiquette line so it bans fiction, not in-harness
  parallelism: "Never simulate a worker in your own transcript. Real fan-out
  is either a real in-harness subagent or a real node."
- [x] Extend the drift-guard test so the manual must contain the layer-choice
  and verification etiquette markers (same pattern as the existing
  catalog-event guard), keeping `LAYER-003`'s "drift-checked" acceptance true.

Keep the manual terse — it is a reference sheet a harness reads once. Do not
inline the whole concepts doc.

## Workstream 2 — Launch profile end to end (medium)

Branch: `launch-profile`. The dumb-plumbing rule governs everything here:
Asylum passes profile values through verbatim, maintains no model/effort
catalogs, and surfaces harness rejections honestly (`HARN-005`).

- [x] **Verify harness mechanisms first** (the harness-contract-notes
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
- [x] **Types** (`crates/asylum-types`): optional `model: Option<String>` and
  `effort: Option<String>` on `CreateNodeRequest` and `SpawnPeerRequest`
  (spawn inherits none from the parent unless explicitly set — profile is not
  accidental control state, per `WORK-005`'s spirit). Persist the effective
  profile on the node record with an explicit harness-default marker when
  unset (`HARN-007`).
- [x] **Adapters** (`crates/asylum-daemon` harness adapters): translate
  profile fields into the verified per-launch flags/config for local claude
  and codex launches; Loon launches carry the same fields through the guest
  launch contract. An option the adapter cannot express returns an honest
  unsupported error (`CAP-012` shape), never a silent no-op.
- [x] **Descriptors**: harness descriptors advertise which profile options the
  adapter supports (`supports_model`, `supports_effort`), so Cockpit/CLI/MCP
  can offer only real controls (`HARN-005`). No hardcoded model lists.
- [x] **CLI**: `asylum node create --model ... --effort ...` and the same on
  `node spawn`; inspect output shows the recorded profile.
- [x] **MCP**: `node.create`/`node.spawn_peer` tool schemas gain the optional
  params; the launch-packet manual's spawn documentation is updated in the
  same PR (its drift test will force this).
- [x] **Storage**: profile fields on the node row + wire contracts; recorded
  at launch time from what was actually applied, surviving restart/resume.
- [x] Unit/integration tests across types, adapters (arg construction),
  descriptors, CLI parse, MCP schema.

## Workstream 3 — Cockpit surfacing (small)

Branch: `cockpit-launch-profile`. Depends on workstream 2's API.

- [x] `CreateScreen`: advanced launch-profile controls (model, effort),
  rendered only when the selected harness descriptor advertises support;
  free-text with harness-default placeholder, not an Asylum-owned dropdown
  catalog (`UX-005`).
- [x] Node detail/inspect surfaces: show the recorded profile or "harness
  default" for live and historical nodes (`HARN-007`).
- [x] Vitest coverage for the conditional controls and the detail display.

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

- [x] **Menu-dialog answer fidelity** (`DECISION-004`): a decision resolution
  answering a menu-style harness question must select the named non-default
  option, not deliver Enter-takes-default. Investigate the harness's menu
  input contract (arrow-key/number sequences over the PTY), implement typed
  delivery for claude AskUserQuestion-style menus, and prove a non-default
  selection live. Remove the corresponding README known-limit line when done.
- [x] **Claude local PTY crash follow-up**: the one-time claude 2.1.202
  local-`portable_pty` "output.write assertion" crash from the final live
  check. Reproduce against the currently installed claude version first; if
  unreproducible on current versions, record that and close it; if
  reproducible, pin/document the failing version range and fix or guard the
  local launch path.
  **Outcome (2026-07-13): CLOSED, not reproducible.** A standalone repro
  mirroring `local.rs`'s exact launch sequence (openpty -> spawn_command ->
  reader thread -> two-write `/exit` submit) ran 5/5 clean against claude
  2.1.207 / portable-pty 0.8.1 — exit 0 every time, no assertion, no code
  change. The failing range could not be pinned (single occurrence ever; the
  2.1.202 binary is gone). If it recurs, capture the exact assertion text and
  `claude --version` at the time. Dated record appended to
  [2026-07-06-harness-contract-notes.md](../specs/2026-07-06-harness-contract-notes.md).

## Explicitly out of scope (backlog, not this plan)

- Surfacing harness-internal orchestration telemetry (`HARN-004` subagent
  visibility) as node facts in Cockpit — record in Linear; needs harness-side
  signal research first.
- Node naming, completion criteria, and monitoring-policy launch fields —
  already tracked as the work-envelope launch gap (`WORK-001`+, README known
  limits); this plan does not absorb it.
- Any Asylum-side orchestration engine or workflow DSL — permanent non-goal.

## Delivery record (2026-07-13)

Executed as four reviewed branches squash-merged to main (`launch packet: add
layer-choice and verification etiquette`, `add launch profile (model/effort)
end to end; fork reproduces source profile`, `cockpit: surface launch-profile
controls in create and node screens`, `add typed menu-option delivery for
claude askuserquestion decisions`). Adversarial review confirmed and fixed two
findings pre-merge: fork was silently dropping the source profile
(`ForkNodeRequest` gained optional model/effort, fork reproduces the source
profile with override), and menu routing gained a service-layer regression
test. Full suite green post-merge: 200 daemon + 71 cli + 11 types Rust tests,
112 cockpit vitest.

Live acceptance (dev daemon 127.0.0.1:7788, evidence in
`~/.claude/jobs/8fda2981/tmp/live-check-evidence/`):

- HARN-005/007 PASS: node created with `--model haiku --effort low` — real
  process argv carried both flags, inspect recorded `model=haiku effort=low`,
  and the session transcript's assistant turn was served by
  `claude-haiku-4-5-20251001` (the harness actually launched with it).
- HARN-006 PASS: `POST /api/nodes/{id}/spawn` with model/effort produced a
  peer whose argv and inspect both carried the profile.
- HARN-007 marker PASS: a node created without a profile showed
  `model=None effort=None` and an argv with no profile flags.
- DECISION-004 PASS: an AskUserQuestion menu (Apple/Banana/Cherry) created a
  pending decision carrying the option labels; answer "Durian" returned an
  honest 400 naming the stored options and left the decision pending; answer
  "Cherry" (non-default) selected the menu option and the node wrote
  `PICKED=Cherry`.

One new finding surfaced during acceptance, fixed in this same delivery:
claude 2.1.207's longer welcome//rc-connecting startup swallowed the
launch-prompt auto-delivery (the quiet-window heuristic in
`substrate/local.rs` typed too early; manual `node send` afterward landed
fine).

**Workstream 5 (added during acceptance) — claude launch-prompt readiness.**
Instrumentation proved neither PTY-output quiescence nor the `SessionStart`
event can gate delivery (the composer swallows input for ~9s after both).
Delivered fix: claude-only deliver-and-confirm — floor-gated on
`session_started`, confirmed via a newly injected async `UserPromptSubmit`
hook, redelivering at a 15s interval up to 3 attempts, with an accepted-latch
closing the notify/wait race; codex path split off byte-identical.
Adversarially reviewed; two findings remediated (retimed interval/budget to
bound the duplicate-delivery worst case at 2 with every redelivery
warn-logged; acceptance-latch exits logged so the narrow operator-takeover
case is diagnosable — full correlation would require TUI parsing, which is
barred). Live-proven twice: prompt lands unassisted, exactly once
(confirmation latency 0.9s vs the 15s redelivery interval).

Both acceptance-time backlog observations were closed same-cycle (2026-07-13,
owner instruction), each implemented, adversarially reviewed, and live-proven:

- **Claude token telemetry**: claude 2.1.207's statusline payload carries
  real token counts (`context_window.total_input_tokens/total_output_tokens`)
  that `ingest_statusline` was discarding; they now flow into
  `tokens_in`/`tokens_out` (char/4 estimate remains the fallback when absent).
  Live-proven: inspect showed `tokens_in=32651 tokens_out=63` after one turn.
  Semantics documented honestly in-code: these are the harness's
  context-window occupancy snapshot (including cached tokens), not a
  turn-cumulative sum, and are not magnitude-comparable to codex's estimate —
  a true cross-harness cumulative would need per-turn structured usage (e.g.
  transcript usage on `turn_complete`), recorded as a possible future
  refinement, not a gap in this delivery.
- **Loon deliver-and-confirm port**: the loon substrate now uses the same
  claude launch-prompt mechanism as local (SessionStart floor,
  UserPromptSubmit confirmation latch, 15s x3 retry, logged redeliveries and
  latch exits), with `post_harness_event` dispatching confirmation signals to
  the substrate that owns the node; codex-on-loon and loon pacing unchanged.
  Live-proven on a real microVM: `session_started` arrived from the guest,
  the prompt landed unassisted exactly once, and the prompt-accepted
  confirmation routed to the loon substrate.

## Release status

Released as [v0.3.0](https://github.com/CaseyID/Asylum/releases/tag/v0.3.0)
on 2026-07-13 (owner-authorized cut): all four platforms (linux-x86_64,
linux-arm64, darwin-arm64, darwin-x86_64), signed checksums, packaging
validated via `cargo test-asylum-release`. Ledger row updated in
[RELEASES.md](../../../RELEASES.md). Deployed to this machine's installed
daemon 2026-07-14 via `asylum update` (owner instruction): binary and daemon
report 0.3.0, doctor ready, service running, and a one-session live smoke on
the installed daemon proved the release features together — a
`--model haiku --effort low` node carried both flags in its real argv,
recorded the profile in inspect (still visible after stop), landed its launch
prompt unassisted, and reported real token telemetry
(tokens_in=30887, tokens_out=356); graceful stop, no stray processes.
