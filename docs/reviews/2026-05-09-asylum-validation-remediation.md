# Asylum Validation Remediation Tracker - 2026-05-09

**Branch:** `validation-remediation`
**Base verified:** `main` and `origin/main` both at `537f7172fc62a8b654724f972a7b3eaf552a8d19` after `git fetch --prune`.
**Release context:** v0.1.10 is published and claims the 2026-05-07 validation findings were fixed. This tracker treats that as unverified until code/tests/browser evidence below confirms it.
**Release status:** On branch, not released - awaiting validation and authorization. Latest published release is tracked in [RELEASES.md](../../RELEASES.md).

## Validator Readiness

| Check | Status | Evidence | Remaining risk |
|---|---|---|---|
| `playwright-cli` visible entrypoint | verified | `command -v playwright-cli` -> `/home/casey/.nvm/versions/node/v25.9.0/bin/playwright-cli`; `playwright-cli --version` -> `0.1.13`; fresh `codex debug prompt-input` included `playwright-cli`, `validate-ui-cli`, and `ui-validation`. | None known. |
| `validate-ui-cli` workflow file exists and is Codex-specific | verified | `/home/casey/.agents/skills/validate-ui-cli/SKILL.md` read; `rg "claude-playwright-cli|~/.claude|/home/casey/.claude"` returned no matches. | None known. |
| Codex Playwright config exists | verified | `/home/casey/.codex/playwright-cli.config.json` exists with Chromium channel config. | None known. |
| Cache/profile/recording dirs exist and are writable | verified | `/home/casey/.cache/codex-playwright-cli/{output,recordings,profile}` exist; write probe for Codex cache and `/home/casey/.cache/ms-playwright/daemon` passed. | None known. |
| Bundled Chromium exists | verified | `/home/casey/.cache/ms-playwright/chromium-1224/chrome-linux64/chrome --version` -> `Google Chrome for Testing 149.0.7827.3`; smoke and Cockpit validation opened through the Codex Playwright config. | None known. |
| Browser-validation path for this run | verified | Used `playwright-cli` + `validate-ui-cli` workflow for Cockpit validation; `ui-validation` was not needed. Smoke against `https://example.com` opened, snapshotted, checked console/network, and closed. Real Cockpit validation ran on `http://127.0.0.1:7791/`. | First sandboxed open failed on `~/.cache/ms-playwright/b/...` EROFS; escalated or writable-root-backed `playwright-cli` commands work and should be used for browser evidence if this sandbox shape persists. |

## Report Item Matrix

