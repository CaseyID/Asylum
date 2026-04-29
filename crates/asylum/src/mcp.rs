use std::io::{self, BufRead, BufReader, Write};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::client::AsylumClient;
use asylum_core::api::CreateNodeRequest;

#[derive(Deserialize)]
struct RpcRequest {
    _jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Serialize)]
struct ToolSpec {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
}

#[derive(Deserialize)]
struct ToolsCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

pub async fn run_stdio_server(client: Arc<AsylumClient>) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = io::BufWriter::new(stdout.lock());

    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .context("read stdio line from stdin")?;
        if bytes == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // M7: JSON-RPC notifications (id=None) must not receive a response.
        let maybe_response = match serde_json::from_str::<RpcRequest>(trimmed) {
            Ok(request) => handle_request(&client, request).await,
            Err(err) => Some(RpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError {
                    code: -32700,
                    message: format!("invalid JSON-RPC request: {err}"),
                    data: None,
                }),
            }),
        };

        if let Some(response) = maybe_response {
            let response = serde_json::to_string(&response)?;
            writer.write_all(response.as_bytes())?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }

    Ok(())
}

fn tool_definitions() -> Vec<ToolSpec> {
    vec![
        // — node lifecycle —
        ToolSpec {
            name: "node.create",
            description: "Create a new node",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "harness": {"type":"string"},
                    "substrate": {"type":"string"},
                    "role_hint": {"type":"string"},
                    "workspace": {"type":"string"},
                    "description": {"type":"string"},
                    "created_by": {"type":"string"},
                },
                "required":["harness","substrate"]
            }),
        },
        ToolSpec {
            name: "node.list",
            description: "List nodes",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "node.inspect",
            description: "Inspect a node",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "node.send_input",
            description: "Send input text to a node",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "node_id":{"type":"string"},
                    "text":{"type":"string"},
                },
                "required":["node_id","text"]
            }),
        },
        ToolSpec {
            name: "node.interrupt",
            description: "Interrupt a node (send SIGINT)",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "node.stop",
            description: "Stop a node gracefully",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "node.archive",
            description: "Archive a node and export its transcript",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "node.events",
            description: "List events for a node",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "node.fork",
            description: "Fork a node into a new node inheriting harness/substrate/workspace",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "node_id":{"type":"string"},
                    "role_hint":{"type":"string"},
                    "workspace":{"type":"string"},
                    "description":{"type":"string"},
                },
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "node.attach_url",
            description: "Issue browser attach URL for a node (alias for attach_url.issue)",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        // — graph —
        ToolSpec {
            name: "graph.get",
            description: "Fetch current node graph (nodes + relationships)",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "relationship.create",
            description: "Create a relationship between two nodes",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "source_node_id":{"type":"string"},
                    "target_node_id":{"type":"string"},
                    "kind":{"type":"string","description":"e.g. spawned_for, parent_of"},
                    "label":{"type":"string"},
                },
                "required":["source_node_id","target_node_id","kind"]
            }),
        },
        ToolSpec {
            name: "relationship.list",
            description: "List all relationships in the graph",
            input_schema: json!({"type":"object","properties":{}}),
        },
        // — hooks —
        ToolSpec {
            name: "hook.list",
            description: "List all automation hooks",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "hook.create",
            description: "Create an automation hook",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "name":{"type":"string"},
                    "event":{"type":"string"},
                    "filter":{"type":"string"},
                    "actions":{"type":"array","items":{"type":"object"}},
                    "enabled":{"type":"boolean"},
                },
                "required":["name","event","actions"]
            }),
        },
        ToolSpec {
            name: "hook.delete",
            description: "Delete an automation hook by id",
            input_schema: json!({
                "type":"object",
                "properties":{"hook_id":{"type":"string"}},
                "required":["hook_id"]
            }),
        },
        ToolSpec {
            name: "hook.firings",
            description: "List recent hook firing records",
            input_schema: json!({"type":"object","properties":{}}),
        },
        // — channels —
        ToolSpec {
            name: "channel.list",
            description: "List notification channels",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "channel.send",
            description: "Send a notification via a channel",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "channel_id":{"type":"string"},
                    "title":{"type":"string"},
                    "body":{"type":"string"},
                },
                "required":["channel_id","title","body"]
            }),
        },
        // — system —
        ToolSpec {
            name: "health.get",
            description: "Check daemon health",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "attach_url.issue",
            description: "Issue browser attach URL for a node",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        // Skipped (out of scope for v1 MCP): token management (security-sensitive),
        // substrate/harness descriptors (static metadata), recipe spawning,
        // workspace operations, artifact refs, decision request/resolve.
    ]
}

