use anyhow::{anyhow, Result};
use asylum_core::api::{ChannelDescriptor, ChannelMessageRecord};

use crate::storage::{ChannelMessageRow, ChannelRow, Store};

pub const NTFY_DEFAULT_ID: &str = "ntfy-default";
pub const WEBHOOK_SUBSTRATE_ID: &str = "webhook-substrate";

pub fn descriptor_from_row(store: &Store, row: ChannelRow) -> Result<ChannelDescriptor> {
    let count = store.count_channel_messages_24h(&row.id).unwrap_or(0);
    let config = serde_json::from_str(&row.config_json).unwrap_or(serde_json::Value::Null);
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
    }
}

pub struct SeedConfig {
    pub ntfy_configured: bool,
}

pub fn seed_builtin_channels(store: &Store, seed: SeedConfig) -> Result<()> {
    let ntfy_status = if seed.ntfy_configured { "live" } else { "configured" };
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
    seed_one(
        store,
        "sms-twilio",
        "sms",
        "SMS (Twilio)",
        "Twilio SMS",
        "duplex",
        "future",
        "Future stub for Twilio SMS bridge",
        serde_json::json!({}),
        false,
    )?;
    seed_one(
        store,
        "discord",
        "discord",
        "Discord",
        "Discord channel",
        "duplex",
        "future",
        "Future stub for Discord bridge",
        serde_json::json!({}),
        false,
    )?;
    seed_one(
        store,
        "slack",
        "slack",
        "Slack",
        "Slack channel",
        "duplex",
        "future",
        "Future stub for Slack bridge",
        serde_json::json!({}),
        false,
    )?;
    seed_one(
        store,
        "email-relay",
        "email",
        "Email relay",
        "Email relay",
        "outbound",
        "future",
        "Future stub for transactional email relay",
        serde_json::json!({}),
        false,
    )?;
    Ok(())
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
