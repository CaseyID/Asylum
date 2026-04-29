# Handoff — Cockpit Deliverability + Prototype Cleanup (DELIVERED)

Date: 2026-04-29
Status: **All 7 PRs merged to `origin/main`**.

## What this was

After the 9 High-severity findings from the 2026-04-29 local-ultrareview were merged, a second-pass audit found substantial prototype-era residue in the cockpit (client-side simulation machinery, hardcoded fake values in Settings, a Potemkin ntfy inbound feature, dead UI affordances). This handoff scoped a 7-PR roadmap to remove all of it and reach a releasable v1. Casey ran the delivery autonomously on 2026-04-29; everything below is now landed.

## What shipped

| PR | Branch | Summary |
|---|---|---|
| 1 | `cockpit-strip-prototype-scaffolding` | Tweaks/simSpeed/runResponse/decision purged; useUiPrefs persistence; Inspector parent display; localStorage polyfill for vitest |
| 2 | `cockpit-real-settings` | HealthResponse extended (daemon_version, bind, db, transcripts); GET /api/tokens + POST /api/tokens/{id}/rotate; SettingsScreen rewritten; api/cli/mcp panels deleted |
| 3 | `daemon-ntfy-inbound` | ntfy.sh JSON-stream subscriber; reconnect with backoff; channel.inbound hook fires |
| 4 | `cockpit-wire-or-remove-dead-ui` | Every dead button deleted or wired; native-attach copies command line to clipboard |
| 5 | `cockpit-cmdk-real` | Cmd-K finds nodes; wires browser-attach + remote-command via prompt |
| 6 | `daemon-cockpit-medium-cleanup` | 21 Mediums (M1–M21 minus M18 which was PR 3); M9 part-2 (WS subprotocol) deferred |
| 7 | `release-prep-v1` | 17 Lows; CHANGELOG; README install update; PRD completion-bar updates; clippy clean |

Commit range on `main`: `6e5054a..7458e4c` (40 commits).

## Release status

**Cut as v0.1.2 on 2026-04-29.** Published to https://github.com/CaseyID/Asylum/releases/tag/v0.1.2.

- linux-x86_64 archive shipped + verified.
- darwin-arm64, darwin-x86_64, linux-arm64 archives outstanding — need a Mac (and, for linux-arm64, either an arm64 Linux box or `qemu-user-static`+`binfmt-support` installed on the x86_64 build host). Re-run `publish-release.sh --version 0.1.2 --targets <missing> --allow-clobber` to fill them in.

See the canonical record in [RELEASES.md](../../RELEASES.md).

## Manual verification still owed by Casey

- Fresh-machine `curl | bash` install on Ubuntu + macOS
- H1: revoke token via `DELETE /api/tokens/{id}` → confirm 401 on next request without daemon restart
- H5+PR3: configure ntfy server+topic, `curl -d "approve" ntfy.sh/<topic>` → cockpit toast within ~10s
- H8: `publish-release.sh --dry-run` against a fixture tag with mismatched HEAD/tag
- minisign trust path activates once a real signing key is pasted into `ASYLUM_RELEASE_PUBKEY_DEFAULT`

## Deferred (with rationale)

Documented in `CHANGELOG.md`:
- **M9 part 2** — WS auth via `Sec-WebSocket-Protocol`. Token storage was hardened to module memory in PR 6; the WS query-string remains.
- **L11** — installer integrity check needs an embedded sha256 coordinated at release time.
- **L12** — MCP stdio loop async refactor; current loop is correct, just blocking-style.

## Known follow-up: ntfy inbound auto-routing

Discussed with Casey post-delivery. The transport works (PR 3) and hooks can act on `channel.inbound`, but inbound messages don't auto-route to a target node's input stream. Casey's design principle: Asylum is dumb plumbing — no Rust-side grammar parsing; route raw bytes, let the harness/agent interpret. Implementation sketch (proposed, not committed):
- `node_id` column on `channel_messages` (channel-agnostic; useful beyond ntfy)
- correlation table `(token, channel_id, node_id, expires_at)` for outbound→reply addressing
- ntfy `notify_send(node_id=Some(..))` mints a 5-char correlation token, suffixes outbound body
- ntfy_inbound subscriber: try token → fall back to most-recent-pinged-within-window → fall back to command-center → log-only
- Cockpit toast quick-reply: now functional (toast knows the correlated node)

Open calls before building: window length (suggest 30 min), multi-ping disambiguation policy, token visibility in body vs ntfy tags. Worth designing the schema channel-agnostically from day one so Discord/Slack/SMS bridges share the path later.

## Conventions to preserve (still apply)

- Lowercase, terse, verb-first commit messages.
- No Claude/AI attribution in commits, PRs, issues, code.
- Each PR ships working software; no land-broken-fix-later.
- TDD where testable.
- "No simulation in user-facing code" principle (now memorialized in `cockpit/src` — see PR 1).

## Pointers

- Plan + audit (canonical, has full detail and final Status section): [docs/reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md](../reviews/2026-04-29-cockpit-audit-and-deliverability-plan.md)
- Prior ultrareview report: [docs/reviews/2026-04-29-local-ultrareview-findings.md](../reviews/2026-04-29-local-ultrareview-findings.md)
- Release notes: [CHANGELOG.md](../../CHANGELOG.md)
- PRD (with completion bar updated post-delivery): [docs/prd/asylum-live-v2-prd.md](../prd/asylum-live-v2-prd.md)
