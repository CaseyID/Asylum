use anyhow::Result;
use asylum_daemon::auth::issue_owner_token;
use asylum_daemon::auth::AuthMode;
use asylum_daemon::capability_service::{AppConfig, CapabilityService};
use asylum_daemon::storage::Store;
use asylum_types::api::{ChannelInboundRequest, DecisionCreateRequest};
use asylum_types::config::AsylumConfig;
use asylum_types::node::{CapabilitySnapshot, HarnessKind, SubstrateKind};

fn test_app_config() -> AppConfig {
    let core = AsylumConfig::default();
    AppConfig {
        base_url: core.base_url,
        bind_addr: "127.0.0.1:0".to_string(),
        socket_path: None,
        transcripts_dir: "/tmp/asylum-test-channel-remote-commands/transcripts".to_string(),
        workspace_recent_limit: core.workspace.recent_limit,
        ntfy_server: core.ntfy.server,
        ntfy_topic: core.ntfy.topic,
        ntfy_token: core.ntfy.token,
        ntfy_poll_interval_seconds: Some(core.ntfy.poll_interval_seconds),
        harness: core.harness,
        loon: core.loon,
        autonomy: core.autonomy,
    }
}

fn seed_owner_token(store: &Store) -> Result<String> {
    let issued = issue_owner_token("operator", &["*".to_string()], None)?;
    store.insert_token(
        issued.token_id,
        &issued.name,
        &issued.stored_hash,
        &serde_json::to_string(&issued.scope)?,
        issued.expires_at_epoch_secs,
    )?;
    Ok(issued.raw_token)
}

#[tokio::test]
async fn channel_inbound_status_command_with_valid_token_executes_and_records_inbound() -> Result<()>
{
    let store = Store::open_in_memory()?;
    let token = seed_owner_token(&store)?;
    let service = CapabilityService::new(
        store.clone(),
        AuthMode::OwnerToken {
            config_token_hash: None,
        },
        test_app_config(),
    );

    let command_body = format!("status token={token}");
    service
        .channel_inbound(
            "webhook-substrate",
            ChannelInboundRequest {
                sender: "ntfy:test".to_string(),
                subject: "ops".to_string(),
                body: command_body.clone(),
                replies: vec![],
                node_id: None,
                correlation_token: None,
            },
        )
        .await?;

    let messages = store.list_channel_messages("webhook-substrate", 20)?;
    assert!(
        messages
            .iter()
            .any(|message| message.direction == "in" && message.body == command_body),
        "expected status command envelope to be recorded as inbound channel message"
    );

    let notifications = store.list_notifications()?;
    assert!(
        notifications
            .iter()
            .any(|notification| notification.2 == "remote_command"
                && notification.3 == "Remote command received"
                && notification.4 == "status requested"
                && notification.1.is_none()),
        "expected authenticated status command to execute through channel inbound"
    );
    Ok(())
}

#[tokio::test]
async fn channel_inbound_without_token_stays_plain_and_does_not_fire_remote_command() -> Result<()>
{
    let store = Store::open_in_memory()?;
    let _ = seed_owner_token(&store)?;
    let service = CapabilityService::new(
        store.clone(),
        AuthMode::OwnerToken {
            config_token_hash: None,
        },
        test_app_config(),
    );

    let before = store.list_notifications()?;
    service
        .channel_inbound(
            "webhook-substrate",
            ChannelInboundRequest {
                sender: "manual".to_string(),
                subject: "ops".to_string(),
                body: "status".to_string(),
                replies: vec![],
                node_id: None,
                correlation_token: None,
            },
        )
        .await?;

    let messages = store.list_channel_messages("webhook-substrate", 20)?;
    assert!(
        messages
            .iter()
            .any(|message| message.direction == "in" && message.body == "status"),
        "expected inbound plain text to be recorded unchanged"
    );

    let after = store.list_notifications()?;
    assert_eq!(
        before.len(),
        after.len(),
        "expected no notification fanout for non-command inbound text"
    );
    Ok(())
}

#[tokio::test]
async fn inbound_approve_decision_command_resolves_decision() -> Result<()> {
    let store = Store::open_in_memory()?;
    let token = seed_owner_token(&store)?;
    let service = CapabilityService::new(
        store.clone(),
        AuthMode::OwnerToken {
            config_token_hash: None,
        },
        test_app_config(),
    );

    let node = store.insert_node(
        HarnessKind::Codex,
        SubstrateKind::Local,
        "worker",
        Some("/tmp"),
        Some("remote-command"),
        None,
        CapabilitySnapshot::default(),
        None,
    )?;
    let decision = service
        .create_decision(DecisionCreateRequest {
            node_id: Some(node.id.to_string()),
            text: "allow this action?".to_string(),
        })
        .await?;

    let command_body = format!("approve decision={} token={token}", decision.id);
    service
        .channel_inbound(
            "webhook-substrate",
            ChannelInboundRequest {
                sender: "ntfy:test".to_string(),
                subject: "approval".to_string(),
                body: command_body.clone(),
                replies: vec![],
                node_id: None,
                correlation_token: None,
            },
        )
        .await?;

    let updated = store
        .get_decision(&decision.id)?
        .expect("decision should exist after inbound command");
    assert_eq!(
        updated.3, "approved",
        "expected inbound approve command to resolve decision"
    );

    let messages = store.list_channel_messages("webhook-substrate", 20)?;
    assert!(
        messages
            .iter()
            .any(|message| message.direction == "in" && message.body == command_body),
        "expected inbound decision command to be recorded"
    );
    Ok(())
}

#[tokio::test]
async fn inbound_deny_decision_command_resolves_decision() -> Result<()> {
    let store = Store::open_in_memory()?;
    let token = seed_owner_token(&store)?;
    let service = CapabilityService::new(
        store.clone(),
        AuthMode::OwnerToken {
            config_token_hash: None,
        },
        test_app_config(),
    );

    let node = store.insert_node(
        HarnessKind::Codex,
        SubstrateKind::Local,
        "worker",
        Some("/tmp"),
        Some("remote-command"),
        None,
        CapabilitySnapshot::default(),
        None,
    )?;
    let decision = service
        .create_decision(DecisionCreateRequest {
            node_id: Some(node.id.to_string()),
            text: "allow this action?".to_string(),
        })
        .await?;

    let command_body = format!("deny decision={} token={token}", decision.id);
    service
        .channel_inbound(
            "webhook-substrate",
            ChannelInboundRequest {
                sender: "ntfy:test".to_string(),
                subject: "approval".to_string(),
                body: command_body.clone(),
                replies: vec![],
                node_id: None,
                correlation_token: None,
            },
        )
        .await?;

    let updated = store
        .get_decision(&decision.id)?
        .expect("decision should exist after inbound command");
    assert_eq!(
        updated.3, "denied",
        "expected inbound deny command to resolve decision"
    );

    let messages = store.list_channel_messages("webhook-substrate", 20)?;
    assert!(
        messages
            .iter()
            .any(|message| message.direction == "in" && message.body == command_body),
        "expected inbound decision command to be recorded"
    );
    Ok(())
}
