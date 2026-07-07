# Asylum Docs Map

This repository keeps only current product documentation and active branch notes.
Older PRDs, handoffs, audits, and implementation-history files were removed so
fresh agents do not treat stale development records as product truth.

## Current Product Sources

| Doc | Purpose |
|---|---|
| [specs/asylum-current-product-spec.md](specs/asylum-current-product-spec.md) | Canonical product contract. Start here for requirements and audits. |
| [../README.md](../README.md) | User-facing install, run, and source-development path. |
| [../RELEASES.md](../RELEASES.md) | Manual release ledger and release-process rules. |
| [../AGENTS.md](../AGENTS.md) | Agent-facing repo conventions and active branch context. |

## Active Branch Notes

| Doc | Purpose |
|---|---|
| [superpowers/specs/2026-05-09-cockpit-node-session-ux-design.md](superpowers/specs/2026-05-09-cockpit-node-session-ux-design.md) | Session-first Cockpit UX intent. |
| [superpowers/plans/2026-05-09-cockpit-node-session-ux.md](superpowers/plans/2026-05-09-cockpit-node-session-ux.md) | Implementation checklist and verification evidence for this branch. |
| [superpowers/plans/2026-05-10-harness-asylum-control.md](superpowers/plans/2026-05-10-harness-asylum-control.md) | `node.spawn_peer` + local harness MCP injection plan. Delivered on main; not yet released. |

## Active Mission

| Doc | Purpose |
|---|---|
| [superpowers/specs/2026-07-06-asylum-completion-mission.md](superpowers/specs/2026-07-06-asylum-completion-mission.md) | Current mission document driving Asylum to a genuinely working autonomous-supervision state. |

## Rule Of Thumb

- Product truth lives in `docs/specs/asylum-current-product-spec.md`.
- Branch-local execution notes live under `docs/superpowers/`.
- Release truth lives in `RELEASES.md`.
- If an old audit, handoff, or PRD is needed, recover it from git history rather than restoring it as live documentation.
