# Asylum Live v2 V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an installable single-user Asylum v1 that runs as an always-on control plane for real Codex and Claude Code nodes across local and Loon substrates, with Cockpit, CLI, MCP, browser/native attach, ntfy notifications, inbound remote commands, and shared root capabilities.

**Architecture:** A Rust service owns the node registry, SQLite event store, capability API, local PTY supervision, Loon substrate client, notification workers, and static Cockpit serving. Cockpit is a Vite/React application that calls the same capability API used by the CLI and MCP server. Harness intelligence stays inside real Codex and Claude Code processes; Asylum only launches, observes, controls, attaches, and records explicit relationships.

**Tech Stack:** Rust workspace (`tokio`, `axum`, `rusqlite`, `portable-pty`, `reqwest`, `clap`, `serde`, `time`, `uuid`, `tracing`), SQLite, React + TypeScript + Vite, `@xyflow/react` for the graph, `@xterm/xterm` for browser attach, ntfy HTTP API, JSON-RPC over stdio for MCP.

---

## Planning Decisions

- API transport: localhost-first HTTP JSON plus WebSocket streams. Every route invokes a typed root capability in `asylum-daemon`; no route-only powers.
- Schema format: Rust `serde` types in `asylum-core` are the source of truth. Generate `openapi.json` after the API is stable enough for client documentation.
- Browser attach: local nodes use a `portable-pty` session bridged to an authenticated WebSocket and rendered by xterm.js. Loon nodes use the Loon attach endpoint when available; otherwise Asylum opens a PTY relay to `loon attach <id>`.
- Native attach: Asylum returns a native target object containing a command and environment. On macOS the default target is `osascript` opening Terminal with `asylum attach <node-id>`; Linux returns `x-terminal-emulator`/`gnome-terminal`/`konsole` candidates; unsupported OSes receive the attach command as copyable text.
- Harness telemetry: v1 records lifecycle events, PTY output chunks, optional harness JSONL/session file references, and honest capability flags. It does not pretend Codex and Claude Code expose the same structured stream.
- Node output storage: SQLite stores event metadata and redacted output chunks; full external transcript references are stored as artifacts when the harness exposes a readable session file.
- Loon strategy: Asylum integrates with Loon as an independent substrate through Loon's HTTPS API and CLI-compatible operations. Claude Code is expected to be supported by a configured Loon host. Codex-on-Loon is exposed when the Loon host advertises or accepts a `codex` harness profile; otherwise the API returns `unsupported_on_substrate` with visible capability flags.
- ntfy authentication: outbound notifications include a short command token. Inbound commands must include the token or reply to a message whose token is still active. Tokens are HMAC-SHA256 digests stored in SQLite with expiry and node correlation.
- Packaging: ship one `asylum` Rust binary with `serve`, `node`, `graph`, `attach`, `notify`, `token`, and `mcp` subcommands. `asylum serve` serves built Cockpit assets from `cockpit/dist` in dev and from embedded assets in release.

## Repository Structure

Create this layout during implementation:

```text
Cargo.toml
crates/asylum-core/Cargo.toml
crates/asylum-core/src/api.rs
crates/asylum-core/src/capabilities.rs
crates/asylum-core/src/config.rs
crates/asylum-core/src/event.rs
crates/asylum-core/src/lib.rs
crates/asylum-core/src/node.rs
crates/asylum-core/src/relationship.rs
crates/asylum-core/src/security.rs
crates/asylum-daemon/Cargo.toml
crates/asylum-daemon/src/app.rs
crates/asylum-daemon/src/attach.rs
crates/asylum-daemon/src/auth.rs
crates/asylum-daemon/src/capability_service.rs
crates/asylum-daemon/src/harness/claude.rs
crates/asylum-daemon/src/harness/codex.rs
crates/asylum-daemon/src/harness/launch_context.rs
crates/asylum-daemon/src/harness/mod.rs
crates/asylum-daemon/src/lib.rs
crates/asylum-daemon/src/notifications/mod.rs
crates/asylum-daemon/src/notifications/ntfy.rs
crates/asylum-daemon/src/recipes.rs
crates/asylum-daemon/src/remote_commands.rs
crates/asylum-daemon/src/storage.rs
crates/asylum-daemon/src/substrate/local.rs
crates/asylum-daemon/src/substrate/loon.rs
crates/asylum-daemon/src/substrate/mod.rs
crates/asylum/Cargo.toml
crates/asylum/src/client.rs
crates/asylum/src/cli.rs
crates/asylum/src/main.rs
crates/asylum/src/mcp.rs
crates/asylum/src/native_attach.rs
cockpit/package.json
cockpit/index.html
cockpit/src/App.tsx
cockpit/src/api.ts
cockpit/src/main.tsx
cockpit/src/state.ts
cockpit/src/components/AttachTerminal.tsx
cockpit/src/components/CommandCenter.tsx
cockpit/src/components/CreateNodePanel.tsx
cockpit/src/components/GraphView.tsx
cockpit/src/components/NotificationCenter.tsx
cockpit/src/components/NodeInspector.tsx
cockpit/src/components/NodeTable.tsx
cockpit/src/styles.css
```

Keep node lifecycle simple and node-first. The allowed liveness values are `starting`, `running`, `waiting_for_input`, `exited`, `stopped`, `failed`, and `archived`. Role hints are strings stored separately from liveness and never drive a workflow state machine.

## Task 1: Workspace And Core Contracts

**Files:**
- Create: `Cargo.toml`
- Create: `crates/asylum-core/Cargo.toml`
- Create: `crates/asylum-core/src/lib.rs`
- Create: `crates/asylum-core/src/node.rs`
- Create: `crates/asylum-core/src/event.rs`
- Create: `crates/asylum-core/src/relationship.rs`
- Create: `crates/asylum-core/src/capabilities.rs`
- Create: `crates/asylum-core/src/api.rs`
- Create: `crates/asylum-core/src/config.rs`
- Create: `crates/asylum-core/src/security.rs`

