---
title: Cockpit Node Session UX And Attach Semantics Cleanup Design
status: Approved direction, ready for implementation planning
date: 2026-05-09
branch: updating-attach-semantics
---

# Cockpit Node Session UX And Attach Semantics Cleanup Design

## Purpose

Cockpit should make node interaction feel direct. A user should click a node and be in the node's live session, terminal, chat, or control workspace without learning an "attach" workflow first.

The current implementation exposes "attach" as a visible product concept in Cockpit. That is the wrong user model. Attach may remain a backend transport/security primitive for signed terminal access, Loon proxying, or future pop-out views, but Cockpit should not ask users to manually attach to nodes during normal workflows.

## Product Principle

The user-facing primitive is the **node session**.

A node session is the live place where the user can view output, send input, and work with the node. Depending on the node, that surface may look like a harness TUI, a terminal on a Loon-backed machine, a structured transcript, or a control-oriented node workspace. The user's job is to open or select the node, not to attach.

## Current Problem

Cockpit currently leaks attach mechanics in several places:

- Inspector controls expose an `attach` button.
- Node detail headers expose `open attach tab`.
- Session chrome exposes "open attach tab" and "open in terminal" actions.
- Structured session history can render `attach tab` artifacts.
- Loon observe copy tells the user to "use attach" for an interactive session.
- Native terminal attach is presented as a normal Cockpit capability even though it should be treated as future/backlog UX.

These affordances make the user think there is a separate attach ritual required before they can work with a node. That is not the intended Asylum experience.

## Cockpit UX Contract

Clicking a node in Cockpit means "focus this node."

On graph, fleet, and main Cockpit screens:

- Clicking a node selects and highlights it.
- The visible session/terminal panel points at the selected node.
- The selected node's live session is the default interaction surface.
- No attach button, attach URL, or attach-issued concept is required for normal interaction.

On full node pages:

- Expanding or opening a selected node lands on the node workspace/detail page.
- The page defaults to the live session view.
- Events, activity, capabilities, relationships, metadata, and controls may remain available around or beside the session.
- The session remains the primary surface for talking to and operating the node.

Across Cockpit:

- Use "session", "terminal", "node", "open", "focus", "pop out", or "workspace" depending on context.
- Do not use "attach", "browser attach", "native attach", "attach tab", or "attach URL" in visible user copy.
- Do not render attach-issued history as a user-facing transcript artifact.

## Controls

Primary controls should map to user intent:

- **Open session** or selecting the node: focus the node's live session.
- **Expand**: open the full node workspace/session page.
- **Send input**: route text or keystrokes to the node.
- **Interrupt**, **stop**, **archive**, **fork**, and relationship controls remain explicit capability actions.

The old native terminal attach action should be removed from Cockpit for this pass. If the backend or CLI still has native attach capability, Cockpit should not present it as a supported ordinary workflow.

If a future pop-out browser session is useful, label it by outcome, such as "Open in new tab" or "Pop out session." The implementation may still use signed attach URLs behind the scenes.

## Backend Boundary

This design does not require a broad backend rename.

Allowed to remain internal for now:

- `/attach/{token}`
- `/api/attach/{token}/ws`
- attach token issuer/verifier types
- attach-related storage event kinds
- Loon adapter calls that invoke `loon attach`
- CLI or MCP compatibility surfaces that already expose attach URLs

The implementation should avoid unnecessary churn across stable routes, wire contracts, and daemon internals unless a later API cleanup plan explicitly chooses that work.

## Loon And Plain Terminal Nodes

The same UX contract applies to harness nodes and machine-like nodes.

For local harness nodes, the session surface is the live harness TUI/chat/terminal.

For Loon-backed or plain terminal nodes, the session surface should feel like opening a terminal on that node or machine. If live observe and interactive session transport differ behind the scenes, Cockpit may disclose limitations in outcome language, for example:

> Live output is not available from this substrate here. Open the node session to use an interactive terminal.

It should not instruct the user to "attach" as a separate product action.

## Native Terminal Boundary

Native terminal attach is out of Cockpit scope for this cleanup.

The CLI/backend may retain the feature for compatibility, experiments, or later validation. Cockpit should remove visible native-terminal attach controls, notices, and tooltips. Future work can reintroduce native terminal launch only after it is proven reliable and designed as an intentional advanced action.

## Documentation Boundary

Product docs and current spec references should be updated enough to stop teaching attach as a core Cockpit workflow.

Docs may still mention attach when describing internal transports, CLI compatibility, security-sensitive URLs, or Loon implementation details. In user-facing Cockpit workflow docs, the expected language is node session or terminal access.

## Acceptance Criteria

- Clicking/selecting a node in graph/fleet/main Cockpit views focuses that node and points the visible session panel at it.
- The full node page defaults to the live session view.
- Cockpit contains no visible user-facing copy for "attach", "browser attach", "native attach", "attach tab", or "attach URL" in normal node interaction surfaces.
- Cockpit no longer exposes native terminal attach controls.
- Historical attach-issued events do not render as an actionable user-facing transcript card.
- Loon limitations are described in terminal/session language, not attach language.
- Backend attach routes and token plumbing continue to work where still used internally or by compatibility clients.
- Tests cover the visible Cockpit behavior and copy regressions.

## Validation Plan

Implementation should include:

- Focused Cockpit unit tests for node selection/session focus behavior.
- Snapshot or DOM tests proving attach/native-attach buttons and copy are absent from main node interaction surfaces.
- Regression coverage that attach-issued events do not trigger popups and do not render as attach action cards.
- Static/API tests as needed to prove existing backend attach routes still issue and validate signed terminal URLs.
- Browser validation with Playwright CLI against graph/fleet/node-page workflows before claiming the UX works.

## Non-Goals

- Do not rename every backend attach type, route, event, capability, CLI command, or MCP tool in this pass.
- Do not redesign all Cockpit navigation or merge every node view into one page.
- Do not add native terminal launching to Cockpit.
- Do not introduce fake terminal/session behavior to make the UI look cleaner.

## Release Status

Design only. No release needed until an implementation delivery lands. See `RELEASES.md` for the current published release ledger.
