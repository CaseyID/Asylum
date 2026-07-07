# Phase C plan — durability and Loon parity

Part of the completion mission (see ../specs/2026-07-06-asylum-completion-mission.md). Loon-side prerequisites are DONE (LoonV2 fixes merged, host reinstalled, claude-dev guest image live-verified) — the verified guest contract is ../specs/2026-07-07-loon-guest-contract.md. Harness facts: ../specs/2026-07-06-harness-contract-notes.md. Status at the bottom.

## Workstreams

C1 — Loon substrate rewrite (crates/asylum-daemon: substrate/loon.rs, capability_service.rs guards, app.rs observe/attach, harness adapters as needed)

The old substrate was written against a CLI generation that no longer exists. Rewrite against the verified v0.1.5 contract (guest-contract doc):
- Lifecycle: create = `loon vm create` from the claude-dev image + per-VM provisioning (credential injection via `loon cp`, `{"hasCompletedOnboarding": true}` /root/.claude.json, workspace dir in-guest, repo clone in-guest when requested) + harness launch as a persistent interactive session under `loon exec` PTY (`exec attach` stream). Observe/attach stream that PTY (today Local-only in app.rs — un-guard). send_input follows the W0 submit contract (body write, gap, distinct CR). Interrupt = `loon exec signal` (Ctrl-C semantics, no forced Stopped). Stop = graceful harness stop then `loon vm stop`/`rm`; prune tombstones after teardown. Enumerate VMs with the default (destroyed-hidden) listing.
- Asylum-in-guest: the injected MCP server and harness-event bridge run the `asylum` binary INSIDE the guest — build a static musl `asylum` and stage it into the VM at provision (`loon cp`). Hooks/statusline/notify + MCP injection mirror W3/Local, except daemon resolution is HTTP: mint a per-node token, inject ASYLUM_BASE_URL (daemon HTTP address reachable from the 10.42.0.0/16 guest network — daemon must listen there; make the bind address configurable) + ASYLUM_TOKEN + ASYLUM_NODE_ID. The W2 bridge and MCP already support the HTTP+token path.
- Honesty rules: no lossy shims — workspace/harness/launch-args must survive into the guest launch; if something is genuinely unsupportable on Loon (e.g. fork), reject loudly rather than silently degrade. Quiescence-idle sweep currently Local-only: extend to Loon nodes (codex) or record why not.
- Tests: arg/provision-plan construction, guard removals, token minting; live gate below is the real proof.

C2 — durability + resume (crates/asylum-daemon capability_service.rs/storage.rs, asylum-cli, cockpit)
- Startup reconciliation: on daemon boot, reconcile DB liveness against reality (local PTYs are gone after restart -> mark honestly; Loon VMs may still exist -> query and adopt or mark). No eternal-Running rows. `list_nodes_by_liveness` finally earns its keep.
- Resume: claude `--resume <harness_session_id>` from the node workspace dir (id recorded at create since W3); codex `codex resume <thread-id>` (recorded from first notify). Surface as a node action (API + CLI + MCP + Cockpit button) that relaunches the harness in the same node identity (new PTY, same node row, session id preserved). Honest failure when no session id or workspace is gone.
- Sequencing: C2 daemon work after C1 merges (both touch capability_service.rs). C2 cockpit bits after W5 merges.

## Live gate (frugal)

1. Create/observe/send/interrupt/stop a real Claude node on a real Loon microVM from Asylum; the node calls at least one Asylum MCP tool from inside the guest (C1, one session).
2. Kill the daemon mid-session (local node running), restart: state honest (no eternal Running), and a resumable claude node resumes and continues (C2, one session, may reuse the C1 session's sibling).

## Status

- C1 (Loon substrate rewrite): in progress
- C2 (reconciliation + resume): not started
- Gate: not run

Release status: local-only mission work; not released. Last published release: v0.1.10 (2026-05-07).