- [ ] **Step 1: Write failing core serialization tests**

Create `crates/asylum-core/src/node.rs` with the test module first:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRecord {
    pub id: Uuid,
    pub harness: HarnessKind,
    pub substrate: SubstrateKind,
    pub role_hint: String,
    pub liveness: NodeLiveness,
    pub workspace: Option<PathBuf>,
    pub description: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub external_id: Option<String>,
    pub capabilities: CapabilitySnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    Codex,
    ClaudeCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstrateKind {
    Local,
    Loon,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeLiveness {
    Starting,
    Running,
    WaitingForInput,
    Exited,
    Stopped,
    Failed,
    Archived,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySnapshot {
    pub browser_attach: bool,
    pub native_attach: bool,
    pub send_input: bool,
    pub interrupt: bool,
    pub stop: bool,
    pub resume: bool,
    pub structured_events: bool,
    pub transcript_export: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_record_uses_snake_case_wire_values() {
        let node = NodeRecord {
            id: Uuid::nil(),
            harness: HarnessKind::ClaudeCode,
            substrate: SubstrateKind::Local,
            role_hint: "command-center".to_string(),
            liveness: NodeLiveness::WaitingForInput,
            workspace: Some(PathBuf::from("/tmp/asylum-demo")),
            description: "Main command center".to_string(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            external_id: None,
            capabilities: CapabilitySnapshot {
                browser_attach: true,
                native_attach: true,
                send_input: true,
                interrupt: true,
                stop: true,
                resume: false,
                structured_events: false,
                transcript_export: false,
            },
        };

        let value = serde_json::to_value(&node).unwrap();
        assert_eq!(value["harness"], "claude_code");
        assert_eq!(value["substrate"], "local");
        assert_eq!(value["liveness"], "waiting_for_input");
    }
}
```

- [ ] **Step 2: Run the failing core test**

Run: `cargo test -p asylum-core node_record_uses_snake_case_wire_values`

Expected: fail because the workspace and crate do not exist.

- [ ] **Step 3: Add the workspace and core crate**

Create `Cargo.toml` with only the core crate at first:

```toml
[workspace]
members = ["crates/asylum-core"]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
version = "0.1.0"

[workspace.dependencies]
anyhow = "1"
axum = { version = "0.8", features = ["ws", "macros"] }
clap = { version = "4", features = ["derive", "env"] }
portable-pty = "0.8"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
thiserror = "2"
time = { version = "0.3", features = ["formatting", "macros", "parsing", "serde"] }
tokio = { version = "1", features = ["macros", "process", "rt-multi-thread", "signal", "sync", "time"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "fs", "trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["serde", "v4"] }
```

Create `crates/asylum-core/Cargo.toml`:

```toml
[package]
name = "asylum-core"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
time.workspace = true
uuid.workspace = true
```

Create `crates/asylum-core/src/lib.rs`:

```rust
pub mod api;
pub mod capabilities;
pub mod config;
pub mod event;
pub mod node;
pub mod relationship;
pub mod security;
```

- [ ] **Step 4: Add the remaining core contract modules**

Create `crates/asylum-core/src/capabilities.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityName {
    NodeCreate,
    NodeList,
    NodeInspect,
    NodeObserve,
    NodeSendInput,
    NodeInterrupt,
    NodeStop,
    NodeTerminate,
    NodeArchive,
    NodeAttachBrowser,
    NodeAttachNativeTarget,
    NodeRestart,
    NodeResume,
    RelationshipCreate,
    RelationshipRemove,
    RelationshipList,
    GraphGet,
    HarnessList,
    HarnessInspect,
    HarnessConfigure,
    HarnessCapabilities,
    HarnessLaunchContext,
    SubstrateList,
    SubstrateInspect,
    SubstrateHealth,
    SubstrateLaunchNode,
    SubstrateStopNode,
    SubstrateDiagnostics,
    WorkspaceListRecent,
    WorkspaceInspect,
    ContextCurrentSystemMap,
    ContextLaunchPacket,
    ArtifactListRefs,
    ArtifactAddRef,
    NotifyChannelsList,
    NotifySend,
    RemoteCommandReceive,
    RemoteCommandReply,
    DecisionRequest,
    DecisionResolve,
    ClientConfig,
    TokenIssue,
    TokenRevoke,
    BaseUrlInspect,
    AttachUrlIssue,
}
```

Create `crates/asylum-core/src/event.rs`:

```rust
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeEvent {
    pub id: i64,
    pub node_id: Uuid,
    pub sequence: i64,
    pub kind: NodeEventKind,
    pub body: serde_json::Value,
    pub created_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeEventKind {
    NodeStarted,
    OutputChunk,
    InputSent,
    LivenessChanged,
    HarnessFailure,
    SubstrateFailure,
    HumanInputRequested,
    NotificationSent,
    RemoteCommandReceived,
    AttachIssued,
}
```

Create `crates/asylum-core/src/relationship.rs`:

```rust
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipRecord {
    pub id: Uuid,
    pub source_node_id: Uuid,
    pub target_node_id: Uuid,
    pub kind: RelationshipKind,
    pub label: Option<String>,
    pub created_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Supervises,
    SpawnedFor,
    UserCreated,
    PlatformResponsibility,
}
```

Create `crates/asylum-core/src/api.rs` with request/response structs for node create/list/inspect/send/attach and graph get. Create `crates/asylum-core/src/config.rs` with `AsylumConfig`, `HarnessConfig`, `LoonConfig`, and `NtfyConfig`. Create `crates/asylum-core/src/security.rs` with `IssuedToken`, `TokenScope`, and `AttachToken`.

- [ ] **Step 5: Run core tests and format**

Run: `cargo test -p asylum-core`

Expected: pass.

Run: `cargo fmt --check`

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/asylum-core
git commit -m "feat: define Asylum core contracts"
```

## Task 2: SQLite Store And Zero-Node Service

**Files:**
- Create: `crates/asylum-daemon/Cargo.toml`
- Create: `crates/asylum-daemon/src/lib.rs`
- Create: `crates/asylum-daemon/src/storage.rs`
- Create: `crates/asylum-daemon/src/app.rs`
- Create: `crates/asylum-daemon/src/auth.rs`
- Create: `crates/asylum-daemon/src/capability_service.rs`
- Create: `crates/asylum/Cargo.toml`
- Create: `crates/asylum/src/main.rs`

- [ ] **Step 1: Write failing storage migration test**

Create `crates/asylum-daemon/src/storage.rs` with this test first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_create_empty_zero_node_store() {
        let store = Store::open_in_memory().unwrap();
        let nodes = store.list_nodes().unwrap();
        let graph = store.graph().unwrap();

        assert!(nodes.is_empty());
        assert!(graph.nodes.is_empty());
        assert!(graph.relationships.is_empty());
    }
}
```

- [ ] **Step 2: Run the failing daemon test**

Run: `cargo test -p asylum-daemon migrations_create_empty_zero_node_store`

Expected: fail because `Store` and the daemon crate do not exist.

- [ ] **Step 3: Create daemon and binary crates**

Modify the root `Cargo.toml` workspace member list:

```toml
[workspace]
members = ["crates/asylum-core", "crates/asylum-daemon", "crates/asylum"]
resolver = "2"
```

Create `crates/asylum-daemon/Cargo.toml`:

```toml
[package]
name = "asylum-daemon"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
anyhow.workspace = true
asylum-core = { path = "../asylum-core" }
axum.workspace = true
rusqlite.workspace = true
serde.workspace = true
serde_json.workspace = true
sha2.workspace = true
thiserror.workspace = true
time.workspace = true
tokio.workspace = true
tower.workspace = true
tower-http.workspace = true
tracing.workspace = true
uuid.workspace = true
```

Create `crates/asylum/Cargo.toml`:

```toml
[package]
name = "asylum"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
anyhow.workspace = true
asylum-core = { path = "../asylum-core" }
asylum-daemon = { path = "../asylum-daemon" }
clap.workspace = true
tokio.workspace = true
tracing-subscriber.workspace = true
```

Create `crates/asylum-daemon/src/lib.rs`:

```rust
pub mod app;
pub mod auth;
pub mod capability_service;
pub mod storage;
```

- [ ] **Step 4: Implement migrations and zero-node reads**

Create `Store::open`, `Store::open_in_memory`, `Store::list_nodes`, and `Store::graph`. The schema must include `nodes`, `events`, `transcript_chunks`, `relationships`, `artifacts`, `tokens`, `remote_commands`, and `decisions`. Use `PRAGMA foreign_keys = ON`.

The `nodes` table must store `role_hint`, `liveness`, and `capabilities_json` separately so no state machine is implied by role.

- [ ] **Step 5: Run storage tests**

Run: `cargo test -p asylum-daemon migrations_create_empty_zero_node_store`

Expected: pass.

- [ ] **Step 6: Write failing zero-node HTTP test**

Add to `crates/asylum-daemon/src/app.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_and_graph_work_with_zero_nodes() {
        let store = crate::storage::Store::open_in_memory().unwrap();
        let app = build_router(store, AuthMode::Disabled);

        let health = app
            .clone()
            .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);

        let graph = app
            .oneshot(Request::builder().uri("/api/graph").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(graph.status(), StatusCode::OK);
    }
}
```

- [ ] **Step 7: Implement health and graph routes through capability service**

Create `CapabilityService` with `graph_get`, `node_list`, and `health` methods. `build_router` must route `/api/health`, `/api/nodes`, and `/api/graph` through `CapabilityService`.

- [ ] **Step 8: Add initial serve command**

Create `crates/asylum/src/main.rs`:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::SocketAddr;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value = "127.0.0.1:7717")]
        bind: SocketAddr,
        #[arg(long, default_value = ".asylum/asylum.sqlite3")]
        database: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let args = Args::parse();
    match args.command {
        Command::Serve { bind, database } => asylum_daemon::app::serve(bind, database).await,
    }
}
```

- [ ] **Step 9: Run daemon verification**

Run: `cargo test -p asylum-daemon`

Expected: pass.

Run: `cargo run -p asylum -- serve --database target/asylum-dev.sqlite3`

Expected: prints the bound address and stays running. Stop it with `Ctrl-C`.

- [ ] **Step 10: Commit**

```bash
git add crates/asylum-daemon crates/asylum Cargo.toml
git commit -m "feat: add zero-node service and store"
```

## Task 3: Auth, Tokens, And Typed Capability API

**Files:**
- Modify: `crates/asylum-core/src/api.rs`
- Modify: `crates/asylum-core/src/security.rs`
- Modify: `crates/asylum-daemon/src/auth.rs`
- Modify: `crates/asylum-daemon/src/capability_service.rs`
- Modify: `crates/asylum-daemon/src/app.rs`
- Modify: `crates/asylum/src/main.rs`

- [ ] **Step 1: Write failing token test**

Add to `crates/asylum-daemon/src/auth.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_verification_rejects_wrong_secret() {
        let issued = issue_owner_token("test-owner", &["node.list".to_string()]).unwrap();
        assert!(verify_token(&issued.raw_token, &issued.stored_hash));
        assert!(!verify_token("wrong-token", &issued.stored_hash));
    }
}
```

- [ ] **Step 2: Run failing auth test**

Run: `cargo test -p asylum-daemon token_hash_verification_rejects_wrong_secret`

Expected: fail because token issue and verify functions do not exist.

- [ ] **Step 3: Implement token issue and verify**

Use `uuid::Uuid::new_v4()` plus SHA-256. Store only the hex digest. Return the raw token once from `asylum token issue`.

- [ ] **Step 4: Add API routes for documented root capabilities**

Add routes for:

```text
GET    /api/capabilities
GET    /api/client-config
POST   /api/tokens
DELETE /api/tokens/:id
GET    /api/harnesses
GET    /api/substrates
GET    /api/workspaces/recent
GET    /api/context/system-map
```

Each route calls a method on `CapabilityService`. Routes that are not yet backed by node launch behavior return truthful empty lists or capability metadata, not fake nodes.

- [ ] **Step 5: Add auth middleware**

Require `Authorization: Bearer <token>` when `AuthMode::OwnerToken` is enabled. Keep `AuthMode::Disabled` available only for tests.

- [ ] **Step 6: Run auth and route tests**

Run: `cargo test -p asylum-daemon auth`

Expected: pass.

Run: `cargo test -p asylum-daemon app`

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add crates/asylum-core crates/asylum-daemon crates/asylum
git commit -m "feat: protect typed capability API"
```

## Task 4: Local PTY Harness Nodes

**Files:**
- Create: `crates/asylum-daemon/src/harness/mod.rs`
- Create: `crates/asylum-daemon/src/harness/codex.rs`
- Create: `crates/asylum-daemon/src/harness/claude.rs`
- Create: `crates/asylum-daemon/src/harness/launch_context.rs`
- Create: `crates/asylum-daemon/src/substrate/mod.rs`
- Create: `crates/asylum-daemon/src/substrate/local.rs`
- Modify: `crates/asylum-daemon/src/capability_service.rs`
- Modify: `crates/asylum-daemon/src/storage.rs`
- Modify: `crates/asylum-daemon/src/app.rs`

- [ ] **Step 1: Write failing harness command tests**

In `crates/asylum-daemon/src/harness/mod.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use asylum_core::node::HarnessKind;

    #[test]
    fn default_harness_commands_are_real_clis() {
        let codex = HarnessRegistry::default().get(&HarnessKind::Codex).unwrap();
        let claude = HarnessRegistry::default().get(&HarnessKind::ClaudeCode).unwrap();

        assert_eq!(codex.command(), "codex");
        assert_eq!(claude.command(), "claude");
        assert!(codex.capabilities().send_input);
        assert!(claude.capabilities().send_input);
    }
}
```

- [ ] **Step 2: Run failing harness test**

Run: `cargo test -p asylum-daemon default_harness_commands_are_real_clis`

Expected: fail because harness modules do not exist.

- [ ] **Step 3: Implement harness registry**

Create a `HarnessAdapter` trait with:

```rust
pub trait HarnessAdapter: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn command(&self) -> &str;
    fn launch_args(&self) -> &[String];
    fn capabilities(&self) -> CapabilitySnapshot;
    fn launch_context(&self, request: &CreateNodeRequest) -> String;
}
```

Codex and Claude Code adapters both launch real CLI commands and receive initial instructions through PTY input. Both adapters set `browser_attach`, `native_attach`, `send_input`, `interrupt`, and `stop` true for local nodes.

- [ ] **Step 4: Write failing create-node test with mock command**

Add a test that configures a harness command as `/bin/sh -lc 'printf ready; read line; printf "$line"'`, creates a local node, sends `ping`, and verifies an output chunk contains `ping`.

- [ ] **Step 5: Implement local substrate process supervision**

Use `portable-pty` to spawn a process in the requested workspace. Store node row immediately with `starting`, update to `running` after spawn, record stdout/stderr chunks as `NodeEventKind::OutputChunk`, and store process handles in an in-memory `NodeRuntimeRegistry`.

- [ ] **Step 6: Add node controls**

Implement:

```text
POST /api/nodes
GET  /api/nodes/:id
GET  /api/nodes/:id/events
POST /api/nodes/:id/input
POST /api/nodes/:id/interrupt
POST /api/nodes/:id/stop
POST /api/nodes/:id/archive
```

`interrupt` sends `Ctrl-C` through the PTY. `stop` closes the PTY writer, terminates the child, records `stopped`, and keeps transcript chunks.

- [ ] **Step 7: Run local node verification**

Run: `cargo test -p asylum-daemon local`

Expected: pass.

Run: `cargo test -p asylum-daemon harness`

Expected: pass.

- [ ] **Step 8: Commit**

```bash
git add crates/asylum-daemon/src/harness crates/asylum-daemon/src/substrate crates/asylum-daemon/src/app.rs crates/asylum-daemon/src/capability_service.rs crates/asylum-daemon/src/storage.rs
git commit -m "feat: launch local harness nodes"
```

## Task 5: Browser And Native Attach

**Files:**
- Create: `crates/asylum-daemon/src/attach.rs`
- Create: `crates/asylum/src/native_attach.rs`
- Modify: `crates/asylum-daemon/src/app.rs`
- Modify: `crates/asylum-daemon/src/capability_service.rs`
- Modify: `crates/asylum/src/main.rs`

- [ ] **Step 1: Write failing attach token test**

Create `crates/asylum-daemon/src/attach.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn attach_tokens_are_node_scoped_and_expire() {
        let issuer = AttachTokenIssuer::new_for_tests("secret");
        let node_id = Uuid::new_v4();
        let token = issuer.issue(node_id, 60).unwrap();

        assert_eq!(issuer.verify(&token.raw).unwrap().node_id, node_id);
        assert!(issuer.verify("not-the-token").is_err());
    }
}
```

- [ ] **Step 2: Run failing attach test**

Run: `cargo test -p asylum-daemon attach_tokens_are_node_scoped_and_expire`

Expected: fail because attach issuer does not exist.

- [ ] **Step 3: Implement browser attach capabilities**

Add:

```text
POST /api/nodes/:id/attach/browser
GET  /attach/:token
GET  /api/attach/:token/ws
```

The POST route returns an attach URL. The WebSocket route validates the token, connects to the node runtime PTY, sends output chunks to the browser, and writes browser input back to the PTY.

- [ ] **Step 4: Implement native attach target**

Add `POST /api/nodes/:id/attach/native-target`. Response shape:

```json
{
  "label": "Open in Terminal",
  "command": "asylum",
  "args": ["attach", "<node-id>"],
  "environment": {"ASYLUM_BASE_URL": "http://127.0.0.1:7717"}
}
```

Implement `asylum attach <node-id>` to request a browser token and open a terminal relay. On macOS, `asylum native-open <node-id>` may invoke `osascript` only after printing the exact command it will run.

- [ ] **Step 5: Run attach tests**

Run: `cargo test -p asylum-daemon attach`

Expected: pass.

Run: `cargo test -p asylum`

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/asylum-daemon/src/attach.rs crates/asylum-daemon/src/app.rs crates/asylum-daemon/src/capability_service.rs crates/asylum/src
git commit -m "feat: add browser and native attach"
```

## Task 6: Loon Substrate

**Files:**
- Create: `crates/asylum-daemon/src/substrate/loon.rs`
- Modify: `crates/asylum-core/src/config.rs`
- Modify: `crates/asylum-daemon/src/substrate/mod.rs`
- Modify: `crates/asylum-daemon/src/capability_service.rs`
- Modify: `crates/asylum-daemon/src/app.rs`

- [ ] **Step 1: Write failing Loon capability mapping test**

Create `crates/asylum-daemon/src/substrate/loon.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use asylum_core::node::HarnessKind;

    #[test]
    fn loon_reports_codex_unsupported_without_profile() {
        let health = LoonHealth {
            status: "ok".to_string(),
            running_instances: 0,
            harness_profiles: vec!["claude_code".to_string()],
        };

        let flags = capability_flags_from_health(&health, &HarnessKind::Codex);
        assert!(!flags.send_input);
        assert!(!flags.resume);
    }
}
```

- [ ] **Step 2: Run failing Loon test**

Run: `cargo test -p asylum-daemon loon_reports_codex_unsupported_without_profile`

Expected: fail because Loon types do not exist.

- [ ] **Step 3: Implement Loon config and health**

`LoonConfig` fields:

```rust
pub struct LoonConfig {
    pub endpoint: String,
    pub api_key_file: Option<PathBuf>,
    pub cert_fingerprint_file: Option<PathBuf>,
    pub cli_path: Option<PathBuf>,
    pub enabled: bool,
}
```

Implement `substrate.health` by calling `GET /version` or the configured Loon health endpoint. If the daemon lacks profile metadata, infer `claude_code` support and mark `codex` unsupported with a clear reason.

- [ ] **Step 4: Implement Loon-backed node creation**

For Claude Code, call Loon spawn with a launch packet prompt and store the Loon instance id as `external_id`. For Codex, require an advertised `codex` profile and call the profile-specific spawn endpoint or CLI argument. If the profile is absent, `node.create` returns HTTP 409 with code `unsupported_on_substrate`.

- [ ] **Step 5: Implement Loon controls**

Map:

```text
node.send_input -> loon tell
node.interrupt  -> loon interrupt
node.stop       -> loon stop
node.terminate  -> loon terminate
node.resume     -> loon resume when Loon reports resumable
node.observe    -> loon events
node.attach.browser -> Loon console WebSocket or Asylum relay
node.attach.native_target -> asylum attach <node-id>
```

- [ ] **Step 6: Run Loon tests with mocked HTTP server**

Run: `cargo test -p asylum-daemon loon`

Expected: pass without requiring a real Loon host.

- [ ] **Step 7: Commit**

```bash
git add crates/asylum-core/src/config.rs crates/asylum-daemon/src/substrate crates/asylum-daemon/src/capability_service.rs crates/asylum-daemon/src/app.rs
git commit -m "feat: integrate Loon substrate"
```

## Task 7: Relationships, Artifacts, Launch Packets, And Recipes

**Files:**
- Create: `crates/asylum-daemon/src/recipes.rs`
- Modify: `crates/asylum-daemon/src/harness/launch_context.rs`
- Modify: `crates/asylum-daemon/src/capability_service.rs`
- Modify: `crates/asylum-daemon/src/storage.rs`
- Modify: `crates/asylum-daemon/src/app.rs`

- [ ] **Step 1: Write failing graph semantics test**

Add to `crates/asylum-daemon/src/storage.rs`:

```rust
#[test]
fn created_by_is_provenance_not_graph_edge() {
    let store = Store::open_in_memory().unwrap();
    let parent = store.insert_test_node("parent").unwrap();
    let child = store.insert_test_node_with_created_by("child", parent.id).unwrap();

    let graph = store.graph().unwrap();
    assert_eq!(graph.nodes.len(), 2);
    assert!(graph.relationships.is_empty());
}
```

- [ ] **Step 2: Implement explicit relationships only**

Add routes:

```text
POST   /api/relationships
DELETE /api/relationships/:id
GET    /api/relationships
```

Only `relationship.create` creates graph edges. `created_by` stays in node provenance.

- [ ] **Step 3: Implement launch packet**

`context.launch_packet` returns a Markdown packet containing base URL, node id, role hint, harness/substrate, available capabilities, current graph summary, and starter recipes. Store the packet path as an artifact ref when written to disk.

- [ ] **Step 4: Add starter recipes**

Create recipes for:

```text
start-command-center
spawn-worker-nodes
observe-and-summarize-system
run-plan-to-completion
checkpoint-or-handoff-node
parallel-exploration
```

Each recipe is a prompt template that calls root capabilities by name and states that role hints are not workflow states.

- [ ] **Step 5: Run graph and recipe tests**

Run: `cargo test -p asylum-daemon graph`

Expected: pass.

Run: `cargo test -p asylum-daemon recipes`

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/asylum-daemon/src/recipes.rs crates/asylum-daemon/src/harness/launch_context.rs crates/asylum-daemon/src/capability_service.rs crates/asylum-daemon/src/storage.rs crates/asylum-daemon/src/app.rs
git commit -m "feat: add graph semantics and launch recipes"
```

## Task 8: ntfy Notifications And Inbound Remote Commands

**Files:**
- Create: `crates/asylum-daemon/src/notifications/mod.rs`
- Create: `crates/asylum-daemon/src/notifications/ntfy.rs`
- Create: `crates/asylum-daemon/src/remote_commands.rs`
- Modify: `crates/asylum-core/src/config.rs`
- Modify: `crates/asylum-daemon/src/capability_service.rs`
- Modify: `crates/asylum-daemon/src/app.rs`

- [ ] **Step 1: Write failing remote command parse test**

Create `crates/asylum-daemon/src/remote_commands.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_and_send_input_commands() {
        assert_eq!(
            parse_remote_command("status token=abc").unwrap().kind,
            RemoteCommandKind::Status
        );

        let parsed = parse_remote_command("send node=00000000-0000-0000-0000-000000000000 token=abc text=hello").unwrap();
        assert_eq!(parsed.kind, RemoteCommandKind::SendInput);
        assert_eq!(parsed.args["text"], "hello");
    }
}
```

- [ ] **Step 2: Implement command parser and token correlation**

Support inbound text forms:

```text
status token=<token>
attach node=<node-id> token=<token>
send node=<node-id> token=<token> text=<message>
start harness=<codex|claude_code> substrate=<local|loon> role=<role> token=<token>
interrupt node=<node-id> token=<token>
stop node=<node-id> token=<token>
approve decision=<decision-id> token=<token>
deny decision=<decision-id> token=<token>
```

Reject commands with missing, expired, or mismatched tokens. Store accepted commands in `remote_commands`.

- [ ] **Step 3: Implement ntfy outbound**

`notify.send` posts to configured ntfy topic with title, body, tags, priority, and optional click URL. Include a short command token when the notification invites a reply.

- [ ] **Step 4: Implement ntfy inbound polling**

Use ntfy JSON stream or periodic poll. Convert messages into `remote_command.receive`, call the same root capabilities as API routes, then call `remote_command.reply` to post success or failure.

- [ ] **Step 5: Add dashboard notification records**

Persist notifications in SQLite and expose:

```text
GET  /api/notifications
POST /api/notifications/:id/read
```

- [ ] **Step 6: Run notification tests**

Run: `cargo test -p asylum-daemon remote_commands`

Expected: pass.

Run: `cargo test -p asylum-daemon ntfy`

Expected: pass with mocked HTTP.

- [ ] **Step 7: Commit**

```bash
git add crates/asylum-daemon/src/notifications crates/asylum-daemon/src/remote_commands.rs crates/asylum-core/src/config.rs crates/asylum-daemon/src/capability_service.rs crates/asylum-daemon/src/app.rs
git commit -m "feat: add ntfy remote control"
```

## Task 9: CLI And MCP Surfaces

**Files:**
- Create: `crates/asylum/src/client.rs`
- Create: `crates/asylum/src/cli.rs`
- Create: `crates/asylum/src/mcp.rs`
- Modify: `crates/asylum/src/main.rs`

- [ ] **Step 1: Write failing CLI command rendering test**

Create `crates/asylum/src/cli.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_create_cli_maps_to_root_capability() {
        let request = build_create_request("codex", "local", "command-center", Some("/tmp/demo")).unwrap();
        assert_eq!(request.harness, "codex");
        assert_eq!(request.substrate, "local");
        assert_eq!(request.role_hint, "command-center");
        assert_eq!(request.workspace.unwrap(), "/tmp/demo");
    }
}
```

- [ ] **Step 2: Implement API client**

Create `AsylumClient` with methods mirroring root capabilities. The client reads `ASYLUM_BASE_URL` and `ASYLUM_TOKEN`, defaulting to `http://127.0.0.1:7717`.

