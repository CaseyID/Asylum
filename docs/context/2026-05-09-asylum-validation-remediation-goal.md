# Asylum Validation Remediation Goal

Use this file as the detailed instruction set for a short Codex `/goal`.

## Mission

Address the full validation report at `docs/reviews/asylum-validation-report.md` and leave Asylum/Cockpit working end to end for the basic user flow:

1. launch a Codex or Claude Code node from Cockpit,
2. deliver the launch packet as the first real harness turn,
3. observe a usable terminal/TUI stream in Cockpit,
4. send follow-up input and see it acknowledged,
5. open browser attach and native attach affordances honestly,
6. stop/archive/clean up without phantom running nodes.

Current `main` already contains v0.1.10 release notes claiming this report was fixed. Treat that as a hypothesis, not closure. Verify current code and live behavior against the report, then fix any missing, partial, regressed, or unverified items.

## Sources To Read First

- `AGENTS.md`
- `docs/context/2026-05-09-codex-playwright-cli-port-handoff.md`
- `docs/reviews/asylum-validation-report.md`
- `docs/specs/asylum-current-product-spec.md`
- `RELEASES.md`
- `CHANGELOG.md`

## Autonomy

Run as an autonomous supervisor. Do not stop for design signoff. Ask the user only for real host/sandbox approvals or release authorization.

Create a work branch if starting from clean `main`. Commit coherent completed slices locally using the repo commit style. Do not push, open PRs, cut releases, or mutate the installed user service unless explicitly authorized or required for a bounded validation step.

Treat `/tmp` as scratch space and remove temp dirs/processes created during the run before finishing.

## Model And Agent Routing

Use subagents aggressively where work is independent, with disjoint write scopes.

- Main supervisor and ambiguous reasoning/debugging: `gpt-5.5`, reasoning `high`; escalate to `xhigh` only for stuck cross-cutting architecture/debug loops.
- Deep review of daemon/harness/Cockpit integration decisions: `gpt-5.5`, reasoning `high`.
- Well-defined code-writing tasks after the design is clear: `gpt-5.3-codex-spark`, reasoning `medium`, fast mode off when the platform exposes that toggle. Give each worker explicit file/module ownership and tell it not to revert others' edits.
- Cheap evidence-only UI/browser checks: use the `playwright-cli` skill by default, following its `validate-ui-cli` workflow wrapper, preferably with `gpt-5.4-mini` and low reasoning, returning structured evidence only. If evidence is weak, the supervisor must re-check.

Do not assign ambiguous product decisions, systemd/PATH diagnosis, harness onboarding, PTY streaming races, or final acceptance calls to cheap workers.

## UI Validator Prep

Before any Cockpit browser validation, confirm the Codex-side Playwright CLI validator is ready:

- Fresh Codex skill discovery sees `playwright-cli` and `ui-validation`. `playwright-cli` is the model-visible entrypoint and points at `/home/casey/.agents/skills/validate-ui-cli/SKILL.md` for the detailed workflow.
- `/home/casey/.agents/skills/validate-ui-cli/SKILL.md` exists and has no stale Claude-path references.
- `playwright-cli --version` works.
- `/home/casey/.codex/playwright-cli.config.json` exists.
- `/home/casey/.cache/codex-playwright-cli/{output,recordings,profile}` exists.
- `/home/casey/.cache/ms-playwright/` has a usable bundled Chromium.
- The active session can write to `/home/casey/.cache/codex-playwright-cli/` and `/home/casey/.cache/ms-playwright/daemon/`, or the agent requests a bounded sandbox escalation for the Playwright CLI command.

Use `playwright-cli` plus the `validate-ui-cli` workflow for clickthroughs, snapshot/console/network checks, recordings, traces, and cheap validator subagents. Use `ui-validation` / Playwright MCP / Chrome DevTools MCP only when MCP exploration or Chrome-specific debugging is needed.

## Required Tracker

Create or update `docs/reviews/2026-05-09-asylum-validation-remediation.md` as a living tracker. Keep it current throughout the run.

Track every report item:

- B1-B7
- M1-M6
- all listed nits
- the report's leftover state under `State left behind by this validation`

For each item record:

- current status: fixed, still broken, partial, already fixed but newly verified, deferred with reason
- evidence: code pointer, test name, command output summary, or browser validation note
- fix commit or changed files when applicable
- remaining risk

## Execution Order

1. Verify repo state: branch, cleanliness, `main`/`origin/main`, current HEAD, latest release ledger entry.
2. Read the source documents above.
3. Build the tracker from the validation report.
4. Map each finding to current code/tests before editing. Do not churn code that is already correct and covered.
5. Fix remaining gaps in this priority order:
   - B1/B2: daemon harness PATH resolution, service generation, `asylum doctor`, and accurate Cockpit copy.
   - B7: harness onboarding/trust bypass or honest launch profiles.
   - B4: launch packet must be delivered to local harnesses or the UI must stop promising that.
   - B5/B6: Cockpit terminal rendering and browser attach must be real xterm-backed surfaces, or capabilities/affordances must be made honest.
   - B3/M1: failed spawn and PTY/output lifecycle races must leave durable, honest node liveness/events.
   - M2-M6 and nits: post-launch navigation, optimistic send echo, Notifications naming/empty state, Hooks empty states, first-run CTAs, Cmd+K/native attach/archive/settings polish.
6. Add or update focused tests for every behavioral fix. Prefer existing test patterns and avoid broad rewrites.
7. Run proportional verification. Minimum target:
   - Rust tests for touched crates, usually `cargo test --workspace`
   - Cockpit tests/build for touched UI, usually through the repo stack commands
   - `cargo test-stack` or an equivalent full-stack verification before final completion when feasible
8. Use `playwright-cli` and a real browser before claiming Cockpit behavior works, following the `validate-ui-cli` workflow wrapper. Capture visible state, console errors, failed network requests, and the exact flow tested. Fall back to `ui-validation` / MCP only when the CLI path is insufficient.
9. Perform an end-to-end smoke with real harnesses when available:
   - create a temp workspace,
   - launch Codex and/or Claude from Cockpit with a hello-world launch packet,
   - verify no raw ANSI wall in the terminal surface,
   - send a follow-up input and see immediate feedback,
   - open browser attach,
   - stop/archive and confirm liveness is honest.
10. Clean up temp workspaces, temp daemon state, and any processes started by the run.

## Acceptance Criteria

- The tracker covers every finding from `docs/reviews/asylum-validation-report.md`.
- No user-facing false promises remain: unsupported features are hidden/disabled or explicitly described.
- The core Cockpit launch/chat/attach/stop flow is live-verified in a browser or clearly blocked by a documented external dependency.
- Failed harness launches do not leave phantom running/starting nodes.
- Harness availability errors are actionable and distinguish missing binaries from missing adapters.
- Launch packets reach local harnesses when the UI says they do.
- Terminal output is usable, not raw ANSI text.
- Browser attach is a real terminal page when advertised.
- Tests and browser validation evidence are recorded in the tracker.
- The tracker records the Playwright CLI validator readiness check and which browser-validation path was used.
- `RELEASES.md` is not modified unless a real delivery/release tracking update is warranted; do not cut a release without explicit authorization.
