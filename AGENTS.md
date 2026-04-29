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

- `crates/asylum-core/` — shared types and contracts (API, capabilities, config, security primitives)
- `crates/asylum-daemon/` — HTTP service, storage (SQLite), substrates (`local`, `loon`), harnesses (`claude_code`, `codex`), hooks engine, channels, capability service
- `crates/asylum/` — CLI binary + MCP server, native attach helpers
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
cargo run -p asylum-daemon -- start

# dev cockpit (alongside the daemon)
npm --prefix cockpit run dev

# tests
cargo test --workspace
npm --prefix cockpit run test
```

## Conventions to preserve

- Asylum is single-user in v1. Do not introduce multi-tenancy, RBAC, or org-scoping.
- Asylum is harness-intelligence-first. Do not introduce a mandatory workflow engine or fixed node state machine.
- Asylum is Loon-independent. Loon is one of two supported substrates; do not couple core logic to it.
- Asylum is graph-first. Nodes are the core object; runs/workflows may come later.
- Capability surface (CLI, API, MCP, cockpit) shares the same root capabilities — do not let one drift from the others.

## Other entry points

- General product handoff (older, pre-implementation): [docs/handoff/transition-to-implementation-planning.md](docs/handoff/transition-to-implementation-planning.md)
- Source-trail / context: [docs/context/source-trail.md](docs/context/source-trail.md)
- README (user-facing install/run): [README.md](README.md)