- [ ] **Step 3: Implement CLI commands**

Add subcommands:

```text
asylum serve
asylum node create --harness codex --substrate local --role command-center --workspace .
asylum node list
asylum node inspect <node-id>
asylum node send <node-id> <text>
asylum node interrupt <node-id>
asylum node stop <node-id>
asylum node archive <node-id>
asylum graph get
asylum attach <node-id>
asylum token issue --name <name>
asylum notify send --title <title> --body <body>
asylum mcp
```

Every command calls `AsylumClient`, not daemon internals.

- [ ] **Step 4: Write failing MCP tools/list test**

Add to `crates/asylum/src/mcp.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_includes_node_create_and_graph_get() {
        let tools = list_tools();
        assert!(tools.iter().any(|tool| tool.name == "node.create"));
        assert!(tools.iter().any(|tool| tool.name == "graph.get"));
    }
}
```

- [ ] **Step 5: Implement MCP stdio server**

Handle JSON-RPC methods `initialize`, `tools/list`, and `tools/call`. Expose tools for all root capabilities that can be represented as JSON requests. Tools that cannot render terminals return attach URLs or native target commands.

- [ ] **Step 6: Run CLI and MCP tests**

Run: `cargo test -p asylum`

Expected: pass.

Run: `cargo test --workspace`

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add crates/asylum/src
git commit -m "feat: expose CLI and MCP clients"
```

## Task 10: Cockpit Foundation, Graph, Table, And Inspector

**Files:**
- Create: `cockpit/package.json`
- Create: `cockpit/index.html`
- Create: `cockpit/src/main.tsx`
- Create: `cockpit/src/App.tsx`
- Create: `cockpit/src/api.ts`
- Create: `cockpit/src/state.ts`
- Create: `cockpit/src/components/GraphView.tsx`
- Create: `cockpit/src/components/NodeTable.tsx`
- Create: `cockpit/src/components/NodeInspector.tsx`
- Create: `cockpit/src/components/CreateNodePanel.tsx`
- Create: `cockpit/src/components/NotificationCenter.tsx`
- Create: `cockpit/src/styles.css`
- Modify: `crates/asylum-daemon/src/app.rs`

- [ ] **Step 1: Create frontend package**

Create `cockpit/package.json`:

```json
{
  "name": "asylum-cockpit",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite --host 127.0.0.1",
    "build": "tsc && vite build",
    "test": "vitest run"
  },
  "dependencies": {
    "@vitejs/plugin-react": "^5.0.0",
    "@xyflow/react": "^12.0.0",
    "@xterm/addon-fit": "^0.10.0",
    "@xterm/xterm": "^5.5.0",
    "lucide-react": "^0.468.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "zustand": "^5.0.0"
  },
  "devDependencies": {
    "typescript": "^5.6.0",
    "vite": "^7.0.0",
    "vitest": "^2.1.0"
  }
}
```

- [ ] **Step 2: Write failing API normalization test**

Create `cockpit/src/api.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { graphToFlow } from "./api";