/// M7: Returns None for JSON-RPC notifications (id is absent); caller must not send a response.
async fn handle_request(client: &AsylumClient, request: RpcRequest) -> Option<RpcResponse> {
    let is_notification = request.id.is_none();
    let request_id = request.id.unwrap_or(Value::Null);

    let mut response = match request.method.as_str() {
        "initialize" => RpcResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name":"asylum-mcp","version":"0.0.1"},
            })),
            error: None,
        },
        "notifications/initialized" | "notifications/cancelled" => {
            // well-known notification types — no response
            return None;
        }
        "tools/list" => RpcResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            result: Some(json!({
                "tools": tool_definitions().into_iter().map(|tool| {
                    json!({
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": tool.input_schema
                    })
                }).collect::<Vec<_>>()
            })),
            error: None,
        },
        "tools/call" => handle_tools_call(client, request.params).await,
        unknown => {
            if is_notification {
                // Unknown notification — log and discard; do not reply
                eprintln!("[mcp] received unknown JSON-RPC notification '{unknown}'; ignoring");
                return None;
            }
            RpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError {
                    code: -32601,
                    message: format!("method not found: {unknown}"),
                    data: None,
                }),
            }
        }
    };

    if is_notification {
        // Even if we built a response, do not send for notifications
        return None;
    }

    response.id = request_id;
    Some(response)
}

