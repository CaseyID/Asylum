# Asylum Spec Coverage Audit Brief

**Status:** audit brief for comparing implementation against the canonical current product spec
**Date:** 2026-05-05
**Spec under audit:** [docs/specs/asylum-current-product-spec.md](../specs/asylum-current-product-spec.md)
**Release status:** Doc-only / internal - no release needed. Latest published release is tracked in [RELEASES.md](../../RELEASES.md).

## Goal

Produce a complete, evidence-grounded report of where the current Asylum repository and local runtime behavior pass, partially satisfy, or fail the current product spec.

The audit must not implement fixes. Its output should become the working backlog for bringing Asylum to the desired deliverable state.

## Inputs

Read these first:

- [docs/specs/asylum-current-product-spec.md](../specs/asylum-current-product-spec.md)
- [AGENTS.md](../../AGENTS.md)
- [README.md](../../README.md)
- [RELEASES.md](../../RELEASES.md)
- [docs/prd/asylum-live-v2-prd.md](../prd/asylum-live-v2-prd.md)
- [docs/reviews/2026-05-04-asylum-architecture-refactor-spec.md](./2026-05-04-asylum-architecture-refactor-spec.md)
- [cockpit/prototype/README.md](../../cockpit/prototype/README.md)
- `crates/`, `cockpit/src/`, `scripts/`

Historical handoffs and reviews may be used as context, but the spec above is the audit contract.

## Non-Negotiable Rules

- Do not fix findings during audit.
- Do not treat prototype behavior as implementation truth.
- Do not count test fixtures or mocked unit tests as product behavior.
- Do not mark a requirement pass unless behavior is backed by code, runtime behavior, an automated test, or a manual smoke step.
- Do not silently skip Cockpit. Cockpit is the highest-risk surface and must be audited deeply.
- Every non-pass finding needs concrete evidence.

## Recommended Work Split

Use parallel agents by surface if available:

| Slice | Scope |
|---|---|
| Cockpit UX/workflows | `cockpit/src`, prototype intent, browser behavior, no-prototype-residue check, screen-by-screen workflow audit. |
| Daemon/API/storage | `crates/asylum-daemon`, `crates/asylum-types`, HTTP/WS routes, SQLite schema, events, attach, auth, capability descriptors. |
| CLI/MCP/lifecycle | `crates/asylum-cli`, `crates/asylum`, install/update/uninstall/service, Unix socket client, MCP tools. |
| Harness/substrate/channels/hooks | Codex/Claude adapters, local PTY, Loon CLI, ntfy inbound/outbound, remote commands, hook engine/actions. |
| Docs/release/security | README, PRD/handoff conflicts, release ledger, exposed-auth posture, stale command names. |

Each slice should return findings keyed to spec requirement IDs.

## Evidence Levels

Use the strongest available evidence:

| Level | Meaning |
|---|---|
| Automated test | Existing or newly written test proves behavior. For audit-only work, new tests may be placed in a temporary branch/worktree but should not be mixed with fixes. |
| Runtime smoke | Built binary/daemon/Cockpit behavior observed locally with commands, HTTP, WebSocket, or browser. |
| Static source | Code path clearly implements or contradicts behavior. Include file and line. |
| Documentation | Docs claim behavior. This never proves runtime behavior by itself. |
| Blocked | Environment dependency unavailable, with exact blocker and command/output. |

## Suggested Commands

Use temp runtime state for smoke work so the user's real install is not disturbed:

```bash
cargo test --workspace
npm --prefix cockpit run test
npm --prefix cockpit run build
cargo build --workspace

AUDIT_ASYLUM_HOME="$(mktemp -d)"
ASYLUM_HOME="$AUDIT_ASYLUM_HOME" ./target/debug/asylum setup
ASYLUM_HOME="$AUDIT_ASYLUM_HOME" ./target/debug/asylum daemon run --bind 127.0.0.1:7717
```

For live daemon checks, run the daemon in one terminal/process and use:

```bash
ASYLUM_HOME="$AUDIT_ASYLUM_HOME" ./target/debug/asylum status --json
ASYLUM_HOME="$AUDIT_ASYLUM_HOME" ./target/debug/asylum node list
curl -fsS http://127.0.0.1:7717/api/health
curl -fsS http://127.0.0.1:7717/api/capabilities
```

If owner-token auth is enabled, include the token in CLI env or HTTP headers.
Remove `$AUDIT_ASYLUM_HOME` after the smoke run.

## Coverage Report Format

Create a report under:

```text
docs/reviews/YYYY-MM-DD-asylum-spec-coverage-report.md
```

Recommended structure:

```markdown
# Asylum Spec Coverage Report

## Summary

- Pass:
- Partial:
- Fail:
- Blocked:
- Highest-risk gaps:

## Requirement Coverage

| ID | Status | Evidence | Notes |
|---|---|---|---|
| ARCH-001 | Pass | `Cargo.toml:1` | Four product crates present. |

## Findings

### HIGH: COCKPIT-017 - Prototype attach preview still renders canned output

Expected:

Observed:

Evidence:

Impact:

Recommended remediation:

## Blockers

## Appendix: Commands Run
```

## Severity Guidance

| Severity | Use When |
|---|---|
| Critical | User-facing fake/simulated behavior ships as real, security boundary broken, install/update destroys user data, or daemon cannot perform core node operations. |
| High | Core v1 workflow missing or broken: create/observe/control/attach nodes, Cockpit graph/session flow, CLI lifecycle, auth for exposed routes, install/update. |
| Medium | Important capability partial or inconsistent across surfaces: MCP parity, channels/hooks, settings truth, Loon honesty, docs that can mislead operators. |
| Low | Polish, naming, stale historical wording, non-blocking docs cleanup, minor ergonomics. |

## Known Risk Seeds To Verify

These are not pre-written findings. Verify them before reporting:

- `COCKPIT-017`: current `NodeSession` attach preview appears to include canned router/test output and a token TTL that may not match daemon attach TTL.
- `CHAN-003` / `CHAN-004`: ntfy inbound transport records messages, but reply/node correlation may not route into node input.
- `DECISION-001` through `DECISION-003`: decision records and remote approve/deny parsing exist in pieces, but Cockpit/user workflow may be missing.
- `HARN-003`: local Codex/Claude launches may not receive Asylum-aware launch context.
- `SUB-005`: Loon observe/browser attach semantics may differ from local and must be described honestly in UI/API.
- `MCP-003` / `MCP-005`: MCP notification/channel tools may not align with actual daemon routes or canonical capability names.
- `CLI-006`: CLI may not expose all root capabilities practical for a terminal.
- `DOC-003`: README and historical docs may still contain stale `install systemd|launchd`, old release examples, or pre-refactor crate names.
- `SEC-004`: token scopes may be represented but not enforced.

## Done Criteria

The audit is complete when:

- Every requirement ID in the spec is listed once in the coverage table.
- Every Partial/Fail/Blocked row has concrete evidence.
- Cockpit has been manually or browser-smoke audited, not only read statically.
- CLI/MCP/API behavior has at least one live smoke path where practical.
- Docs conflicts are separated from runtime behavior gaps.
- The report contains no implementation changes.
