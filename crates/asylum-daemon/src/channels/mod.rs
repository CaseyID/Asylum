use anyhow::{anyhow, Context, Result};
use asylum_types::api::{ChannelDescriptor, ChannelMessageRecord};

use crate::storage::{ChannelMessageRow, ChannelRow, Store};

pub mod ntfy_inbound;
pub use ntfy_inbound::NtfyInboundConfig;

pub const NTFY_DEFAULT_ID: &str = "ntfy-default";
pub const WEBHOOK_SUBSTRATE_ID: &str = "webhook-substrate";
pub const IMPLEMENTED_CHANNEL_KINDS: [&str; 2] = ["ntfy", "webhook"];
const LEGACY_FAKE_BUILTIN_CHANNEL_IDS: [&str; 4] =
    ["sms-twilio", "discord", "slack", "email-relay"];

pub fn descriptor_from_row(store: &Store, row: ChannelRow) -> Result<ChannelDescriptor> {
    let count = store.count_channel_messages_24h(&row.id)?;
    let config = serde_json::from_str(&row.config_json).context(format!(
        "failed to decode config_json for channel '{}'",
        row.id
    ))?;
    Ok(ChannelDescriptor {
        id: row.id,
        kind: row.kind,
        name: row.name,
        label: row.label,
        direction: row.direction,
        status: row.status,
        detail: row.detail,
        config,
        live: row.live,
        builtin: row.builtin,
        created_at_epoch_secs: row.created_at,
        message_count_24h: count,
    })
}

pub fn message_record_from_row(row: ChannelMessageRow) -> ChannelMessageRecord {
    let replies = row
        .replies_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default();
    ChannelMessageRecord {
        id: row.id,
        channel_id: row.channel_id,
        direction: row.direction,
        ts_epoch_secs: row.ts,
        sender: row.sender,
        subject: row.subject,
        body: row.body,
        replies,
        node_id: row.node_id,
        correlation_token: row.correlation_token,
    }
}

pub fn is_implemented_channel_kind(kind: &str) -> bool {
    IMPLEMENTED_CHANNEL_KINDS.contains(&kind)
}

pub struct SeedConfig {
    pub ntfy_configured: bool,
}

pub fn seed_builtin_channels(store: &Store, seed: SeedConfig) -> Result<()> {
    purge_legacy_fake_builtin_channels(store)?;

    let ntfy_status = if seed.ntfy_configured {
        "live"
    } else {
        "configured"
    };
    seed_one(
        store,
        NTFY_DEFAULT_ID,
        "ntfy",
        "ntfy default",
        "ntfy",
        "duplex",
        ntfy_status,
        if seed.ntfy_configured {
            "ntfy.sh outbound + inbound; configured via daemon ntfy settings"
        } else {
            "ntfy outbound; configure server+topic to enable sending"
        },
        serde_json::json!({}),
        seed.ntfy_configured,
    )?;
    seed_one(
        store,
        WEBHOOK_SUBSTRATE_ID,
        "webhook",
        "webhook substrate",
        "Webhook substrate",
        "inbound",
        "live",
        "Inbound webhook receiver protected by owner-token middleware",
        serde_json::json!({}),
        true,
    )?;
    Ok(())
}

