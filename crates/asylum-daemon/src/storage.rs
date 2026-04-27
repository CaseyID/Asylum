use anyhow::{Context, Result};
use asylum_core::event::{NodeEvent, NodeEventKind};
use asylum_core::node::{
    CapabilitySnapshot, GraphRecord, HarnessKind, NodeLiveness, NodeRecord, SubstrateKind,
};
use asylum_core::relationship::RelationshipKind;
use asylum_core::relationship::RelationshipRecord;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;
use std::path::Path;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
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

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path.as_ref())
            .with_context(|| format!("failed to open store at {:?}", path.as_ref()))?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
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

            CREATE INDEX IF NOT EXISTS idx_events_node_seq ON events(node_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_artifacts_node ON artifacts(node_id);
            CREATE INDEX IF NOT EXISTS idx_relationships_source ON relationships(source_node_id);
            CREATE INDEX IF NOT EXISTS idx_relationships_target ON relationships(target_node_id);
            ",
        )?;
        Ok(())
    }

    pub fn list_nodes(&self) -> Result<Vec<NodeRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT id,harness,substrate,role_hint,liveness,workspace,description,created_at,updated_at,external_id,capabilities_json
            FROM nodes
            ORDER BY created_at DESC
            ",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(row_to_node_record(row)?);
        }
        Ok(out)
    }

    pub fn list_nodes_by_liveness(&self, liveness: NodeLiveness) -> Result<Vec<NodeRecord>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "
            SELECT id,harness,substrate,role_hint,liveness,workspace,description,created_at,updated_at,external_id,capabilities_json
            FROM nodes
            WHERE liveness = ?1
            ORDER BY created_at DESC
            ",
        )?;
        let mut rows = stmt.query([liveness.to_string()])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(row_to_node_record(row)?);
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
            SELECT id,harness,substrate,role_hint,liveness,workspace,description,created_at,updated_at,external_id,capabilities_json
            FROM nodes
            WHERE id = ?1
            ",
        )?;
        let mut rows = stmt.query([id_string])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_node_record(row)?)),
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

    pub fn set_node_external_id(&self, id: Uuid, external_id: Option<String>) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE nodes SET external_id = ?1 WHERE id = ?2",
            params![external_id, id.to_string()],
        )?;
        Ok(())
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
        Self::record_event_with_conn(&conn, node_id, kind, body)
    }

    pub fn append_transcript_chunk(&self, node_id: Uuid, text: &str) -> Result<i64> {
        let conn = self.conn()?;
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

    pub fn mark_notification_read(&self, id: i64) -> Result<()> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let conn = self.conn()?;
        conn.execute(
            "UPDATE notifications SET read_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
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
        capabilities,
    })
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
}