describe("graphToFlow", () => {
  it("keeps only explicit relationships as edges", () => {
    const graph = {
      nodes: [{ id: "a", role_hint: "command-center" }, { id: "b", role_hint: "worker" }],
      relationships: [{ id: "r1", source_node_id: "a", target_node_id: "b", kind: "supervises" }],
    };

    const flow = graphToFlow(graph);
    expect(flow.nodes).toHaveLength(2);
    expect(flow.edges).toEqual([{ id: "r1", source: "a", target: "b", label: "supervises" }]);
  });
});
```

- [ ] **Step 3: Implement Cockpit shell**

Build a graph-first layout with these persistent regions:

```text
left toolbar: create node controls
center: graph view with pan/zoom
right: selected node inspector
bottom: command-center chat and secondary table tabs
top-right: notification center and exposure warning
```

Do not make a marketing landing page. The first viewport is the operational Cockpit.

- [ ] **Step 4: Implement graph and table**

Use `@xyflow/react` for the graph and plain accessible tables for the secondary view. Node cards show harness, substrate, role hint, liveness, and output preview. Edges come only from `graph.relationships`.

- [ ] **Step 5: Implement inspector controls**

Inspector buttons call root API routes for send input, interrupt, stop, browser attach, native target, relationship create/remove, archive, and resume when capability flags allow it.

- [ ] **Step 6: Serve Cockpit from daemon**

`asylum serve` serves `/` and static assets from `cockpit/dist` when the directory exists. If missing, `/` returns a concise message to run `npm --prefix cockpit run build`.

- [ ] **Step 7: Run frontend verification**

Run: `npm --prefix cockpit install`

Expected: dependencies install.

Run: `npm --prefix cockpit test`

Expected: pass.

Run: `npm --prefix cockpit run build`

Expected: pass and produce `cockpit/dist`.

Run: `cargo test -p asylum-daemon app`

Expected: pass.

- [ ] **Step 8: Commit**

```bash
git add cockpit crates/asylum-daemon/src/app.rs
git commit -m "feat: add graph-first Cockpit"
```

## Task 11: Command Center Chat And Live Observation

**Files:**
- Create: `cockpit/src/components/CommandCenter.tsx`
- Create: `cockpit/src/components/AttachTerminal.tsx`
- Modify: `cockpit/src/App.tsx`
- Modify: `cockpit/src/api.ts`
- Modify: `cockpit/src/state.ts`
- Modify: `crates/asylum-daemon/src/app.rs`
- Modify: `crates/asylum-daemon/src/capability_service.rs`

- [ ] **Step 1: Write failing command-center state test**

Create `cockpit/src/state.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { selectCommandCenter } from "./state";

