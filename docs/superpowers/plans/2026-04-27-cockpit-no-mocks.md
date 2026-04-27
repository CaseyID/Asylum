# Cockpit No-Mocks Implementation Plan

**Goal:** Eliminate every mock from the Asylum cockpit. Telemetry, channels, hooks,
recipes, and the fork action must all be real, persisted, and executable end to
end. Ship a daemon + cockpit that a real operator can run today.

**Architecture:** Extend the daemon (`crates/asylum-daemon`) with three new
durable tables — `channels`, `hooks`, `hook_firings` — plus a node-level
telemetry projection derived from the existing `events` log. Add five resource
groups under `/api/*`: telemetry (folded into `NodeRecord`), channels, hooks,
recipes, fork. Replace every cockpit screen that imports from
`cockpit/src/data/mock.ts` with API-driven data. Drop `mock.ts` entirely.

**Tech stack:** Rust (axum/tokio/rusqlite) on the backend; React 19 + TypeScript
+ zustand on the frontend. No new dependencies.

---

## Phase 1 — Backend (one agent, opus)

### B1: Telemetry on NodeRecord

- Add fields to `CapabilitySnapshot` neighbour: `tokens_in: u64`, `tokens_out: u64`,
  `tool_calls: u64`, `ctx_pct: f32`, `idle_seconds: u64` on `NodeRecord` (or via a
  parallel struct `NodeTelemetry` embedded into `NodeRecord` as a flatten field).
- Compute on read in `Store::list_nodes` / `get_node`:
  - `tokens_in` = sum of `len(events.body.text) / 4` across `input_sent`.
  - `tokens_out` = sum of `len(events.body.text) / 4` across `output_chunk`.
  - `tool_calls` = count of output chunks where the chunk matches one of the
    harness tool-call signatures: claude-code emits lines starting with
    `⏺` and `⎿`; codex emits `tool ` headers. Detect via regex
    `(?m)^\s*(?:⏺|tool\s)`.
  - `ctx_pct` = clamp(0..1, (tokens_in + tokens_out) / 200_000.0).
  - `idle_seconds` = `now - max(events.created_at)`.
- Test `Store::node_telemetry` with synthetic events.

### B2: Channels

- Schema: `channels(id TEXT PK, kind TEXT, name TEXT, label TEXT, direction TEXT,
  status TEXT, detail TEXT, config_json TEXT, live INTEGER, created_at INTEGER)`.
- Schema: `channel_messages(id INTEGER PK AUTOINCREMENT, channel_id TEXT,
  direction TEXT, ts INTEGER, sender TEXT, subject TEXT, body TEXT,
  replies_json TEXT)`.
- Seed at startup: ntfy (live iff config.ntfy.server+topic present), webhook
  (always live), sms-twilio/discord/slack/email-relay (live=0 future stubs).
- Endpoints:
  - `GET /api/channels` -> `{channels:[ChannelDescriptor]}`
  - `GET /api/channels/:id` -> single
  - `POST /api/channels` -> create custom channel
  - `PATCH /api/channels/:id` -> update name/detail/live/config
  - `DELETE /api/channels/:id` -> remove user-created (built-ins protected)
  - `GET /api/channels/:id/messages?limit=200` -> messages
  - `POST /api/channels/:id/test` body `{title, body}` -> sends through
    adapter (ntfy/webhook), records out message, returns success.
  - `POST /api/channels/:id/inbound` -> webhook receiver: HMAC verify with
    owner token, record an `in` message, optionally fan out to hook engine.
- `notify_send` already exists — wrap so every send records a message row.

### B3: Hooks

- Schema: `hooks(id TEXT PK, name TEXT, enabled INTEGER, event TEXT, filter TEXT,
  actions_json TEXT, future INTEGER, created_at INTEGER, updated_at INTEGER)`.
- Schema: `hook_firings(id INTEGER PK AUTOINCREMENT, hook_id TEXT, ts INTEGER,
  trigger TEXT, outcome TEXT, ok INTEGER, payload_json TEXT)`.
