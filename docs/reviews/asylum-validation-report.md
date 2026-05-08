# Asylum + Cockpit basic-flow validation — 2026-05-07

This is a black-box-with-occasional-source-peeks validation of the basic flow you asked for: launch a codex/claude_code node from cockpit, run hello-world, chat with it, clean up. I drove cockpit through a real browser (via the ui-validator), and inspected daemon source where surface behavior was confusing.

## Setup I tested

Two daemons:

- **Installed daemon** at `127.0.0.1:7717` (the one users get from `asylum start`). Asylum 0.1.9, systemd user service.
- **Dev daemon** at `127.0.0.1:7790` that I started by hand on top of the existing `target/debug/asylum`, with `~/.local/bin` and the nvm bin prepended to PATH so codex/claude were findable. This was a deliberate side-by-side to separate "Asylum is broken" from "PATH is misconfigured."

I used both `/tmp/asylum-codex-test` and `/tmp/asylum-claude-test` as workspaces.

## TL;DR

The cockpit shell (nav, fleet table, create form, events, decisions, settings) is built and renders cleanly. Launching a node creates a real harness subprocess. Stop/interrupt/archive work.

But **the basic flow you asked about — launch a node, watch it do hello-world, chat with it through the cockpit TUI — does not work end to end.** Three independent blockers stack on top of each other:

1. The installed daemon can't find `codex` or `claude` because the systemd service has a sanitized PATH that doesn't include `~/.local/bin` or the nvm bin path. Out of the box, **no node can be spawned at all.**
2. Once you fix PATH and a harness does spawn, **its TUI output is delivered to the cockpit as raw ANSI escape codes.** The cockpit has `@xterm/xterm` in `package.json` but doesn't import it anywhere — the "session" pane just appends raw text. Codex's full-screen TUI shows up as a wall of `\x1b[1;36H\x1b[0m\x1b[49m\x1b[K…` in the chat surface.
3. **The "launch packet (initial prompt)" textarea is a lie on the local substrate.** It is stored as the node's `description` in SQLite and never injected into the harness as a first user turn (`crates/asylum-daemon/src/capability_service.rs:1149-1242`). What actually happens: codex starts up, hits its own first-run "do you trust this directory?" prompt, and sits there waiting. No hello-world is ever requested.

On top of those, the **browser-attach feature is a Potemkin facade.** The "open attach tab" button opens `/attach/<token>` which serves `text/plain` with `{"node_id":"…"}` and nothing else. Capabilities advertise `browser_attach: true` for both harnesses (`harness/codex.rs:35`, `harness/claude.rs`).

Below: every concrete finding, ranked, with file pointers.

---

## Blockers (the basic flow is broken because of these)

### B1. Daemon can't find harness binaries when launched from systemd

The installed daemon at 7717 has these descriptors:

```
GET /api/harness-descriptors → both available: false
```

Because the systemd unit (`/home/casey/.config/systemd/user/asylum.service`) starts asylum with the default systemd PATH:

```
/home/casey/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/usr/games:/usr/local/games:/snap/bin
```

`claude` lives at `/home/casey/.local/bin/claude` and `codex` lives in nvm. Neither is on that PATH. So `command_available()` (`capability_service.rs:1066`) returns false, the cockpit grays out the picker buttons, and the form goes nowhere.

**Fix shape:** the daemon's launch path needs to either (a) inherit the user's interactive PATH at install time and bake it into the unit's `Environment=PATH=…`, or (b) on harness-spawn, look up binaries via login-shell PATH and surface a clear error if missing. `asylum doctor` should warn about this; right now it doesn't.

### B2. Cockpit advertises "future · adapter not built" when really the binary is just missing

`cockpit/src/screens/CreateScreen.tsx:179` and `SettingsScreen.tsx:410` both hardcode "future · adapter not built" / "not built" copy when `available: false`. The adapter IS built — the binary just isn't reachable. So the user sees "future" and concludes Asylum hasn't shipped these adapters yet, when it has, and the fix is on their PATH. Misleading copy is worse than missing copy here.