describe("selectCommandCenter", () => {
  it("selects a running command-center node before workers", () => {
    const selected = selectCommandCenter([
      { id: "worker", role_hint: "worker", liveness: "running" },
      { id: "cc", role_hint: "command-center", liveness: "running" },
    ]);

    expect(selected?.id).toBe("cc");
  });
});
```

- [ ] **Step 2: Implement New Command Center flow**

Create a control that chooses harness and substrate, calls `node.create` with `role_hint=command-center`, opens inline chat for that node, and keeps the node visible in graph and table.

- [ ] **Step 3: Implement live observe WebSocket**

Add `GET /api/nodes/:id/observe/ws`. It streams stored output chunks followed by live chunks. Cockpit subscribes for the selected node and command-center node.

- [ ] **Step 4: Implement inline chat send**

Chat input calls `node.send_input`. It displays user-sent inputs and PTY output in chronological order from node events. The UI never labels this as an Asylum chatbot; it labels it by the harness and node role.

- [ ] **Step 5: Implement browser terminal component**

`AttachTerminal` connects to `GET /api/attach/:token/ws`, mounts xterm.js, fits to its container, and sends key input to the node runtime.

- [ ] **Step 6: Run command-center verification**

Run: `npm --prefix cockpit test`

Expected: pass.

Run: `npm --prefix cockpit run build`

Expected: pass.

Run: `cargo test -p asylum-daemon observe`

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add cockpit/src crates/asylum-daemon/src/app.rs crates/asylum-daemon/src/capability_service.rs
git commit -m "feat: add command-center chat and live observe"
```

