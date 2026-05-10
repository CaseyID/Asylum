# Harness Asylum Control Implementation Plan

> **For agentic workers:** Execute autonomously. Do not use TDD for this delivery; add or update focused tests after implementation and run the verification gates before claiming completion.

**Goal:** Let Asylum-launched Claude Code and Codex nodes create and supervise other Asylum nodes through the daemon-owned capability surface.

**Architecture:** Add a first-class peer-spawn capability that defaults from the calling/source node, records an explicit graph relationship, and is exposed through API, CLI, and MCP. Local harness launches inject an `asylum` MCP server per process, using the current daemon socket and current `asylum` executable, so commands like "spawn a worker" can become real Asylum tool calls rather than private harness-local behavior.

**Tech Stack:** Rust workspace (`asylum-types`, `asylum-daemon`, `asylum-cli`), TypeScript/React Cockpit, existing stdio MCP bridge.

---

## Files

- Modify: `crates/asylum-types/src/api.rs`
- Modify: `crates/asylum-types/src/capabilities.rs`
- Modify: `crates/asylum-daemon/src/app.rs`
- Modify: `crates/asylum-daemon/src/capability_service.rs`
- Modify: `crates/asylum-daemon/src/harness/*.rs`
- Modify: `crates/asylum-cli/src/client.rs`
- Modify: `crates/asylum-cli/src/cli.rs`
- Modify: `crates/asylum-cli/src/mcp.rs`
- Modify: `cockpit/src/components/NodeSession.tsx`
- Modify: `cockpit/src/screens/FirstRunScreen.tsx`
- Modify: `cockpit/src/screens/ChannelsScreen.test.tsx` or `cockpit/src/screens/ChannelsScreen.tsx`
- Modify: `docs/specs/asylum-current-product-spec.md`
- Modify: `RELEASES.md`

## Tasks

- [x] **Task 1: Add `node.spawn_peer` root capability**
  - Add request/response DTOs.
  - Add `CapabilityName::NodeSpawnPeer`.
  - Add daemon service method that creates a real node with defaults inherited from the source node.
  - Record an explicit source-to-child relationship, defaulting to `spawned_for`.
  - Add HTTP route `/api/nodes/{id}/spawn`.
  - Add CLI command `asylum node spawn <source-node-id>`.
  - Add MCP tool `node.spawn_peer`; if `node_id` is omitted, default from `ASYLUM_NODE_ID`.

- [x] **Task 2: Inject Asylum MCP into local harness launches**
  - Build per-node Asylum MCP launch args for Claude Code using `--mcp-config`, `--strict-mcp-config`, and `--allowedTools mcp__asylum__*`.
  - Build per-node Asylum MCP launch args for Codex using per-invocation `-c mcp_servers.asylum.*` overrides.
  - Use the current `asylum` executable and current `ASYLUM_SOCKET_PATH`.
  - Ensure `asylum mcp` launched from a node prefers the local socket even when the parent harness env includes `ASYLUM_BASE_URL`.
  - Update launch prompt copy so the node knows to call Asylum tools rather than simulate child workers inside its own session.

- [x] **Task 3: Make Cockpit and docs match real behavior**
  - Update command-center placeholder and first-run steps to name the now-supported peer-spawn behavior.
  - Fix the current ChannelsScreen baseline test failure without weakening the assertion.
  - Update the current product spec for `node.spawn_peer` and harness-injected Asylum MCP.
  - Add Release status to this plan and update `RELEASES.md` with unreleased main state after delivery.

- [x] **Task 4: Validate end to end**
  - Run Rust formatting and focused tests.
  - Run `cargo test --workspace`.
  - Run `npm --prefix cockpit test -- --run`.
  - Run `npm --prefix cockpit run build`.
  - Run `cargo test-asylum`.
  - Run a local source-dev smoke with fake harnesses that confirms `node.spawn_peer` creates a node and graph edge through MCP/API.

## Release Status

See [RELEASES.md](../../../RELEASES.md).

Current status: On main, not released — awaiting release authorization. Last published release: v0.1.10 (2026-05-07).
