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
curl -fsSL https://raw.githubusercontent.com/CaseyID/Asylum/main/scripts/install.sh | bash
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

Local release packaging produces archives named:

- `asylum-darwin-arm64.tar.gz`
- `asylum-darwin-x86_64.tar.gz`
- `asylum-linux-arm64.tar.gz`
- `asylum-linux-x86_64.tar.gz`

Each archive should contain exactly one executable `asylum` binary.

Build artifacts locally from a MacBook with Docker running:

```bash
scripts/build-release-artifacts.sh --version v0.1.1
scripts/publish-release.sh --version v0.1.1 --dry-run
scripts/publish-release.sh --version v0.1.1
scripts/test-release-install.sh --version v0.1.1
```

The local release builder uses native macOS Rust targets for `darwin-arm64` and `darwin-x86_64`, and Docker for `linux-arm64` and `linux-x86_64`. On Apple Silicon, `linux-x86_64` is cross-compiled from a native arm64 Linux container so the compiler does not run under amd64 emulation. The release build runs the Cockpit production build before compiling the release binaries so Cockpit is embedded into each archive.

### Trust model

The installer pulls the release archive and checksum file from
`https://github.com/CaseyID/Asylum/releases/download/<tag>/...` over HTTPS,
which pins the host's TLS identity to GitHub. On top of that:

1. **Checksum verification (mandatory by default).** The installer downloads
   `checksums.txt` (falling back to `<archive>.sha256`) and verifies the
   archive's SHA-256 against it.
   - If neither `sha256sum` nor `shasum` is on PATH, the installer
     **hard-fails** with a clear error rather than silently skipping. Install
     one of those tools and re-run.
   - To bypass verification deliberately (NOT RECOMMENDED — used only for
     local rescue when no hash tool is reachable), set
     `ASYLUM_SKIP_CHECKSUM=1`. The installer prints a loud warning and
     proceeds.

2. **Detached signature on the checksum file (optional today, mandatory
   once a key is published).** If `checksums.txt.minisig` exists in the
   release, `minisign` is on PATH, and a public key is configured (env
   `ASYLUM_RELEASE_PUBKEY`, or the embedded `ASYLUM_RELEASE_PUBKEY_DEFAULT`
   constant in `scripts/install.sh`), the installer verifies the signature
   before trusting the checksum file. Until the maintainer publishes a
   release-signing pubkey, the embedded constant is empty and the installer
   prints `warning: checksum file is unsigned` and proceeds with checksum-only
   verification. Once the maintainer pastes the pubkey into that constant,
   every existing installer download upgrades to verified-mode automatically.

3. **Publisher signing.** `scripts/publish-release.sh` produces
   `checksums.txt.minisig` alongside `checksums.txt` when
   `ASYLUM_RELEASE_SIGNING_KEY` is set in the publisher's environment and
   `minisign` is on PATH. Until that env var is set, no signature is
   produced and behavior matches the pre-signing flow.

Legacy fallback: if `checksums.txt` is unavailable from the release, the
installer falls back to `<archive>.sha256`. If neither artifact is reachable,
verification fails the same way as the missing-tool path (use
`ASYLUM_SKIP_CHECKSUM=1` to override).

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
