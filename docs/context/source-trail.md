# Source Trail

This file records the context that shaped the Asylum PRD.

## Primary Product Sources

- Asylum PRD in this repo: `docs/prd/asylum-live-v2-prd.md`
- Original source design commit in the Loon repo: `997e46c Add Asylum live v2 design spec`
- Original source file:
  `/Users/chyde/Projects/IntegralDragon/Workflows/Loon/docs/superpowers/specs/2026-04-27-asylum-live-v2-design.md`

## Related Local Context

These files informed the brainstorm and may be useful during implementation planning:

- `/Users/chyde/Projects/IntegralDragon/supervisor/README.md`
- `/Users/chyde/Projects/IntegralDragon/supervisor/docs/RUNBOOK.md`
- `/Users/chyde/Projects/IntegralDragon/supervisor/docs/QUICK-REF.md`
- `/Users/chyde/Projects/IntegralDragon/Workflows/Loon/README.md`
- `/Users/chyde/Projects/IntegralDragon/Workflows/Loon/docs/PRDs/AGENTIC_OPERATING_MODEL_PRD.md`
- `/Users/chyde/Projects/IntegralDragon/Workflows/Loon/docs/superpowers/README.md`

## Hermes Context

Hermes Agent was used as a product and architecture comparison point, especially around:

- always-on agent experience,
- TUI and messaging surfaces,
- remote interaction,
- user-facing autonomy,
- setup burden and observability gaps,
- how product polish can make an agent installable and immediately useful.

Local hydrated Hermes references used during brainstorming:

- `/Users/chyde/.agents/knowledge/hermes-agent/upstream/hermes-agent/repos/NousResearch/hermes-agent/website/docs/user-guide/tui.md`
- `/Users/chyde/.agents/knowledge/hermes-agent/upstream/hermes-agent/repos/NousResearch/hermes-agent/website/docs/user-guide/messaging/index.md`

Official public sources:

- `https://hermes-agent.nousresearch.com/`
- `https://hermes-agent.nousresearch.com/docs`
- `https://github.com/NousResearch/hermes-agent`

## Design Decision Summary

- Asylum should learn from Hermes' installable, usable, multi-surface product feel.
- Asylum should differ by centering observability, graph-first control, harness interoperability, and user-owned execution substrates.
- Asylum should let Codex, Claude Code, and future harnesses remain themselves instead of forcing them into one replacement harness.

