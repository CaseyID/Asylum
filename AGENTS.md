# Agent Guide — Asylum

This file is the entry point for AI coding agents (Claude Code, Codex, others) working in the Asylum repository. Read this first; follow the links from here.

## What Asylum is

A single-user, always-on control plane for real agent harness sessions (Codex, Claude Code, etc.). It launches them, gives them shared tools and context, observes them, lets humans attach or intervene, and lets harnesses coordinate other harnesses across local and Loon-backed substrates. It does NOT replace harnesses; it orchestrates them.

The core product object is the **Node**: a live or resumable harness session. Roles (command-center, supervisor, worker, evaluator, assistant) are hints, not mandatory workflow states. The cockpit is the primary first-party UI; CLI, MCP, and API share the same root capabilities.

Full PRD: [docs/prd/asylum-live-v2-prd.md](docs/prd/asylum-live-v2-prd.md).

## Current focus — read this if you're picking up active work

**Cockpit deliverability and prototype-residue cleanup is delivered (2026-04-29).** All 7 PRs merged to `main` (range `6e5054a..7458e4c`). See:

- **Delivery handoff:** [docs/handoff/2026-04-29-cockpit-deliverability-and-prototype-cleanup.md](docs/handoff/2026-04-29-cockpit-deliverability-and-prototype-cleanup.md) — what shipped, what's deferred, manual smoke owed.
- **Plan + audit (canonical):** [docs/reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md](docs/reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md) — Status section reflects all 7 PRs landed.
- **CHANGELOG:** [CHANGELOG.md](CHANGELOG.md) — release notes for the delivery.
- **Prior ultrareview (companion findings, all addressed):** [docs/reviews/2026-04-29-local-ultrareview-findings.md](docs/reviews/2026-04-29-local-ultrareview-findings.md)

**Active follow-up under discussion:** ntfy inbound auto-routing into node input streams. Transport works; addressing/correlation does not. Design notes in the delivery handoff; no PR open yet.

## The principle that drives current work

**Asylum is shipping as a real product. No simulated, mocked, stubbed, canned, or demo-only behavior may exist in user-facing code.**

- A button with `onClick={() => {}}` or `.catch(()=>{})` is a lie — wire it or delete it.
- A hardcoded value that could be derived from the daemon (version, bind, paths, counts) must be derived.
- Typed-state shapes inherited from the Claude Design Tool prototype (`Tweaks`, `simSpeed`, `still/slow/live` speed enums, `runResponse(seq)` canned-step animators, `SessionStep`) are confessions of prototype residue — remove them.
- A UI feature that references a backend that doesn't exist is a Potemkin facade — implement the backend or remove the feature.

Test fixtures and unit-test mocks are fine. The principle applies to behavior delivered by the running daemon and cockpit at runtime.

## Repository layout

- `crates/asylum/` — tiny composition crate that builds the installed `asylum` binary
- `crates/asylum-cli/` — CLI, MCP bridge, Unix-socket daemon client, service lifecycle, native attach helpers
- `crates/asylum-daemon/` — daemon runtime, HTTP/WebSocket Cockpit service, storage (SQLite), substrates (`local`, `loon`), harnesses (`claude_code`, `codex`), hooks engine, channels, capability service
- `crates/asylum-types/` — shared types and contracts (API, capabilities, config, security primitives)
- `cockpit/` — TypeScript/React single-page app served by the daemon (`/api/...` routes; `/` serves the SPA)
- `scripts/` — release, install, build-artifact scripts
- `docs/` — PRD, handoffs, reviews, source trail, plan archive

## How to work

- **Commit style:** lowercase, terse, action verb first. Match recent history (`Fix H1: <one-line>`, `cockpit: drop simSpeed and Tweaks`, etc.).
- **No AI attribution** in commits, PRs, issues, or code unless explicitly asked.
- **TDD where testable:** write the failing test first, implement, verify, commit. Existing patterns: `cargo test --workspace` for Rust; `npm --prefix cockpit run test` (Vitest) for cockpit.
- **Each PR ships working software.** No land-broken-fix-later patterns.
- **Branch per PR.** Use the branch names listed in the plan (`cockpit-strip-prototype-scaffolding`, `daemon-ntfy-inbound`, etc.).
- **Update progress.** When you complete a checkbox task in the plan, mark `- [x]` and commit the file change with the code change.

## Build & run

```bash
# build everything (cockpit assets must exist for release builds)
npm --prefix cockpit run build
cargo build --release

# dev daemon
cargo run -p asylum -- daemon run

# dev cockpit (alongside the daemon)
npm --prefix cockpit run dev

# tests
cargo test --workspace
npm --prefix cockpit run test
```

## Release tracking — read this when finishing a delivery

Asylum is **released manually**. There is no GitHub Actions release pipeline by design. "Merged to main" ≠ "shipped to users." The point of this section is so a fresh agent session opening this repo can answer the question *"is what's on main actually on a user's machine?"* without guessing.

**Tracking is your job. Cutting is the user's call.**

When you finish a delivery cycle (a multi-PR plan from `docs/reviews/` or `docs/handoff/`):

1. Open [RELEASES.md](RELEASES.md) and skim the ledger so you know the current published version and what's outstanding.
2. **Update tracking, always:** the delivery's plan/handoff must end with a "Release status" section that says one of:
   - `Released as vX.Y.Z` (with link to GitHub release + which platforms shipped)
   - `On main, not released — awaiting authorization. Last release: vX.Y.Z (date).`
   - `Doc-only / internal — no release needed. Last release: vX.Y.Z (date).`
3. **Don't cut a release on your own initiative.** If you think a release is warranted, surface that to the user with a one-line recommendation ("recommend cutting v0.1.3 — first user-facing changes since v0.1.2"). Wait for their go-ahead.
4. **Exception:** if the user has explicitly authorized autonomous mode for the delivery (e.g., "execute the whole plan and ship it") — then cut the release as the final step and update the ledger.
5. Whenever you do cut: bump version, build, tag, publish, **update the RELEASES.md ledger row**. The ledger update is what makes it real for the next agent.

Every plan/handoff document must include a "Release status" section that links to the ledger. If you're authoring a plan and you don't see one, add one. If you're picking up an existing plan and the section is missing or stale, fix it before starting work.

## Conventions to preserve

- Asylum is single-user in v1. Do not introduce multi-tenancy, RBAC, or org-scoping.
- Asylum is harness-intelligence-first. Do not introduce a mandatory workflow engine or fixed node state machine.
- Asylum is Loon-independent. Loon is one of two supported substrates; do not couple core logic to it.
- Asylum is graph-first. Nodes are the core object; runs/workflows may come later.
- Capability surface (CLI, API, MCP, cockpit) shares the same root capabilities — do not let one drift from the others.
- CLI/MCP local control uses `~/.asylum/run/asylum.sock`; Cockpit remains HTTP/WebSocket.

## Other entry points

- General product handoff (older, pre-implementation): [docs/handoff/transition-to-implementation-planning.md](docs/handoff/transition-to-implementation-planning.md)
- Source-trail / context: [docs/context/source-trail.md](docs/context/source-trail.md)
- README (user-facing install/run): [README.md](README.md)
