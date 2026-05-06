# Asylum Current-Spec Coverage Audit Goal

Complete an autonomous end-to-end repo-vs-spec audit, then begin fixing confirmed well-scoped gaps.

## Sources

Primary source of truth:

- `docs/specs/asylum-current-product-spec.md`

Audit/control docs:

- `docs/reviews/2026-05-05-asylum-spec-coverage-audit-brief.md`
- `cockpit/prototype/README.md`
- `AGENTS.md`
- `README.md`
- recent handoffs/reviews
- `RELEASES.md`

## Required Output

Create and continuously maintain:

- `docs/reviews/2026-05-05-asylum-spec-coverage-findings.md`

Do not wait until the end to write it. Keep it updated after each meaningful audit slice.

## Start Check

Before doing audit work, verify git is clean on `main`, matches `origin/main`, and HEAD is expected checkpoint `f14a7c4`.

## Audit Scope

Compare real implemented behavior against `docs/specs/asylum-current-product-spec.md`.

Cover:

- daemon/API
- CLI/MCP
- substrates/harness behavior
- storage/state
- Cockpit UI
- docs/install/release surface
- tests

Build a spec coverage matrix. Every major spec area needs evidence-backed status.

## Evidence Rules

Validate real behavior, not just types or static code.

Do not count simulated, mocked, stubbed, canned, or demo-only runtime behavior as implemented.

Each finding must include:

- spec requirement
- current behavior
- evidence source
- status: implemented, partial, missing, wrong, unclear, or deferred
- severity/user impact
- recommended fix or backlog task
- whether verified live or by code inspection

## Cockpit/UI Rules

Use the `ui-validation` skill.

Validate Cockpit in a real browser with Playwright MCP and Chrome DevTools MCP. Record:

- URL/path
- rendered title
- visible primary elements
- screenshot or visual check
- accessibility snapshot or equivalent
- console errors
- failed network requests
- blockers

Do not claim visible UI behavior works from code inspection alone.

## Subagents And Models

Use subagents for independent audit slices.

Use cheaper models for mechanical work.

For simple UI validation/browser automation:

- use `gpt-5.4-mini`
- use low reasoning
- return structured evidence only

For bounded code fixes:

- use `gpt-5.3-codex-spark` when appropriate
- give explicit file ownership
- tell workers not to revert others' work

Use smarter models for ambiguous product interpretation, complex debugging, architecture decisions, or cross-cutting Rust/TypeScript changes.

If a cheap subagent gives weak evidence or gets stuck, supervisor/main model must step in.

## Process

1. Verify git checkpoint.
2. Read required docs/specs.
3. Create findings report and initial coverage matrix.
4. Split audit across CLI/MCP, daemon/API/storage, harness/substrates, Cockpit UI, docs/release/install, and tests.
5. Run relevant real commands/tests.
6. Keep findings updated continuously.
7. After audit coverage is useful and complete enough, fix confirmed well-scoped gaps directly.
8. For large or ambiguous gaps, leave precise backlog items instead of guessing.
9. Do not cut a release unless explicitly authorized.

## Completion Criteria

- Git checkpoint verified before work began.
- Findings report exists and covers the full current spec.
- Each major area has evidence-backed status.
- Cockpit has real browser validation with console/network status recorded.
- Confirmed gaps are actionable backlog items.
- Well-scoped fixes have been started or completed.
- Unresolved items are explicit and prioritized.
