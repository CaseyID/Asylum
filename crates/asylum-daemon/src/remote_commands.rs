use anyhow::{anyhow, Result};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteCommandKind {
    Status,
    Attach,
    SendInput,
    Start,
    Interrupt,
    Stop,
    ApproveDecision,
    DenyDecision,
}

impl RemoteCommandKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteCommandKind::Status => "status",
            RemoteCommandKind::Attach => "attach",
            RemoteCommandKind::SendInput => "send",
            RemoteCommandKind::Start => "start",
            RemoteCommandKind::Interrupt => "interrupt",
            RemoteCommandKind::Stop => "stop",
            RemoteCommandKind::ApproveDecision => "approve",
            RemoteCommandKind::DenyDecision => "deny",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ParsedRemoteCommand {
    pub kind: RemoteCommandKind,
    pub token: String,
    pub node_id: Option<Uuid>,
    pub args: std::collections::HashMap<String, String>,
}

pub fn parse_remote_command(raw: &str) -> Result<ParsedRemoteCommand> {
    let tokens = raw.split_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(anyhow!("empty command"));
    }
    let mut args = std::collections::HashMap::new();
    let command = tokens[0];
    let mut token = None::<String>;

    for item in tokens.iter().skip(1) {
        if let Some((key, value)) = item.split_once('=') {
            if key == "token" {
                token = Some(value.to_string());
            }
            args.insert(key.to_string(), value.to_string());
        }
    }

    let token = token.ok_or_else(|| anyhow!("token required"))?;
    let parsed = match command {
        "status" => ParsedRemoteCommand {
            kind: RemoteCommandKind::Status,
            token,
            node_id: None,
            args,
        },
        "attach" => ParsedRemoteCommand {
            kind: RemoteCommandKind::Attach,
            token,
            node_id: args
                .get("node")
                .map(|value| Uuid::parse_str(value))
                .transpose()?,
            args,
        },
        "send" => ParsedRemoteCommand {
            kind: RemoteCommandKind::SendInput,
            token,
            node_id: args
                .get("node")
                .map(|value| Uuid::parse_str(value))
                .transpose()?,
            args,
        },
        "start" => ParsedRemoteCommand {
            kind: RemoteCommandKind::Start,
            token,
            node_id: None,
            args,
        },
        "interrupt" => ParsedRemoteCommand {
            kind: RemoteCommandKind::Interrupt,
            token,
            node_id: args
                .get("node")
                .map(|value| Uuid::parse_str(value))
                .transpose()?,
            args,
        },
        "stop" => ParsedRemoteCommand {
            kind: RemoteCommandKind::Stop,
            token,
            node_id: args
                .get("node")
                .map(|value| Uuid::parse_str(value))
                .transpose()?,
            args,
        },
        "approve" => ParsedRemoteCommand {
            kind: RemoteCommandKind::ApproveDecision,
            token,
            node_id: None,
            args,
        },
        "deny" => ParsedRemoteCommand {
            kind: RemoteCommandKind::DenyDecision,
            token,
            node_id: None,
            args,
        },
        _ => return Err(anyhow!("unsupported command")),
    };
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_retry_command() {
        let result =
            parse_remote_command("retry token=abc node=00000000-0000-0000-0000-000000000000");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unsupported command"));
    }

    #[test]
    fn parses_status_and_send_input_commands() {
        assert_eq!(
            parse_remote_command("status token=abc").unwrap().kind,
            RemoteCommandKind::Status
        );

        let parsed = parse_remote_command(
            "send node=00000000-0000-0000-0000-000000000000 token=abc text=hello",
        )
        .unwrap();
        assert_eq!(parsed.kind, RemoteCommandKind::SendInput);
        assert_eq!(parsed.args["text"], "hello");
    }
}
