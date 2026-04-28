# Asylum

Asylum is a single-user, always-on control plane for real agent harness sessions.

It does not replace Codex, Claude Code, Pi, Hermes, or future harnesses. It launches them, gives them shared tools and context, observes them, lets humans attach or intervene, and lets harnesses coordinate other harnesses across local and Loon-backed substrates.

The core product object is the **Node**: a live or resumable harness session running somewhere. A node may be a command center, supervisor, worker, evaluator, plain assistant, or custom role, but those are role hints, not mandatory workflow states.

## Start Here

- Product PRD: [docs/prd/asylum-live-v2-prd.md](docs/prd/asylum-live-v2-prd.md)
- Implementation-planning handoff: [docs/handoff/transition-to-implementation-planning.md](docs/handoff/transition-to-implementation-planning.md)
- Source and context trail: [docs/context/source-trail.md](docs/context/source-trail.md)

## Product Path

```bash
curl -fsSL https://raw.githubusercontent.com/caseyID/Asylum/main/scripts/install.sh | bash
asylum
```
If installed via a piped/noninteractive install, `asylum` may only be on PATH after you restart/open a new shell. Ensure `~/.local/bin` is on PATH, add the printed `export PATH="...` line to your shell config, or run `~/.local/bin/asylum` directly when using the default install directory.

Running bare `asylum` does the product bootstrap path:
- Runs `asylum setup` if runtime files do not exist.
- Starts Asylum if it is not already running.
- Waits for service health during startup.
- Opens Cockpit in your browser.
- Prints the Cockpit URL.

### Core Commands

```bash
asylum setup
asylum cockpit
asylum start
asylum stop
asylum restart
asylum status
asylum doctor
asylum logs
asylum logs --tail
asylum update
```

`asylum update` reuses release resolution and fetch flow and then runs a health check afterwards.

## Release Artifact Expectations

Manual release packaging should produce archives named:

- `asylum-darwin-arm64.tar.gz`
- `asylum-darwin-x86_64.tar.gz`
- `asylum-linux-arm64.tar.gz`
- `asylum-linux-x86_64.tar.gz`

Each archive should contain exactly one executable `asylum` binary.

Checksum behavior:
- Installer first tries `checksums.txt` from the release.
- If absent, it tries `<archive>.sha256`.
- If neither exists, checksum verification is reported as skipped.
- If checksum tools are missing locally, checksum verification is also skipped.

For release binaries, ensure `npm --prefix cockpit run build` is run before `cargo build --release` so embedded Cockpit assets are present.

## Source and Advanced CLI (below product path)

### Source Build

```bash
cargo build --workspace
npm --prefix cockpit run build
```

### Source Run

```bash
./target/debug/asylum serve --database ./.asylum/asylum.sqlite3
```

For protected mode, bootstrap with an owner token and point CLI/Cockpit at it:

```bash
# terminal A
export ASYLUM_OWNER_TOKEN="$(uuidgen)"
./target/debug/asylum serve --owner-tokens-enabled

# terminal B
ASYLUM_TOKEN="$ASYLUM_OWNER_TOKEN" ./target/debug/asylum graph get
open "http://127.0.0.1:7717/?token=$ASYLUM_OWNER_TOKEN"
```

The debug daemon serves:
- `http://127.0.0.1:7717/api/...` for APIs
- `/` for the Cockpit single-page UI when `cockpit/dist/index.html` exists
- `/assets/*` for static assets from `cockpit/dist/assets`

### Service File Output

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
- `ASYLUM_OWNER_TOKEN` and `ASYLUM_OWNER_TOKENS_ENABLED` for daemon-side owner-token auth
- `ASYLUM_ATTACH_SECRET` for attach URL signing; omitted means a per-process random secret
- `ASYLUM_NTFY_SERVER`, `ASYLUM_NTFY_TOPIC`, `ASYLUM_NTFY_TOKEN`
- `ASYLUM_LOON_ENABLED`, `ASYLUM_LOON_ENDPOINT`, and optional config-file `loon.cli_path`, `loon.api_key_file`, `loon.cert_fingerprint_file`

When Loon is enabled, Asylum drives the documented `loon` CLI contract (`spawn`, `tell`, `interrupt`, `stop`, `terminate`, `attach`) and passes `LOON_ENDPOINT` plus configured auth/cert env vars to that process.

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
8. Exercise remote commands:
   - Issue a token, then `curl -X POST -H 'Content-Type: application/json' -d '{"command":"status token=<token>"}' http://127.0.0.1:7717/api/remote-commands`.
9. If Loon is configured:
   - `asylum serve --loon-enabled --loon-endpoint https://<host>:7777`
   - `asylum node create --harness claude_code --substrate loon --role worker`
