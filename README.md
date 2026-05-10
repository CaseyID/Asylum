# Asylum

Asylum is a single-user, always-on control plane for real agent harness sessions.

It does not replace Codex, Claude Code, Pi, Hermes, or future harnesses. It launches them, gives them shared tools and context, observes them, lets humans open live node sessions and intervene, and lets harnesses coordinate other harnesses across local and Loon-backed substrates.

The core product object is the **Node**: a live or resumable harness session running somewhere. A node may be a command center, supervisor, worker, evaluator, plain assistant, or custom role, but those are role hints, not mandatory workflow states.

## Start Here

- Current product spec: [docs/specs/asylum-current-product-spec.md](docs/specs/asylum-current-product-spec.md)
- Docs map: [docs/README.md](docs/README.md)
- Current node session UX plan: [docs/superpowers/plans/2026-05-09-cockpit-node-session-ux.md](docs/superpowers/plans/2026-05-09-cockpit-node-session-ux.md)
- Release ledger: [RELEASES.md](RELEASES.md)

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
- Keep Cockpit bound to localhost unless you are deliberately protecting access with a private network such as Tailscale. Session URLs and transcripts are sensitive.

## Release Artifact Expectations

Asylum releases are built locally from this checkout. There is no GitHub
Actions release pipeline. Use the Cargo release commands for the normal path;
they wrap the release scripts behind the scenes.

Local release packaging produces archives named:

- `asylum-darwin-arm64.tar.gz`
- `asylum-darwin-x86_64.tar.gz`
- `asylum-linux-arm64.tar.gz`
- `asylum-linux-x86_64.tar.gz`

Each archive should contain exactly one executable `asylum` binary.

Normal release flow:

```bash
# after version/changelog/release-ledger updates are committed
git tag -a vX.Y.Z -m "vX.Y.Z"
cargo build-asylum-release -- --version vX.Y.Z
cargo test-asylum-release -- --version vX.Y.Z
cargo publish-asylum-release -- --version vX.Y.Z --dry-run
cargo publish-asylum-release -- --version vX.Y.Z
```

If you omit `--version`, the commands use the workspace version in
`Cargo.toml`. Build/test/publish read and write `dist/release/vX.Y.Z/`. To
pass release-script options, put them after `--`:

```bash
cargo build-asylum-release -- --version v0.1.11 --targets linux-x86_64,darwin-arm64
cargo test-asylum-release -- --version v0.1.11
cargo publish-asylum-release -- --version v0.1.11 --targets linux-x86_64,darwin-arm64 --dry-run
```

`cargo publish-asylum-release` expects a clean working tree and a local tag
for the release version pointing at `HEAD`. It preserves existing GitHub
Release assets unless you explicitly pass `--allow-clobber`.

The underlying scripts are still available for lower-level work:

- `scripts/build-release-artifacts.sh`
- `scripts/publish-release.sh`
- `scripts/test-release-install.sh`

The release builder runs `npm --prefix cockpit ci`, builds Cockpit production
assets, then compiles the release binaries so Cockpit is embedded into each
archive. Both Apple Silicon macOS and Linux x86_64 hosts can build the full
four-archive matrix:

- On Linux x86_64, `linux-x86_64` builds natively in Docker, while
  `darwin-arm64`, `darwin-x86_64`, and `linux-arm64` build through the pinned
  `ghcr.io/rust-cross/cargo-zigbuild` Docker image. That image provides
  cargo-zigbuild, zig, and the macOS SDK; no QEMU, osxcross, or extra apt
  setup is required beyond Docker.
- On Apple Silicon macOS, Darwin targets use native/cross Rust targets, Linux
  ARM builds in an arm64 Docker container, and Linux x86_64 is cross-compiled
  from that arm64 Linux container.

Installers fetch release archives from GitHub Releases over HTTPS and verify
the archive checksum before installing. If release signing is configured,
`checksums.txt.minisig` is published and verified as well.

## Source Development With Cargo

Cargo commands operate on this source checkout only. They do not manage the
installed `asylum` binary or user service. The repo-local `xtask` crate backs
these aliases so day-to-day source work does not require typing direct
`npm --prefix cockpit ...` or release-script commands.

Naming:

- `run-*` starts a source-built process.
- `build-*` produces artifacts and exits.
- `test-*` runs tests and exits.
- `check-*` runs fast validation and exits.
- `status-*` reports source-dev runtime state.
- `stop-*` stops source-dev runtime processes.
- `reset-*` stops source-dev processes and removes source-dev runtime state.
- `*-dev` means watch/hot reload/source-dev runtime state.