- Static event catalog: `node.permission_requested`, `node.exited`,
  `node.errored`, `node.idle`, `node.ctx_pressure`, `node.tool_call`,
  `graph.spawn`, `substrate.unreachable`, `channel.inbound`, `schedule.5m`,
  `schedule.30m`, `schedule.cron`. Returned via `GET /api/hooks/events`.
- Engine: a `HookEngine` actor owning a `tokio::sync::broadcast` event channel.
  Producers: `Store::record_event` posts the appropriate hook-event tag;
  scheduler tick task posts `schedule.5m`/`schedule.30m`. The engine evaluates
  matching rules against a tiny filter language (`any` | `key OP value` joined
  by `&&`/`||`) and runs actions.
- Action executors:
  - `channel`: lookup channel by id and call `notify_send` (with template
    interpolation of `{node.id}`, `{summary}`, etc).
  - `spawn`: target string `recipe:<id>` -> call `recipe_spawn(id)`.
  - `tool`: target string is a built-in (`graph.get`,
    `transcript.checkpoint`); a small dispatch table.
  - `pause_node`: `event.node` -> interrupt that node.
  - `archive`: `event.node` -> archive that node.
- Endpoints:
  - `GET /api/hooks` / `POST /api/hooks` / `PATCH /api/hooks/:id` /
    `DELETE /api/hooks/:id`
  - `GET /api/hooks/firings?limit=200`
  - `GET /api/hooks/events` (catalog)
  - `POST /api/hooks/:id/test` -> dry-run firing with a synthetic payload.

### B4: Recipes

- Promote `recipes::starter_recipes` to a public catalog API.
- `GET /api/recipes` -> `{recipes:[{id,title,prompt_template,kind}]}` — kind
  is `single` or `fanout` (fanout = "spawn-worker-nodes" /
  "parallel-exploration").
- `POST /api/recipes/:id/spawn` body `{harness, substrate, workspace?,
  description?}` -> create node(s). For `fanout` recipes, create one supervisor
  + 2 workers with `spawned_for` relationships.

### B5: Fork

- `POST /api/nodes/:id/fork` body `{role_hint?, workspace?, description?}` ->
  read source node; create new node with same harness+substrate (override
  workspace/role/description if supplied else inherit); insert relationship
  source=src target=new kind=`spawned_for` label=`fork`.
- Returns the new `NodeRecord`.

### B6: Wire-up

- `app.rs` registers all new routes inside the protected router.
- `capability_service.rs` exposes service methods used by handlers.
- Add capability descriptors for the new endpoints so `/api/capabilities`
  reflects them.

---

## Phase 2 — Cockpit (parallel agents after Phase 1 lands)

### F0: api.ts + types.ts (must run first; everything else depends)

- Extend `cockpit/src/types.ts`:
  - Add `tokens_in/tokens_out/tool_calls/ctx_pct/idle_seconds` (numbers) to
    `AsylumNode`.
  - Drop `HarnessDescriptor`, `SubstrateDescriptor`, `RecipeDescriptor`,
    `ChannelDescriptor`, `ChannelMessage`, `HookAction`, `HookRule`,
    `HookFiring`, `EventCatalogEntry`, `NtfyTemplate` design-system
    descriptors and re-introduce ones that match daemon shapes.
- Extend `cockpit/src/api.ts` with:
  - `fetchChannels`, `fetchChannelMessages`, `createChannel`, `updateChannel`,
    `deleteChannel`, `testChannel`
  - `fetchHooks`, `createHook`, `updateHook`, `deleteHook`, `fetchHookFirings`,
    `fetchHookEvents`, `dryRunHook`
  - `fetchRecipes`, `spawnRecipe`
  - `forkNode`
  - `openNodeObserveSocket(nodeId, onMessage, onError)` returning a `WebSocket`
    handle (use `ws://` from current location, append owner token as
    query string for auth, since the WS upgrade still passes through the
    auth middleware).

### F1: NodeSession real session (depends on F0)