async fn handle_tools_call(client: &AsylumClient, params: Option<Value>) -> RpcResponse {
    let params = match params {
        Some(params) => params,
        None => {
            return rpc_error(-32602, "missing params");
        }
    };

    let params: ToolsCallParams = match serde_json::from_value(params) {
        Ok(params) => params,
        Err(err) => {
            return rpc_error(-32602, &format!("invalid params: {err}"));
        }
    };

    match params.name.as_str() {
        "node.create" => handle_node_create(client, params.arguments).await,
        "node.list" => match client.list_nodes().await {
            Ok(response) => content_result(json!({ "nodes": response.nodes })),
            Err(err) => rpc_error(-32000, &format!("node.list failed: {err}")),
        },
        "node.inspect" => {
            let node_id = match parse_node_id(&params.arguments) {
                Ok(node_id) => node_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.inspect_node(node_id).await {
                Ok(response) => content_result(json!({ "node": response.node })),
                Err(err) => rpc_error(-32000, &format!("node.inspect failed: {err}")),
            }
        }
        "node.send_input" => {
            #[derive(Deserialize)]
            struct SendInputArgs {
                node_id: String,
                text: String,
            }
            let args: SendInputArgs = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("node.send_input: invalid args: {err}"));
                }
            };
            let node_id = match parse_node_id_str(&args.node_id) {
                Ok(node_id) => node_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.send_input(node_id, args.text).await {
                Ok(()) => content_result(json!({"ok":true})),
                Err(err) => rpc_error(-32000, &format!("node.send_input failed: {err}")),
            }
        }
        "node.interrupt" => {
            let node_id = match parse_node_id(&params.arguments) {
                Ok(node_id) => node_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.interrupt_node(node_id).await {
                Ok(()) => content_result(json!({"ok":true})),
                Err(err) => rpc_error(-32000, &format!("node.interrupt failed: {err}")),
            }
        }
        "node.stop" => {
            let node_id = match parse_node_id(&params.arguments) {
                Ok(node_id) => node_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.stop_node(node_id).await {
                Ok(()) => content_result(json!({"ok":true})),
                Err(err) => rpc_error(-32000, &format!("node.stop failed: {err}")),
            }
        }
        "node.archive" => {
            let node_id = match parse_node_id(&params.arguments) {
                Ok(node_id) => node_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.archive_node(node_id).await {
                Ok(()) => content_result(json!({"ok":true})),
                Err(err) => rpc_error(-32000, &format!("node.archive failed: {err}")),
            }
        }
        "node.events" => {
            let node_id = match parse_node_id(&params.arguments) {
                Ok(node_id) => node_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.node_events(node_id).await {
                Ok(response) => content_result(json!({ "events": response.events })),
                Err(err) => rpc_error(-32000, &format!("node.events failed: {err}")),
            }
        }
        "node.fork" => {
            #[derive(Deserialize)]
            struct ForkArgs {
                node_id: String,
                role_hint: Option<String>,
                workspace: Option<String>,
                description: Option<String>,
            }
            let args: ForkArgs = match serde_json::from_value(params.arguments) {
                Ok(a) => a,
                Err(err) => return rpc_error(-32602, &format!("node.fork: invalid args: {err}")),
            };
            let node_id = match parse_node_id_str(&args.node_id) {
                Ok(id) => id,
                Err(err) => return rpc_error(-32602, &err),
            };
            let path = format!("/api/nodes/{node_id}/fork");
            let body = json!({
                "role_hint": args.role_hint,
                "workspace": args.workspace,
                "description": args.description,
            });
            match client
                .send_request_json::<Value, _>(reqwest::Method::POST, &path, Some(&body))
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("node.fork failed: {err}")),
            }
        }
        "node.attach_url" | "attach_url.issue" => {
            let node_id = match parse_node_id(&params.arguments) {
                Ok(node_id) => node_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.browser_attach_url(node_id).await {
                Ok(response) => content_result(
                    json!({"url": response.url, "expires_in_seconds": response.expires_in_seconds}),
                ),
                Err(err) => rpc_error(-32000, &format!("attach_url.issue failed: {err}")),
            }
        }
        "graph.get" => match client.graph().await {
            Ok(response) => content_result(json!({ "graph": response.graph })),
            Err(err) => rpc_error(-32000, &format!("graph.get failed: {err}")),
        },
        "relationship.create" => {
            #[derive(Deserialize)]
            struct RelArgs {
                source_node_id: String,
                target_node_id: String,
                kind: String,
                label: Option<String>,
            }
            let args: RelArgs = match serde_json::from_value(params.arguments) {
                Ok(a) => a,
                Err(err) => {
                    return rpc_error(-32602, &format!("relationship.create: invalid args: {err}"))
                }
            };
            let body = json!({
                "source_node_id": args.source_node_id,
                "target_node_id": args.target_node_id,
                "kind": args.kind,
                "label": args.label,
            });
            match client
                .send_request_json::<Value, _>(reqwest::Method::POST, "/api/relationships", Some(&body))
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("relationship.create failed: {err}")),
            }
        }
        "relationship.list" => {
            match client
                .send_request_json::<Value, ()>(reqwest::Method::GET, "/api/relationships", None)
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("relationship.list failed: {err}")),
            }
        }
        "hook.list" => {
            match client
                .send_request_json::<Value, ()>(reqwest::Method::GET, "/api/hooks", None)
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("hook.list failed: {err}")),
            }
        }
        "hook.create" => {
            match client
                .send_request_json::<Value, _>(
                    reqwest::Method::POST,
                    "/api/hooks",
                    Some(&params.arguments),
                )
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("hook.create failed: {err}")),
            }
        }
        "hook.delete" => {
            let hook_id = extract_arg_string(&params.arguments, "hook_id")
                .ok_or_else(|| "missing hook_id".to_string());
            match hook_id {
                Err(err) => rpc_error(-32602, &err),
                Ok(id) => {
                    let path = format!("/api/hooks/{id}");
                    match client
                        .send_request_no_content_pub(reqwest::Method::DELETE, &path, None::<&()>)
                        .await
                    {
                        Ok(()) => content_result(json!({"ok":true})),
                        Err(err) => rpc_error(-32000, &format!("hook.delete failed: {err}")),
                    }
                }
            }
        }
        "hook.firings" => {
            match client
                .send_request_json::<Value, ()>(reqwest::Method::GET, "/api/hooks/firings", None)
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("hook.firings failed: {err}")),
            }
        }
        "channel.list" => {
            match client
                .send_request_json::<Value, ()>(reqwest::Method::GET, "/api/channels", None)
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("channel.list failed: {err}")),
            }
        }
        "channel.send" => {
            match client
                .send_request_json::<Value, _>(
                    reqwest::Method::POST,
                    "/api/channels/inbound",
                    Some(&params.arguments),
                )
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("channel.send failed: {err}")),
            }
        }
        "health.get" => match client.health().await {
            Ok(response) => content_result(json!({ "status": response.status })),
            Err(err) => rpc_error(-32000, &format!("health.get failed: {err}")),
        },
        unknown => rpc_error(-32601, &format!("tool not found: {unknown}")),
    }
}

