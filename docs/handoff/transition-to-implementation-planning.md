# Transition To Implementation Planning

Date: 2026-04-27

## Purpose

This repo was created so a fresh session can start implementation planning for Asylum without needing the full brainstorming transcript.

The canonical product shape is captured in:

- [../prd/asylum-live-v2-prd.md](../prd/asylum-live-v2-prd.md)

## Suggested Prompt For The Next Session

```text
We are in /Users/chyde/Projects/IntegralDragon/Asylum.

Use Superpowers writing-plans. Read README.md, docs/prd/asylum-live-v2-prd.md, and docs/handoff/transition-to-implementation-planning.md.

Create the implementation plan for Asylum Live v2. The plan must deliver a usable v1 product, not a partial scaffold. It must support Codex and Claude Code out of the box, local and Loon substrates, graph-first Cockpit, command-center nodes, browser/native attach where possible, shared API/CLI/MCP capabilities, and ntfy notifications plus inbound remote commands.

Do not bake in a mandatory workflow engine or fixed node state machine. Keep the system node-first, capability-first, harness-intelligence-first, and Loon-independent.
```

## Product Commitments To Preserve

- Name: Asylum.
- V1 is single-user.
- Asylum is an always-on control plane, not a chatbot provider.
- Intelligence lives in real harnesses such as Codex and Claude Code.
- Codex and Claude Code are both required for v1.
- Loon is optional to configure but supported in v1 as an independent substrate.
- Node is the core object. Runs/workflows may come later.
- Graph-first Cockpit is the primary first-party UI.
- Command-center chat is an inline persistent node powered by a selected harness.
- API, CLI, MCP, dashboard, command-center chat, and third-party clients share the same root capabilities.
- ntfy is in v1 for outbound notifications and inbound remote command/reply.
- Remote clients may request status, send input, start nodes, stop/interrupt nodes, resolve decisions, and request attach URLs.

## Implementation-Planning Focus

The planning session should turn the PRD into a build plan with:

- repository structure,
- runtime architecture,
- storage model,
- capability API contract,
- adapter boundaries,
- Cockpit UI milestones,
- Codex adapter strategy,
- Claude Code adapter strategy,
- local substrate strategy,
- Loon substrate strategy,
- attach strategy,
- ntfy strategy,
- packaging and install path,
- test and verification plan,
- sequencing that produces a usable product early.

## Questions To Resolve In Planning

- Exact API transport and schema format.
- Browser attach implementation for Codex and Claude Code PTY/TUI.
- Native attach strategy per OS.
- How much structured telemetry each harness can expose in v1.
- Loon integration details for Codex and Claude Code session setup.
- Node output and transcript storage strategy.
- Authentication and correlation for ntfy inbound commands.
- Minimal install/packaging approach for the always-on service, CLI, MCP server, and Cockpit.

## Avoid These Traps

- Do not turn Asylum into a workflow engine before it is a control plane.
- Do not require every node to march through fixed states like planning, running, blocked, verifying, and done.
- Do not make dashboard-only capabilities. Everything should flow from shared root capabilities.
- Do not absorb Loon into Asylum. Integrate with it cleanly.
- Do not build a custom assistant model or chatbot surface. Launch real harnesses.
- Do not defer Loon, Codex, Claude Code, or ntfy out of v1.