- Drop `CC_RESPONSES`, `WORKER_RESPONSES`, `intent`. Replace `submit` with a
  call to `postNodeInput`.
- On mount: open the observe WebSocket. Each text frame is appended as a `text`
  entry with streaming caret on the latest. Server-emitted JSON frames
  (already-recorded events) populate `tool` / `sys-line` entries based on
  `kind`.
- Banner uses real telemetry (`telemetryFor(node)` reads from the node).

### F2: ChannelsScreen real (depends on F0)

- Replace `CHANNELS` / `CHANNEL_MESSAGES` imports with `useEffect` fetches.
- Implement send-test (calls `testChannel`), settings modal that PATCHes the
  channel, new-channel button that POSTs.

### F3: HooksScreen real (depends on F0)

- Replace `HOOKS` / `HOOK_FIRINGS` / `EVENT_CATALOG` imports with fetches.
- HookEditor saves via `createHook` / `updateHook`. Toggle persists
  immediately. Firings tab polls every 6s.

### F4: CreateScreen recipes (depends on F0)

- Fetch `/api/recipes`. Replace the local recipe constants with the response.
- "Use this recipe" button calls `spawnRecipe(id, {harness, substrate, ...})`
  and routes to the new node.

### F5: App.tsx fork + glyphs telemetry (depends on F0)

- `glyphs.ts::telemetryFor` returns the real fields from the node.
- `App.tsx`: replace the fork stub with `await forkNode(target.id)` then
  refresh and route to the new node.

### F6: Delete mock.ts (depends on F1..F5)

- Remove `cockpit/src/data/mock.ts` and its directory.
- Remove `NTFY_TEMPLATES` consumer from `App.tsx` toast spawner — the toast
  spawner now reads inbound channel messages from `fetchChannelMessages` for
  the live ntfy channel and renders the latest unseen one.

---

## Phase 3 — Verification

1. `cargo test --workspace` clean.
2. `cargo build --release` clean.
3. `npm --prefix cockpit ci && npm --prefix cockpit run build && npm --prefix
   cockpit test` clean.
4. `cargo run -p asylum-daemon -- serve --listen 127.0.0.1:7717` then load
   cockpit. Click through every screen, verify zero mock data references in
   the resulting JS bundle (`grep mock cockpit/dist/assets/*.js` must be
   empty of mock-driven imports).
5. Smoke through:
   - Create a node via Create screen using a recipe — see node appear, real
     transcript stream.
   - Trigger a hook (toggle a `schedule.5m` hook -> wait or hit
     `/api/hooks/:id/test`) — see firing land in the firings tab and ntfy
     channel get a message.
   - Fork a node from the inspector — see the new node + edge appear.
   - Send a test message from Channels -> see it appear in the messages list.
6. Commit on `main` with message describing the no-mocks delivery.

---

## Files map (final shape)

Backend:
- `crates/asylum-core/src/api.rs` (+ telemetry/channel/hook/recipe/fork types)
- `crates/asylum-core/src/node.rs` (+ telemetry fields on NodeRecord)
- `crates/asylum-daemon/src/app.rs` (+ new routes)
- `crates/asylum-daemon/src/capability_service.rs` (+ new service methods)
- `crates/asylum-daemon/src/storage.rs` (+ migrations + queries + telemetry)
- `crates/asylum-daemon/src/hooks/mod.rs` (NEW: HookEngine, filter eval, action exec)
- `crates/asylum-daemon/src/channels/mod.rs` (NEW: channel adapters + send/recv)
- `crates/asylum-daemon/src/recipes.rs` (+ catalog API surface)

Frontend:
- `cockpit/src/types.ts`
- `cockpit/src/api.ts`
- `cockpit/src/lib/glyphs.ts`
- `cockpit/src/components/NodeSession.tsx`
- `cockpit/src/screens/ChannelsScreen.tsx`
- `cockpit/src/screens/HooksScreen.tsx`
- `cockpit/src/screens/CreateScreen.tsx`
- `cockpit/src/App.tsx`
- (delete) `cockpit/src/data/mock.ts`
