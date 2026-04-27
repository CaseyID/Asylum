# Asylum

Asylum is a single-user, always-on control plane for real agent harness sessions.

It does not replace Codex, Claude Code, Pi, Hermes, or future harnesses. It launches them, gives them shared tools and context, observes them, lets humans attach or intervene, and lets harnesses coordinate other harnesses across local and Loon-backed substrates.

The core product object is the **Node**: a live or resumable harness session running somewhere. A node may be a command center, supervisor, worker, evaluator, plain assistant, or custom role, but those are role hints, not mandatory workflow states.

## Start Here

- Product PRD: [docs/prd/asylum-live-v2-prd.md](docs/prd/asylum-live-v2-prd.md)
- Implementation-planning handoff: [docs/handoff/transition-to-implementation-planning.md](docs/handoff/transition-to-implementation-planning.md)
- Source and context trail: [docs/context/source-trail.md](docs/context/source-trail.md)

## Current Status

This repository is a handoff shell for moving Asylum from product design into implementation planning.

The next session should not start by redoing the full product brainstorm. It should read the PRD, use the handoff doc, resolve the implementation-planning questions, and produce a concrete implementation plan for a usable v1.

