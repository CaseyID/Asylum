# Cockpit Prototype-Residue Audit + Deliverability Plan

## FRESH AGENT — START HERE

If you are an AI coding agent picking this up in a new session (with no memory of how this plan was produced), read this section first. Everything you need is in this file or directly linked from it.

**What this is.** A complete audit of every prototype-era artifact, fake/hardcoded value, dead UI affordance, and Potemkin feature in the Asylum cockpit (`cockpit/src/`), plus a 7-PR roadmap to fix all of it and reach a releasable v1. The audit lives in **Part A** (with `file:line` evidence for every finding); the plan lives in **Part C** (each PR has bite-sized TDD-style tasks with exact code).

**Why it exists.** The cockpit was scaffolded from a Claude Design Tool prototype that simulated agent behavior client-side (mocked notifications, canned typing animations, fake settings panels, etc.). When that prototype was implemented as the real cockpit, simulation primitives leaked into shipped product code. The user (Casey) caught this on 2026-04-29 during a smoke-test review and asked for a complete deliverability audit and execution plan. This document is the result.

**The principle this serves (non-negotiable).** Asylum must ship with no simulated, mocked, stubbed, canned, or demo-only behavior in user-facing code. Test fixtures and unit-test mocks are fine; behavior delivered by the running daemon and cockpit must be real end-to-end. When you find UI that references a backend feature that doesn't exist, either implement the backend or delete the UI — never leave a dangling button with `() => {}` or a `.catch(()=>{})` swallow. When you find a hardcoded value that could be derived from the daemon (version, bind address, paths, counts), derive it. When you find typed-state shapes that resemble Claude Design Tool primitives (`Tweaks`, `simSpeed`, `runResponse(seq)` canned-step animators), stop and remove them — they are confessions of prototype residue.

**Companion documents.** Read alongside:
- [docs/reviews/2026-04-29-local-ultrareview-findings.md](./2026-04-29-local-ultrareview-findings.md) — prior local-ultrareview report (54 findings: 9 High already fixed, 20 Medium, 25 Low). Part A6 of this document maps every finding to a PR in this plan.
- [docs/prd/asylum-live-v2-prd.md](../prd/asylum-live-v2-prd.md) — product spec.
- [docs/handoff/2026-04-29-cockpit-deliverability-and-prototype-cleanup.md](../handoff/2026-04-29-cockpit-deliverability-and-prototype-cleanup.md) — handoff entry point that points back at this file.

**How to execute.** Read Part A end-to-end before touching code (many fixes require understanding the larger pattern, not just specific lines). Then follow Part C PR-by-PR; each PR has a `Branch:` name, file list, and per-task checklist. Tasks use `- [ ]` checkboxes — mark them as you complete them and commit the updated file along with the code change so the next agent can see progress without asking.

**Recommended skills (Claude Code / superpowers users).** Use `superpowers:subagent-driven-development` (fresh subagent per task with review checkpoints) or `superpowers:executing-plans` (inline batch execution). Both honor the checkbox tracking. If you don't have superpowers, just follow the tasks sequentially — they are self-contained.

