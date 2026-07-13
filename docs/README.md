# Asylum Docs Map

This repository keeps current product documentation plus a small amount of
explicitly labeled delivered evidence. Older PRDs, handoffs, audits, and broad
implementation history were removed so fresh agents do not treat stale records
as product truth.

## Current Product Sources

| Doc | Purpose |
|---|---|
| [specs/asylum-current-product-spec.md](specs/asylum-current-product-spec.md) | Canonical product contract. Start here for requirements and audits. |
| [concepts/orchestration-layers.md](concepts/orchestration-layers.md) | The coordination layer model: where Asylum sits relative to harness-internal parallelism, what each layer isolates, verification and launch-profile doctrine. |
| [../README.md](../README.md) | User-facing install, run, and source-development path. |
| [../RELEASES.md](../RELEASES.md) | Manual release ledger and release-process rules. |
| [../AGENTS.md](../AGENTS.md) | Agent-facing repo conventions and active branch context. |
| [backlog.md](backlog.md) | Linear-backed feedback intake, product-review, triage, and delivery workflow. |

## Planned — Not Yet Implemented

| Doc | Purpose |
|---|---|
| [superpowers/plans/2026-07-12-orchestration-alignment.md](superpowers/plans/2026-07-12-orchestration-alignment.md) | Execution plan for the spec's `LAYER-*` and launch-profile (`HARN-005`..`HARN-007`) requirements: launch-packet etiquette, per-node model/effort plumbing, Cockpit launch-profile controls. Mirror into Linear when the connector is available. |

## Delivered Evidence — Not Current Work

| Doc | Purpose |
|---|---|
| [superpowers/specs/2026-05-09-cockpit-node-session-ux-design.md](superpowers/specs/2026-05-09-cockpit-node-session-ux-design.md) | Delivered session-first interaction invariant retained by the current spec. |
| [superpowers/plans/2026-05-09-cockpit-node-session-ux.md](superpowers/plans/2026-05-09-cockpit-node-session-ux.md) | Completed implementation/verification history; its old release status is superseded by `RELEASES.md`. |
| [superpowers/plans/2026-05-10-harness-asylum-control.md](superpowers/plans/2026-05-10-harness-asylum-control.md) | Delivered `node.spawn_peer` and local MCP-injection evidence; shipped in v0.2.0. |
| [superpowers/specs/2026-07-06-asylum-completion-mission.md](superpowers/specs/2026-07-06-asylum-completion-mission.md) | Completed v0.2.0 north-star mission and live evidence; useful for regression context, not an active plan. Its linked phase plans, notes, and review were pruned 2026-07-12; recover from git history if needed. |
| [superpowers/specs/2026-07-06-harness-contract-notes.md](superpowers/specs/2026-07-06-harness-contract-notes.md) | Living reference: verified Claude Code/Codex per-launch injection, hooks, notify, and resume contracts. Append dated sections when re-verifying. |
| [superpowers/specs/2026-07-07-loon-guest-contract.md](superpowers/specs/2026-07-07-loon-guest-contract.md) | Living reference: the Loon guest control contract the loon substrate is written against. |

## Rule Of Thumb

- Product truth lives in `docs/specs/asylum-current-product-spec.md`.
- Backlog and feedback truth lives in the existing Linear `Asylum` project; `backlog.md` defines the agent workflow.
- Files under `docs/superpowers/` are completed delivery evidence unless this
  map lists one as planned or a branch explicitly names one as its active plan.
- Release truth lives in `RELEASES.md`.
- If an old audit, handoff, or PRD is needed, recover it from git history rather than restoring it as live documentation.