| Item | Current status | Evidence | Fix files | Remaining risk |
|---|---|---|---|---|
| B1 daemon cannot find harness binaries from systemd PATH | verified fixed | `cargo test-stack` includes `command_resolution_uses_login_shell_fallback_path`, service PATH rendering tests, and doctor PATH diagnostics. Live Codex node launched from isolated daemon state. | `crates/asylum-daemon/src/capability_service.rs`, `crates/asylum-cli/src/cli.rs`, `crates/asylum-cli/src/host.rs` | Installed user service was not mutated in this run. |
| B2 unavailable harness copy says adapter not built | verified fixed | Settings/Create now distinguish missing command from unbuilt adapter; source and Cockpit surfaces no longer use "future/not built" for Codex/Claude CLI adapters. | `cockpit/src/screens/CreateScreen.tsx`, `cockpit/src/screens/SettingsScreen.tsx`, `cockpit/src/types.ts` | None known. |
| B3 failed spawn leaves phantom `starting` node | verified fixed | `cargo test-stack` includes `failed_local_spawn_marks_node_failed_and_records_harness_failure`; live stopped/archived nodes no longer displayed as running after lifecycle transitions. | `crates/asylum-daemon/src/capability_service.rs`, `crates/asylum-daemon/src/storage.rs` | Negative live spawn not repeated in final browser run. |
| B4 launch packet is not injected into local harness | verified fixed | Live Codex node received `Print exactly ASYLUM_VALIDATION_HELLO...` as the first turn and rendered `ASYLUM_VALIDATION_HELLO`. | `crates/asylum-daemon/src/capability_service.rs`, `crates/asylum-daemon/src/harness/{codex,claude}.rs` | Depends on installed harness CLI semantics; validated with current Codex CLI. |
| B5 ANSI escape codes leak in Cockpit TUI surface | verified fixed | Browser session showed xterm-like terminal surface with readable TUI output, not raw ANSI text; workspace rendered exact path or `none`, never `~/`. | `cockpit/src/components/NodeSession.tsx`, `cockpit/src/components/NodeSession.test.tsx` | None known. |
| B6 browser attach returns JSON not terminal | verified fixed | `/attach/<token>` rendered HTML page titled `Asylum terminal` with `Terminal input`; post-stop attach POST returned 400, not 200. | `crates/asylum-daemon/src/app.rs`, `cockpit/src/api.ts` | UI still shows an attach affordance for stopped nodes, but backend rejects it honestly. |
| B7 harness onboarding prompts block fresh nodes | verified fixed | Live Codex launch in fresh isolated HOME reached prompt execution after user completed one-time Codex auth; launch proceeded without workspace trust prompt. | `crates/asylum-daemon/src/capability_service.rs`, `crates/asylum-daemon/src/harness/{codex,claude}.rs` | External Codex auth can still block isolated HOME until user signs in. |
| M1 fresh-node output streaming inconsistent | verified fixed | Fresh live Codex node streamed startup output and launch-packet result immediately; tests cover early PTY output persistence and harness-exit liveness. | `crates/asylum-daemon/src/substrate/local.rs`, `crates/asylum-daemon/src/capability_service.rs` | None known. |
| M2 post-launch stays on fleet instead of new node | verified fixed | Browser launch navigated directly to node detail for `d42fd8b2-a818-4a79-94ac-c437d6a84202`. | `cockpit/src/App.tsx`, `cockpit/src/screens/CreateScreen.tsx` | None known. |
| M3 send input lacks immediate echo | fixed and live-verified | Textarea submit immediately showed `› Reply with ASYLUM_VALIDATION_ACK`; after final patch it also drove Codex to produce `• ASYLUM_VALIDATION_ACK` without manual terminal Enter. | `cockpit/src/components/NodeSession.tsx`, `cockpit/src/components/NodeSession.test.tsx`, `crates/asylum-daemon/src/substrate/local.rs` | Raw attach input is the reliable path for browser prompt submissions; `/api/nodes/:id/input` remains fallback. |
| M4 Logs screen mislabeled and empty | verified fixed | Navigation now exposes Notifications; empty state is present in source and tests. | `cockpit/src/App.tsx`, `cockpit/src/App.test.tsx` | None known. |
| M5 Hooks rules/firings empty state missing | verified fixed | Browser Hooks screen showed `no rules yet`; Recent firings showed `no firings yet`. | `cockpit/src/screens/HooksScreen.tsx`, `cockpit/src/screens/HooksScreen.test.tsx` | None known. |
| M6 first-run CTAs both go to Settings | verified fixed | Browser first-run copy shows current CLI/spec affordances; source/tests cover first-run text and no stale tokened-command examples. | `cockpit/src/screens/FirstRunScreen.tsx`, `cockpit/src/screens/FirstRunScreen.test.tsx` | None known. |
| Nit: Cmd+K palette keyboard shortcut | verified fixed | Browser `Ctrl+K` opened the command palette with search textbox. | `cockpit/src/App.tsx` | Mac `Cmd+K` not separately tested in Linux validation environment. |
| Nit: native attach tooltip missing | verified fixed | Node session uses explicit toolbar titles for browser/native attach. | `cockpit/src/components/NodeSession.tsx` | None known. |
| Nit: archived nodes look idle/visible | verified fixed | Browser Fleet graph/list labeled archived node distinctly as `archived`, not idle/running. | `cockpit/src/screens/FleetScreen.tsx`, `cockpit/src/lib/glyphs.ts` | Archived nodes are still visible, but label is honest. |
| Nit: Settings harness copy says future/not built | verified fixed | Covered with B2. | `cockpit/src/screens/SettingsScreen.tsx` | None known. |

## Leftover State From 2026-05-07 Validation

| State | Current status | Evidence | Cleanup / decision | Remaining risk |
|---|---|---|---|---|
| Installed daemon sqlite had phantom node `36eac7c5...` | not mutated | This remediation used isolated daemon state only; installed `~/.asylum` was intentionally left alone. | Leave for user-approved installed-state cleanup. | Phantom historical row may still exist in installed state. |
| `/tmp/asylum-validate/` dev daemon state dir | cleanup deferred to final run cleanup | Current validation used `/tmp/asylum-validation-resume-*`; final cleanup removes only current-run temp state. | Do not delete older temp state unless user asks for broad cleanup. | Older report artifact may remain. |
| `/tmp/asylum-codex-test`, `/tmp/asylum-claude-test` | cleanup deferred | Not used by this run. | Safe to remove in a broad temp cleanup pass. | None known. |
| Stale debug daemons on ports 7817 and 7800 | not touched | Out of scope for isolated validation; no current remediation process used those ports. | Do not kill unknown historical processes without fresh owner verification. | They may still be present. |