async fn handle_node_create(client: &AsylumClient, arguments: Value) -> RpcResponse {
    #[derive(Deserialize)]
    struct CreateArgs {
        harness: String,
        substrate: String,
        role_hint: Option<String>,
        workspace: Option<String>,
        description: Option<String>,
        created_by: Option<String>,
    }
    let args: CreateArgs = match serde_json::from_value(arguments) {
        Ok(args) => args,
        Err(err) => {
            return rpc_error(-32602, &format!("node.create: invalid args: {err}"));
        }
    };
    let request = CreateNodeRequest {
        harness: args.harness,
        substrate: args.substrate,
        role_hint: args.role_hint.unwrap_or_else(|| "worker".to_string()),
        workspace: args.workspace,
        description: args.description,
        created_by: args.created_by,
        launch_args: Vec::new(),
    };
    match client.create_node(request).await {
        Ok(response) => content_result(json!({ "node_id": response.node_id })),
        Err(err) => rpc_error(-32000, &format!("node.create failed: {err}")),
    }
}

fn parse_node_id(input: &Value) -> Result<Uuid, String> {
    let node_id = extract_arg_string(input, "node_id")
        .or_else(|| extract_arg_string(input, "id"))
        .ok_or_else(|| "missing node_id".to_string())?;
    parse_node_id_str(&node_id)
}

fn parse_node_id_str(node_id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(node_id).map_err(|err| format!("invalid node_id: {err}"))
}

fn extract_arg_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn content_result(payload: Value) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id: Value::Null,
        result: Some(json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&payload)
                    .unwrap_or_else(|_| payload.to_string())
            }]
        })),
        error: None,
    }
}

fn rpc_error(code: i32, message: &str) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id: Value::Null,
        result: None,
        error: Some(RpcError {
            code,
            message: message.to_string(),
            data: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_include_expected_names() {
        let names = tool_definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"node.create"));
        assert!(names.contains(&"graph.get"));
        assert!(names.contains(&"attach_url.issue"));
        assert!(names.contains(&"node.fork"));
        assert!(names.contains(&"hook.list"));
        assert!(names.contains(&"channel.list"));
        assert!(names.contains(&"relationship.create"));
        assert!(names.contains(&"health.get"));
    }

    #[test]
    fn parse_node_id_works() {
        let parsed = parse_node_id(&json!({"node_id": "00000000-0000-0000-0000-000000000001"}));
        assert!(parsed.is_ok());
    }

    #[tokio::test]
    async fn notification_returns_none() {
        use std::sync::Arc;
        let client = Arc::new(AsylumClient::new("http://127.0.0.1:1", Option::<String>::None));
        // notifications/initialized has no id — must return None
        let req = RpcRequest {
            _jsonrpc: Some("2.0".to_string()),
            id: None,
            method: "notifications/initialized".to_string(),
            params: None,
        };
        let result = handle_request(&client, req).await;
        assert!(result.is_none(), "notification must not produce a response");
    }

    #[tokio::test]
    async fn unknown_notification_returns_none() {
        use std::sync::Arc;
        let client = Arc::new(AsylumClient::new("http://127.0.0.1:1", Option::<String>::None));
        let req = RpcRequest {
            _jsonrpc: Some("2.0".to_string()),
            id: None,
            method: "unknown/method".to_string(),
            params: None,
        };
        let result = handle_request(&client, req).await;
        assert!(result.is_none(), "unknown notification must not produce a response");
    }
}