## Task 12: Packaging, Install Path, And End-To-End Acceptance

**Files:**
- Modify: `README.md`
- Modify: `crates/asylum/src/main.rs`
- Modify: `crates/asylum/src/cli.rs`
- Modify: `crates/asylum-daemon/src/app.rs`
- Modify: `cockpit/package.json`

- [ ] **Step 1: Add install and config commands**

Add:

```text
asylum config init
asylum config show
asylum install launchd
asylum install systemd
```

`config init` writes `~/.config/asylum/config.toml` with localhost binding, database path, harness commands, optional Loon config, and optional ntfy config. `install launchd` prints a launchd plist to stdout. `install systemd` prints a user service unit to stdout.

- [ ] **Step 2: Embed Cockpit assets for release**

In debug/dev, serve `cockpit/dist` from disk. In release builds, embed the built assets with `include_dir` or `rust-embed`. Add the chosen crate to workspace dependencies and document why it exists in `Cargo.toml`.

- [ ] **Step 3: Update README with operator path**

Document:

```text
cargo build --release
npm --prefix cockpit install
npm --prefix cockpit run build
cargo run -p asylum -- config init
cargo run -p asylum -- serve
cargo run -p asylum -- token issue --name local-cli
ASYLUM_TOKEN=<token> cargo run -p asylum -- node list
cargo run -p asylum -- mcp
```

