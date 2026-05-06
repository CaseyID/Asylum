# Asylum

Asylum is a single-user, always-on control plane for real agent harness sessions.

It does not replace Codex, Claude Code, Pi, Hermes, or future harnesses. It launches them, gives them shared tools and context, observes them, lets humans attach or intervene, and lets harnesses coordinate other harnesses across local and Loon-backed substrates.

The core product object is the **Node**: a live or resumable harness session running somewhere. A node may be a command center, supervisor, worker, evaluator, plain assistant, or custom role, but those are role hints, not mandatory workflow states.

## Start Here

- Current product spec: [docs/specs/asylum-current-product-spec.md](docs/specs/asylum-current-product-spec.md)
- Product PRD: [docs/prd/asylum-live-v2-prd.md](docs/prd/asylum-live-v2-prd.md)
- Spec coverage audit brief: [docs/reviews/2026-05-05-asylum-spec-coverage-audit-brief.md](docs/reviews/2026-05-05-asylum-spec-coverage-audit-brief.md)
- Implementation-planning handoff: [docs/handoff/transition-to-implementation-planning.md](docs/handoff/transition-to-implementation-planning.md)
- Source and context trail: [docs/context/source-trail.md](docs/context/source-trail.md)

## Product Path

### Install

```bash
curl -fsSL https://raw.githubusercontent.com/CaseyID/Asylum/main/scripts/install.sh | bash
```

The installer downloads the latest release archive from GitHub, verifies its SHA-256 checksum, and installs the `asylum` binary to `~/.local/bin` (or the directory you specify with `--install-dir`).

After install, open a new shell (or run the printed `export PATH=...` line), then:

```bash
asylum
```

Running bare `asylum` does the product bootstrap path:
- Runs `asylum setup` if runtime files do not exist.
- Starts Asylum if it is not already running.
- Waits for service health during startup.
- Opens Cockpit in your browser.
- Prints the Cockpit URL.

If installed via a piped/noninteractive install, `asylum` may only be on PATH after you restart/open a new shell.

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

`asylum update` downloads the latest release, verifies its checksum, and restarts the service.

### ntfy Notifications (Optional)

Asylum can send and receive notifications via [ntfy.sh](https://ntfy.sh) or a self-hosted ntfy server. Set these environment variables before `asylum setup`:

```bash
export ASYLUM_NTFY_SERVER="https://ntfy.sh"   # or your self-hosted URL
export ASYLUM_NTFY_TOPIC="your-private-topic"
export ASYLUM_NTFY_TOKEN="your-access-token"  # if your server requires auth
```

When configured, the daemon subscribes to the topic at startup. Inbound ntfy messages appear as toasts in Cockpit and trigger `channel.inbound` hooks. Nodes can send outbound notifications via `asylum notify send`.

### Known Limits

- Asylum is single-user in v0.1.x. Owner tokens protect HTTP access, but token scopes are advisory labels, not per-route authorization.
- Local substrate behavior is the most validated path. Loon is optional and requires a configured Loon endpoint plus the `loon` CLI contract.
- Inbound ntfy/webhook messages are recorded and can trigger hooks, but node addressing/reply correlation is still limited.
- Decisions are not yet a first-class operator workflow; remote approve/deny pieces exist, but pending decision surfacing is incomplete.
- Keep Cockpit bound to localhost unless you are deliberately protecting access with a private network such as Tailscale. Browser attach URLs and transcripts are sensitive.

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
./target/debug/asylum daemon run --database ./.asylum/asylum.sqlite3
```

For protected mode, bootstrap with an owner token and point CLI/Cockpit at it:

```bash
# terminal A
export ASYLUM_OWNER_TOKEN="$(uuidgen)"
./target/debug/asylum daemon run --owner-tokens-enabled

# terminal B
ASYLUM_TOKEN="$ASYLUM_OWNER_TOKEN" ./target/debug/asylum graph get
open "http://127.0.0.1:7717/?token=$ASYLUM_OWNER_TOKEN"
```

The debug daemon exposes:
- `~/.asylum/run/asylum.sock` for local CLI/MCP control
- `http://127.0.0.1:7717/api/...` for Cockpit HTTP APIs
- `/` for the Cockpit single-page UI when `cockpit/dist/index.html` exists
- `/assets/*` for static assets from `cockpit/dist/assets`

### Service File Output

```bash
./target/debug/asylum service generate launchd
./target/debug/asylum service generate systemd
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

Token scopes are advisory labels in v0.1.x. Owner-token auth is enforced at the token level; per-route scope enforcement is not implemented yet.

`asylum` also reads optional environment:
- `ASYLUM_SOCKET_PATH` (default `~/.asylum/run/asylum.sock`) for local CLI/MCP daemon control
- `ASYLUM_BASE_URL` (default `http://127.0.0.1:7717`) for Cockpit URLs and explicit HTTP clients
- `ASYLUM_TOKEN` (Bearer token for protected endpoints)
- `ASYLUM_OWNER_TOKEN` and `ASYLUM_OWNER_TOKENS_ENABLED` for daemon-side owner-token auth
- `ASYLUM_ATTACH_SECRET` for attach URL signing; omitted means a per-process random secret
- `ASYLUM_NTFY_SERVER`, `ASYLUM_NTFY_TOPIC`, `ASYLUM_NTFY_TOKEN`
- `ASYLUM_LOON_ENABLED`, `ASYLUM_LOON_ENDPOINT`, and optional config-file `loon.cli_path`, `loon.api_key_file`, `loon.cert_fingerprint_file`

When Loon is enabled, Asylum drives the documented `loon` CLI contract (`spawn`, `tell`, `interrupt`, `stop`, `terminate`, `attach`) and passes `LOON_ENDPOINT` plus configured auth/cert env vars to that process.

### Acceptance Walkthrough

1. Build and run `asylum daemon run` and confirm startup succeeds.
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
   - `asylum daemon run --loon-enabled --loon-endpoint https://<host>:7777`
   - `asylum node create --harness claude_code --substrate loon --role worker`