fn purge_legacy_fake_builtin_channels(store: &Store) -> Result<()> {
    let removed = store.delete_builtin_channels_by_ids(&LEGACY_FAKE_BUILTIN_CHANNEL_IDS)?;
    if removed > 0 {
        tracing::info!("purged {removed} legacy fake builtin channels");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Store;
    use rusqlite::Connection;
    use tempfile::tempdir;

    #[test]
    fn seed_builtin_channels_only_keeps_real_default_channels() -> Result<()> {
        let store = Store::open_in_memory()?;
        for id in LEGACY_FAKE_BUILTIN_CHANNEL_IDS {
            store.upsert_channel(
                id,
                "webhook",
                id,
                id,
                "outbound",
                "configured",
                "legacy adapter",
                "{}",
                false,
                true,
            )?;
        }
        store.upsert_channel(
            "my-legacy-custom",
            "webhook",
            "my custom channel",
            "Custom webhook",
            "outbound",
            "configured",
            "custom data should remain",
            "{}",
            true,
            false,
        )?;
        seed_builtin_channels(
            &store,
            SeedConfig {
                ntfy_configured: false,
            },
        )?;

        let rows = store.list_channels()?;
        let kinds: Vec<String> = rows.iter().map(|row| row.kind.clone()).collect();
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();

        assert!(ids.contains(&NTFY_DEFAULT_ID.to_string()));
        assert!(ids.contains(&WEBHOOK_SUBSTRATE_ID.to_string()));
        assert!(ids.contains(&"my-legacy-custom".to_string()));
        assert!(!ids.contains(&"sms-twilio".to_string()));
        assert!(!ids.contains(&"discord".to_string()));
        assert!(!ids.contains(&"slack".to_string()));
        assert!(!ids.contains(&"email-relay".to_string()));
        assert!(
            kinds.iter().all(|kind| is_implemented_channel_kind(kind)),
            "expected only real built-in channel kinds, got: {kinds:?}"
        );
        Ok(())
    }

    #[test]
    fn supports_only_implemented_channel_kinds() {
        assert!(is_implemented_channel_kind("ntfy"));
        assert!(is_implemented_channel_kind("webhook"));
        assert!(!is_implemented_channel_kind("email"));
        assert!(!is_implemented_channel_kind("slack"));
    }

    #[test]
    fn seed_builtin_channels_marks_ntfy_live_only_when_configured() -> Result<()> {
        let store = Store::open_in_memory()?;
        seed_builtin_channels(
            &store,
            SeedConfig {
                ntfy_configured: true,
            },
        )?;

        let rows = store.list_channels()?;
        let ntfy = rows
            .into_iter()
            .find(|row| row.id == NTFY_DEFAULT_ID)
            .expect("ntfy channel seeded");
        assert!(ntfy.live);
        assert_eq!(ntfy.status, "live");

        Ok(())
    }

    #[test]
    fn descriptor_from_row_rejects_config_json_corruption() -> Result<()> {
        let store = Store::open_in_memory()?;
        let row = store.upsert_channel(
            "corrupt-config-channel",
            "ntfy",
            "corrupt config",
            "Corrupt",
            "duplex",
            "configured",
            "bad config",
            "{not-json",
            false,
            false,
        )?;

        let error = descriptor_from_row(&store, row)
            .expect_err("descriptor_from_row should reject malformed config_json");
        assert!(
            error
                .to_string()
                .contains("failed to decode config_json for channel 'corrupt-config-channel'"),
            "unexpected error: {error}"
        );
        Ok(())
    }

    #[test]
    fn descriptor_from_row_rejects_corrupted_message_count() -> Result<()> {
        let workdir = tempdir()?;
        let path = workdir.path().join("asylum.sqlite3");
        let store = Store::open(path.to_str().expect("utf-8 path")).expect("open store");
        let row = store.upsert_channel(
            "corrupt-count-channel",
            "ntfy",
            "corrupt count",
            "Corrupt",
            "duplex",
            "configured",
            "count will fail",
            "{}",
            false,
            false,
        )?;

        let connection = Connection::open(path)?;
        connection.execute_batch("DROP TABLE channel_messages;")?;

        let error = descriptor_from_row(&store, row)
            .expect_err("descriptor_from_row should reject message_count query failures");
        assert!(
            error
                .to_string()
                .contains("no such table: channel_messages"),
            "unexpected error: {error}"
        );
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn seed_one(
    store: &Store,
    id: &str,
    kind: &str,
    name: &str,
    label: &str,
    direction: &str,
    status: &str,
    detail: &str,
    config: serde_json::Value,
    live: bool,
) -> Result<()> {
    if store.get_channel(id)?.is_some() {
        return Ok(());
    }
    store.upsert_channel(
        id,
        kind,
        name,
        label,
        direction,
        status,
        detail,
        &config.to_string(),
        live,
        true,
    )?;
    Ok(())
}

pub fn render_template(template: &str, vars: &serde_json::Value) -> String {
    let mut out = template.to_string();
    if let Some(map) = vars.as_object() {
        for (key, value) in map {
            let needle = format!("{{{}}}", key);
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out = out.replace(&needle, &replacement);
        }
    }
    out
}

pub fn require_channel(store: &Store, id: &str) -> Result<ChannelRow> {
    store
        .get_channel(id)?
        .ok_or_else(|| anyhow!("channel '{id}' not found"))
}