Include real acceptance walkthrough steps for the PRD wow sequence.

- [ ] **Step 4: Run full automated verification**

Run: `cargo fmt --check`

Expected: pass.

Run: `cargo clippy --workspace -- -D warnings`

Expected: pass.

Run: `cargo test --workspace`

Expected: pass.

Run: `npm --prefix cockpit test`

Expected: pass.

Run: `npm --prefix cockpit run build`

Expected: pass.

- [ ] **Step 5: Run local manual acceptance**

Start service:

```bash
cargo run -p asylum -- serve --database target/asylum-acceptance.sqlite3
```

In a second terminal:

```bash
cargo run -p asylum -- token issue --name acceptance
export ASYLUM_TOKEN=<issued-token>
cargo run -p asylum -- node create --harness codex --substrate local --role command-center --workspace .
cargo run -p asylum -- node list
cargo run -p asylum -- graph get
```

Open Cockpit at `http://127.0.0.1:7717`, verify graph-first UI, command-center chat, node inspector, browser attach, native target, table view, notification center, and capability flags.

- [ ] **Step 6: Run Loon manual acceptance when a Loon host is configured**

Set `LOON_ENDPOINT`, `LOON_API_KEY_FILE`, and `LOON_CERT_FINGERPRINT_FILE`, then run:

```bash
cargo run -p asylum -- node create --harness claude_code --substrate loon --role worker --workspace .
cargo run -p asylum -- node list
cargo run -p asylum -- attach <loon-backed-node-id>
```

Verify the node appears in graph/table, output/events are observable, send input works, stop works, and unsupported Codex-on-Loon hosts show a visible `unsupported_on_substrate` reason instead of pretending support exists.

- [ ] **Step 7: Run ntfy manual acceptance when ntfy is configured**

Configure a private topic and run:

```bash
cargo run -p asylum -- notify send --title "Asylum acceptance" --body "Reply: status token=<token>"
```

Reply through ntfy with `status token=<token>`. Verify Asylum records the inbound command, replies with current node status, and shows the notification in Cockpit.

- [ ] **Step 8: Commit**

```bash
git add README.md Cargo.toml crates cockpit
git commit -m "feat: package Asylum v1"
```

## Final V1 Verification Checklist

- [ ] Persistent Asylum service starts and remains useful with zero nodes.
- [ ] Graph-first Cockpit is the first screen.
- [ ] Secondary table view exists.
- [ ] Inline command-center chat launches a real Codex or Claude Code node.
- [ ] Codex adapter launches, observes, receives input, interrupts, stops, and attaches locally.
- [ ] Claude Code adapter launches, observes, receives input, interrupts, stops, and attaches locally.
- [ ] Local substrate supports launch, liveness, output observation, input, browser attach, native attach, interrupt, and stop.
- [ ] Loon substrate detects configured hosts, reports health, creates supported nodes, observes output, sends input, attaches, interrupts, stops, and explains unsupported harness profiles.
- [ ] Browser attach works through authenticated WebSocket.
- [ ] Native attach target is returned and works on the current OS where possible.
- [ ] CLI calls root capabilities.
- [ ] MCP server calls root capabilities.
- [ ] Typed API contract exists in `asylum-core` and is exercised by all clients.
- [ ] ntfy outbound notifications work.
- [ ] ntfy inbound remote command/reply works.
- [ ] Dashboard notification center exists.
- [ ] Basic remote connection setup exists with owner token and exposure warning.
- [ ] Capability flags are visible per harness and substrate.
- [ ] Starter recipes are available in launch packets and API responses.

## Self-Review Notes

- Spec coverage: every PRD completion-bar item maps to a task above. The only intentionally conditional behavior is Codex-on-Loon, which is exposed only when a configured Loon host advertises a Codex profile; otherwise the product returns a visible unsupported reason while preserving honest capability flags.
- Type consistency: `HarnessKind`, `SubstrateKind`, `NodeLiveness`, `CapabilitySnapshot`, `NodeEventKind`, and `RelationshipKind` originate in `asylum-core` and are reused by daemon, CLI, MCP, and Cockpit API types.
- Scope discipline: the plan keeps Asylum node-first and capability-first. It does not introduce runs, task contracts, inferred graph edges, RBAC, SaaS relay, or a custom chatbot brain.
