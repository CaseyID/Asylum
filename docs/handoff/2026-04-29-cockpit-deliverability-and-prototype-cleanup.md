# Handoff — Cockpit Deliverability + Prototype Cleanup

Date: 2026-04-29

## Purpose

After the 9 High-severity findings from the 2026-04-29 local-ultrareview were fixed and merged (PRs landed in commits `127814e`..`10585e6`), a second-pass audit on 2026-04-29 revealed that the cockpit (`cockpit/src/`) contains substantial prototype-era residue: client-side simulation machinery, hardcoded fake values masquerading as real settings, a "Potemkin" ntfy inbound feature with no daemon-side subscriber, and dead UI affordances throughout. Bringing Asylum to a releasable v1 requires removing all of that and implementing the missing daemon features that some cockpit UI advertises but cannot deliver.

## What to read first

1. **The audit + plan (canonical):** [../reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md](../reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md). Self-contained. Has a "Fresh Agent — Start Here" section at the top, a complete audit (Part A, every finding with file:line evidence), architectural decisions (Part B), 7-PR execution plan with TDD tasks (Part C), and verification matrix (Part D).
2. **Prior ultrareview report (companion):** [../reviews/2026-04-29-local-ultrareview-findings.md](../reviews/2026-04-29-local-ultrareview-findings.md). 54 findings; the 9 Highs are merged; Mediums/Lows are folded into PR 6 / PR 7 of the plan.
3. **PRD:** [../prd/asylum-live-v2-prd.md](../prd/asylum-live-v2-prd.md). The product spec.

## The principle that drives this work

Asylum is shipping as a real product. **No simulated, mocked, stubbed, canned, or demo-only behavior may exist in user-facing code.** Test fixtures and unit mocks are fine; behavior delivered by the running daemon + cockpit must be real end-to-end.

Concrete consequences:
- A button with `onClick={() => {}}` or `.catch(()=>{})` is a lie. Either wire it or delete it.
- A hardcoded value that could be derived from the daemon (version, bind, paths, counts) must be derived.
- Typed-state shapes inherited from the Claude Design Tool prototype (`Tweaks`, `simSpeed`, `still/slow/live` enums, `runResponse(seq)` canned-step animators, `SessionStep` types) are confessions — remove them, don't preserve them.
- A UI feature that references a backend that doesn't exist is a Potemkin facade. Implement the backend or remove the feature.

The plan in `docs/reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md` enforces this at every step.

## Suggested prompt for the next session

```text
We are in /home/casey/Projects/Asylum.

Read docs/handoff/2026-04-29-cockpit-deliverability-and-prototype-cleanup.md, then read
docs/reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md end to end (especially
the "Fresh Agent — Start Here" section and Part A audit).

Execute the plan PR-by-PR starting with PR 1 ("cockpit-strip-prototype-scaffolding").
Use the writing-plans-style checkboxes in each task; commit per task; update the
Status / what's done so far section in the plan as PRs land. Follow the no-simulation
principle stated in the plan.

If you have superpowers available, use superpowers:subagent-driven-development or
superpowers:executing-plans to drive PR execution.
```

## Execution order

The plan's PRs are ordered. Land them in sequence; PRs 5 and 6 may be parallelized after PR 4 lands.

1. **PR 1** — Strip prototype scaffolding from cockpit (Tweaks, simSpeed, runResponse, imperative bus, prototype seeds, dead enums)
2. **PR 2** — Replace fake Settings screen with real daemon-backed settings
3. **PR 3** — Implement ntfy inbound subscription on the daemon (closes ultrareview M18)
4. **PR 4** — Wire or remove dead UI affordances; Logs screen real semantics
5. **PR 5** — CmdK real semantics + node finder (parallelizable with PR 6)
6. **PR 6** — Remaining Mediums from the prior ultrareview (M1–M21 except M18)
7. **PR 7** — Release prep + end-to-end install verification

## Status

See the "Status / what's done so far" section in the plan itself; update it as PRs land.

## Conventions to preserve

- Commit message style: lowercase, terse, action verb first. Match recent history (`Fix H1: ...`, `cockpit: drop simSpeed and Tweaks`, etc.).
- Do not add Claude/AI attribution to commits, PRs, issues, or code unless the user explicitly asks.
- Each PR should leave the cockpit + daemon working at every checkpoint (no land-broken-fix-later patterns).
- TDD where testable: write the failing test first, implement, verify, commit.