**Status tracker.** Update the [Status / what's done so far](#status--whats-done-so-far) section below as PRs land.

---

## Status / what's done so far

Update this list as PRs merge. Format: PR number — branch name — merge commit (or "in progress" / "not started").

- PR 1 — `cockpit-strip-prototype-scaffolding` — **landed on branch (HEAD: 37794b2)**
- PR 2 — `cockpit-real-settings` — **landed on branch (HEAD: cc94bcf)**
- PR 3 — `daemon-ntfy-inbound` — **not started**
- PR 4 — `cockpit-wire-or-remove-dead-ui` — **not started**
- PR 5 — `cockpit-cmdk-real` — **not started**
- PR 6 — `daemon-cockpit-medium-cleanup` — **not started**
- PR 7 — `release-prep-v1` — **not started**

The 9 ultrareview Highs (H1–H9) are already merged into `main` (commits `127814e..10585e6`). Everything else from the prior ultrareview is folded into PR 6 / PR 7.

---

**Goal:** Bring Asylum to a state where it can be released as a real product with no mocked, simulated, hardcoded, or stubbed user-facing capabilities, and with all High-severity bugs and security issues from the prior local-ultrareview report fixed.

**Architecture:** Two-pass approach. Pass 1 (PRs 1–2) strips prototype scaffolding from the cockpit and replaces hardcoded fake data with daemon-backed real data. Pass 2 (PRs 3–6) implements missing daemon features required for advertised cockpit functionality (notably ntfy inbound). A final release-prep pass (PR 7) consolidates remaining ultrareview Mediums and validates an end-to-end installable artifact.

**Tech stack:** Rust (asylum-core, asylum-daemon, asylum CLI), TypeScript/React (cockpit), SQLite (storage), reqwest (outbound HTTP), portable-pty, axum, tokio.

---

## Table of Contents

- [Part A — Audit findings (every issue, with file:line evidence)](#part-a--audit-findings)
  - [A1. Pure prototype residue (cruft to delete)](#a1-pure-prototype-residue)
  - [A2. Potemkin features (UI without backend)](#a2-potemkin-features)
  - [A3. Hardcoded fake data presented as real](#a3-hardcoded-fake-data)
  - [A4. Dead UI affordances (buttons with no handler)](#a4-dead-ui-affordances)
  - [A5. Wiring inconsistencies (real backend, wrong frontend call)](#a5-wiring-inconsistencies)
  - [A6. Cross-reference with prior ultrareview report](#a6-cross-reference-with-prior-ultrareview)
- [Part B — Architectural decisions](#part-b--architectural-decisions)
- [Part C — Implementation plan](#part-c--implementation-plan)
  - [PR 1 — Strip prototype scaffolding from cockpit](#pr-1--strip-prototype-scaffolding-from-cockpit)
  - [PR 2 — Replace fake Settings screen with real daemon-backed settings](#pr-2--replace-fake-settings-screen-with-real-daemon-backed-settings)
  - [PR 3 — Implement ntfy inbound subscription on the daemon](#pr-3--implement-ntfy-inbound-subscription-on-the-daemon)
  - [PR 4 — Wire or remove dead UI affordances + Logs screen real semantics](#pr-4--wire-or-remove-dead-ui-affordances--logs-screen-real-semantics)
  - [PR 5 — CmdK real semantics + node finder](#pr-5--cmdk-real-semantics--node-finder)
  - [PR 6 — Remaining Mediums from prior ultrareview](#pr-6--remaining-mediums-from-prior-ultrareview)
  - [PR 7 — Release prep + end-to-end install verification](#pr-7--release-prep--end-to-end-install-verification)
- [Part D — Verification matrix](#part-d--verification-matrix)
- [Part E — Provenance](#part-e--provenance)

---

# Part A — Audit findings

The audit covers the entire cockpit (`cockpit/src/`) and all daemon code reachable from cockpit endpoints. Every finding lists exact file paths and line ranges as of commit `13e88c2` (main).

## A1. Pure prototype residue

These exist in the codebase only because the cockpit was scaffolded from a Claude Design Tool prototype that simulated agent behavior client-side. They serve no purpose in the real product and must be deleted.

### A1.1 — `Tweaks` interface and the entire "tweaks" concept

- **File:** `cockpit/src/App.tsx:49-79` (interface, default, useState, setTweak)
- **Threading:** `App.tsx:360,361,407,409,436-438,484` — `tweaks.theme`, `tweaks.navCollapsed`, `tweaks.graphLayout`, `tweaks.simSpeed`, `tweaks.ntfyEnabled` are passed to children as if they were a coherent settings concept.
- **CSS leftovers:** `cockpit/src/cockpit.css:688-690` (`.tweaks-card` rule, unused), `:941` ("mode-specific tweaks" comment), `:1` (`/* asylum cockpit — prototype styles */`).
- **Reasoning:** "Tweaks" is a Claude Design Tool primitive — a left-hand side panel that prototypes use to expose ad-hoc demo controls. Real apps don't have a "Tweaks" panel; they have settings, persisted user preferences, or local UI state. Each `Tweaks` field is either a real setting (`theme`), real UI state (`navCollapsed`, `graphLayout`), a feature flag for a feature that doesn't belong to the user (`ntfyEnabled`), or pure simulation residue (`simSpeed`).
- **Severity:** High (gates the entire cockpit cleanup).

### A1.2 — `simSpeed` simulation knob

- **File:** `cockpit/src/App.tsx:53,61,194-232,438,484`; `cockpit/src/screens/CockpitScreen.tsx:22,46,103`; `cockpit/src/screens/ChatScreen.tsx:20,30,90`; `cockpit/src/screens/NodeScreen.tsx:151`; `cockpit/src/components/NodeSession.tsx:45,99,120,126,136,143`; `cockpit/src/state.test.ts:69` (test only).
- **Two real effects in production code:**
  1. Multiplies stream/typing-effect delays in `NodeSession.streamText` and `runResponse` (lines 120, 136, 143). These are dead code — see A1.3.
  2. Gates the ntfy poll cadence in `App.tsx:226`: `live → 4000ms`, otherwise `9000ms`; `still` disables polling entirely.
- **Reasoning:** A "simulation speed" knob has no place in a real product. The ntfy poll cadence belongs as a real config value (or a constant), not as a user-toggleable speed. The typing-effect path is dead weight with no callers.
- **Severity:** High.

### A1.3 — `runResponse` / `SessionStep` / `streamText` typing-effect machinery

- **File:** `cockpit/src/components/NodeSession.tsx:28-33` (`SessionStep` type), `:39` (`runResponse?` in `SessionBus`), `:79-81` (`sleep`), `:120-160` (`streamText`, `runResponse`, `speedMul`).
- **Wiring:** Exposed via `sessionBus.current.runResponse = runResponse` at line 170. **Never called anywhere.** The audit grep confirms zero invocations across the cockpit.
- **Reasoning:** This is the core piece of prototype machinery — a function that takes a pre-canned `SessionStep[]` array and animates it into the transcript with delays, simulating a Claude/Codex session for demo purposes. Asylum's real transcript flow is the WS observe handler (`appendNodeEvent`, lines 243+), which consumes real harness events. The two were grafted side by side and the prototype path was never removed.
- **Severity:** High.

### A1.4 — `pushSystem` / `pushTool` / `pushUser` imperative bus

- **File:** `cockpit/src/components/NodeSession.tsx:35-40` (`SessionBus` interface), `:163-172` (registration); `cockpit/src/App.tsx:91,286-329` (consumers).
- **Reasoning:** This bus was prototype-era machinery for "have the App write things into a NodeSession's transcript." Two real callers remain: `handleNodeAction` writes `pushSystem`/`pushTool` lines like `"sigint sent"` after a successful `interruptNode()` API call. That synthesizes a `tool` entry in the transcript that didn't come from the harness — it's a UI confirmation pretending to be a tool call. The harness emits real events through the WS observe path; cockpit-synthesized "tool" entries muddy the transcript with cockpit-only chrome.
- **Recommended fix:** Remove the bus. Use a separate "system status" lane (e.g., a transient toast/snackbar) for action confirmations, or render them as a clearly-distinguished `kind: "sys-line"` row that is visibly chrome rather than transcript. The bus shape itself is what enables the prototype's `runResponse` injection — eliminating it forecloses re-introducing simulation.
- **Severity:** High.

### A1.5 — "the prototype's notice" no-op JSX

- **File:** `cockpit/src/App.tsx:553-555`
- **Code:** `{graph.nodes.some((n) => !isOperational(n)) && null}`
- **Reasoning:** This is a zero-effect expression. The comment says: *"the prototype's notice; preserved as a verifiable signal that we are still consuming the operational gate from state.ts"*. It exists only to keep `isOperational` from being flagged as an unused import. Pure scar tissue.
- **Severity:** Low (cosmetic but explicit confession of prototype residue).

### A1.6 — `layoutFree` hardcoded prototype node IDs

- **File:** `cockpit/src/components/Graph.tsx:73-93`
- **Code:** A `seed` map keyed by hardcoded short-ids (`"cc-7c2af"`, `"sup-3d1e"`, `"sup-aa01"`, `"asst-d2c9"`, `"w-9a4f1"`, `"w-2b0c8"`, `"w-4e7b"`, `"w-1f3a"`) that are the prototype's mock node IDs. Real daemon nodes have UUIDs, so the seed never matches. The fallback grid math handles it.
- **Reasoning:** Dead seed data. Either delete the seed and use the grid for the "free" layout, or replace the layout with one that uses node-relationship topology (which `layoutTree`/`layoutSwimlanes`/`layoutForce` already do).
- **Severity:** Low.

### A1.7 — `onSpawn` "canned spawns are visual-only" comment

- **File:** `cockpit/src/App.tsx:341-346`
- **Code:**
  ```
  const onSpawn = (_spawn: SpawnEvent) => {
    // canned spawns from the cc session are visual-only until the daemon
    // emits structured spawn events; trigger an immediate refresh in case
    // the spawn maps to a real node creation.
    void refreshAll();
  };
  ```
- **Reasoning:** The `onSpawn` prop chain (App → CockpitScreen/ChatScreen → NodeSession) only ever fires from `runResponse`'s `step.kind === "tool"` with a `step.spawn` field — i.e., from the prototype's canned-step machinery. Once A1.3 is deleted, `onSpawn` becomes unreachable. The comment is also incorrect: the daemon already emits real node creation events (the `node_created` NodeEvent kind exists); the cockpit just doesn't observe them at the App level.
- **Severity:** High (gated by A1.3 removal).

### A1.8 — `decision` action on InspectorAction/NodeScreenAction enums

- **Files:** `cockpit/src/components/Inspector.tsx:24` (enum member); `cockpit/src/screens/NodeScreen.tsx:37` (enum member); `cockpit/src/App.tsx:323-326` (handler that just writes "resume not yet supported"); `cockpit/src/cockpit.css:626-639` (`.decision` selector, unused); `cockpit/src/screens/FirstRunScreen.tsx:17` (mention in onboarding text).
- **Reasoning:** No button anywhere in the cockpit emits a `"decision"` action. The handler is unreachable. The CSS selector is unused. The FirstRunScreen mentions "decision prompts" as an inspect feature that doesn't exist. H7's earlier fix removed `resumeNode` API calls; the residual `"decision"` enum + handler is the same dead-code pattern.
- **Severity:** Low.

### A1.9 — `cockpit.css` line 1 comment

- **File:** `cockpit/src/cockpit.css:1` — `/* asylum cockpit — prototype styles */`
- **Severity:** Trivial (rename to "asylum cockpit styles" or similar).

---

## A2. Potemkin features

These are user-facing capabilities advertised in the UI but with no working backend. They will appear to work to a casual user and silently no-op or never fire.

### A2.1 — ntfy inbound toast feature is structurally broken

- **Cockpit side:** `cockpit/src/App.tsx:190-232` polls `fetchChannelMessages(ntfyChannel.id, 10)` every 4–9 seconds, filters `direction === "in"`, and surfaces matches as toasts.
- **Daemon side:** Inbound ntfy messages are *never inserted* into `channel_messages`. `crates/asylum-daemon/src/capability_service.rs:1455-1482` (notify_send) only inserts `direction="out"`. The only path that inserts `direction="in"` is `capability_service.rs:1621-1642` (`channel_inbound`), which is a webhook receiver triggered by `POST /api/channels/{id}/inbound` — not a ntfy.sh subscriber.
- **Background tasks:** `start_background_tasks` (capability_service.rs:113+) spawns the hooks dispatcher and 5m/30m schedulers. It does NOT subscribe to any ntfy topic.
- **NtfyConfig.poll_interval_seconds** (asylum-core/src/config.rs:86-103) is set on every notify_send call but never read by anything.
- **Net effect:** in production, toasts never appear. Even after H5's timer-rebind fix.
- **Reasoning:** The PRD §16 lists ntfy inbound as a v1 completion-bar item. The cockpit was implemented to look like the prototype (which faked inbound messages). The daemon side was never built. This is the canonical example of "prototype UI faithfully recreated with no real plumbing behind it."
- **Severity:** High.
- **Cross-ref:** matches existing review's M18.

### A2.2 — Toast quick-reply requires `nodeId` correlation that doesn't exist

- **Files:** `cockpit/src/components/NtfyToast.tsx:47-50` shows "reply not available — message has no node target" when `nodeId === null`; `cockpit/src/App.tsx:210` sets `nodeId: null` always (comment: "ChannelMessageRecord has no node_id field, so reply is not available"); `crates/asylum-core/src/api.rs` ChannelMessageRecord has no `node_id` field; `channel_messages` table schema (storage.rs:175-187) has no node correlation column.
- **Reasoning:** Even after PR 3 implements ntfy inbound, the inbound message has no node_id correlation, so quick-reply will permanently show "reply not available". Current behavior is already correct — NtfyToast renders the "reply not available" message and hides the reply chips/input when `nodeId === null`, which it always is. The deeper fix is to either (a) parse inbound messages for a target-node prefix (e.g., `[node sup-3d1e]: approve`) and populate `nodeId` in the inbound flow, or (b) define a remote-command grammar that includes node id explicitly and route inbound through `/api/remote-commands`. PRD §11 implies (b) — the remote-commands handler already parses tokens like `node:<id> attach`.
- **Severity:** Low (UI is honest about the limitation; not blocking release). Land the deeper fix as a follow-up when inbound → remote-command routing is implemented (post-PR 3).

### A2.3 — `tail live` button on Logs has no implementation

- **File:** `cockpit/src/screens/LogsScreen.tsx:74-76`
- **Reasoning:** The button has no `onClick`. The daemon has no live-tail endpoint for notifications or logs. The Logs page already polls `/api/notifications` via App.tsx; "tail live" would be redundant unless it switches to an SSE stream. There is no SSE endpoint. Either implement an SSE stream in the daemon (`/api/notifications/stream`) and wire the button, or remove the button.
- **Severity:** Medium.

### A2.4 — `decision prompts` mentioned in FirstRun onboarding don't exist

- **File:** `cockpit/src/screens/FirstRunScreen.tsx:17`
- **Reasoning:** The "wow sequence" lists "inspect any node — live transcript, capability matrix, decision prompts" as a v1 feature. NodeScreen has no decision-prompt UI; the `decision` action is dead code (A1.8). Either implement decision prompts (PRD-aligned but out of release scope) or remove the line from the onboarding.
- **Severity:** Low.

### A2.5 — Cockpit emits "sigint sent" / "stop issued" lines for actions whose daemon implementation is fire-and-forget without confirmation

- **File:** `cockpit/src/App.tsx:286-329` `handleNodeAction`
- **Reasoning:** The cockpit calls `interruptNode(target.id)` and on resolution writes `"sigint sent"`. The daemon's `interrupt_node` (capability_service.rs) sends a real signal on Local substrate but is a no-op for substrates that don't support it. The cockpit's confirmation message implies success in either case. This is a category-of-confirmation issue, not a category-of-mock issue: the API call really was made; whether the substrate honored it is silent.
- **Recommended fix:** either (a) the daemon returns a structured response indicating actually-honored vs accepted-without-effect, and the cockpit surfaces that distinction, or (b) the cockpit removes the synthesized confirmation lines and lets the WS observe path show real lifecycle events. Path (b) aligns with A1.4.
- **Severity:** Medium.

### A2.6 — `attach in browser` and `native attach` buttons on NodeScreen both call the same action

- **File:** `cockpit/src/screens/NodeScreen.tsx:115-120`
- **Code:**
  ```
  <Btn ... onClick={() => fire("attach", "attach url issued")}>attach in browser</Btn>
  <Btn ... onClick={() => fire("attach", "native attach prepared")}>native attach</Btn>
  ```
- **Reasoning:** Both buttons emit the same `"attach"` NodeScreenAction, which `handleNodeAction` (App.tsx:293) handles by calling `requestBrowserAttach`. So clicking "native attach" actually opens a browser attach. The cockpit `api.ts:218` has a separate `requestNativeTarget` function but nothing calls it from a UI surface. The CLI's `asylum attach <id> --native` exists; the cockpit equivalent does not. Either wire the second button to `requestNativeTarget` and surface its returned command/args/env (a copy-pastable terminal snippet), or remove the button.
- **Severity:** Medium.

---

## A3. Hardcoded fake data

These render in the UI looking like real values but are static literals or fabricated. Worst offender: SettingsScreen.

### A3.1 — Entire SettingsScreen is a static mockup

- **File:** `cockpit/src/screens/SettingsScreen.tsx:8-172` (panels)
- **Specific fakes:**
  - `NtfySettings`: hardcoded channels `asylum-aaron`, `asylum-oncall` with fake "12 sent · 4 received" metrics. NOT WIRED to any backend (lines 11-29).
  - `AuthSettings`: owner token shown as literal `"a8x7…b91"`, pairing code `"ASLM-2F9D-C014"`, "3 active · 0 revoked today", "2 active · ttl 3600s" — all literals (lines 37-46).
  - `NetSettings`: bind shown as `"localhost:5173"` (the Vite dev port, not the Asylum bind), remote access "tailscale (recommended)", reverse proxy "none configured" — all static (lines 60-65).
  - `StorageSettings`: transcripts path `"~/Library/Asylum/transcripts · 1.4 GB"` (Mac path with fake size), retention "30 days (rolling)", redaction "on (api keys, jwt-like)" — all literals (lines 88-93).
  - `ApiSettings`: `${origin}/api/v1` (daemon serves at `/api`, not `/api/v1`), `/openapi.json (37 endpoints)` (no openapi.json endpoint exists), `@asylum/sdk@0.1.0 (typescript)` (no SDK package exists), the SDK quickstart code is fictitious (lines 105-129).
  - `CliSettings`: literal text snippet showing fake CLI output (lines 148-154).
  - `McpSettings`: "37 tools exposed" (real count is ~8), "connected clients: claude desktop, cursor" (the daemon has no MCP-client tracking) (lines 162-168).
- **Reasoning:** A user opening Settings sees what looks like a real account/network/storage panel and might trust the values. None of them are real. This is the highest-trust UI surface in the cockpit and it's pure fiction.
- **Severity:** Critical (must not ship to a real user).

### A3.2 — Cockpit version hardcoded in two places

- **Files:** `cockpit/src/App.tsx:414` `daemonVersion="asylum 0.1.0-rc4"`; `cockpit/src/screens/FirstRunScreen.tsx:38` `[ v0.1.0-rc4 · single-user · localhost ]`.
- **Reasoning:** No version is fetched from the daemon. There is no `/api/version` endpoint. Either add one (or extend `/api/health` to include the version), or read from `Cargo.toml` at build time and bake into a `cockpit/src/version.ts`. Hardcoding works for one release; the next release will silently lie about its version until a developer remembers to update both literals.
- **Severity:** Medium.

### A3.3 — FirstRun "0 nodes alive" hardcoded

- **File:** `cockpit/src/screens/FirstRunScreen.tsx:75` `<span>0 nodes alive</span>`
- **Reasoning:** It's literally always "0 nodes alive". On the first-run path, that may even be true (there are zero nodes when this screen shows). But it should still be derived from `graph.nodes.length` in case the screen is shown after nodes have been created. Trivial fix.
- **Severity:** Low.

### A3.4 — Inspector parent shown as `—` regardless of relationships

- **File:** `cockpit/src/components/Inspector.tsx:84` `["parent", "—"]`
- **Reasoning:** `Inspector` doesn't receive relationships data. NodeScreen does this correctly (`NodeScreen.tsx:88-93` looks up parent via relationships). Inspector should accept a `relationships` prop and resolve the parent the same way, OR accept a `parent` prop computed by the caller.
- **Severity:** Low.

---

## A4. Dead UI affordances

Buttons/items rendered as if interactive that have no `onClick` or have an empty `() => {}` handler. Each is a small lie.

| File | Line | Element | Disposition |
|---|---|---|---|
| `cockpit/src/screens/SettingsScreen.tsx` | 49-51 | "rotate owner token" Btn | Wire to a real rotate action OR delete |
| `cockpit/src/screens/SettingsScreen.tsx` | 264 | "add substrate" Btn | Wire OR delete (no add-substrate API exists; substrates are config-driven) |
| `cockpit/src/screens/SettingsScreen.tsx` | 288 | "install adapter" Btn | Wire OR delete (no harness-install API; harnesses are config-driven) |
| `cockpit/src/screens/SettingsScreen.tsx` | 26, 282, 301 | "more-horizontal" / "settings" iconOnly Btn | Delete |
| `cockpit/src/screens/SettingsScreen.tsx` | 39 | "copy" iconOnly next to owner token | Wire to clipboard copy of real token |
| `cockpit/src/screens/SettingsScreen.tsx` | 10 | "add channel" Btn | Wire to ChannelsScreen new-channel modal OR delete |
| `cockpit/src/screens/HooksScreen.tsx` | 122 | "more-horizontal" iconOnly | Delete |
| `cockpit/src/screens/HooksScreen.tsx` | 420 | "import" Btn | Wire to file-upload / paste-yaml import OR delete |
| `cockpit/src/screens/FleetScreen.tsx` | 62-64 | "filter" Btn | Wire to a filter modal OR delete (state filter buttons already exist below) |
| `cockpit/src/screens/FleetScreen.tsx` | 65-67 | "export" Btn | Wire to CSV export OR delete |
| `cockpit/src/screens/LogsScreen.tsx` | 68-70 | "filter" Btn | Same — delete since search field + level filters exist |
| `cockpit/src/screens/LogsScreen.tsx` | 71-73 | "export" Btn | Wire to JSON/text export OR delete |
| `cockpit/src/screens/LogsScreen.tsx` | 74-76 | "tail live" Btn | See A2.3 — implement SSE OR delete |
| `cockpit/src/screens/LogsScreen.tsx` | 96 | level filter buttons "run", "dbg" | Delete (severity values from daemon are info/warn/error only) |
| `cockpit/src/screens/ChannelsScreen.tsx` | 206-208 | "view spec" Btn | Delete (no spec viewer) |
| `cockpit/src/screens/ChannelsScreen.tsx` | 209-211 | "upvote" Btn | Delete |
| `cockpit/src/screens/NodeScreen.tsx` | 121 | "more-horizontal" iconOnly | Delete |
| `cockpit/src/screens/NodeScreen.tsx` | 118-120 | "native attach" Btn | See A2.6 |
| `cockpit/src/components/CmdK.tsx` | 32-38 | "attach in browser…" item | See A5.2 — should attach selected node |
| `cockpit/src/components/CmdK.tsx` | 39-45 | "send remote command…" item (action: () => {}) | Implement OR delete |
| `cockpit/src/components/CmdK.tsx` | placeholder | "find a node…" placeholder | See A5.3 — items list contains no nodes |
| `cockpit/src/components/Topbar.tsx` | 41 | "bell" iconOnly Btn | Wire to notifications popover OR delete |

**Severity (collective):** Medium. Each is small but together they create a "this is half-finished" impression that's incompatible with releasing.

---

## A5. Wiring inconsistencies

These are real backend features that the cockpit calls incorrectly or partially.

### A5.1 — `ApiSettings` shows wrong base URL prefix

- **File:** `cockpit/src/screens/SettingsScreen.tsx:105`
- **Code:** `<span className="v">{origin}/api/v1</span>`
- **Reality:** The daemon serves at `/api`, not `/api/v1`. There is no version prefix.
- **Fix:** show `${origin}/api`, or remove the API panel entirely until the OpenAPI spec is published.
- **Severity:** Low.

### A5.2 — `CmdK` "attach in browser" item navigates to cockpit screen instead of attaching

- **File:** `cockpit/src/components/CmdK.tsx:35-38`
- **Code:** `action: () => { onPick("cockpit"); }`
- **Fix:** Either delete the item or have it open a node-picker that, on selection, calls `requestBrowserAttach(node.id)`.
- **Severity:** Low.

### A5.3 — `CmdK` claims you can "find a node" but item list has no nodes

- **File:** `cockpit/src/components/CmdK.tsx:128`
- **Reasoning:** The placeholder advertises node search. The items array (lines 20-95) is static screens + 3 actions. No nodes are listed. Either accept `nodes: AsylumNode[]` as a prop and merge them in, or update the placeholder to "run a command, jump to a screen".
- **Severity:** Low.

### A5.4 — Logs `level` filter includes options that map to no data

- **File:** `cockpit/src/screens/LogsScreen.tsx:96`
- **Code:** `["all", "info", "warn", "err", "run", "dbg"]`
- **Reasoning:** `severityToLvl` (line 23-27) only maps `severity` → `info | warn | err`. The "run" and "dbg" buttons always filter to zero rows.
- **Fix:** drop "run" and "dbg".
- **Severity:** Low.

### A5.5 — `FleetScreen` STATE_FILTERS missing "stopped"

- **File:** `cockpit/src/screens/FleetScreen.tsx:26,40-47`
- **Reasoning:** `counts` initializes `stopped: 0` and is incremented by `uiStateOf`, but STATE_FILTERS array doesn't include it. Filter UI will never show "stopped" but the count for it is computed.
- **Fix:** add `"stopped"` to STATE_FILTERS.
- **Severity:** Trivial.

### A5.6 — `App.tsx` `onSendTest` test channel call swallows mismatch

- (handled separately by the existing review's M13/M14; included here as a pointer)

---

## A6. Cross-reference with prior ultrareview

The 2026-04-29 local-ultrareview report (`docs/reviews/2026-04-29-local-ultrareview-findings.md`) covered the codebase from a different angle. Where findings overlap, this audit's PR plan supersedes the workstream groupings in that report.

| Prior finding | This audit | Status |
|---|---|---|
| H5 (ntfy toast effect deps churn) | A2.1 root cause | Cockpit fix landed; structural fix in PR 3 |
| H6 (toast reply uses sender as node id) | A2.2 | "Reply not available" landed; full fix needs daemon-side correlation |
| H7 (resumeNode hits nonexistent endpoints) | A1.8 dead `decision` action | Already removed; clean up dead enum |
| M1 (attach token in events) | unchanged | Address in PR 6 |
| M2 (observe_ws never reads socket) | unchanged | Address in PR 6 |
| M3-M6 (transactionality / DB hygiene) | unchanged | Address in PR 6 |
| M7 (MCP replies to notifications) | unchanged | Address in PR 6 |
| M8 (pid-fallback daemon SIGHUP) | unchanged | Address in PR 6 |
| M9 (token in localStorage + WS query) | unchanged | Address in PR 6 |
| M10-M12 (cockpit duplicates / icons) | partially overlaps A4 | Address in PR 4 |
| M13-M17 (release scripts) | unchanged | Address in PR 6 |
| M18 (ntfy inbound polling not implemented) | A2.1 | Implemented in PR 3 |
| M19 (MCP exposes 8/60 capabilities) | unchanged | Address in PR 6 |
| M20 (NodeEvent has no schema_version) | unchanged | Address in PR 6 |
| M21 (TokenIssueRequest unused) | unchanged | Address in PR 6 |
| L1-L25 | partial overlap | Sweep in PR 7 |

---

# Part B — Architectural decisions

These resolve before-coding ambiguities so the implementer doesn't have to.

## B1. UI state vs. user preferences vs. daemon settings

The cockpit currently conflates three distinct concerns under "Tweaks":

- **Ephemeral UI state** (current screen, command palette open, selected node): plain `useState` in App.tsx.
- **User preferences** (theme, navCollapsed, graphLayout): persist in `localStorage` under a `asylum.uiPrefs` key.
- **Daemon-side settings** (ntfy server/topic, owner token rotation, retention): live on the daemon, exposed via API endpoints.

**Decision:** Split them. PR 1 introduces a `useUiPrefs()` hook backed by `localStorage` for the user-preference set, and direct `useState` for ephemeral state. Daemon settings are read fresh on each Settings open via API (PR 2).

## B2. Action confirmations in transcript

The current cockpit synthesizes "tool" entries when the operator clicks Inspector buttons. This pollutes the transcript with cockpit-generated content that didn't come from the harness.

**Decision:** PR 1 removes synthesized "tool" entries. Action feedback becomes a transient toast (or snackbar) visible briefly, plus the real lifecycle event from the WS observe path when the daemon emits one. Transcript ↔ harness output stays 1:1.

## B3. ntfy inbound architecture

ntfy.sh exposes an SSE endpoint at `https://<server>/<topic>/json` that streams newline-delimited JSON message records. This is the supported pull-from-server mechanism. Long-poll is also available at `https://<server>/<topic>/sse` but JSON-stream is simpler.

**Decision:** PR 3 adds a daemon background task per configured ntfy channel that subscribes to the JSON-stream endpoint, reconnects on disconnect with exponential backoff, parses each message, and inserts it via the existing `channel_inbound` path so that it shows up via `/api/channels/{id}/messages` with `direction="in"` and fires the `channel.inbound` hook event automatically.

## B4. Daemon version exposure

**Decision:** Extend `/api/health` to include `daemon_version: String` (read from `env!("CARGO_PKG_VERSION")`). The cockpit reads it on startup and renders everywhere a version is shown. No more hardcoded literals in cockpit code.

## B5. Settings screen scope for v1

Some Settings panels reflect features that are aspirational (SDK package, OpenAPI spec, MCP client tracking) and aren't part of v1. Settings panels for those would be lying.

**Decision:** PR 2 keeps Settings panels for: Substrates, Harnesses, Channels (link-out to ChannelsScreen), Auth & Tokens (real), Network (bind addr from health), Storage (real DB path + size). Drops API/SDK/CLI/MCP panels until those features actually exist. The CLI panel can become a small "see `asylum --help`" hint.

## B6. Compatibility / migration

Breaking the cockpit's `Tweaks`-driven URL or localStorage shape is fine — the cockpit is a single-page app served by the daemon, so the cockpit and daemon ship together. There are no external integrations against the cockpit's internal state. The owner token in localStorage stays under the same key; everything else is internal.

---

# Part C — Implementation plan

Each PR below is independently testable, lands one cohesive change, and produces a working cockpit + daemon at every checkpoint. PRs 1–4 are sequential (later PRs assume earlier ones landed). PR 5 and PR 6 can land in parallel after PR 4. PR 7 is the final pass.

> **Test discipline:** every code change has a TDD step (write failing test → implement → verify pass → commit). For UI-only changes where headless test coverage is awkward, use Vitest + React Testing Library where possible; for daemon code, integration tests against an in-memory `Store::open(":memory:")`. The repo already has both patterns established.

> **Commit discipline:** commit per task, message style following recent history (lowercase, terse, action verb first; e.g. `cockpit: drop simSpeed and Tweaks`).

---

## PR 1 — Strip prototype scaffolding from cockpit

**Goal:** Remove all simulation/mocking scaffolding from the cockpit. After this PR, the cockpit has no client-side simulated transcript animation, no `Tweaks` panel concept, no hardcoded prototype IDs, and no dead "decision" action wires. Behavior visible to users is unchanged except: theme/nav/layout preferences now persist across reloads (improvement); ntfy poll cadence becomes a constant 6s.

**Branch:** `cockpit-strip-prototype-scaffolding`

**Files touched:**
- Modify: `cockpit/src/App.tsx`
- Modify: `cockpit/src/components/NodeSession.tsx`
- Modify: `cockpit/src/components/Graph.tsx`
- Modify: `cockpit/src/components/Inspector.tsx`
- Modify: `cockpit/src/screens/CockpitScreen.tsx`
- Modify: `cockpit/src/screens/ChatScreen.tsx`
- Modify: `cockpit/src/screens/NodeScreen.tsx`
- Modify: `cockpit/src/screens/FirstRunScreen.tsx`
- Modify: `cockpit/src/cockpit.css`
- Create: `cockpit/src/lib/uiPrefs.ts`
- Create: `cockpit/src/lib/uiPrefs.test.ts`
- Modify: `cockpit/src/state.test.ts`

### Task 1.1: Add `useUiPrefs` hook for persisted preferences

- [x] **Step 1:** Write failing test `cockpit/src/lib/uiPrefs.test.ts`:
  ```ts
  import { describe, it, expect, beforeEach } from "vitest";
  import { renderHook, act } from "@testing-library/react";
  import { useUiPrefs, DEFAULT_UI_PREFS } from "./uiPrefs";

  describe("useUiPrefs", () => {
    beforeEach(() => window.localStorage.clear());

    it("returns defaults when nothing is stored", () => {
      const { result } = renderHook(() => useUiPrefs());
      expect(result.current[0]).toEqual(DEFAULT_UI_PREFS);
    });

    it("persists updates to localStorage", () => {
      const { result } = renderHook(() => useUiPrefs());
      act(() => result.current[1]("theme", "light"));
      const stored = JSON.parse(window.localStorage.getItem("asylum.uiPrefs")!);
      expect(stored.theme).toBe("light");
    });

    it("hydrates from existing localStorage value", () => {
      window.localStorage.setItem(
        "asylum.uiPrefs",
        JSON.stringify({ theme: "light", navCollapsed: true, graphLayout: "force" }),
      );
      const { result } = renderHook(() => useUiPrefs());
      expect(result.current[0].theme).toBe("light");
      expect(result.current[0].navCollapsed).toBe(true);
      expect(result.current[0].graphLayout).toBe("force");
    });

    it("ignores unknown keys in stored value", () => {
      window.localStorage.setItem("asylum.uiPrefs", JSON.stringify({ simSpeed: "live" }));
      const { result } = renderHook(() => useUiPrefs());
      expect(result.current[0]).toEqual(DEFAULT_UI_PREFS);
    });
  });
  ```

- [x] **Step 2:** Run `npm --prefix cockpit run test -- uiPrefs` — expect FAIL ("file not found").

- [x] **Step 3:** Create `cockpit/src/lib/uiPrefs.ts`:
  ```ts
  import { useCallback, useEffect, useState } from "react";
  import type { GraphLayout } from "../screens/CockpitScreen";

  export interface UiPrefs {
    theme: "dark" | "light";
    navCollapsed: boolean;
    graphLayout: GraphLayout;
  }

  export const DEFAULT_UI_PREFS: UiPrefs = {
    theme: "dark",
    navCollapsed: false,
    graphLayout: "tree",
  };

  const STORAGE_KEY = "asylum.uiPrefs";
  const VALID_LAYOUTS: GraphLayout[] = ["tree", "free", "force", "swimlanes"];

  function readStored(): UiPrefs {
    if (typeof window === "undefined") return DEFAULT_UI_PREFS;
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_UI_PREFS;
    try {
      const parsed = JSON.parse(raw) as Partial<UiPrefs>;
      return {
        theme: parsed.theme === "light" ? "light" : "dark",
        navCollapsed: Boolean(parsed.navCollapsed),
        graphLayout: VALID_LAYOUTS.includes(parsed.graphLayout as GraphLayout)
          ? (parsed.graphLayout as GraphLayout)
          : "tree",
      };
    } catch {
      return DEFAULT_UI_PREFS;
    }
  }

  export function useUiPrefs(): [UiPrefs, <K extends keyof UiPrefs>(k: K, v: UiPrefs[K]) => void] {
    const [prefs, setPrefs] = useState<UiPrefs>(readStored);

    useEffect(() => {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
    }, [prefs]);

    const setPref = useCallback(<K extends keyof UiPrefs>(k: K, v: UiPrefs[K]) => {
      setPrefs((cur) => ({ ...cur, [k]: v }));
    }, []);

    return [prefs, setPref];
  }
  ```

- [x] **Step 4:** Run `npm --prefix cockpit run test -- uiPrefs` — expect PASS.

- [x] **Step 5:** Commit.
  ```bash
  git add cockpit/src/lib/uiPrefs.ts cockpit/src/lib/uiPrefs.test.ts
  git commit -m "cockpit: add useUiPrefs persisted-prefs hook"
  ```

### Task 1.2: Replace `Tweaks` in App.tsx with `useUiPrefs` + drop `simSpeed` / `ntfyEnabled`

- [x] **Step 1:** In `cockpit/src/App.tsx`, delete the `Tweaks` interface (lines 49-55), the `DEFAULT_TWEAKS` constant (57-63), and the `tweaks` / `setTweak` block (76-79).

- [x] **Step 2:** Add at the top of `App()`:
  ```ts
  const [uiPrefs, setPref] = useUiPrefs();
  ```
  (and add `import { useUiPrefs } from "./lib/uiPrefs";` at the top of the file.)

- [x] **Step 3:** Replace every `tweaks.theme` with `uiPrefs.theme`, `tweaks.navCollapsed` with `uiPrefs.navCollapsed`, `tweaks.graphLayout` with `uiPrefs.graphLayout`. Replace every `setTweak("theme", v)` with `setPref("theme", v)` etc. (use replace-all).

- [x] **Step 4:** Delete the entire ntfy toast effect (lines 183-232), the `tweaks.simSpeed` and `tweaks.ntfyEnabled` references, and the `simSpeed` props passed to `<CockpitScreen>` and `<ChatScreen>`. Replace with:
  ```ts
  // ntfy toast spawner — polls the live ntfy channel for new inbound messages
  // and surfaces unseen ones as a lower-left toast.
  // channelsRef avoids tearing down the timer on every channel-list refresh.
  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      const ntfyChannel = channelsRef.current.find((c) => c.kind === "ntfy" && c.live);
      if (!ntfyChannel) return;
      try {
        const msgs = await fetchChannelMessages(ntfyChannel.id, 10);
        if (cancelled) return;
        const fresh = msgs.filter((m) => m.direction === "in" && m.id > lastSeenMessageId.current);
        if (fresh.length === 0) return;
        const latest = fresh[fresh.length - 1];
        lastSeenMessageId.current = latest.id;
        setToasts(() => [
          {
            id: "t-" + latest.id,
            from: latest.sender,
            nodeId: null,
            channel: ntfyChannel.name,
            subject: latest.subject,
            body: latest.subject ? `${latest.subject}\n${latest.body}` : latest.body,
            replies: latest.replies,
          },
        ]);
      } catch {
        /* silent — surface via Logs screen */
      }
    };
    const t = setInterval(tick, 6000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, []);
  ```
  Notes: poll cadence is a constant 6000ms; the `still`/`live`/`slow` toggle is gone; the effect runs once on mount and tears down on unmount — channel list updates flow through `channelsRef` (already in the file).

- [x] **Step 5:** Delete the no-op `{graph.nodes.some((n) => !isOperational(n)) && null}` JSX (lines 553-555), and remove the `isOperational` import from line 21 if it's no longer used.

- [x] **Step 6:** Delete the `onSpawn` callback (lines 341-346) and the `onSpawn` prop on `<CockpitScreen>` (line 439) and `<ChatScreen>` (line 485). The prop on `NodeSession` chains to `onSpawn` which only fires from `runResponse` (deleted in Task 1.3) — if leaving `onSpawn?` typed-optional in NodeSessionProps is convenient, that's fine; delete the call site here.

- [x] **Step 7:** Run `npm --prefix cockpit run build` — expect success (TypeScript will flag any leftover `tweaks.simSpeed` / `tweaks.ntfyEnabled` / unused-import that we missed; fix and rebuild).

- [x] **Step 8:** Run `npm --prefix cockpit run test` — all existing tests should pass (the `state.test.ts` test that referenced `simSpeed` will be updated in Task 1.5).

- [x] **Step 9:** Commit.
  ```bash
  git add cockpit/src/App.tsx
  git commit -m "cockpit: replace Tweaks with useUiPrefs; drop simSpeed and ntfyEnabled"
  ```

### Task 1.3: Delete `runResponse` / `SessionStep` / `streamText` machinery from NodeSession

- [x] **Step 1:** In `cockpit/src/components/NodeSession.tsx`, delete:
  - Lines 28-33 (`SessionStep` type)
  - Line 39 (`runResponse?:` field in `SessionBus`)
  - Line 45 (`simSpeed?:` field in `NodeSessionProps`)
  - Line 79-81 (`sleep` helper — only used by deleted code)
  - Line 99 (`simSpeed = "slow"` default param)
  - Lines 120-160 (`speedMul`, `streamText`, `runResponse`)
  - Line 170 (`runResponse,` from sessionBus.current)

  Also remove `import { ... }` / fields no longer used after deletion.

- [x] **Step 2:** In the `onAction` registration block (was lines 163-172), reduce to:
  ```ts
  useEffect(() => {
    if (!onAction) return;
    onAction.current = {
      pushSystem: (text) => setEntries(prev => [...prev, { kind: "sys-line", text }]),
      pushUser: (text) => setEntries(prev => [...prev, { kind: "user", text }]),
    };
  });
  ```
  Removes `pushTool` and `runResponse` from the bus. (See Task 1.4 for full bus removal — this is the safer minimal step that lets PR 1 land without retouching App's `handleNodeAction`.)

  Wait — actually we want to do the full removal here in Task 1.4 since the bus is itself prototype residue. Skip the `pushTool` removal in this step to keep the diff narrow; do it in 1.4.

- [x] **Step 3:** Update `NodeScreen.tsx:151` to remove the `simSpeed="slow"` prop:
  ```
  <NodeSession key={node.id} node={node} mode="fullscreen" />
  ```

- [x] **Step 4:** Update `CockpitScreen.tsx:22,46,103` to remove the `simSpeed` prop and field; same for `ChatScreen.tsx:20,30,90`.

- [x] **Step 5:** Run `npm --prefix cockpit run build` and `npm --prefix cockpit run test` — expect success.

- [x] **Step 6:** Commit.
  ```bash
  git add cockpit/src/components/NodeSession.tsx cockpit/src/screens/CockpitScreen.tsx cockpit/src/screens/ChatScreen.tsx cockpit/src/screens/NodeScreen.tsx
  git commit -m "cockpit: remove runResponse/streamText simulation machinery"
  ```

### Task 1.4: Replace imperative `pushSystem`/`pushTool` bus with toast confirmations

- [x] **Step 1:** In `cockpit/src/components/NodeSession.tsx`, delete the `SessionBus` interface (lines 35-40), the `onAction?:` prop field (line 48), the `onAction` parameter (line 102), and the `useEffect` registering `onAction.current` (now ~163-172 after Task 1.3).

  Also delete the `kind: "sys-line"` entry-type from `TranscriptEntry` (line 61) — without `pushSystem`, no caller emits `sys-line` rows except `initialTranscript` (which already does and should be kept). Wait — `initialTranscript` does emit `sys-line`, and `appendNodeEvent` does too for some kinds. Keep `sys-line`. Just delete the bus.

- [x] **Step 2:** In `cockpit/src/App.tsx`, replace the entire `handleNodeAction` block (lines 286-329) with a version that uses transient toasts via `setLocalError` for failures and a new `setLocalNotice` for successes. Add at the top of `App()`:
  ```ts
  const [localNotice, setLocalNotice] = useState<string | null>(null);
  useEffect(() => {
    if (!localNotice) return;
    const t = setTimeout(() => setLocalNotice(null), 2500);
    return () => clearTimeout(t);
  }, [localNotice]);
  ```
  Replace `handleNodeAction`:
  ```ts
  async function handleNodeAction(target: AsylumNode | undefined, action: InspectorAction, payload?: string) {
    if (!target) return;
    try {
      if (action === "attach") {
        const r = await requestBrowserAttach(target.id);
        setLocalNotice(`attach url issued · ttl ${r.expires_in_seconds ?? 3600}s`);
        if (typeof window !== "undefined" && r.attach_url) {
          window.open(r.attach_url, "_blank", "noopener,noreferrer");
        }
      } else if (action === "send") {
        setSelectedNode(target.id);
      } else if (action === "interrupt") {
        await interruptNode(target.id);
        setLocalNotice("interrupt sent");
      } else if (action === "restart") {
        await stopNode(target.id);
        setLocalNotice("stop issued; node will reset on relaunch");
      } else if (action === "archive") {
        await archiveNode(target.id);
        setLocalNotice("archive issued");
      } else if (action === "terminate") {
        await stopNode(target.id);
        setLocalNotice("stop issued; resources will be released");
      } else if (action === "fork") {
        const newNode = await forkNode(target.id, {});
        setLocalNotice(`forked into ${newNode.id}`);
        setOpenNodeId(newNode.id);
        setSelectedNode(newNode.id);
      }
    } catch (err) {
      setLocalError(`${action} failed: ${String(err instanceof Error ? err.message : err)}`);
    }
  }
  ```
  Note: the `decision` arm is deleted (see Task 1.6).

- [x] **Step 3:** Add a `<NoticeBanner>` render below `localError` (around line 551):
  ```tsx
  {localNotice && <div className="notice-banner">{localNotice}</div>}
  ```
  And add CSS for `.notice-banner` in `cockpit.css` mirroring `.error-banner` but with `--status-running` foreground.

- [x] **Step 4:** Delete the `sessionBus` ref (line 91), the `import type { SessionBus }` (line 37), all `sessionBus={sessionBus}` props on `<CockpitScreen>` (line 441) and `<ChatScreen>` (line 486), the `sessionBus` prop in `CockpitScreen.tsx:25,49,104` and `ChatScreen.tsx:22,32,92`, and the conditional `onAction={isCommandCenter(panelNode) ? sessionBus : undefined}` on the `<NodeSession>` calls in those screens.

- [x] **Step 5:** Run `npm --prefix cockpit run build` and `npm --prefix cockpit run test` — expect success.

- [x] **Step 6:** Commit.
  ```bash
  git add cockpit/
  git commit -m "cockpit: replace transcript-bus action confirmations with toasts"
  ```

### Task 1.5: Delete `layoutFree` seed; rename "free" layout to grid behavior; remove dead `decision` enum members and CSS; clean state.test.ts

- [x] **Step 1:** In `cockpit/src/components/Graph.tsx:75-93`, delete the `seed` map and rewrite `layoutFree` to be a pure grid:
  ```ts
  function layoutFree(nodes: GraphNode[], _w: number, _h: number): PosMap {
    const positions: PosMap = {};
    nodes.forEach((gn, i) => {
      positions[gn.node.id] = { x: 80 + (i % 4) * 200, y: 60 + Math.floor(i / 4) * 160 };
    });
    return positions;
  }
  ```
  Update the comment at lines 72-74 to: `// layout: hand-arranged 4-column grid by node order`.

- [x] **Step 2:** In `cockpit/src/components/Inspector.tsx:24` and `cockpit/src/screens/NodeScreen.tsx:37`, remove `| "decision"` from the action type unions. In `App.tsx:286-329`, the `decision` arm was already removed in Task 1.4.

- [x] **Step 3:** In `cockpit/src/cockpit.css`, delete the `.decision` block (lines 626-640 approximately — search for `.decision`), the `.tweaks-card` rule (line 689), and update line 1's comment to `/* asylum cockpit styles */`. Update line 941 comment from `/* ─── mode-specific tweaks ─── */` to `/* ─── mode-specific styling ─── */`.

- [x] **Step 4:** In `cockpit/src/screens/FirstRunScreen.tsx:17`, remove `, decision prompts` from the description string.

- [x] **Step 5:** In `cockpit/src/state.test.ts:69`, the test's reference to "simulate effect cleanup" can stay as a comment but anywhere it tests `simSpeed` behavior, remove. Read the file and adjust.

- [x] **Step 6:** Run `npm --prefix cockpit run build` and `npm --prefix cockpit run test` — expect success.

- [x] **Step 7:** Commit.
  ```bash
  git add cockpit/
  git commit -m "cockpit: delete prototype seed IDs, dead decision action, tweaks CSS"
  ```

### Task 1.6: Wire `Inspector` parent display

- [x] **Step 1:** In `cockpit/src/components/Inspector.tsx`, add a `relationships?: GraphRelationship[]` prop to `InspectorProps`.

- [x] **Step 2:** In the body of Inspector, replace `["parent", "—"]` (line 84) with parent resolution:
  ```ts
  const parentRel = relationships?.find((r) => r.target_node_id === node.id);
  const parentLabel = parentRel ? shortNodeId(parentRel.source_node_id) : "—";
  ```
  And pass `["parent", parentLabel]`.

- [x] **Step 3:** In `cockpit/src/screens/CockpitScreen.tsx` (where `<Inspector node={selected} ...>` is rendered), pass `relationships={...}`. The relationships array is already in the App's graph state — thread it down through CockpitScreenProps.

- [x] **Step 4:** Add a Vitest test confirming Inspector renders the parent shortNodeId when given a matching relationship.

- [x] **Step 5:** Run build + test, commit.

### Task 1.7: Hardcoded version cleanup (preview pass for PR 4 health endpoint extension)

- [x] **Step 1:** Replace `"asylum 0.1.0-rc4"` literal in `App.tsx:414` with `daemonVersion={daemonVersion}` where `daemonVersion` is a state variable initialized to `null` and to be populated by PR 4. Until then, fall through to the `?? "asylum"` default in Nav.tsx:92.

- [x] **Step 2:** Replace `[ v0.1.0-rc4 · single-user · localhost ]` in `FirstRunScreen.tsx:38` with `[ asylum · single-user · localhost ]` (drop the version literal entirely until PR 4 wires it).

- [x] **Step 3:** Replace hardcoded `0 nodes alive` in `FirstRunScreen.tsx:75` with the actual count: thread a `nodeCount` prop down from App.tsx (where `graph.nodes.length` is available) and render `{nodeCount} nodes alive`.

- [x] **Step 4:** Build + test, commit.

### PR 1 verification

- [x] `npm --prefix cockpit run build` succeeds
- [x] `npm --prefix cockpit run test` all pass
- [x] `cargo build --release` succeeds (cockpit assets are baked in)
- [x] Manually open the cockpit (in dev: `cargo run -- start` then visit `http://localhost:7717`):
  - Theme toggle persists across reload
  - Nav collapse persists across reload
  - Graph layout selection persists across reload
  - No "Tweaks" anywhere; no `simSpeed` setting reachable
  - Inspector buttons (interrupt, fork, restart, archive, terminate) emit toasts on success and red banners on failure; transcript stays clean of cockpit-synthesized lines
  - Reload while in node-detail screen — no `simSpeed="slow"` prop in dev tools
- [x] Grep audit: `rg -i 'simSpeed|tweaks|runResponse|SessionStep|streamText' cockpit/src` returns zero matches.

### PR 1 commit message (final consolidating commit if rebasing)

```
cockpit: strip prototype scaffolding (Tweaks, simSpeed, runResponse, decision)

removes the design-tool-era simulation machinery from cockpit:
- Tweaks interface and tweaks panel concept (replaced by useUiPrefs)
- simSpeed simulation knob (removed; ntfy poll cadence is now a constant)
- runResponse / streamText / SessionStep typing-effect machinery (dead code)
- pushTool/pushSystem/pushUser imperative bus (replaced by toast confirmations)
- "the prototype's notice" no-op JSX expression
- layoutFree's hardcoded prototype node-id seed map
- decision InspectorAction (was unreachable)
- prototype version literals (rc4) and hardcoded "0 nodes alive"

inspector now resolves parent from relationships data; preferences
(theme/nav/layout) persist in localStorage. behavior visible to users
is unchanged except: prefs persist; ntfy poll is constant 6s.
```

---

## PR 2 — Replace fake Settings screen with real daemon-backed settings

**Goal:** Settings screen shows real values from the daemon. No more fictional owner tokens, fake bind addresses, or invented MCP client lists.

**Branch:** `cockpit-real-settings`

**Daemon side — minor extensions:**
- Extend `/api/health` response with `daemon_version`, `bind_addr`, `database_path`, `database_size_bytes`, `transcripts_dir`.
- Add `GET /api/tokens` returning a list of `{ id, label, created_at, expires_at, revoked, last_used_at? }` (NEVER the raw value or hash).
- Add `POST /api/tokens/{id}/rotate` for owner-token rotation (issues a new token; returns the raw value once; revokes the old one when caller confirms).
- Reuse existing `/api/client-config` for ntfy server/topic.

**Files touched:**
- Modify: `crates/asylum-core/src/api.rs` (HealthResponse extension, TokenListResponse, TokenRotateResponse)
- Modify: `crates/asylum-daemon/src/capability_service.rs` (health, list_tokens, rotate_token)
- Modify: `crates/asylum-daemon/src/storage.rs` (list_tokens query — already exists as list_active_tokens; needs adjustment)
- Modify: `crates/asylum-daemon/src/app.rs` (route additions)
- Modify: `cockpit/src/api.ts` (fetch helpers)
- Modify: `cockpit/src/types.ts` (response types)
- Modify: `cockpit/src/screens/SettingsScreen.tsx` (rewrite all panels)

### Task 2.1: Extend `/api/health` with daemon_version + paths + sizes

- [x] **Step 1:** Write failing test in `crates/asylum-daemon/src/capability_service.rs` test module:
- [x] **Step 2:** `cargo test -p asylum-daemon health_response_includes_daemon_version_and_paths` — expect FAIL.
- [x] **Step 3:** Extend `HealthResponse` in `crates/asylum-core/src/api.rs`.
- [x] **Step 4:** Update `CapabilityService::health` to populate the new fields.
- [x] **Step 5:** Run test, expect PASS. Commit.

### Task 2.2: Add `GET /api/tokens` endpoint

- [x] **Step 1:** Failing test `list_tokens_returns_metadata_only`.
- [x] **Step 2:** Define `TokenSummary`, `TokenListResponse` in `crates/asylum-core/src/api.rs`.
- [x] **Step 3:** Implement `CapabilityService::list_tokens` + `Store::list_all_tokens()`.
- [x] **Step 4:** Add route `GET /api/tokens` + `api_tokens_list` handler in `app.rs`.
- [x] **Step 5:** Test passes. Commit.

### Task 2.3: Add `POST /api/tokens/{id}/rotate` endpoint (optional for Settings rotate button)

- [x] **Step 1:** Test `rotate_token_revokes_old_and_issues_new`.
- [x] **Step 2:** `CapabilityService::rotate_token(id)` + `Store::get_token_metadata()`.
- [x] **Step 3:** Wire route `POST /api/tokens/{id}/rotate`. Test passes. Commit.

### Task 2.4: Cockpit api.ts helpers for new endpoints

- [x] **Step 1:** In `cockpit/src/api.ts`, add `fetchHealth()`, `fetchTokens()`, `rotateToken(id)`. Add types in `cockpit/src/types.ts`.
- [x] **Step 2:** Commit.

### Task 2.5: Rewrite each Settings panel

- [x] **Step 1: NtfySettings.** Real channels from `fetchChannels()` filtered to `kind === "ntfy"`.
- [x] **Step 2: AuthSettings.** Masked token with copy; issued token counts from `fetchTokens()`; rotate flow.
- [x] **Step 3: NetSettings.** `health.bind_addr` replaces `localhost:5173`; dropped remote/proxy rows.
- [x] **Step 4: StorageSettings.** `health.transcripts_dir` and `formatBytes(health.database_size_bytes)`.
- [x] **Step 5: ApiSettings.** Dropped entirely.
- [x] **Step 6: CliSettings.** Dropped entirely.
- [x] **Step 7: McpSettings.** Dropped entirely.
- [x] **Step 8:** Removed developer group and `api | cli | mcp` from SectionId union.
- [x] **Step 9:** `npm run build` and `npm run test` — all pass.
- [x] **Step 10:** Commit.

### PR 2 verification

- [x] Each Settings panel value, when checked against the daemon's actual state, is correct.
- [x] Owner-token rotation flow implemented end-to-end (test confirms old token revoked, new token valid).
- [x] Removed panels (api, cli, mcp) no longer appear in the section sidebar.

---

## PR 3 — Implement ntfy inbound subscription on the daemon

**Goal:** When ntfy is configured, the daemon subscribes to the topic's JSON-stream endpoint and inserts incoming messages into `channel_messages` with `direction="in"`. The cockpit's existing toast spawner then surfaces them. Also fires the existing `channel.inbound` hook event.

**Branch:** `daemon-ntfy-inbound`

**Files touched:**
- Create: `crates/asylum-daemon/src/channels/ntfy_inbound.rs`
- Modify: `crates/asylum-daemon/src/channels/mod.rs` (re-export, integration)
- Modify: `crates/asylum-daemon/src/capability_service.rs` (start_background_tasks)
- Modify: `crates/asylum-core/src/config.rs` (poll_interval_seconds is consumed)
- Test: `crates/asylum-daemon/tests/ntfy_inbound.rs` (integration test against a mock ntfy server)

### Task 3.1: Implement the JSON-stream subscriber

- [ ] **Step 1:** Failing integration test. Stand up a `wiremock` server that streams two newline-delimited JSON message records (e.g.,`{"id":"abc","time":1714367400,"event":"message","topic":"asylum-test","message":"hello","title":"approve"}`). Configure the daemon's NtfyConfig to point at that mock server. Start `start_background_tasks`. Wait up to 5s for `store.list_channel_messages("ntfy-default", 10)` to contain a row with `direction="in"` and `body="hello"`.

- [ ] **Step 2:** `cargo test -p asylum-daemon ntfy_inbound` — expect FAIL.

- [ ] **Step 3:** Create `crates/asylum-daemon/src/channels/ntfy_inbound.rs`:
  ```rust
  use anyhow::{anyhow, Result};
  use asylum_core::api::ChannelInboundRequest;
  use futures::StreamExt;
  use serde::Deserialize;
  use std::sync::Arc;
  use std::time::Duration;
  use tokio::time::sleep;

  use crate::storage::Store;

  #[derive(Debug, Deserialize)]
  struct NtfyMessage {
      #[serde(default)]
      id: String,
      #[allow(dead_code)]
      #[serde(default)]
      time: u64,
      event: String,
      #[serde(default)]
      title: String,
      #[serde(default)]
      message: String,
      #[serde(default)]
      tags: Vec<String>,
  }

  pub struct NtfyInboundConfig {
      pub server: String,
      pub topic: String,
      pub channel_id: String,
      pub poll_interval_seconds: u64,
  }

  pub fn spawn(store: Arc<Store>, cfg: NtfyInboundConfig, hook_post: impl Fn(&str, Option<uuid::Uuid>, serde_json::Value) + Send + Sync + 'static) {
      let hook_post = Arc::new(hook_post);
      tokio::spawn(async move {
          let mut backoff = Duration::from_secs(2);
          loop {
              match run_subscription(&store, &cfg, hook_post.clone()).await {
                  Ok(_) => {
                      backoff = Duration::from_secs(2);
                  }
                  Err(error) => {
                      tracing::warn!(target: "ntfy_inbound", "subscription error: {error}");
                      sleep(backoff).await;
                      backoff = (backoff * 2).min(Duration::from_secs(60));
                  }
              }
          }
      });
  }

  async fn run_subscription(
      store: &Store,
      cfg: &NtfyInboundConfig,
      hook_post: Arc<impl Fn(&str, Option<uuid::Uuid>, serde_json::Value) + Send + Sync + 'static>,
  ) -> Result<()> {
      let url = format!("{}/{}/json", cfg.server.trim_end_matches('/'), cfg.topic);
      let response = reqwest::Client::new()
          .get(&url)
          .timeout(Duration::from_secs(0))   // streaming
          .send()
          .await?;
      if !response.status().is_success() {
          return Err(anyhow!("ntfy stream returned status {}", response.status()));
      }
      let mut stream = response.bytes_stream();
      let mut buffer = Vec::<u8>::new();
      while let Some(chunk) = stream.next().await {
          let chunk = chunk?;
          buffer.extend_from_slice(&chunk);
          while let Some(idx) = buffer.iter().position(|b| *b == b'\n') {
              let line = buffer.drain(..=idx).collect::<Vec<_>>();
              let line_str = std::str::from_utf8(&line[..line.len() - 1])
                  .unwrap_or("")
                  .trim();
              if line_str.is_empty() {
                  continue;
              }
              let msg: NtfyMessage = match serde_json::from_str(line_str) {
                  Ok(m) => m,
                  Err(_) => continue,   // ignore non-message lines (keepalive, errors)
              };
              if msg.event != "message" {
                  continue;
              }
              let req = ChannelInboundRequest {
                  sender: format!("ntfy:{}", cfg.topic),
                  subject: msg.title.clone(),
                  body: msg.message.clone(),
                  replies: msg.tags.clone(),
              };
              let _ = store.insert_channel_message(
                  &cfg.channel_id,
                  "in",
                  &req.sender,
                  &req.subject,
                  &req.body,
                  &req.replies,
              );
              (hook_post)(
                  "channel.inbound",
                  None,
                  serde_json::json!({
                      "channel_id": &cfg.channel_id,
                      "sender": req.sender,
                      "subject": req.subject,
                      "body": req.body,
                  }),
              );
          }
      }
      Ok(())
  }
  ```
  Notes: this uses `reqwest`'s streaming body. The `poll_interval_seconds` from NtfyConfig is honored as the reconnect-backoff floor, capped at 60s.

- [ ] **Step 4:** Re-export from `crates/asylum-daemon/src/channels/mod.rs` and call `ntfy_inbound::spawn` from `CapabilityService::start_background_tasks` when `config.ntfy_server` and `config.ntfy_topic` are both set. The hook closure passes through to `self.post_hook_event`.

- [ ] **Step 5:** Run integration test, expect PASS. Commit:
  ```
  daemon: subscribe to ntfy json stream and record inbound messages
  ```

### Task 3.2: Honor `poll_interval_seconds` as reconnect floor

- [ ] **Step 1:** In `ntfy_inbound.rs`, change the initial backoff from `Duration::from_secs(2)` to `Duration::from_secs(cfg.poll_interval_seconds.max(2))`.
- [ ] **Step 2:** Test that a deliberate connection failure (mock server returns 500) leads to a wait of at least `poll_interval_seconds` before retry. Commit.

### Task 3.3: Cockpit toast surfaces real inbound messages

- [ ] **Step 1:** No cockpit code changes needed — the existing toast spawner from PR 1 already polls `fetchChannelMessages` and filters `direction === "in"`. After PR 3's daemon subscriber lands, real ntfy messages flow through.
- [ ] **Step 2:** End-to-end manual smoke (record in PR description):
  ```
  curl -d "approve" ntfy.sh/<your-test-topic>
  # within 6s, the cockpit should display a toast with body "approve"
  ```

### Task 3.4: Update the seeded ntfy channel detail string

- [ ] **Step 1:** In `crates/asylum-daemon/src/channels/mod.rs:64-67`, the seeded ntfy channel says `"ntfy.sh outbound + inbound; configured via daemon ntfy settings"` when configured. After PR 3 lands, this is now accurate. No change required, but verify the string is still right.

### PR 3 verification

- [ ] Send a real ntfy message to your configured topic, see it as a toast in the cockpit within ~10s.
- [ ] Same message appears via `GET /api/channels/ntfy-default/messages` with `direction:"in"`.
- [ ] Define a hook with event `channel.inbound`; firing-log shows it triggered when a message arrives.
- [ ] Disable network, observe daemon logs show `subscription error`; restore network, see subscription reconnect.

---

## PR 4 — Wire or remove dead UI affordances + Logs screen real semantics

**Goal:** Every button/link in the cockpit either does what it claims or is removed. No UI affordance silently no-ops.

**Branch:** `cockpit-wire-or-remove-dead-ui`

**Approach:** Per the table in §A4, walk every dead affordance and decide: wire it, or delete it. For PR 4, default to "delete unless trivial to wire."

### Task 4.1: Fleet/Logs filter+export buttons

- [ ] **Step 1:** Delete the `<Btn icon="filter">filter</Btn>` and `<Btn icon="download">export</Btn>` buttons in `FleetScreen.tsx:62-67` (state filters below provide filter; CSV export is post-v1).
- [ ] **Step 2:** Delete `<Btn icon="filter">filter</Btn>` and `<Btn icon="download">export</Btn>` and `<Btn icon="play">tail live</Btn>` in `LogsScreen.tsx:67-76`.
- [ ] **Step 3:** Drop level filter buttons "run" and "dbg" from `LogsScreen.tsx:96`. Update test if any.
- [ ] **Step 4:** Add `"stopped"` to `STATE_FILTERS` in `FleetScreen.tsx:26`.
- [ ] **Step 5:** Build + test, commit.

### Task 4.2: Hooks/Channels small dead UI

- [ ] **Step 1:** Delete `<Btn icon="upload">import</Btn>` in `HooksScreen.tsx:420`.
- [ ] **Step 2:** Delete the trailing `<Btn icon="more-horizontal" iconOnly />` in `HooksScreen.tsx:122` (HookCard footer).
- [ ] **Step 3:** Delete `<Btn icon="git-pull-request">view spec</Btn>` and `<Btn icon="thumbs-up">upvote</Btn>` in `ChannelsScreen.tsx:206-211`.
- [ ] **Step 4:** Delete the `<Btn icon="more-horizontal" iconOnly />` in `NodeScreen.tsx:121`.
- [ ] **Step 5:** Build + test, commit.

### Task 4.3: NodeScreen native attach actually does native attach

- [ ] **Step 1:** Add a new `NodeScreenAction` value `"native-attach"`.
- [ ] **Step 2:** In `NodeScreen.tsx:118-120`, change the button to `fire("native-attach", "native attach prepared")`.
- [ ] **Step 3:** In `App.tsx:handleNodeAction`, add a case:
  ```ts
  } else if (action === "native-attach") {
    const target = await requestNativeTarget(target.id);
    const cmdLine = [target.command, ...(target.args ?? [])].join(" ");
    setLocalNotice(`copy this to a terminal:\n${cmdLine}`);
    if (typeof navigator !== "undefined" && navigator.clipboard) {
      void navigator.clipboard.writeText(cmdLine);
    }
  ```
- [ ] **Step 4:** Build + test, commit.

### Task 4.4: Topbar bell button → notifications popover

- [ ] **Step 1:** Either delete the bell button (line 41 in `Topbar.tsx`) OR wire it to a small popover that lists the latest 5 unread notifications and a link to `/logs`. Recommendation: delete for v1.
- [ ] **Step 2:** If deleting, build + test, commit.

### Task 4.5: Settings buttons that have no daemon backing get deleted

- [ ] **Step 1:** Already largely covered by PR 2. Verify no leftover dead Settings buttons after PR 2 + PR 4.

### PR 4 verification

- [ ] Click every visible button in the cockpit: each either does its labeled action or doesn't exist.
- [ ] Run `rg -l 'iconOnly' cockpit/src` — every result has an `onClick` handler nearby.

---

## PR 5 — CmdK real semantics + node finder

**Goal:** ⌘K palette can find nodes and execute the actions it claims.

**Branch:** `cockpit-cmdk-real`

**Files touched:**
- Modify: `cockpit/src/components/CmdK.tsx`
- Modify: `cockpit/src/App.tsx` (pass nodes to CmdK)

### Task 5.1: Pass nodes to CmdK and surface them in the items list

- [ ] **Step 1:** Add `nodes: AsylumNode[]` and `onPickNode: (node: AsylumNode) => void` props to CmdKProps.
- [ ] **Step 2:** Build a second item list dynamically from nodes:
  ```ts
  const nodeItems = nodes.map((n) => ({
    sec: "nodes",
    label: `${shortNodeId(n.id)} · ${n.role_hint} · ${n.harness}`,
    kbd: "",
    icon: n.role_hint === "command-center" ? "circle" : "square",
    action: () => onPickNode(n),
  }));
  ```
  Append to the existing items array.
- [ ] **Step 3:** In App.tsx, pass `nodes={graph.nodes}` and `onPickNode={(n) => { setOpenNodeId(n.id); setSelectedNode(n.id); setScreen("node"); setCmdkOpen(false); }}` to `<CmdK ...>`.
- [ ] **Step 4:** Test: open CmdK with two nodes in the graph, type a partial id, see it filter to that node, pick it, end up on NodeScreen for that node.

### Task 5.2: Wire "attach in browser…" and "send remote command…"

- [ ] **Step 1:** "attach in browser…" — change action to call `requestBrowserAttach(selectedNodeId)` if a node is selected; otherwise show a toast "select a node first".
- [ ] **Step 2:** "send remote command…" — open a modal that takes a node id (or the currently selected node) and a remote-command string, POSTs to `/api/remote-commands`. The remote-commands endpoint already exists on the daemon (`api_remote_commands`).
- [ ] **Step 3:** Test both end-to-end.

### Task 5.3: Update the placeholder

- [ ] **Step 1:** Change placeholder from "run a command, jump to a screen, find a node…" to "search nodes, jump to screens, run actions" (or keep as-is once nodes are surfaced — the placeholder will be accurate).

### Task 5.4: Decide on keyboard shortcut chips

- [ ] The `kbd: "N" | "1" | ","` labels next to items are not actually wired to global keyboard shortcuts. Either implement a global keymap (out of scope for v1) or remove the kbd column. Recommendation: remove.
- [ ] **Step 1:** Remove `kbd` from CmdKItem and the rendered chip.
- [ ] **Step 2:** Build + test, commit.

### PR 5 verification

- [ ] Open ⌘K, type partial node id, navigate to that node.
- [ ] Open ⌘K, "attach in browser" with a node selected, browser tab opens to attach URL.
- [ ] Open ⌘K, "send remote command", complete the modal, command executes (visible via `/api/remote-commands` log or daemon side-effect).

---

## PR 6 — Remaining Mediums from prior ultrareview

**Goal:** Land the remaining ultrareview Mediums that weren't covered by PRs 1–5. This is a large PR by surface area but each fix is small and independent — implement task-by-task.

**Branch:** `daemon-cockpit-medium-cleanup`

The list (from the prior review, with one-line restate):
- **M1:** Redact attach token raw value from events table
- **M2:** `handle_node_observe_ws` should `socket.split()` and `select!`
- **M3:** `fork_node` should propagate relationship-creation error
- **M4:** `append_transcript_chunk` should be transactional
- **M5:** PTY transcript persistence should log on error
- **M6:** Event sequence allocation should be transactional + UNIQUE(node_id, sequence)
- **M7:** MCP server should not respond to JSON-RPC notifications (`request.id.is_none()`)
- **M8:** pid-fallback daemon needs `Stdio::null()` + `setsid`
- **M9:** Owner token: keep in module-level memory (not localStorage) and use `Sec-WebSocket-Protocol` for WS auth — *this is PR 6 only because the localStorage path is convenient for development; a real release should harden this*
- **M10:** Drop optimistic local push for user input, rely on server `input_sent` event
- **M11:** Add deps array on `useEffect` with bus closures
- **M12:** Register or replace missing icon names (`layout-grid`, `list`, `activity`, `zap`, `sun`, `moon`)
- **M13:** publish-release recomputes archive sha256 against checksums.txt
- **M14:** install.sh hard-fails when archive is absent from checksums.txt
- **M15:** Hardlink rejection in install.sh's extract_binary handles all hardlinks
- **M16:** install.sh shell-quotes `$install_dir` in PATH block; atomic rc file write
- **M17:** Linux release builds run docker as `--user $(id -u):$(id -g)`
- **M19:** MCP exposes all root capabilities (generate ToolSpecs from CapabilityName)
- **M20:** NodeEvent gets a per-kind body type via tagged enum + schema_version
- **M21:** Delete `TokenIssueRequest` (use `TokenRequest` everywhere); decide on `TokenScope` (wire it or delete)

Each gets its own commit. Group by file when convenient (e.g., M13–M16 in two commits across `publish-release.sh` and `install.sh`).

**No detailed task breakdown here** — each Medium is well-scoped in the prior report (`docs/reviews/2026-04-29-local-ultrareview-findings.md` lines 147–326). The implementer should:
1. Read the corresponding section of that report.
2. Write a failing test where the bug is testable (most are).
3. Implement the fix.
4. Run the relevant test.
5. Commit with a message mirroring the existing style: `Fix M<n>: <one-line summary>`.

### PR 6 verification

- [ ] All 19 Mediums from the prior report addressed (M18 was PR 3).
- [ ] `cargo test --workspace` passes
- [ ] `npm --prefix cockpit run test` passes

---

## PR 7 — Release prep + end-to-end install verification

**Goal:** A user-installable Asylum that they can `curl | bash` and have working in <30s.

**Branch:** `release-prep-v1`

### Task 7.1: Lows sweep

- [ ] Walk L1–L25 from the prior ultrareview report. Most are trivial. Group by file. Skip Lows whose context has changed (e.g., L23 same-as-H5 already fixed; L24 covered by M20; L25 comment fix is two characters).

### Task 7.2: Build and verify the release pipeline end-to-end

- [ ] **Step 1:** From a clean clone on a fresh machine (or `docker run` an Ubuntu image), `bash scripts/build-release-artifacts.sh`. Verify all platform archives + checksums + minisign signature are produced under `dist/`.
- [ ] **Step 2:** `bash scripts/publish-release.sh --dry-run` against a fixture tag — verify HEAD/tag mismatch refuses; verify match proceeds.
- [ ] **Step 3:** From a fresh machine, run the published install one-liner: `curl -sSL https://asylum.../install.sh | bash`. Verify:
  - Binary lands in expected path
  - PATH is updated
  - First-run UX shows the `asylum start` hint
  - `asylum start` brings up the daemon
  - `asylum cockpit` opens the browser to a working cockpit

### Task 7.3: Smoke the H1, H2, H8 fixes that haven't been manually smoke-tested

- [ ] H1 manual smoke from the prior PR description: revoke a token via DELETE /api/tokens/{id}, confirm 401 without restart.
- [ ] H8 manual smoke: dry-run publish-release.sh against a fixture repo with mismatched HEAD/tag, confirm the abort message.
- [ ] H5 + PR 3 manual smoke: send an inbound ntfy message, confirm toast appears within 10s.

### Task 7.4: Documentation pass

- [ ] Update `README.md` with current install instructions, current minisign trust path setup, current `asylum start` flow.
- [ ] Update `docs/PRD.md` (or equivalent) — strike completion-bar items now done (notably ntfy inbound under §16, MCP catalog parity under §9 if PR 6 lands M19).
- [ ] Add a `CHANGELOG.md` entry for the release.

### PR 7 verification

- [ ] Fresh-machine install → working cockpit in under 30 seconds
- [ ] No "TODO" / "stub" / "mock" / "simulate" matches in `cockpit/src`, `crates/asylum-daemon/src`, `crates/asylum-core/src`, `crates/asylum/src` (excluding test files)
- [ ] `rg -i 'prototype|tweaks|simSpeed|runResponse' cockpit/src` returns zero matches
- [ ] All ultrareview Highs/Mediums from the prior report are crossed off OR have a tracking issue with deferral rationale
- [ ] CHANGELOG documents the release

---

# Part D — Verification matrix

A consolidated checklist for "is Asylum ready to release?" — should be runnable end-to-end from a fresh machine.

## Cockpit prototype residue eliminated

- [ ] No `Tweaks` / `tweaks-card` / `simSpeed` / `runResponse` / `SessionStep` / `streamText` references anywhere in `cockpit/src`
- [ ] No "the prototype's notice" comments
- [ ] No hardcoded short-id seed maps in graph layouts
- [ ] No "decision" InspectorAction or NodeScreenAction enum members
- [ ] No comments containing the word "prototype" or "mock" or "simulate" in production code (test/spec files exempt)

## Settings show real daemon-backed values

- [ ] Owner token panel shows masked real token; rotate works
- [ ] Network panel shows real bind addr from health
- [ ] Storage panel shows real DB path/size
- [ ] ntfy panel reflects real channel state
- [ ] No fake API/SDK/CLI/MCP panels remain

## ntfy inbound feature works end-to-end

- [ ] Daemon subscribes to configured ntfy topic at startup
- [ ] Inbound message → `direction='in'` row → cockpit toast
- [ ] `channel.inbound` hook event fires
- [ ] Subscription reconnects after transient failure

## All UI affordances are real

- [ ] Every button has a working `onClick` or doesn't exist
- [ ] Every menu item executes its labeled action or doesn't exist
- [ ] Inspector parent display resolves correctly when relationships exist

## Prior ultrareview Highs all crossed off

- [ ] H1–H9 all verified by manual smoke (see PR 7 Task 7.3)

## Prior ultrareview Mediums all addressed

- [ ] M1–M21 either fixed (PRs 3, 6) or explicitly deferred with rationale

## Installable from release artifact

- [ ] `curl | bash` install succeeds on fresh Ubuntu, fresh macOS
- [ ] Verifier rejects mismatched checksum
- [ ] minisign signature verified (where minisign is installed)
- [ ] First-run UX guides the user to `asylum start`
- [ ] Cockpit opens to a working empty graph + first-run hero

---

# Part E — Provenance

This audit was produced 2026-04-29 by reading every cockpit screen + component (`cockpit/src/{App.tsx,api.ts,types.ts,state.ts,components/*,screens/*,lib/*,*.css}`) and the daemon's HTTP route surface (`crates/asylum-daemon/src/{app.rs,capability_service.rs,channels/*,storage.rs}`), then cross-referencing each cockpit-visible feature against the daemon's actual capabilities.

The methodology was:

1. Read the entire route table in `app.rs:122-191`.
2. For each cockpit api.ts call, confirm the route exists and what its handler does.
3. For each cockpit screen, list every interactive element and trace its eventual API call (or absence).
4. Categorize findings (cruft / facade / hardcoded / dead-UI / wiring).
5. Cross-reference with the prior 2026-04-29 local-ultrareview report.

Reviewer: Claude (Opus 4.7), single-pass full-cockpit audit, no subagents.

Confidence: high for findings A1–A5 (every finding has cited file:line that I read directly). The cross-reference in A6 trusts the prior report's accuracy. Findings about backend feature presence/absence were verified by grep across the entire daemon source tree.

Known unknowns: I did not exercise the cockpit in a live browser during this audit (only static code analysis), so any "the timer doesn't fire" or "the button click does X" claim is grounded in code reading, not runtime observation. PR 7 Task 7.3 manual smokes will catch anything missed.