## Browser Validation Evidence

| Scope | Result | Evidence |
|---|---|---|
| First-run state | PASS | Copy includes `tokened replies require command payloads`; eval confirmed no `tokened commands include approve / attach` and no bare reply examples. |
| Command palette | PASS | `Ctrl+K` opened the command palette/search textbox. |
| Create screen | PASS | Empty workspace placeholder `/abs/path/to/workspace`; helper says `absolute path to a local workspace directory`; no repo URL/tilde promise and no fake recipe panel. |
| Real Codex launch | PASS | Node `d42fd8b2-a818-4a79-94ac-c437d6a84202` launched from `/tmp/asylum-validation-resume-workspace`; terminal showed launch packet and `ASYLUM_VALIDATION_HELLO`. |
| Node terminal/session UI | PASS | xterm-like terminal rendered readable Codex TUI; workspace displayed `/tmp/asylum-validation-resume-workspace`, never `~/`. |
| Follow-up input | PASS | Final patched browser flow submitted `Reply with ASYLUM_VALIDATION_ACK` from Cockpit textarea and Codex responded `• ASYLUM_VALIDATION_ACK` without manual terminal Enter. |
| Browser attach while running | PASS | `/attach/<token>` rendered HTML terminal page titled `Asylum terminal`, not plaintext JSON. |
| Stop / attach after stop | PASS | After stop, backend `POST /api/nodes/<id>/attach/browser` returned 400 `node not attachable in current state: stopped`, not 200. UI still showed attach button after stop. |
| Archive label | PASS | Fleet/graph/session labels displayed archived node distinctly as `archived`. |
| Channels | PASS | Only implemented channel kinds visible; webhook inbound live, disabled ntfy adapter honest; no manual inbound controls. |
| Hooks | PASS | Empty rules/firings states visible; new hook action options were `channel`, `tool`, `pause_node`, `archive`; no `spawn` while `/api/recipes` is empty. |

## Verification Log

- `git fetch --prune`: updated remotes; removed stale `origin/fix/cockpit-validation-deliverables`.
- `git rev-parse HEAD origin/main`: both `537f7172fc62a8b654724f972a7b3eaf552a8d19` at tracker creation time.
- `codex --version`: `codex-cli 0.130.0`.
- `codex debug prompt-input "validate UI with playwright-cli and inspect ui-validation" | rg ...`: prompt context included `playwright-cli`, `validate-ui-cli`, and `ui-validation`. Initial sandboxed attempt failed with read-only `.codex` state; escalated run succeeded.
- Playwright prep checks listed in Validator Readiness. Smoke flow: sandboxed `playwright-cli -s=validator open https://example.com ...` failed with `EROFS` under `/home/casey/.cache/ms-playwright/b/...`; escalated open succeeded, snapshot showed Example Domain content, console check showed only favicon 404, network check showed no dynamic failures, `playwright-cli -s=validator close` closed the browser. Removed smoke `.playwright-cli` scratch under Codex cache output.
- `cargo fmt --all --check`: PASS after final code changes.
- `cargo test-stack`: PASS after final code changes. Rust tests passed; Cockpit `vitest run` passed 14 files / 60 tests.
- `cargo build-stack`: PASS after final code changes. Vite emitted only the standard chunk-size warning.
- Browser validation used `playwright-cli` session `asylum-resume-7791`, config `/home/casey/.codex/playwright-cli.config.json`, profile `/home/casey/.cache/codex-playwright-cli/profile`, viewport `1440x1000`, URL `http://127.0.0.1:7791/`.
- Console/network summary: final `playwright-cli console error` reported 0 errors. Requests were dominated by polling GETs returning 200; node create/attach returned 200, stop/archive returned expected 204, and deliberate post-stop attach/send probes returned expected 400.
- Cleanup before final: live validation nodes `d42fd8b2-a818-4a79-94ac-c437d6a84202` and `d0f6a8b7-0525-4b31-bcbb-1562838bd603` were stopped successfully in the isolated daemon; `playwright-cli` session `asylum-resume-7791` was closed; `.playwright-cli` scratch and `/tmp/asylum-validation-resume-{run,home,workspace,logs}` were removed; host port `7791` was closed.
