# Product Backlog

Linear is the canonical backlog for Asylum product work. Do not maintain a
second issue list in this repository.

## Location

- Linear project: `Asylum` (use the existing project; do not create a duplicate)
- Scope: Asylum, Cockpit, agent coordination, and Asylum's Loon integration
- The older Linear `Supervisor` project is predecessor history, not a second
  active backlog. Reuse/migrate still-relevant issues into `Asylum`; resolve or
  cancel obsolete items with a short supersession note.
- The existing Linear `Loon` project may retain standalone LoonV2 infrastructure
  work. Cross-product outcomes and Asylum integration stay in `Asylum`; link the
  Loon issue as a dependency instead of duplicating it.

Once the Linear connector is available, confirm the workspace, team, existing
project, workflow states, and labels before creating or changing anything.

## Feedback Intake

When the owner provides notes or says to capture feedback:

1. Preserve the raw observation and its context. Do not translate away useful
   wording, frustration, uncertainty, or examples.
2. Separate observations, desired outcomes, questions, and proposed solutions.
   A proposed solution is not automatically the requirement.
3. Group duplicates and closely related notes. Keep distinct user problems as
   distinct issues even if one implementation might address several of them.
4. Search Linear before creating issues. Update or comment on an existing issue
   when it already represents the same outcome.
5. Create or update issues in the `Asylum` project. Ask a question only when the
   answer would materially change the issue; otherwise record the uncertainty.
6. Report what was created, updated, merged, or left as an open question.

Unless the owner asks for a draft-only pass, "capture these notes" authorizes
publishing the normalized result to Linear. Capturing feedback does not authorize
starting implementation.

## Issue Shape

Use the smallest issue that represents a meaningful product outcome:

- **Title:** outcome or user-visible problem, not an implementation instruction
- **Observation:** what happened or what is missing
- **Why it matters:** utility, usability, reliability, or product impact
- **Desired outcome:** what should be true when resolved
- **Evidence/context:** reproduction details, screenshots, review notes, or the
  owner's original wording when it adds meaning
- **Scope:** Asylum, Cockpit, LoonV2, or cross-cutting; name the owning repository
- **Acceptance notes:** observable completion criteria when they are already known
- **Dependencies:** required product decisions or linked Asylum/Loon issues;
  avoid ordering unrelated work
- **Open questions:** uncertainty that should survive triage

Do not invent technical acceptance criteria during product-feedback intake.
Implementation detail belongs in the issue only when it constrains the product
outcome or is established by later engineering investigation.

## Product Reviews

A feature, utility, UI, or UX review should produce:

1. One Linear index issue summarizing the reviewed versions, environment, scope,
   method, strengths, systemic themes, and important limitations.
2. Separate actionable issues for distinct findings, each linked back to the
   index issue and carrying its evidence.
3. No issue for praise alone, but preserve strengths in the index so later work
   does not accidentally remove what is already valuable.
4. Explicit separation between verified behavior, inference, and personal
   product judgment.

The review index is completed when the audit and backlog publication are done.
Its child findings remain in Backlog/Todo according to implementation readiness;
do not leave the review index open as a fictional umbrella delivery task.

For UI and UX findings, validate the rendered product in a browser before
presenting behavior as verified.

## Triage Defaults

Prefer Linear's existing workflow and labels. If the project has no useful label
set, start small:

- Area: `cockpit`, `asylum-core`, `agent-coordination`, `loon`
- Kind: `bug`, `ux`, `product`, `reliability`, `documentation`
- Source: `owner-feedback`, `product-review`

Use Linear's built-in priority rather than encoding priority in labels or titles.
Leave priority unset when impact and urgency have not been established.

Use project milestones as outcome streams, not calendars. The default Asylum
streams are:

- `Product semantics and trust` — object/state/authority contracts that other
  surfaces depend on
- `Daily-driver Cockpit` — launch, active work, inbox, session, history, and
  narrow-width owner experience
- `Agent and operator coordination` — MCP/CLI summaries, monitoring, delegation,
  results, and automation durability
- `Loon substrate productization` — secure guest control, environment readiness,
  durable workspaces, and honest lifecycle parity
- `Workflow integrations and finish` — external dispatch/reporting, onboarding,
  retention, and polish after the core loop is trustworthy

Milestones do not imply estimates, dates, or serial human execution. Agents may
work across streams in parallel when explicit blockers are satisfied. Record
real blockers with Linear relations; do not manufacture dependency chains merely
to make the backlog look ordered.

An issue in the backlog is not automatically ready for implementation. Before an
agent starts work, it should confirm the issue still matches the current product
spec, identify the owning repository, and make the acceptance boundary concrete.

Use `Todo` only when that boundary and its blockers are clear enough for an agent
to execute. `Backlog` means the outcome is wanted but still needs product or
engineering refinement. Do not assign estimates or human-duration timelines.

## Delivery Hygiene

When implementing a Linear issue:

- Move it to the appropriate active state when work actually begins.
- Keep the issue identifier in the branch or PR context when practical.
- Comment with verification evidence and important scope changes.
- Close it only when the requested outcome is delivered, not merely coded.
- Keep release truth in [../RELEASES.md](../RELEASES.md); Linear does not replace
  the release ledger.
