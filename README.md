# Asylum

Asylum is a single-user, always-on control plane for real agent harness sessions.

It does not replace Codex, Claude Code, Pi, Hermes, or future harnesses. It launches them, gives them shared tools and context, observes them, lets humans attach or intervene, and lets harnesses coordinate other harnesses across local and Loon-backed substrates.

The core product object is the **Node**: a live or resumable harness session running somewhere. A node may be a command center, supervisor, worker, evaluator, plain assistant, or custom role, but those are role hints, not mandatory workflow states.

## Start Here

- Product PRD: [docs/prd/asylum-live-v2-prd.md](docs/prd/asylum-live-v2-prd.md)
- Implementation-planning handoff: [docs/handoff/transition-to-implementation-planning.md](docs/handoff/transition-to-implementation-planning.md)
- Source and context trail: [docs/context/source-trail.md](docs/context/source-trail.md)

## Quick Start

### Build

```bash
cargo build --workspace
npm --prefix cockpit run build
```

### Run

```bash
./target/debug/asylum serve --database ./.asylum/asylum.sqlite3
```

The daemon serves:
- `http://127.0.0.1:7717/api/...` for APIs
- `/` for the Cockpit single-page UI when `cockpit/dist/index.html` exists
- `/assets/*` for static assets from `cockpit/dist/assets`

### Install

```bash
./target/debug/asylum install launchd
./target/debug/asylum install systemd
```

These commands print service definitions you can save as launch artifacts.

### CLI Operators

```bash
./target/debug/asylum config init
./target/debug/asylum config show
./target/debug/asylum node create --harness codex --substrate local --role worker
./target/debug/asylum node list
./target/debug/asylum node inspect <node-id>
./target/debug/asylum node send <node-id> "hello"
./target/debug/asylum node interrupt <node-id>
./target/debug/asylum node stop <node-id>
./target/debug/asylum node archive <node-id>
./target/debug/asylum graph get
./target/debug/asylum attach <node-id>
./target/debug/asylum token issue --name operator --scope node.create node.list graph.get
./target/debug/asylum notify send --title "note" --body "message"
./target/debug/asylum mcp
```

`asylum` also reads optional environment:
- `ASYLUM_BASE_URL` (default `http://127.0.0.1:7717`)
- `ASYLUM_TOKEN` (Bearer token for protected endpoints)

### Acceptance Walkthrough

1. Build and run `asylum serve` and confirm startup succeeds.
2. `curl http://127.0.0.1:7717/api/graph` returns a JSON object with `graph`.
3. Create a node:
   - `asylum node create --harness codex --substrate local --role worker`
   - Verify `/api/nodes/:id` returns created `node_id`.
4. Attach and observe:
   - `asylum attach <node-id>` prints a native attach command.
   - `wss`/WS path `/api/nodes/:id/observe/ws` returns at least an initial message and closes cleanly.
5. Emit a browser attach:
   - `asylum node inspect <node-id>` shows a node record.
   - `asylum mcp` starts JSON-RPC stdio server and advertises `node.create`, `node.list`, `node.inspect`, `node.send_input`, `node.interrupt`, `node.stop`, `graph.get`, `attach_url.issue`.
6. Generate a browser URL:
   - `curl -X POST -H 'Content-Type: application/json' /api/nodes/<id>/attach/browser` returns `{ "url", "expires_in_seconds" }` in `attach` response.
7. Generate notifications:
   - `asylum notify send --title "hello" --body "it works"` returns `notify sent: true` when sender config is enabled.