| Command                         | Meaning |
|---------------------------------|---------|
| `cargo run-asylum-dev`          | Full source dev loop: daemon + Cockpit hot reload. Long-running. |
| `cargo run-daemon-dev`          | Source daemon only, watched/restarted on Rust changes, `.asylum-dev`, `127.0.0.1:7788`. |
| `cargo run-cockpit-dev`         | Cockpit/Vite only, hot reload, proxies to source daemon. |
| `cargo run-asylum`              | Product-like source run: build Cockpit once, then run daemon serving built UI. No hot reload. |
| `cargo run-daemon`              | Source daemon only, no watch/hot reload. |
| `cargo build-asylum`            | Full source product build: Cockpit assets + Rust workspace. |
| `cargo build-rust`              | Rust workspace only. |
| `cargo build-cockpit`           | Cockpit production assets only. |
| `cargo build-asylum-release`    | Build release artifacts into `dist/release/vX.Y.Z/`; wraps the release build script internally. |
| `cargo test-asylum`             | Full repo test pass: Rust + Cockpit. |
| `cargo test-rust`               | Rust workspace tests only. |
| `cargo test-cockpit`            | Cockpit/Vitest only. |
| `cargo test-asylum-release`     | Smoke-test the host release archive from `dist/release/vX.Y.Z/`. |
| `cargo check-asylum`            | Fast preflight: format/check/build-style validation, no long-running server. |
| `cargo status-asylum-dev`       | Show source-dev daemon/Vite processes, ports, and `.asylum-dev` state. |
| `cargo stop-asylum-dev`         | Stop source-dev daemon/Vite processes for this checkout. |
| `cargo reset-asylum-dev`        | Stop source-dev processes and clear `.asylum-dev`. |
| `cargo publish-asylum-release`  | Publish already-built release artifacts to GitHub Releases; wraps the release publish script internally. |

Runtime and artifact paths:

| Path                    | Purpose |
|-------------------------|---------|
| `target/debug/`         | Source-built Rust binaries and debug artifacts. |
| `target/release/`       | Source-built Rust release artifacts. |
| `cockpit/dist/`         | Cockpit production build used by product-like source runs and releases. |
| `.asylum-dev/`          | Source-dev runtime state: config, DB, socket, logs. Safe to delete via `cargo reset-asylum-dev`. |
| `dist/release/vX.Y.Z/`  | Local release archives/checksums before publishing. |

Common workflows:

```bash
# Full source dev loop
cargo run-asylum-dev

# Backend-only work
cargo run-daemon-dev

# Frontend-only work
cargo run-cockpit-dev

# Check what source dev left running
cargo status-asylum-dev

# Stop source dev processes
cargo stop-asylum-dev

# Product-like run from the checkout, no hot reload
cargo run-asylum

# Full build and test
cargo build-asylum
cargo test-asylum
```

Release workflow from source:

```bash
cargo build-asylum-release
cargo test-asylum-release
cargo publish-asylum-release
```

Useful source-dev overrides: `ASYLUM_DEV_BIND=127.0.0.1:7790` changes the
daemon bind, and `ASYLUM_COCKPIT_DEV_PORT=5174` changes the Vite dev-server
port.

For protected mode, bootstrap with an owner token and point CLI/Cockpit at it:

```bash
# terminal A
export ASYLUM_OWNER_TOKEN="$(uuidgen)"
cargo run -p asylum -- daemon run --owner-tokens-enabled

# terminal B
ASYLUM_TOKEN="$ASYLUM_OWNER_TOKEN" ./target/debug/asylum graph get
open "http://127.0.0.1:7717/?token=$ASYLUM_OWNER_TOKEN"
```

The source daemon exposes:
- `.asylum-dev/run/asylum.sock` for local CLI/MCP control when using the Cargo source workflow
- `http://127.0.0.1:7788/api/...` for Cockpit HTTP APIs by default
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
./target/debug/asylum node spawn <source-node-id> --role worker
./target/debug/asylum node list
./target/debug/asylum node inspect <node-id>
./target/debug/asylum node send <node-id> "hello"
./target/debug/asylum node interrupt <node-id>
./target/debug/asylum node stop <node-id>
./target/debug/asylum node archive <node-id>
./target/debug/asylum graph get
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
- `ASYLUM_ATTACH_SECRET` for internal signed session transport; omitted means a per-process random secret
- `ASYLUM_NTFY_SERVER`, `ASYLUM_NTFY_TOPIC`, `ASYLUM_NTFY_TOKEN`
- `ASYLUM_LOON_ENABLED`, `ASYLUM_LOON_ENDPOINT`, and optional config-file `loon.cli_path`, `loon.api_key_file`, `loon.cert_fingerprint_file`

When Loon is enabled, Asylum drives the documented `loon` CLI contract for launch, input, interrupt, stop, terminate, and session relay operations. It passes `LOON_ENDPOINT` plus configured auth/cert env vars to that process.

### Acceptance Walkthrough

1. Build and run `asylum daemon run` and confirm startup succeeds.
2. `curl http://127.0.0.1:7717/api/graph` returns a JSON object with `graph`.
3. Create a node:
   - `asylum node create --harness codex --substrate local --role worker`
   - Verify `/api/nodes/:id` returns created `node_id`.
4. Observe and interact:
   - `wss`/WS path `/api/nodes/:id/observe/ws` returns at least an initial message and closes cleanly.
   - `asylum node send <node-id> "status"` records input and reaches the node when the harness supports input.
5. Inspect root capabilities:
   - `asylum node inspect <node-id>` shows a node record.
   - `asylum mcp` starts JSON-RPC stdio server and advertises `node.create`, `node.spawn_peer`, `node.list`, `node.inspect`, `node.send_input`, `node.interrupt`, `node.stop`, `graph.get`, and relationship tools.
6. Generate notifications:
   - `asylum notify send --title "hello" --body "it works"` returns `notify sent: true` when sender config is enabled.
7. Exercise remote commands:
   - Issue a token, then `curl -X POST -H 'Content-Type: application/json' -d '{"command":"status token=<token>"}' http://127.0.0.1:7717/api/remote-commands`.
8. If Loon is configured:
   - `asylum daemon run --loon-enabled --loon-endpoint https://<host>:7777`
   - `asylum node create --harness claude_code --substrate loon --role worker`
