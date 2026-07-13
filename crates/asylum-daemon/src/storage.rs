use anyhow::{Context, Result};
use asylum_types::event::{NodeEvent, NodeEventKind};
use asylum_types::node::{
    CapabilitySnapshot, GraphRecord, HarnessKind, NodeLiveness, NodeRecord, SubstrateKind,
};
use asylum_types::relationship::RelationshipKind;
use asylum_types::relationship::RelationshipRecord;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use std::path::Path;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
    path: String,
}

type ActiveTokenRecord = (Uuid, String, String, i64, bool);
type NotificationRecord = (
    i64,
    Option<String>,
    String,
    String,
    String,
    i64,
    Option<i64>,
);
pub type DecisionStorageRecord = (String, Option<String>, String, String, i64, Option<i64>);

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_str = path.as_ref().display().to_string();
        let conn = Connection::open(path.as_ref())
            .with_context(|| format!("failed to open store at {:?}", path.as_ref()))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: path_str,
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: ":memory:".to_string(),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| anyhow::anyhow!("database connection lock poisoned"))
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                harness TEXT NOT NULL,
                substrate TEXT NOT NULL,
                role_hint TEXT NOT NULL,
                liveness TEXT NOT NULL,
                workspace TEXT,
                description TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                external_id TEXT,
                capabilities_json TEXT NOT NULL,
                created_by TEXT
            );

            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                node_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                kind TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS transcript_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                node_id TEXT NOT NULL,
                event_id INTEGER NOT NULL,
                chunk TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE,
                FOREIGN KEY (event_id) REFERENCES events(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS relationships (
                id TEXT PRIMARY KEY,
                source_node_id TEXT NOT NULL,
                target_node_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                label TEXT,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (source_node_id) REFERENCES nodes(id) ON DELETE CASCADE,
                FOREIGN KEY (target_node_id) REFERENCES nodes(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                node_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                metadata_json TEXT,
                FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS tokens (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                hash TEXT NOT NULL,
                scope TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                revoked INTEGER NOT NULL DEFAULT 0,
                raw_preview TEXT
            );

            CREATE TABLE IF NOT EXISTS remote_commands (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                args_json TEXT NOT NULL,
                token TEXT NOT NULL,
                node_id TEXT,
                status TEXT NOT NULL,
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS notifications (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                node_id TEXT,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                read_at INTEGER,
                external_ref TEXT
            );

            CREATE TABLE IF NOT EXISTS decisions (
                id TEXT PRIMARY KEY,
                node_id TEXT,
                text TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                decided_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS channels (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                label TEXT NOT NULL,
                direction TEXT NOT NULL,
                status TEXT NOT NULL,
                detail TEXT NOT NULL,
                config_json TEXT NOT NULL,
                live INTEGER NOT NULL,
                builtin INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS channel_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                channel_id TEXT NOT NULL,
                direction TEXT NOT NULL,
                ts INTEGER NOT NULL,
                sender TEXT NOT NULL,
                subject TEXT NOT NULL,
                body TEXT NOT NULL,
                replies_json TEXT,
                node_id TEXT,
                correlation_token TEXT,
                FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS channel_reply_correlations (
                token TEXT PRIMARY KEY,
                channel_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_channel_messages_channel_ts ON channel_messages(channel_id, ts);
            CREATE INDEX IF NOT EXISTS idx_channel_reply_correlations_channel_expires
                ON channel_reply_correlations(channel_id, expires_at);

            CREATE TABLE IF NOT EXISTS hooks (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                event TEXT NOT NULL,
                filter TEXT NOT NULL,
                actions_json TEXT NOT NULL,
                future INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS hook_firings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hook_id TEXT NOT NULL,
                ts INTEGER NOT NULL,
                trigger TEXT NOT NULL,
                outcome TEXT NOT NULL,
                ok INTEGER NOT NULL,
                payload_json TEXT NOT NULL,
                FOREIGN KEY (hook_id) REFERENCES hooks(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_events_node_seq ON events(node_id, sequence);
            CREATE UNIQUE INDEX IF NOT EXISTS events_node_seq_unique ON events(node_id, sequence);
            -- At most one pending decision per node (M6): the awaiting-input
            -- producer relies on this to dedup, so make it a DB invariant rather
            -- than a check-then-insert race. NULL node_id decisions are exempt
            -- (SQLite treats NULLs as distinct), matching operator-raised
            -- decisions that are not node-scoped.
            CREATE UNIQUE INDEX IF NOT EXISTS decisions_one_pending_per_node
                ON decisions(node_id) WHERE status = 'pending' AND node_id IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_artifacts_node ON artifacts(node_id);
            CREATE INDEX IF NOT EXISTS idx_relationships_source ON relationships(source_node_id);
            CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_node_id);
            CREATE INDEX IF NOT EXISTS idx_hook_firings_hook_ts ON hook_firings(hook_id, ts);
            ",
        )?;
        ensure_column(&conn, "channel_messages", "node_id", "TEXT")?;
        ensure_column(&conn, "channel_messages", "correlation_token", "TEXT")?;
        ensure_column(&conn, "nodes", "harness_session_id", "TEXT")?;
        // m2: persist the per-node create-time launch_args (JSON array of
        // strings) so resume can reuse the exact extra flags the node was
        // created with, instead of silently reverting to the adapter baseline.
        ensure_column(&conn, "nodes", "launch_args", "TEXT")?;
        // WS2: record the launch-profile model/effort the node was actually
        // launched with (NULL = harness default). Passed through verbatim; Asylum
        // keeps no catalog. Survives restart/resume so historical nodes report the
        // profile they ran under (HARN-007).
        ensure_column(&conn, "nodes", "model", "TEXT")?;
        ensure_column(&conn, "nodes", "effort", "TEXT")?;
        // M7: track the ctx_pressure fired-state on the node row so ingest_statusline
        // never has to load-and-scan every prior harness-event body per post.
        ensure_column(&conn, "nodes", "ctx_pressure_session", "TEXT")?;
        ensure_column(&conn, "nodes", "ctx_pressure_max", "REAL")?;
        Ok(())
    }

    pub fn list_nodes(&self) -> Result<Vec<NodeRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT id,harness,substrate,role_hint,liveness,workspace,description,created_at,updated_at,external_id,capabilities_json,harness_session_id,model,effort
            FROM nodes
            ORDER BY created_at DESC
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut node = row_to_node_record(row)?;
            hydrate_node_telemetry(&conn, &mut node)?;
            out.push(node);
        }
        Ok(out)
    }

    pub fn list_nodes_by_liveness(&self, liveness: NodeLiveness) -> Result<Vec<NodeRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT id,harness,substrate,role_hint,liveness,workspace,description,created_at,updated_at,external_id,capabilities_json,harness_session_id,model,effort
            FROM nodes
            WHERE liveness = ?1
            ORDER BY created_at DESC
            ",
        )?;
        let mut rows = stmt.query([liveness.to_string()])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut node = row_to_node_record(row)?;
            hydrate_node_telemetry(&conn, &mut node)?;
            out.push(node);
        }
        Ok(out)
    }

    pub fn graph(&self) -> Result<GraphRecord> {
        let nodes = self.list_nodes()?;
        let relationships = self.list_relationships()?;
        Ok(GraphRecord {
            nodes,
            relationships,
        })
    }

    pub fn list_relationships(&self) -> Result<Vec<RelationshipRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT id,source_node_id,target_node_id,kind,label,created_at
            FROM relationships
            ORDER BY created_at ASC
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let source_node_id: String = row.get(1)?;
            let target_node_id: String = row.get(2)?;
            let kind: String = row.get(3)?;
            let label: Option<String> = row.get(4)?;
            let created_at: i64 = row.get(5)?;
            let rel_kind = parse_relationship_kind(&kind)
                .ok_or_else(|| anyhow::anyhow!("unknown relationship kind in DB: {kind}"))?;
            out.push(RelationshipRecord {
                id: Uuid::parse_str(&id).context("invalid uuid for relationship id")?,
                source_node_id: Uuid::parse_str(&source_node_id)
                    .context("invalid uuid for source node id")?,
                target_node_id: Uuid::parse_str(&target_node_id)
                    .context("invalid uuid for target node id")?,
                kind: rel_kind,
                label,
                created_at: epoch_to_offset_dt(created_at),
            });
        }
        Ok(out)
    }

    pub fn get_node(&self, id: Uuid) -> Result<Option<NodeRecord>> {
        let conn = self.conn()?;
        Self::get_node_with_conn(&conn, id)
    }

    fn get_node_with_conn(conn: &Connection, id: Uuid) -> Result<Option<NodeRecord>> {
        let id_string = id.to_string();
        let mut stmt = conn.prepare(
            "
            SELECT id,harness,substrate,role_hint,liveness,workspace,description,created_at,updated_at,external_id,capabilities_json,harness_session_id,model,effort
            FROM nodes
            WHERE id = ?1
            ",
        )?;
        let mut rows = stmt.query([id_string])?;
        match rows.next()? {
            Some(row) => {
                let mut node = row_to_node_record(row)?;
                hydrate_node_telemetry(conn, &mut node)?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_node(
        &self,
        harness: HarnessKind,
        substrate: SubstrateKind,
        role_hint: &str,
        workspace: Option<&str>,
        description: Option<&str>,
        external_id: Option<&str>,
        capabilities: CapabilitySnapshot,
        created_by: Option<Uuid>,
    ) -> Result<NodeRecord> {
        let id = Uuid::new_v4();
        let now = OffsetDateTime::now_utc();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO nodes(
                id,harness,substrate,role_hint,liveness,workspace,description,created_at,updated_at,external_id,capabilities_json,created_by
            )
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                id.to_string(),
                harness.to_string(),
                substrate.to_string(),
                role_hint,
                NodeLiveness::Starting.to_string(),
                workspace,
                description.unwrap_or(""),
                now.unix_timestamp(),
                now.unix_timestamp(),
                external_id,
                serde_json::to_string(&capabilities)?,
                created_by.map(|id| id.to_string())
            ],
        )?;
        Self::record_event_with_conn(
            &conn,
            id,
            NodeEventKind::NodeStarted,
            JsonValue::Object(serde_json::Map::new()),
        )?;
        Self::get_node_with_conn(&conn, id)?
            .ok_or_else(|| anyhow::anyhow!("node inserted but not found"))
    }

    pub fn set_node_liveness(&self, id: Uuid, liveness: NodeLiveness) -> Result<()> {
        let now = OffsetDateTime::now_utc();
        let conn = self.conn()?;
        conn.execute(
            "UPDATE nodes SET liveness = ?1, updated_at = ?2 WHERE id = ?3",
            params![liveness.to_string(), now.unix_timestamp(), id.to_string()],
        )?;
        let body = serde_json::json!({ "liveness": liveness.to_string() });
        Self::record_event_with_conn(&conn, id, NodeEventKind::LivenessChanged, body)?;
        Ok(())
    }

    /// Set liveness AND record a single `LivenessChanged` event carrying an
    /// explicit reason (and optional extra fields). Used by startup
    /// reconciliation so the honest transition is auditable ("why is this node
    /// Stopped?") without inventing a new event-catalog kind.
    pub fn set_node_liveness_with_reason(
        &self,
        id: Uuid,
        liveness: NodeLiveness,
        reason: &str,
        extra: JsonValue,
    ) -> Result<()> {
        let now = OffsetDateTime::now_utc();
        let conn = self.conn()?;
        conn.execute(
            "UPDATE nodes SET liveness = ?1, updated_at = ?2 WHERE id = ?3",
            params![liveness.to_string(), now.unix_timestamp(), id.to_string()],
        )?;
        let mut body = serde_json::Map::new();
        body.insert("liveness".to_string(), JsonValue::String(liveness.to_string()));
        body.insert("reason".to_string(), JsonValue::String(reason.to_string()));
        if let JsonValue::Object(extra_map) = extra {
            for (k, v) in extra_map {
                body.insert(k, v);
            }
        }
        Self::record_event_with_conn(
            &conn,
            id,
            NodeEventKind::LivenessChanged,
            JsonValue::Object(body),
        )?;
        Ok(())
    }

    /// Compare-and-set liveness (M1/M2): move the node to `target` ONLY if its
    /// current liveness is one of `allowed_from`. Returns true iff the row was
    /// transitioned. A single atomic `UPDATE ... WHERE liveness IN (...)` makes
    /// the transition race-safe: a concurrent terminal write (exit sink) and a
    /// stale-snapshot active write (post_harness_event / resume) can no longer
    /// clobber each other -- whichever CAS runs against a still-matching state
    /// wins, the other becomes a no-op. Records `LivenessChanged` (with the
    /// optional reason/extra) only on a real transition.
    pub fn transition_node_liveness(
        &self,
        id: Uuid,
        target: NodeLiveness,
        allowed_from: &[NodeLiveness],
        reason: Option<&str>,
        extra: JsonValue,
    ) -> Result<bool> {
        if allowed_from.is_empty() {
            return Ok(false);
        }
        let now = OffsetDateTime::now_utc();
        let conn = self.conn()?;
        let placeholders = std::iter::repeat("?")
            .take(allowed_from.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE nodes SET liveness = ?1, updated_at = ?2              WHERE id = ?3 AND liveness IN ({placeholders})"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(target.to_string()),
            Box::new(now.unix_timestamp()),
            Box::new(id.to_string()),
        ];
        for from in allowed_from {
            params.push(Box::new(from.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let affected = conn.execute(&sql, param_refs.as_slice())?;
        if affected == 0 {
            return Ok(false);
        }
        let mut body = serde_json::Map::new();
        body.insert("liveness".to_string(), JsonValue::String(target.to_string()));
        if let Some(reason) = reason {
            body.insert("reason".to_string(), JsonValue::String(reason.to_string()));
        }
        if let JsonValue::Object(extra_map) = extra {
            for (k, v) in extra_map {
                body.insert(k, v);
            }
        }
        Self::record_event_with_conn(
            &conn,
            id,
            NodeEventKind::LivenessChanged,
            JsonValue::Object(body),
        )?;
        Ok(true)
    }

    /// Read the persisted ctx_pressure fired-state for a node (M7): the session
    /// the state belongs to and the highest threshold already fired in it. Absent
    /// columns / rows yield (None, None).
    pub fn ctx_pressure_state(&self, id: Uuid) -> Result<(Option<String>, Option<f64>)> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT ctx_pressure_session, ctx_pressure_max FROM nodes WHERE id = ?1",
                params![id.to_string()],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<f64>>(1)?)),
            )
            .optional()?;
        Ok(row.unwrap_or((None, None)))
    }

    /// Persist the ctx_pressure fired-state for a node (M7).
    pub fn set_ctx_pressure_state(
        &self,
        id: Uuid,
        session: Option<&str>,
        max_threshold: f64,
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE nodes SET ctx_pressure_session = ?1, ctx_pressure_max = ?2 WHERE id = ?3",
            params![session, max_threshold, id.to_string()],
        )?;
        Ok(())
    }

    /// Revoke every active token whose name matches `name` (M3). Per-node guest
    /// tokens are named `loon-node-{node_id}`, so this is called on every node
    /// stop/archive/teardown/reconcile path to kill the guest credential the
    /// moment its VM is gone. Returns the number of tokens revoked.
    pub fn revoke_tokens_by_name(&self, name: &str) -> Result<usize> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE tokens SET revoked = 1 WHERE name = ?1 AND revoked = 0",
            params![name],
        )?;
        Ok(affected)
    }

    pub fn set_node_external_id(&self, id: Uuid, external_id: Option<String>) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE nodes SET external_id = ?1 WHERE id = ?2",
            params![external_id, id.to_string()],
        )?;
        Ok(())
    }

    /// Record the harness-native session id (claude `session_id`, codex
    /// `thread-id`) on the node row. Used by the harness-event bridge so the
    /// session can later be resumed.
    pub fn set_node_harness_session_id(
        &self,
        id: Uuid,
        harness_session_id: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE nodes SET harness_session_id = ?1 WHERE id = ?2",
            params![harness_session_id, id.to_string()],
        )?;
        Ok(())
    }

    /// m2: persist the per-node create-time `launch_args` (the extra flags the
    /// operator passed at create, e.g. a model override) as a JSON array so
    /// resume can reproduce the same argv. Empty is stored as NULL.
    pub fn set_node_launch_args(&self, id: Uuid, launch_args: &[String]) -> Result<()> {
        let conn = self.conn()?;
        let json = if launch_args.is_empty() {
            None
        } else {
            Some(serde_json::to_string(launch_args)?)
        };
        conn.execute(
            "UPDATE nodes SET launch_args = ?1 WHERE id = ?2",
            params![json, id.to_string()],
        )?;
        Ok(())
    }

    /// WS2: record the launch-profile the node was actually launched with. Each
    /// value passes through verbatim (`None` = harness default, stored as NULL).
    /// Called at launch time from what the adapter actually applied, so inspect /
    /// Cockpit / CLI / MCP report the real profile and resume can re-apply it.
    pub fn set_node_launch_profile(
        &self,
        id: Uuid,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE nodes SET model = ?1, effort = ?2 WHERE id = ?3",
            params![model, effort, id.to_string()],
        )?;
        Ok(())
    }

    /// m2: read back the persisted per-node create-time `launch_args`. Absent or
    /// unparseable rows yield an empty vec (the baseline argv).
    pub fn get_node_launch_args(&self, id: Uuid) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let raw: Option<String> = conn.query_row(
            "SELECT launch_args FROM nodes WHERE id = ?1",
            params![id.to_string()],
            |row| row.get::<_, Option<String>>(0),
        )?;
        Ok(raw
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default())
    }

    /// Epoch seconds of the most recent PTY output chunk for a node, or None if
    /// the node has emitted no output yet. Used by the quiescence idle timer.
    pub fn last_output_chunk_epoch(&self, node_id: Uuid) -> Result<Option<i64>> {
        let conn = self.conn()?;
        let kind = serde_json::to_string(&NodeEventKind::OutputChunk)?;
        let value: Option<i64> = conn.query_row(
            "SELECT MAX(created_at) FROM events WHERE node_id = ?1 AND kind = ?2",
            params![node_id.to_string(), kind],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        Ok(value)
    }

    /// Bodies of all ingested harness-event records for a node, oldest first.
    /// Used to dedup `node.ctx_pressure` threshold crossings per session.
    pub fn harness_event_bodies(&self, node_id: Uuid) -> Result<Vec<JsonValue>> {
        let conn = self.conn()?;
        let kind = serde_json::to_string(&NodeEventKind::HarnessEvent)?;
        let mut stmt = conn.prepare(
            "SELECT body FROM events WHERE node_id = ?1 AND kind = ?2 ORDER BY created_at ASC",
        )?;
        let mut rows = stmt.query(params![node_id.to_string(), kind])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let body_text: String = row.get(0)?;
            if let Ok(value) = serde_json::from_str::<JsonValue>(&body_text) {
                out.push(value);
            }
        }
        Ok(out)
    }

    pub fn update_node_description(&self, id: Uuid, description: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE nodes SET description = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                description,
                OffsetDateTime::now_utc().unix_timestamp(),
                id.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn record_event(&self, node_id: Uuid, kind: NodeEventKind, body: JsonValue) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = Self::record_event_with_conn(&conn, node_id, kind, body);
        if result.is_ok() {
            conn.execute_batch("COMMIT")?;
        } else {
            let _ = conn.execute_batch("ROLLBACK");
        }
        result
    }

    pub fn append_transcript_chunk(&self, node_id: Uuid, text: &str) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<i64> {
            let event_id = Self::record_event_with_conn(
                &conn,
                node_id,
                NodeEventKind::OutputChunk,
                serde_json::json!({ "text": text }),
            )?;
            let now = OffsetDateTime::now_utc();
            conn.execute(
                "INSERT INTO transcript_chunks(node_id,event_id,chunk,created_at) VALUES(?1,?2,?3,?4)",
                params![node_id.to_string(), event_id, text, now.unix_timestamp()],
            )?;
            Ok(event_id)
        })();
        if result.is_ok() {
            conn.execute_batch("COMMIT")?;
        } else {
            let _ = conn.execute_batch("ROLLBACK");
        }
        result
    }

    pub fn list_events(&self, node_id: Uuid) -> Result<Vec<NodeEvent>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT id, node_id, sequence, kind, body, created_at
            FROM events
            WHERE node_id = ?1
            ORDER BY sequence ASC
            ",
        )?;
        let mut rows = stmt.query([node_id.to_string()])?;
        let mut events = Vec::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let node_str: String = row.get(1)?;
            let sequence: i64 = row.get(2)?;
            let kind_text: String = row.get(3)?;
            let body_text: String = row.get(4)?;
            let body = serde_json::from_str::<JsonValue>(&body_text)?;
            let created_at: i64 = row.get(5)?;
            let event_kind = parse_event_kind(&kind_text)?;
            events.push(NodeEvent {
                id,
                node_id: Uuid::parse_str(&node_str)?,
                sequence,
                kind: event_kind,
                body,
                created_at: epoch_to_offset_dt(created_at),
                schema_version: NodeEvent::default_schema_version(),
            });
        }
        Ok(events)
    }

    pub fn create_relationship(
        &self,
        source: Uuid,
        target: Uuid,
        kind: RelationshipKind,
        label: Option<String>,
    ) -> Result<RelationshipRecord> {
        let id = Uuid::new_v4();
        let now = OffsetDateTime::now_utc();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO relationships(id,source_node_id,target_node_id,kind,label,created_at)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                id.to_string(),
                source.to_string(),
                target.to_string(),
                relationship_kind_to_string(&kind),
                label,
                now.unix_timestamp()
            ],
        )?;
        Ok(RelationshipRecord {
            id,
            source_node_id: source,
            target_node_id: target,
            kind,
            label,
            created_at: now,
        })
    }

    pub fn delete_relationship(&self, id: Uuid) -> Result<bool> {
        let conn = self.conn()?;
        let count = conn.execute("DELETE FROM relationships WHERE id = ?1", [id.to_string()])?;
        Ok(count > 0)
    }

    pub fn insert_token(
        &self,
        token_id: Uuid,
        name: &str,
        hash: &str,
        scope_json: &str,
        expires_at: i64,
    ) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO tokens(id,name,hash,scope,created_at,expires_at,revoked,raw_preview)
             VALUES(?1,?2,?3,?4,?5,?6,0,?7)",
            params![
                token_id.to_string(),
                name,
                hash,
                scope_json,
                OffsetDateTime::now_utc().unix_timestamp(),
                expires_at,
                name,
            ],
        )?;
        Ok(())
    }

    pub fn revoke_token(&self, token_id: Uuid) -> Result<bool> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE tokens SET revoked = 1 WHERE id = ?1",
            params![token_id.to_string()],
        )?;
        Ok(affected > 0)
    }

    pub fn list_active_tokens(&self) -> Result<Vec<ActiveTokenRecord>> {
        let conn = self.conn()?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut stmt = conn.prepare(
            "SELECT id,name,hash,expires_at,revoked FROM tokens
                WHERE revoked = 0
                ORDER BY created_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let token_id = Uuid::parse_str(&row.get::<_, String>(0)?)?;
            let name = row.get::<_, String>(1)?;
            let hash = row.get::<_, String>(2)?;
            let expires_at = row.get::<_, i64>(3)?;
            let revoked = row.get::<_, i64>(4)? == 1;
            if expires_at >= now && !revoked {
                out.push((token_id, name, hash, expires_at, revoked));
            }
        }
        Ok(out)
    }

    /// Returns ALL tokens (active, expired, revoked) for management UI display.
    /// Never returns the raw token value or hash — only metadata.
    pub fn list_all_tokens(&self) -> Result<Vec<asylum_types::api::TokenSummary>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, created_at, expires_at, revoked FROM tokens ORDER BY created_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(asylum_types::api::TokenSummary {
                id: row.get::<_, String>(0)?,
                label: row.get::<_, String>(1)?,
                created_at_epoch_secs: row.get::<_, i64>(2)?,
                expires_at_epoch_secs: row.get::<_, i64>(3)?,
                revoked: row.get::<_, i64>(4)? == 1,
            });
        }
        Ok(out)
    }

    pub fn get_token_metadata(&self, id: Uuid) -> Result<Option<(String, i64, i64)>> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT name, created_at, expires_at FROM tokens WHERE id = ?1")?;
        let row = stmt
            .query_row(params![id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .optional()?;
        Ok(row)
    }

    pub fn find_token_by_hash(&self, hash: &str) -> Result<Option<(Uuid, String, String, i64)>> {
        let conn = self.conn()?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut stmt = conn.prepare(
            "
            SELECT id, name, scope, expires_at
            FROM tokens
            WHERE hash = ?1
              AND revoked = 0
              AND expires_at >= ?2
            LIMIT 1
            ",
        )?;
        let row = stmt
            .query_row(params![hash, now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .optional()?;
        let Some((id, name, scope, expires_at)) = row else {
            return Ok(None);
        };
        Ok(Some((Uuid::parse_str(&id)?, name, scope, expires_at)))
    }

    pub fn insert_relationship_seed(&self, source: Uuid, target: Uuid) -> Result<Uuid> {
        self.create_relationship(
            source,
            target,
            RelationshipKind::PlatformResponsibility,
            Some("created_by".to_string()),
        )
        .map(|record| record.id)
    }

    pub fn insert_artifact(
        &self,
        node_id: Uuid,
        kind: &str,
        path: &str,
        metadata: Option<&str>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = OffsetDateTime::now_utc();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO artifacts(id,node_id,kind,path,created_at,metadata_json)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                id.to_string(),
                node_id.to_string(),
                kind,
                path,
                now.unix_timestamp(),
                metadata
            ],
        )?;
        Ok(id)
    }

    pub fn list_artifacts(
        &self,
        node_id: Option<Uuid>,
    ) -> Result<Vec<(Uuid, String, String, String)>> {
        let conn = self.conn()?;
        let mut stmt = if node_id.is_some() {
            conn.prepare(
                "SELECT id,node_id,kind,path
                 FROM artifacts
                 WHERE node_id = ?1
                 ORDER BY created_at DESC",
            )?
        } else {
            conn.prepare("SELECT id,node_id,kind,path FROM artifacts ORDER BY created_at DESC")?
        };
        let mut rows = if let Some(node_id) = node_id {
            stmt.query([node_id.to_string()])?
        } else {
            stmt.query([])?
        };
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push((
                Uuid::parse_str(&row.get::<_, String>(0)?)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            ));
        }
        Ok(out)
    }

    pub fn insert_remote_command(
        &self,
        id: Uuid,
        kind: &str,
        args_json: &str,
        token: &str,
        node_id: Option<Uuid>,
    ) -> Result<()> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO remote_commands(id,kind,args_json,token,node_id,status,error,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,'received',NULL,?6,?6)",
            params![
                id.to_string(),
                kind,
                args_json,
                token,
                node_id.map(|id| id.to_string()),
                now
            ],
        )?;
        Ok(())
    }

    pub fn update_remote_command_status(
        &self,
        id: Uuid,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        conn.execute(
            "UPDATE remote_commands
                SET status = ?1, error = ?2, updated_at = ?3
                WHERE id = ?4",
            params![status, error, now, id.to_string()],
        )?;
        Ok(())
    }

    pub fn resolve_decision(&self, id: &str, status: &str) -> Result<bool> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE decisions
                SET status = ?1, decided_at = ?2
                WHERE id = ?3",
            params![status, now, id],
        )?;
        Ok(affected > 0)
    }

    /// The single pending decision for a node, if any. Used to dedup the
    /// awaiting-input decision producer so at most one pending decision exists
    /// per node at a time.
    pub fn pending_decision_for_node(&self, node_id: Uuid) -> Result<Option<DecisionStorageRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "
            SELECT id,node_id,text,status,created_at,decided_at
            FROM decisions
            WHERE node_id = ?1 AND status = 'pending'
            ORDER BY created_at DESC
            LIMIT 1
            ",
            params![node_id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
        .context("pending decision for node")
    }

    /// Refresh the question text of an existing (pending) decision. Used when a
    /// fresh awaiting-input arrives while a decision is already pending.
    pub fn update_decision_text(&self, id: &str, text: &str) -> Result<bool> {
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE decisions SET text = ?1 WHERE id = ?2 AND status = 'pending'",
            params![text, id],
        )?;
        Ok(affected > 0)
    }

    pub fn insert_decision(
        &self,
        node_id: Option<Uuid>,
        text: &str,
    ) -> Result<DecisionStorageRecord> {

        let id = Uuid::new_v4().to_string();
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let node_id = node_id.map(|id| id.to_string());
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO decisions(id,node_id,text,status,created_at,decided_at)
             VALUES(?1,?2,?3,'pending',?4,NULL)",
            params![id, node_id, text, now],
        )?;
        Ok((
            id,
            node_id,
            text.to_string(),
            "pending".to_string(),
            now,
            None,
        ))
    }

    /// Create the single pending decision for a node, or -- if one already exists
    /// -- refresh its text instead of stacking a second (M6). Atomic against the
    /// partial unique index `decisions_one_pending_per_node`: two concurrent
    /// awaiting-input posts for the same node can no longer both insert. Returns
    /// the decision plus whether it was newly created (so the caller only
    /// notifies once).
    pub fn upsert_pending_node_decision(
        &self,
        node_id: Uuid,
        text: &str,
    ) -> Result<(DecisionStorageRecord, bool)> {
        let conn = self.conn()?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let id = Uuid::new_v4().to_string();
        let node_id_s = node_id.to_string();
        let insert = conn.execute(
            "INSERT INTO decisions(id,node_id,text,status,created_at,decided_at)
             VALUES(?1,?2,?3,'pending',?4,NULL)",
            params![id, node_id_s, text, now],
        );
        match insert {
            Ok(_) => Ok((
                (
                    id,
                    Some(node_id_s),
                    text.to_string(),
                    "pending".to_string(),
                    now,
                    None,
                ),
                true,
            )),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                // A pending decision already exists for this node: refresh its
                // text and return it (no duplicate notification).
                conn.execute(
                    "UPDATE decisions SET text = ?1 WHERE node_id = ?2 AND status = 'pending'",
                    params![text, node_id_s],
                )?;
                let existing = conn.query_row(
                    "SELECT id,node_id,text,status,created_at,decided_at FROM decisions
                     WHERE node_id = ?1 AND status = 'pending' ORDER BY created_at DESC LIMIT 1",
                    params![node_id_s],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )?;
                Ok((existing, false))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_decisions(&self) -> Result<Vec<DecisionStorageRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT id,node_id,text,status,created_at,decided_at
            FROM decisions
            ORDER BY created_at DESC
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ));
        }
        Ok(out)
    }

    pub fn get_decision(&self, id: &str) -> Result<Option<DecisionStorageRecord>> {
        let conn = self.conn()?;
        conn.query_row(
            "
            SELECT id,node_id,text,status,created_at,decided_at
            FROM decisions
            WHERE id = ?1
            ",
            params![id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
        .context("get decision")
    }

    pub fn mark_notification_read(&self, id: i64) -> Result<()> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE notifications SET read_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        if affected == 0 {
            return Err(anyhow::anyhow!("notification not found"));
        }
        Ok(())
    }

    pub fn list_notifications(&self) -> Result<Vec<NotificationRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT id,node_id,kind,title,body,created_at,read_at
            FROM notifications
            ORDER BY created_at DESC
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let node_id: Option<String> = row.get(1)?;
            let kind: String = row.get(2)?;
            let title: String = row.get(3)?;
            let body: String = row.get(4)?;
            let created: i64 = row.get(5)?;
            let read: Option<i64> = row.get(6)?;
            out.push((id, node_id, kind, title, body, created, read));
        }
        Ok(out)
    }

    pub fn insert_notification(
        &self,
        node_id: Option<Uuid>,
        kind: &str,
        title: &str,
        body: &str,
    ) -> Result<i64> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO notifications(node_id,kind,title,body,created_at,read_at)
             VALUES(?1,?2,?3,?4,?5,NULL)",
            params![node_id.map(|id| id.to_string()), kind, title, body, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_recent_workspaces(&self, limit: usize) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT DISTINCT workspace FROM nodes
            WHERE workspace IS NOT NULL
            ORDER BY updated_at DESC
            LIMIT ?1
            ",
        )?;
        let mut rows = stmt.query([limit as i64])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let ws: String = row.get(0)?;
            out.push(ws);
        }
        Ok(out)
    }

    pub fn insert_test_node(&self, role_hint: &str) -> Result<NodeRecord> {
        self.insert_node(
            HarnessKind::ClaudeCode,
            SubstrateKind::Local,
            role_hint,
            Some("/tmp"),
            Some("test"),
            None,
            CapabilitySnapshot::default(),
            None,
        )
    }

    pub fn insert_test_node_with_created_by(
        &self,
        role_hint: &str,
        created_by: Uuid,
    ) -> Result<NodeRecord> {
        self.insert_node(
            HarnessKind::ClaudeCode,
            SubstrateKind::Local,
            role_hint,
            Some("/tmp"),
            Some("test"),
            None,
            CapabilitySnapshot::default(),
            Some(created_by),
        )
    }

    pub fn list_channels(&self) -> Result<Vec<ChannelRow>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id,kind,name,label,direction,status,detail,config_json,live,builtin,created_at
             FROM channels ORDER BY builtin DESC, name ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(row_to_channel_row(row)?);
        }
        Ok(out)
    }

    pub fn get_channel(&self, id: &str) -> Result<Option<ChannelRow>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id,kind,name,label,direction,status,detail,config_json,live,builtin,created_at
             FROM channels WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_channel_row(row)?)),
            None => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_channel(
        &self,
        id: &str,
        kind: &str,
        name: &str,
        label: &str,
        direction: &str,
        status: &str,
        detail: &str,
        config_json: &str,
        live: bool,
        builtin: bool,
    ) -> Result<ChannelRow> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO channels(id,kind,name,label,direction,status,detail,config_json,live,builtin,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(id) DO UPDATE SET
                 kind=excluded.kind,
                 name=excluded.name,
                 label=excluded.label,
                 direction=excluded.direction,
                 status=excluded.status,
                 detail=excluded.detail,
                 config_json=excluded.config_json,
                 live=excluded.live,
                 builtin=excluded.builtin",
            params![
                id,
                kind,
                name,
                label,
                direction,
                status,
                detail,
                config_json,
                live as i64,
                builtin as i64,
                now,
            ],
        )?;
        let mut stmt = conn.prepare(
            "SELECT id,kind,name,label,direction,status,detail,config_json,live,builtin,created_at
             FROM channels WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        let row = rows
            .next()?
            .ok_or_else(|| anyhow::anyhow!("channel upsert returned no row"))?;
        row_to_channel_row(row)
    }

    pub fn delete_channel(&self, id: &str) -> Result<bool> {
        let conn = self.conn()?;
        let builtin: Option<i64> = conn
            .query_row("SELECT builtin FROM channels WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?;
        match builtin {
            Some(1) => Err(anyhow::anyhow!("cannot delete builtin channel")),
            Some(_) => {
                conn.execute("DELETE FROM channels WHERE id = ?1", [id])?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn delete_builtin_channels_by_ids(&self, ids: &[&str]) -> Result<usize> {
        let conn = self.conn()?;
        let mut deleted = 0usize;
        for id in ids {
            deleted += conn
                .execute("DELETE FROM channels WHERE id = ?1 AND builtin = 1", [id])
                .map_err(|err| anyhow::anyhow!("failed to delete channel '{id}': {err}"))?
                as usize;
        }
        Ok(deleted)
    }

    pub fn list_channel_messages(&self, id: &str, limit: usize) -> Result<Vec<ChannelMessageRow>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id,channel_id,direction,ts,sender,subject,body,replies_json,node_id,correlation_token
             FROM channel_messages WHERE channel_id = ?1 ORDER BY ts DESC LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![id, limit as i64])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(row_to_channel_message(row)?);
        }
        out.reverse();
        Ok(out)
    }

    pub fn insert_channel_message(
        &self,
        channel_id: &str,
        direction: &str,
        sender: &str,
        subject: &str,
        body: &str,
        replies: &[String],
        node_id: Option<Uuid>,
        correlation_token: Option<&str>,
    ) -> Result<i64> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        let replies_json = serde_json::to_string(&replies.to_vec())?;
        conn.execute(
            "INSERT INTO channel_messages(channel_id,direction,ts,sender,subject,body,replies_json,node_id,correlation_token)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                channel_id,
                direction,
                now,
                sender,
                subject,
                body,
                replies_json,
                node_id.map(|id| id.to_string()),
                correlation_token
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn insert_channel_reply_correlation(
        &self,
        token: &str,
        channel_id: &str,
        node_id: Uuid,
        expires_at: i64,
    ) -> Result<()> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO channel_reply_correlations(token,channel_id,node_id,created_at,expires_at)
             VALUES(?1,?2,?3,?4,?5)",
            params![
                token,
                channel_id,
                node_id.to_string(),
                now,
                expires_at
            ],
        )?;
        Ok(())
    }

    pub fn resolve_channel_reply_correlation(
        &self,
        channel_id: &str,
        token: &str,
    ) -> Result<Option<Uuid>> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        let node_id = conn
            .query_row(
                "
            SELECT node_id
            FROM channel_reply_correlations
            WHERE token = ?1
              AND channel_id = ?2
              AND expires_at >= ?3
            LIMIT 1",
                params![token, channel_id, now],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(node_id.map(|value| Uuid::parse_str(&value)).transpose()?)
    }

    pub fn count_channel_messages_24h(&self, channel_id: &str) -> Result<u64> {
        let conn = self.conn()?;
        let cutoff = OffsetDateTime::now_utc().unix_timestamp() - 86_400;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM channel_messages WHERE channel_id = ?1 AND ts >= ?2",
            params![channel_id, cutoff],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn list_hooks(&self) -> Result<Vec<HookRow>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id,name,enabled,event,filter,actions_json,future,created_at,updated_at
             FROM hooks ORDER BY created_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(row_to_hook_row(row)?);
        }
        Ok(out)
    }

    pub fn get_hook(&self, id: &str) -> Result<Option<HookRow>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id,name,enabled,event,filter,actions_json,future,created_at,updated_at
             FROM hooks WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_hook_row(row)?)),
            None => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_hook(
        &self,
        id: &str,
        name: &str,
        enabled: bool,
        event: &str,
        filter: &str,
        actions_json: &str,
        future: bool,
    ) -> Result<HookRow> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO hooks(id,name,enabled,event,filter,actions_json,future,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",
            params![
                id,
                name,
                enabled as i64,
                event,
                filter,
                actions_json,
                future as i64,
                now,
            ],
        )?;
        Self::load_hook_with_conn(&conn, id)?
            .ok_or_else(|| anyhow::anyhow!("hook insert returned no row"))
    }

    fn load_hook_with_conn(conn: &Connection, id: &str) -> Result<Option<HookRow>> {
        let mut stmt = conn.prepare(
            "SELECT id,name,enabled,event,filter,actions_json,future,created_at,updated_at
             FROM hooks WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_hook_row(row)?)),
            None => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_hook(
        &self,
        id: &str,
        name: Option<&str>,
        enabled: Option<bool>,
        event: Option<&str>,
        filter: Option<&str>,
        actions_json: Option<&str>,
        future: Option<bool>,
    ) -> Result<Option<HookRow>> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        let existing = Self::load_hook_with_conn(&conn, id)?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        let new_name = name.map(str::to_string).unwrap_or(existing.name);
        let new_enabled = enabled.unwrap_or(existing.enabled);
        let new_event = event.map(str::to_string).unwrap_or(existing.event);
        let new_filter = filter.map(str::to_string).unwrap_or(existing.filter);
        let new_actions = actions_json
            .map(str::to_string)
            .unwrap_or(existing.actions_json);
        let new_future = future.unwrap_or(existing.future);
        conn.execute(
            "UPDATE hooks SET name=?2, enabled=?3, event=?4, filter=?5, actions_json=?6, future=?7, updated_at=?8 WHERE id = ?1",
            params![
                id,
                new_name,
                new_enabled as i64,
                new_event,
                new_filter,
                new_actions,
                new_future as i64,
                now,
            ],
        )?;
        Self::load_hook_with_conn(&conn, id)
    }

    pub fn delete_hook(&self, id: &str) -> Result<bool> {
        let conn = self.conn()?;
        let count = conn.execute("DELETE FROM hooks WHERE id = ?1", [id])?;
        Ok(count > 0)
    }

    pub fn list_hook_firings(&self, limit: usize) -> Result<Vec<HookFiringRow>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id,hook_id,ts,trigger,outcome,ok,payload_json
             FROM hook_firings ORDER BY ts DESC LIMIT ?1",
        )?;
        let mut rows = stmt.query([limit as i64])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(row_to_hook_firing(row)?);
        }
        out.reverse();
        Ok(out)
    }

    pub fn insert_hook_firing(
        &self,
        hook_id: &str,
        trigger: &str,
        outcome: &str,
        ok: bool,
        payload_json: &str,
    ) -> Result<HookFiringRow> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO hook_firings(hook_id,ts,trigger,outcome,ok,payload_json)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![hook_id, now, trigger, outcome, ok as i64, payload_json],
        )?;
        let id = conn.last_insert_rowid();
        Ok(HookFiringRow {
            id,
            hook_id: hook_id.to_string(),
            ts: now,
            trigger: trigger.to_string(),
            outcome: outcome.to_string(),
            ok,
            payload_json: payload_json.to_string(),
        })
    }

    fn next_event_sequence_with_conn(conn: &Connection, node_id: Uuid) -> Result<i64> {
        let next: Option<i64> = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence), -1) + 1 FROM events WHERE node_id = ?1",
                [node_id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(next.unwrap_or(0))
    }

    fn record_event_with_conn(
        conn: &Connection,
        node_id: Uuid,
        kind: NodeEventKind,
        body: JsonValue,
    ) -> Result<i64> {
        let sequence = Self::next_event_sequence_with_conn(conn, node_id)?;
        let now = OffsetDateTime::now_utc();
        conn.execute(
            "INSERT INTO events(node_id,sequence,kind,body,created_at)
             VALUES(?1,?2,?3,?4,?5)",
            params![
                node_id.to_string(),
                sequence,
                serde_json::to_string(&kind)?,
                body.to_string(),
                now.unix_timestamp()
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }
}

fn row_to_node_record(row: &rusqlite::Row<'_>) -> Result<NodeRecord> {
    let id = Uuid::parse_str(&row.get::<_, String>(0)?).context("invalid node uuid")?;
    let harness = parse_harness_kind(&row.get::<_, String>(1)?).context("invalid harness")?;
    let substrate = parse_substrate_kind(&row.get::<_, String>(2)?).context("invalid substrate")?;
    let role_hint = row.get::<_, String>(3)?;
    let liveness = parse_node_liveness(&row.get::<_, String>(4)?).context("invalid liveness")?;
    let workspace = row.get::<_, Option<String>>(5)?;
    let description = row.get::<_, String>(6)?;
    let created_at = epoch_to_offset_dt(row.get::<_, i64>(7)?);
    let updated_at = epoch_to_offset_dt(row.get::<_, i64>(8)?);
    let external_id = row.get::<_, Option<String>>(9)?;
    let capabilities_json = row.get::<_, String>(10)?;
    let capabilities: CapabilitySnapshot =
        serde_json::from_str(&capabilities_json).context("failed to decode capabilities")?;
    let harness_session_id = row.get::<_, Option<String>>(11)?;
    let model = row.get::<_, Option<String>>(12)?;
    let effort = row.get::<_, Option<String>>(13)?;
    Ok(NodeRecord {
        id,
        harness,
        substrate,
        role_hint,
        liveness,
        workspace,
        description,
        created_at,
        updated_at,
        external_id,
        harness_session_id,
        model,
        effort,
        capabilities,
        tokens_in: 0,
        tokens_out: 0,
        tool_calls: 0,
        ctx_pct: 0.0,
        idle_seconds: 0,
    })
}

fn hydrate_node_telemetry(conn: &Connection, node: &mut NodeRecord) -> Result<()> {
    let id_string = node.id.to_string();

    let mut stmt = conn.prepare(
        "SELECT kind, body, created_at FROM events WHERE node_id = ?1 ORDER BY created_at ASC",
    )?;
    let mut rows = stmt.query([id_string])?;

    let mut tokens_in: u64 = 0;
    let mut tokens_out: u64 = 0;
    let mut tool_calls: u64 = 0;
    let mut last_event_created_at: Option<i64> = None;
    let mut last_output_chunk_at: Option<i64> = None;
    // Harness-reported context usage (claude statusline `used_percentage`),
    // authoritative over the crude token estimate below when present.
    let mut harness_used_percentage: Option<f64> = None;

    while let Some(row) = rows.next()? {
        let kind_text: String = row.get(0)?;
        let body_text: String = row.get(1)?;
        let created_at: i64 = row.get(2)?;
        last_event_created_at = Some(created_at);

        let kind = parse_event_kind(&kind_text)?;
        if matches!(kind, NodeEventKind::HarnessEvent) {
            if let Some(pct) = serde_json::from_str::<JsonValue>(&body_text)
                .ok()
                .as_ref()
                .and_then(|value| value.get("used_percentage"))
                .and_then(|value| value.as_f64())
            {
                harness_used_percentage = Some(pct);
            }
        }
        let text_len = match serde_json::from_str::<JsonValue>(&body_text)
            .ok()
            .as_ref()
            .and_then(|value| value.get("text"))
            .and_then(|value| value.as_str())
            .map(|text| text.to_string())
        {
            Some(text) => {
                // Use codepoint count so non-ASCII text doesn't inflate token estimates (L4).
                let len = text.chars().count();
                match kind {
                    NodeEventKind::OutputChunk => {
                        last_output_chunk_at = Some(created_at);
                        if text.contains("⏺ ") || text.contains("tool ") || text.contains("Tool: ")
                        {
                            tool_calls = tool_calls.saturating_add(1);
                        }
                    }
                    NodeEventKind::InputSent => {}
                    _ => {}
                }
                len
            }
            None => 0,
        };

        match kind {
            NodeEventKind::InputSent => {
                tokens_in = tokens_in.saturating_add((text_len as u64) / 4);
            }
            NodeEventKind::OutputChunk => {
                tokens_out = tokens_out.saturating_add((text_len as u64) / 4);
            }
            _ => {}
        }
    }

    node.tokens_in = tokens_in;
    node.tokens_out = tokens_out;
    node.tool_calls = tool_calls;
    node.ctx_pct = match harness_used_percentage {
        Some(pct) => (pct as f32 / 100.0).clamp(0.0, 1.0),
        None => {
            let total = (tokens_in + tokens_out) as f32;
            (total / 200_000.0).clamp(0.0, 1.0)
        }
    };

    let now = OffsetDateTime::now_utc().unix_timestamp();
    node.idle_seconds = match last_event_created_at {
        Some(ts) => {
            let elapsed = (now - ts).max(0) as u64;
            if matches!(node.liveness, NodeLiveness::Running)
                && last_output_chunk_at
                    .map(|out_ts| (now - out_ts).max(0) <= 5)
                    .unwrap_or(false)
            {
                0
            } else {
                elapsed
            }
        }
        None => 0,
    };

    Ok(())
}

fn parse_node_liveness(raw: &str) -> Result<NodeLiveness> {
    let parsed = match raw {
        "starting" => NodeLiveness::Starting,
        "running" => NodeLiveness::Running,
        "waiting_for_input" => NodeLiveness::WaitingForInput,
        "exited" => NodeLiveness::Exited,
        "stopped" => NodeLiveness::Stopped,
        "failed" => NodeLiveness::Failed,
        "archived" => NodeLiveness::Archived,
        other => return Err(anyhow::anyhow!("unknown node liveness: {other}")),
    };
    Ok(parsed)
}

fn parse_harness_kind(raw: &str) -> Result<HarnessKind> {
    raw.parse()
        .map_err(|err| anyhow::anyhow!("invalid harness '{raw}': {err}"))
}

fn parse_substrate_kind(raw: &str) -> Result<SubstrateKind> {
    raw.parse()
        .map_err(|err| anyhow::anyhow!("invalid substrate '{raw}': {err}"))
}

#[derive(Clone, Debug)]
pub struct ChannelRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub label: String,
    pub direction: String,
    pub status: String,
    pub detail: String,
    pub config_json: String,
    pub live: bool,
    pub builtin: bool,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct ChannelMessageRow {
    pub id: i64,
    pub channel_id: String,
    pub direction: String,
    pub ts: i64,
    pub sender: String,
    pub subject: String,
    pub body: String,
    pub replies_json: Option<String>,
    pub node_id: Option<String>,
    pub correlation_token: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HookRow {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub event: String,
    pub filter: String,
    pub actions_json: String,
    pub future: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct HookFiringRow {
    pub id: i64,
    pub hook_id: String,
    pub ts: i64,
    pub trigger: String,
    pub outcome: String,
    pub ok: bool,
    pub payload_json: String,
}

fn row_to_channel_row(row: &rusqlite::Row<'_>) -> Result<ChannelRow> {
    Ok(ChannelRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        label: row.get(3)?,
        direction: row.get(4)?,
        status: row.get(5)?,
        detail: row.get(6)?,
        config_json: row.get(7)?,
        live: row.get::<_, i64>(8)? != 0,
        builtin: row.get::<_, i64>(9)? != 0,
        created_at: row.get(10)?,
    })
}

fn row_to_channel_message(row: &rusqlite::Row<'_>) -> Result<ChannelMessageRow> {
    Ok(ChannelMessageRow {
        id: row.get(0)?,
        channel_id: row.get(1)?,
        direction: row.get(2)?,
        ts: row.get(3)?,
        sender: row.get(4)?,
        subject: row.get(5)?,
        body: row.get(6)?,
        replies_json: row.get(7)?,
        node_id: row.get(8)?,
        correlation_token: row.get(9)?,
    })
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(());
        }
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn row_to_hook_row(row: &rusqlite::Row<'_>) -> Result<HookRow> {
    Ok(HookRow {
        id: row.get(0)?,
        name: row.get(1)?,
        enabled: row.get::<_, i64>(2)? != 0,
        event: row.get(3)?,
        filter: row.get(4)?,
        actions_json: row.get(5)?,
        future: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn row_to_hook_firing(row: &rusqlite::Row<'_>) -> Result<HookFiringRow> {
    Ok(HookFiringRow {
        id: row.get(0)?,
        hook_id: row.get(1)?,
        ts: row.get(2)?,
        trigger: row.get(3)?,
        outcome: row.get(4)?,
        ok: row.get::<_, i64>(5)? != 0,
        payload_json: row.get(6)?,
    })
}

fn parse_relationship_kind(raw: &str) -> Option<RelationshipKind> {
    Some(match raw {
        "supervises" => RelationshipKind::Supervises,
        "spawned_for" => RelationshipKind::SpawnedFor,
        "user_created" => RelationshipKind::UserCreated,
        "platform_responsibility" => RelationshipKind::PlatformResponsibility,
        _ => return None,
    })
}

fn relationship_kind_to_string(kind: &RelationshipKind) -> &'static str {
    match kind {
        RelationshipKind::Supervises => "supervises",
        RelationshipKind::SpawnedFor => "spawned_for",
        RelationshipKind::UserCreated => "user_created",
        RelationshipKind::PlatformResponsibility => "platform_responsibility",
    }
}

fn parse_event_kind(raw: &str) -> Result<NodeEventKind> {
    serde_json::from_str(raw).or_else(|_| {
        Ok(match raw {
            "node_started" => NodeEventKind::NodeStarted,
            "output_chunk" => NodeEventKind::OutputChunk,
            "input_sent" => NodeEventKind::InputSent,
            "liveness_changed" => NodeEventKind::LivenessChanged,
            "harness_failure" => NodeEventKind::HarnessFailure,
            "substrate_failure" => NodeEventKind::SubstrateFailure,
            "human_input_requested" => NodeEventKind::HumanInputRequested,
            "notification_sent" => NodeEventKind::NotificationSent,
            "remote_command_received" => NodeEventKind::RemoteCommandReceived,
            "attach_issued" => NodeEventKind::AttachIssued,
            "harness_event" => NodeEventKind::HarnessEvent,
            other => return Err(anyhow::anyhow!("unknown event kind: {other}")),
        })
    })
}

fn epoch_to_offset_dt(value: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(value).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

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

    #[test]
    fn launch_profile_round_trips_through_node_row() {
        let store = Store::open_in_memory().unwrap();
        let node = store.insert_test_node("worker").unwrap();

        // A fresh node has no recorded profile (None = harness default marker).
        let fresh = store.get_node(node.id).unwrap().unwrap();
        assert_eq!(fresh.model, None);
        assert_eq!(fresh.effort, None);

        store
            .set_node_launch_profile(node.id, Some("opus"), Some("high"))
            .unwrap();

        // Both get_node and list_nodes (the row-mapping paths) surface it.
        let got = store.get_node(node.id).unwrap().unwrap();
        assert_eq!(got.model.as_deref(), Some("opus"));
        assert_eq!(got.effort.as_deref(), Some("high"));
        let listed = store
            .list_nodes()
            .unwrap()
            .into_iter()
            .find(|n| n.id == node.id)
            .unwrap();
        assert_eq!(listed.model.as_deref(), Some("opus"));
        assert_eq!(listed.effort.as_deref(), Some("high"));

        // A partial profile (model only) keeps effort at the default marker.
        store
            .set_node_launch_profile(node.id, Some("sonnet"), None)
            .unwrap();
        let got = store.get_node(node.id).unwrap().unwrap();
        assert_eq!(got.model.as_deref(), Some("sonnet"));
        assert_eq!(got.effort, None);
    }

    // M1/M2: compare-and-set liveness only transitions FROM an allowed state.
    #[test]
    fn transition_node_liveness_is_a_compare_and_set() {
        let store = Store::open_in_memory().unwrap();
        let node = store.insert_test_node("worker").unwrap();
        // Fresh node is Starting. CAS Starting -> Running succeeds.
        assert!(store
            .transition_node_liveness(
                node.id,
                NodeLiveness::Running,
                &[NodeLiveness::Starting],
                None,
                serde_json::json!({}),
            )
            .unwrap());
        assert_eq!(store.get_node(node.id).unwrap().unwrap().liveness, NodeLiveness::Running);

        // Exit sink wins: Running -> Stopped from the active set.
        assert!(store
            .transition_node_liveness(
                node.id,
                NodeLiveness::Stopped,
                &[NodeLiveness::Running, NodeLiveness::Starting, NodeLiveness::WaitingForInput],
                Some("exited"),
                serde_json::json!({}),
            )
            .unwrap());

        // M2: a stale-snapshot active write (post_harness_event) can no longer
        // resurrect the now-terminal node -- CAS from the active set no-ops.
        assert!(!store
            .transition_node_liveness(
                node.id,
                NodeLiveness::Running,
                &[NodeLiveness::Running, NodeLiveness::Starting, NodeLiveness::WaitingForInput],
                None,
                serde_json::json!({}),
            )
            .unwrap());
        assert_eq!(store.get_node(node.id).unwrap().unwrap().liveness, NodeLiveness::Stopped);

        // M1: resume CAS Starting -> Running no-ops once the node is terminal, so
        // a resumed child that died during launch is not resurrected.
        store.set_node_liveness(node.id, NodeLiveness::Failed).unwrap();
        assert!(!store
            .transition_node_liveness(
                node.id,
                NodeLiveness::Running,
                &[NodeLiveness::Starting],
                Some("resumed"),
                serde_json::json!({}),
            )
            .unwrap());
        assert_eq!(store.get_node(node.id).unwrap().unwrap().liveness, NodeLiveness::Failed);
    }

    // M6: at most one pending decision per node, enforced atomically.
    #[test]
    fn upsert_pending_decision_dedups_per_node() {
        let store = Store::open_in_memory().unwrap();
        let node = store.insert_test_node("worker").unwrap();

        let (first, created1) = store.upsert_pending_node_decision(node.id, "q1").unwrap();
        assert!(created1);
        let (second, created2) = store.upsert_pending_node_decision(node.id, "q2").unwrap();
        assert!(!created2, "second awaiting-input must refresh, not stack");
        assert_eq!(first.0, second.0, "same decision id refreshed");
        assert_eq!(second.2, "q2", "text refreshed");

        // Exactly one pending decision exists for the node.
        let pending: Vec<_> = store
            .list_decisions()
            .unwrap()
            .into_iter()
            .filter(|d| d.1.as_deref() == Some(node.id.to_string().as_str()) && d.3 == "pending")
            .collect();
        assert_eq!(pending.len(), 1);
    }

    // M6: a raw INSERT of a second pending decision violates the partial unique
    // index, proving the DB (not just app logic) enforces the invariant.
    #[test]
    fn raw_second_pending_decision_insert_is_rejected_by_db() {
        let store = Store::open_in_memory().unwrap();
        let node = store.insert_test_node("worker").unwrap();
        store.insert_decision(Some(node.id), "first").unwrap();
        let dup = store.insert_decision(Some(node.id), "second");
        assert!(dup.is_err(), "DB must reject a second pending decision per node");
    }

    // m2: per-node create-time launch_args round-trip; absent column reads empty.
    #[test]
    fn node_launch_args_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let node = store.insert_test_node("worker").unwrap();
        // Never set: baseline (empty), so resume falls back to the adapter args.
        assert!(store.get_node_launch_args(node.id).unwrap().is_empty());

        let args = vec!["--model".to_string(), "opus".to_string()];
        store.set_node_launch_args(node.id, &args).unwrap();
        assert_eq!(store.get_node_launch_args(node.id).unwrap(), args);

        // Setting empty clears back to NULL/empty.
        store.set_node_launch_args(node.id, &[]).unwrap();
        assert!(store.get_node_launch_args(node.id).unwrap().is_empty());
    }

    // M7: ctx_pressure fired-state round-trips on the node row.
    #[test]
    fn ctx_pressure_state_round_trips() {
        let store = Store::open_in_memory().unwrap();
        let node = store.insert_test_node("worker").unwrap();
        assert_eq!(store.ctx_pressure_state(node.id).unwrap(), (None, None));
        store.set_ctx_pressure_state(node.id, Some("sess-1"), 0.7).unwrap();
        let (sess, max) = store.ctx_pressure_state(node.id).unwrap();
        assert_eq!(sess.as_deref(), Some("sess-1"));
        assert_eq!(max, Some(0.7));
    }

    // M3: guest tokens revoke by name (loon-node-{id}).
    #[test]
    fn revoke_tokens_by_name_revokes_matching_active_tokens() {
        let store = Store::open_in_memory().unwrap();
        let id = Uuid::new_v4();
        store
            .insert_token(id, "loon-node-abc", "hash-abc", "[\"loon-node\"]", i64::MAX)
            .unwrap();
        assert_eq!(store.revoke_tokens_by_name("loon-node-abc").unwrap(), 1);
        // Already revoked: no-op the second time.
        assert_eq!(store.revoke_tokens_by_name("loon-node-abc").unwrap(), 0);
        assert!(store.find_token_by_hash("hash-abc").unwrap().is_none());
    }

    #[test]
    fn created_by_is_provenance_not_graph_edge() {
        let store = Store::open_in_memory().unwrap();
        let parent = store.insert_test_node("parent").unwrap();
        let child = store
            .insert_test_node_with_created_by("child", parent.id)
            .unwrap();
        assert_ne!(parent.id, child.id);

        let graph = store.graph().unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.relationships.is_empty());
    }

    #[test]
    fn telemetry_hydration_counts_tokens_and_tool_calls() {
        let store = Store::open_in_memory().unwrap();
        let node = store.insert_test_node("worker").unwrap();
        store
            .record_event(
                node.id,
                NodeEventKind::InputSent,
                serde_json::json!({"text": "abcdefgh"}),
            )
            .unwrap();
        store
            .append_transcript_chunk(node.id, "hello world chunk text")
            .unwrap();
        store
            .append_transcript_chunk(node.id, "⏺ tool invoked Bash")
            .unwrap();
        let hydrated = store.get_node(node.id).unwrap().unwrap();
        assert_eq!(hydrated.tokens_in, 2);
        assert!(hydrated.tokens_out >= 1);
        assert_eq!(hydrated.tool_calls, 1);
        assert!(hydrated.ctx_pct >= 0.0 && hydrated.ctx_pct <= 1.0);
    }

    #[test]
    fn channel_seed_and_messages_round_trip() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_channel(
                "ntfy-default",
                "ntfy",
                "ntfy default",
                "ntfy default",
                "duplex",
                "live",
                "detail",
                "{}",
                true,
                true,
            )
            .unwrap();
        store
            .insert_channel_message(
                "ntfy-default",
                "out",
                "asylum",
                "subject",
                "body",
                &[],
                None,
                None,
            )
            .unwrap();
        let messages = store.list_channel_messages("ntfy-default", 10).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].subject, "subject");
        let count = store.count_channel_messages_24h("ntfy-default").unwrap();
        assert_eq!(count, 1);
        let err = store.delete_channel("ntfy-default");
        assert!(err.is_err(), "builtin channel delete should fail");
    }

    #[test]
    fn delete_builtin_channels_by_ids_only_removes_builtin_rows() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_channel(
                "legacy-fake-builtin",
                "webhook",
                "legacy fake",
                "legacy fake",
                "outbound",
                "configured",
                "from legacy installer",
                "{}",
                false,
                true,
            )
            .unwrap();
        store
            .upsert_channel(
                "legacy-fake-custom",
                "webhook",
                "legacy custom",
                "legacy custom",
                "outbound",
                "configured",
                "user custom",
                "{}",
                false,
                false,
            )
            .unwrap();

        let removed = store
            .delete_builtin_channels_by_ids(&["legacy-fake-builtin", "legacy-fake-custom"])
            .unwrap();
        assert_eq!(removed, 1);

        assert!(store.get_channel("legacy-fake-builtin").unwrap().is_none());
        assert!(store.get_channel("legacy-fake-custom").unwrap().is_some());
    }

    #[test]
    fn channel_reply_correlation_round_trip() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_channel(
                "ntfy-default",
                "ntfy",
                "ntfy default",
                "ntfy default",
                "duplex",
                "live",
                "detail",
                "{}",
                true,
                true,
            )
            .unwrap();
        let node = store.insert_test_node("worker").unwrap();
        store
            .insert_channel_reply_correlation(
                "token",
                "ntfy-default",
                node.id,
                OffsetDateTime::now_utc().unix_timestamp() + 60,
            )
            .unwrap();
        let resolved = store
            .resolve_channel_reply_correlation("ntfy-default", "token")
            .unwrap()
            .expect("expected active correlation to resolve");
        assert_eq!(resolved, node.id);
    }

    #[test]
    fn channel_reply_correlation_expires() {
        let store = Store::open_in_memory().unwrap();
        store
            .upsert_channel(
                "ntfy-default",
                "ntfy",
                "ntfy default",
                "ntfy default",
                "duplex",
                "live",
                "detail",
                "{}",
                true,
                true,
            )
            .unwrap();
        let node = store.insert_test_node("worker").unwrap();
        store
            .insert_channel_reply_correlation(
                "token-2",
                "ntfy-default",
                node.id,
                OffsetDateTime::now_utc().unix_timestamp() - 1,
            )
            .unwrap();
        assert!(store
            .resolve_channel_reply_correlation("ntfy-default", "token-2")
            .unwrap()
            .is_none());
    }

    #[test]
    fn hook_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let hook = store
            .insert_hook("hook-1", "Test", true, "node.exited", "any", "[]", false)
            .unwrap();
        assert_eq!(hook.name, "Test");
        let updated = store
            .update_hook(
                "hook-1",
                Some("Renamed"),
                Some(false),
                None,
                None,
                None,
                None,
            )
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "Renamed");
        assert!(!updated.enabled);
        assert!(store.delete_hook("hook-1").unwrap());
        assert!(!store.delete_hook("hook-1").unwrap());
    }

    #[test]
    fn mark_notification_read_sets_read_at_when_exists() {
        let store = Store::open_in_memory().unwrap();
        let notification_id = store
            .insert_notification(None, "status", "Ready", "Node is ready")
            .unwrap();
        let before = store
            .list_notifications()
            .unwrap()
            .first()
            .and_then(|notification| notification.6);
        assert!(before.is_none());

        store.mark_notification_read(notification_id).unwrap();

        let after = store
            .list_notifications()
            .unwrap()
            .first()
            .and_then(|notification| notification.6);
        assert!(after.is_some());
    }

    #[test]
    fn mark_notification_read_returns_not_found_for_missing_row() {
        let store = Store::open_in_memory().unwrap();
        let error = store.mark_notification_read(424242).unwrap_err();
        assert!(error.to_string().contains("notification not found"));
    }
}