**Fix shape:** when `available: false`, surface "binary `codex` not found in daemon PATH" with the daemon's PATH dumped, plus a "rerun `asylum doctor`" affordance. Reserve "not built" copy for the channels-style "planned, no adapter shipped" case.

### B3. Failed spawn leaves a phantom node behind

In `capability_service.rs:1164-1190`:

```rust
let node = self.store.insert_node(...)?;        // committed first
…
self.local_substrate.launch(context).await?;    // failure here returns error,
self.store.set_node_liveness(...Running)?;      // never reached
```

If `launch` fails (binary not on PATH, workspace doesn't exist, etc.), the row is already in SQLite with `liveness = Starting`. `Starting` displays in cockpit as "running" with a live uptime counter that ticks forever. Subsequent send-input attempts return 400 `node not running`, which is at least surfaced inline in the session view, but the node never auto-transitions to `errored` and never disappears.

I reproduced this on the installed daemon: clicking "launch" left a node `36eac7c5…` that the validator saw show "running" indefinitely while the daemon log had no live PTY for it.

**Fix shape:** wrap the `launch + set_running` in something that rolls back the row (or transitions to `errored`) on spawn failure. At minimum, set liveness to `errored` in the error path.

### B4. "Launch packet (initial prompt)" is not injected into the harness on local substrate

`CreateScreen.tsx` puts a big "launch packet (initial prompt) — injected as the first user turn, after asylum context" textarea in front of the user. It is stored as `request.description` and saved to the node's `description` column. **Nothing ever feeds it into the harness's stdin or as a CLI arg** for the local substrate (`capability_service.rs:1188-1193`). The Loon path does construct a markdown prompt (line 1206), but local does not.

So when you typed "Print exactly the words: hello world from codex," codex never saw it. The codex CLI just started its own onboarding sequence (trust prompt, model picker) and waited.

This is the single most misleading piece of UX in the create flow. The label literally says "injected as the first user turn" — that's a contract the daemon doesn't honor.

**Fix shape:** either pipe `description` to the harness (codex via `--prompt`, claude via stdin or the equivalent), or relabel the field to "node description, not sent to the harness." Either is fine; the current state is a lie.

### B5. ANSI escape codes leak through to the cockpit "TUI" surface

`@xterm/xterm` and `@xterm/addon-fit` are in `cockpit/package.json` but `grep -rn "xterm" cockpit/src/` returns nothing. They aren't imported anywhere. `NodeSession.tsx:166-170` simply pushes incoming `output_chunk.text` as a `{ kind: "text", text }` entry, which renders as plain text. So when codex emits a full-screen TUI redraw, the user sees:

```
\x1b[?2004h\x1b[>7u\x1b[?1004h\x1b[6n\x1b[?u\x1b[c\x1b]10;?\x1b\\…
```

Same for claude_code, just with a different escape vocabulary. The "every node is a live tui session" copy in the chat rail is unsupported by what the renderer can actually do.

**Fix shape:** wire xterm.js into NodeSession — a real terminal emulator decoding the PTY bytes is what `browser_attach: true` and "live tui session" both promise. Until that's in, the chat surface should either (a) decode at minimum cursor-positioning and SGR via a minimal ANSI parser, or (b) honestly label itself as "raw transcript" and direct users to native attach.

### B6. Browser attach returns JSON, not a terminal

`POST /api/nodes/<id>/attach/browser` returns `{"url": "http://…/attach/<token>", …}`. Opening that URL hits `api_attach_page` (`app.rs:798-810`) which does:

```rust
let body = serde_json::json!({ "node_id": record.node_id }).to_string();
(StatusCode::OK, body).into_response()
```

`content-type: text/plain`, body is a JSON literal. No HTML wrapper, no xterm.js page, no WebSocket connection. The `/api/attach/<token>/ws` endpoint exists, but nothing consumes it from a browser.

Capabilities still advertise `browser_attach: true` for both harnesses (hardcoded in `harness/codex.rs:35` and `harness/claude.rs`). The cockpit's capability matrix shows a green check for it. The "open attach tab" button is everywhere. Clicking it is the most prominent broken thing in the product.

**Fix shape:** either ship a real `/attach/<token>` page that mounts xterm.js against `/api/attach/<token>/ws`, or set `browser_attach: false` on both harnesses and hide/disable the button. Right now this is straight-up false advertising.

### B7. Harness onboarding prompts block every fresh node

Even when codex spawns successfully, it starts in interactive onboarding:

- "Do you trust the contents of this directory?" (codex)
- "Quick safety check: Is this a project you created or one you trust?" (claude)

The validator sent `1` then "What is 2+2?"; neither selected the trust option (codex's TUI is in raw / kitty-keyboard mode and doesn't process the `1\n` we send the same way a keypress event would). Net result: the node is spawned but functionally inert until a human does a native attach to confirm.

This is why you can launch a node and see "running" forever and never get any hello-world. Asylum has no concept of "pre-trust the workspace before launch" or of harness-aware launch flags (codex has `--ask-for-approval untrusted`, claude has `--allow-dangerously-skip-permissions`).

**Fix shape:** per-harness launch profiles that pass the right "trust this dir" / "skip permissions" / "yes-to-all" flags by default for nodes that asylum is launching as workers. Make it opt-out, not opt-in. The user already trusts the workspace path they typed into asylum; asking them again inside the harness is friction with no benefit.

---

## Major (the flow is unworkable even after blockers)

### M1. Output streaming is inconsistent between harnesses on fresh nodes

On the dev daemon (PATH fixed):

- A freshly launched codex node had **zero** `output_chunk` events in 3+ minutes despite the codex process running and sitting at the trust prompt.
- A freshly launched claude_code node started streaming (raw ANSI but actual data) within ~35s.
- An older codex node from earlier in the session DID have one initial `output_chunk` capturing the trust prompt UI.

This smells like a race in the PTY reader hookup: if cockpit doesn't open the observe websocket fast enough on a fresh node, or if the daemon's reader task starts before the PTY has any output, the first frame can be lost. Worth investigating in `substrate/local.rs:95-118`.

### M2. Post-launch UX is jarring

Clicking "launch" in the create form takes you to `/fleet` (the nodes list), not to the node you just created. Users have to scan the list, find the new UUID, and click in. The flow is actively making you do extra work to see your own node's output.

**Fix shape:** navigate to `/<node-id>` after a successful POST.

### M3. Send-input doesn't echo what the user typed during running state

The cockpit `submit()` (`NodeSession.tsx:88-99`) does `postNodeInput` with no optimistic echo, relying on the daemon's `input_sent` event to round-trip back. In the validator's session, "What is 2+2?" disappeared from the input box and didn't reappear until the node was stopped (which fired a flush of the events into the transcript). For a running node, the user just sees their text vanish. Combined with the trust prompt issue, it looks like the input is being silently dropped.

**Fix shape:** either render a local "you sent: …" entry immediately, or ack on POST success rather than waiting for the event echo. The comment at line 92 ("No optimistic push — rely on the server input_sent event to avoid duplicates") chose correctness over feedback; for a chat surface, the trade-off is wrong.

### M4. Logs screen is mislabeled and visually empty

`LogsScreen.tsx` is titled "logs & telemetry" but the data source is the `notifications` array, not daemon logs. There's no `/api/logs` endpoint at all. With zero notifications the table renders just column headers over a black void, so the page looks broken on a fresh daemon. There's no way to view actual daemon log output (`asylum.log`) from the cockpit.

**Fix shape:** rename to "Notifications" (matches what it shows) and ship an empty-state with copy. If a daemon-log viewer is desirable, it needs a backend route.

### M5. Hooks "rules" tab is a black void on first use

Same disease: the rules list is empty by default and there's no empty-state copy. Three tabs: rules / firings / catalog. Catalog has 12 events; rules and firings are empty without explanation.

**Fix shape:** match the Decisions screen pattern — "no rules yet · rules show up here when you create one."

### M6. First-run screen "open cli" / "read the spec" both go to Settings

`App.tsx:429-430`: both buttons just `setScreen("settings")`. There's no actual CLI snippet in Settings, no spec reader, no link out. Two of the three CTA buttons on the first-run hero deliver the same dead end.

**Fix shape:** "open cli" should open a copy-pasteable `asylum cockpit`/`asylum status` recipe, or surface the asylum-cli reference. "read the spec" should link to `docs/specs/asylum-current-product-spec.md` or the README.

---

## Minor / nits

- **Cmd+K palette**: Ctrl+K does not open the palette on Linux. Clicking the search button does. The footer hints "↵ run · ↕ navigate · esc close" but doesn't say "click here, kbd doesn't work."
- **"Open in terminal" (native attach)** has no hover tooltip — users have to click to find out what it does.
- **Archive** changes state to "idle," keeps the node visible in graph and fleet list. Not obviously different from a stopped node. Consider hiding archived nodes by default with a toggle, or labeling the state more distinctly.
- **Settings → harnesses** lists both with "future · cli adapter · 7 capabilities · not built" identically — same misleading copy as the create form.

---

## Things that genuinely work

I want to be specific about what's actually solid so the picture isn't all bad:

- Cockpit nav and screen layout are consistent and clean. Dark theme, monospace, well-organized panels.
- Create form's structure (harness / substrate / role / workspace / launch packet / recipes) is the right shape; it just doesn't honor its own promises (B4).
- Fleet table is a real working node list with live uptime.
- Events tab shows real, structured, timestamped events — this is the honest, working surface for inspecting node activity.
- Decisions screen has good empty-state copy and a working creation form.
- Channels screen is honest: it labels planned-but-unbuilt adapters as "planned · adapters not built," and the live webhook channel works.
- Interrupt / stop / archive all transition state in <3s and update everywhere in the UI simultaneously.
- The error-display path for failed sends (`{"code":400, "message":"node not running"}`) appears inline in the transcript rather than as a popup — good design choice.
- Harness availability is detected at runtime (it's not hardcoded), so the moment PATH is fixed, the create form unblocks itself.

---

## Recommended order of attack

If I were prioritizing fixes for "the basic flow works":

1. **B1 + B2 together** — make the installed daemon find binaries via a sane PATH (or shell-based lookup) and either fix the descriptor copy or add a real diagnostic to `asylum doctor`. Without this, nothing else matters because no node ever spawns.
2. **B7** — auto-trust workspaces / pass per-harness skip-permission flags by default. Without this, every node sits at a trust prompt forever.
3. **B4** — either pipe the launch-packet text into the harness or relabel the field. Pick one.
4. **B5** — wire xterm.js into NodeSession. The "live TUI session" promise is real cockpit value — losing it makes the chat surface useless for any non-trivial harness output.
5. **B6** — ship the `/attach/<token>` HTML page or remove the affordance.
6. **B3** — roll back / mark-errored phantom nodes on spawn failure.
7. **M1-M6 + nits** — polish.

After 1, 2, 3, 4 you'd have a working hello-world flow. After 5, 6, 7 you'd have the product the cockpit shell already implies.

---

## State left behind by this validation

- `~/.asylum/` (installed daemon): four pre-existing stopped nodes plus one new phantom (`36eac7c5…`) created during the round-1 validator run. None are running processes — all sqlite rows.
- `/tmp/asylum-validate/` (dev daemon's state dir): leaving in place in case useful for repro. `asylum.sqlite3`, `asylum.sock`, `logs/daemon.log`. Daemon process and harness children stopped.
- `/tmp/asylum-codex-test`, `/tmp/asylum-claude-test`: empty workspace dirs I created. Safe to remove.
- Two stale debug daemons from prior sessions (pids 2069450, 3697341 on ports 7817 and 7800) were already running at the start and are still running. Out of scope.
