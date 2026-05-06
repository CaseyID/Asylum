# Asylum Docs Map

Use this file to decide which docs are current product references, which are
audit/tracking records, and which are historical background.

## Start Here

If you are reviewing the current spec-coverage PR, read these in order:

1. [`specs/asylum-current-product-spec.md`](specs/asylum-current-product-spec.md) - current product contract.
2. [`reviews/2026-05-05-asylum-spec-coverage-findings.md`](reviews/2026-05-05-asylum-spec-coverage-findings.md) - what the audit found, what was fixed, live evidence, test evidence, and remaining backlog.
3. [`context/2026-05-05-asylum-spec-audit-goal.md`](context/2026-05-05-asylum-spec-audit-goal.md) - the controlling goal/prompt for that audit.

For a fast PR review, the findings doc is the main artifact. The goal doc only
explains why the findings doc exists.

## Current Product Sources

| Doc | Status | Purpose |
|---|---|---|
| [`specs/asylum-current-product-spec.md`](specs/asylum-current-product-spec.md) | current | Canonical current product/spec contract. Prefer this over older PRDs, handoffs, and dated plans. |
| [`prd/asylum-live-v2-prd.md`](prd/asylum-live-v2-prd.md) | background | Original product-intent source. Useful for intent, but superseded by the current spec when they disagree. |
| [`context/source-trail.md`](context/source-trail.md) | background | Source trail/context notes. Not a delivery plan. |

## Current Audit And Review Records

| Doc | Status | Purpose |
|---|---|---|
| [`reviews/2026-05-05-asylum-spec-coverage-findings.md`](reviews/2026-05-05-asylum-spec-coverage-findings.md) | current audit result | Completed repo-vs-current-spec audit, evidence matrix, fix log, browser/runtime evidence, commands, and follow-on queue. |
| [`reviews/2026-05-05-asylum-spec-coverage-audit-brief.md`](reviews/2026-05-05-asylum-spec-coverage-audit-brief.md) | audit input | Brief used to start the full coverage audit. Superseded by the findings doc for results. |
| [`context/2026-05-05-asylum-spec-audit-goal.md`](context/2026-05-05-asylum-spec-audit-goal.md) | audit control | Goal/instructions for the audit run. Not a product spec. |

## Handoffs And Delivery Ledgers

Handoff docs are delivery records. They are useful for reconstructing what
landed and what was deferred at that time, but they are not the current product
contract.

| Doc | Status | Purpose |
|---|---|---|
| [`handoff/2026-04-29-cockpit-deliverability-and-prototype-cleanup.md`](handoff/2026-04-29-cockpit-deliverability-and-prototype-cleanup.md) | historical delivery record | Cockpit deliverability/prototype-cleanup handoff. Updated in the current PR only to keep release status/tracking honest. |
| [`handoff/2026-04-30-release-tooling-and-cli-composability-handoff.md`](handoff/2026-04-30-release-tooling-and-cli-composability-handoff.md) | historical delivery record | Release tooling / CLI composability handoff. Updated in the current PR only for release-status normalization. |
| [`handoff/transition-to-implementation-planning.md`](handoff/transition-to-implementation-planning.md) | older background | Transition planning from earlier implementation work. |

## Older Reviews And Plans

Dated reviews and plans are point-in-time artifacts. Treat them as historical
unless the current spec or the latest findings doc explicitly points back to
them.

| Doc | Status | Purpose |
|---|---|---|
| [`reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md`](reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md) | historical plan/audit | Older Cockpit audit plan. Updated in the current PR only for release-status normalization. |
| [`reviews/2026-04-29-local-ultrareview-findings.md`](reviews/2026-04-29-local-ultrareview-findings.md) | historical review | Companion review findings from the older Cockpit delivery. |
| [`reviews/2026-04-30-cli-composability-and-uninstall.md`](reviews/2026-04-30-cli-composability-and-uninstall.md) | historical review | Older CLI composability/uninstall review. |
| [`reviews/2026-05-04-asylum-architecture-refactor-spec.md`](reviews/2026-05-04-asylum-architecture-refactor-spec.md) | dated spec/review | Architecture refactor spec from the prior slice. Check the current product spec first. |
| [`superpowers/plans/*`](superpowers/plans/) | historical plans | Execution plans from earlier work. Not current product truth by default. |
| [`superpowers/specs/*`](superpowers/specs/) | historical specs | Earlier scoped specs. Prefer the current product spec unless explicitly referenced. |

## Rule Of Thumb

- Product truth: `docs/specs/asylum-current-product-spec.md`.
- Current PR/audit truth: `docs/reviews/2026-05-05-asylum-spec-coverage-findings.md`.
- Release truth: [`../RELEASES.md`](../RELEASES.md).
- Handoffs/plans/reviews older than the current findings doc are historical unless a current doc says otherwise.
