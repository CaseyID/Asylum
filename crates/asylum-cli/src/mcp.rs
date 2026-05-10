use std::io::{self, BufRead, BufReader, Write};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::client::AsylumClient;
use asylum_types::api::{
    ChannelCreateRequest, ChannelInboundRequest, ChannelTestRequest, ChannelUpdateRequest,
    CreateNodeRequest,
};

#[derive(Deserialize)]
struct RpcRequest {
    #[serde(rename = "jsonrpc")]
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
    let mut tools = vec![
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
        ToolSpec {
            name: "relationship.remove",
            description: "Delete an explicit graph relationship by id",
            input_schema: json!({
                "type":"object",
                "properties":{"relationship_id":{"type":"string"}},
                "required":["relationship_id"]
            }),
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
        // — channels / notifications —
        ToolSpec {
            name: "channel.list",
            description: "List notification channels",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "channel.inspect",
            description: "Inspect a notification channel",
            input_schema: json!({
                "type":"object",
                "properties":{"channel_id":{"type":"string"}},
                "required":["channel_id"]
            }),
        },
        ToolSpec {
            name: "channel.create",
            description: "Create a notification channel",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "kind":{"type":"string"},
                    "name":{"type":"string"},
                    "label":{"type":"string"},
                    "direction":{"type":"string"},
                    "detail":{"type":"string"},
                    "config":{"type":"object"},
                    "live":{"type":"boolean"},
                },
                "required":["kind","name","direction"]
            }),
        },
        ToolSpec {
            name: "channel.update",
            description: "Update a notification channel",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "channel_id":{"type":"string"},
                    "name":{"type":"string"},
                    "label":{"type":"string"},
                    "detail":{"type":"string"},
                    "direction":{"type":"string"},
                    "status":{"type":"string"},
                    "config":{"type":"object"},
                    "live":{"type":"boolean"},
                },
                "required":["channel_id"]
            }),
        },
        ToolSpec {
            name: "channel.delete",
            description: "Delete a notification channel",
            input_schema: json!({
                "type":"object",
                "properties":{"channel_id":{"type":"string"}},
                "required":["channel_id"]
            }),
        },
        ToolSpec {
            name: "channel.messages",
            description: "List messages for a notification channel",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "channel_id":{"type":"string"},
                    "limit":{"type":"integer","minimum":1},
                },
                "required":["channel_id"]
            }),
        },
        ToolSpec {
            name: "channel.test",
            description: "Send a test message through a channel",
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
        ToolSpec {
            name: "channel.inbound",
            description: "Ingest an inbound message through a channel",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "channel_id":{"type":"string"},
                    "sender":{"type":"string"},
                    "subject":{"type":"string"},
                    "body":{"type":"string"},
                    "replies":{"type":"array","items":{"type":"string"}},
                    "node_id":{"type":"string"},
                    "correlation_token":{"type":"string"},
                },
                "required":["channel_id","sender","subject","body"]
            }),
        },
        ToolSpec {
            name: "notify.send",
            description: "Send a notification through the daemon notification capability",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "topic":{"type":"string"},
                    "title":{"type":"string"},
                    "body":{"type":"string"},
                },
                "required":["title","body"]
            }),
        },
        ToolSpec {
            name: "notify.list",
            description: "List recent notifications",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "notify.read",
            description: "Mark one notification as read",
            input_schema: json!({
                "type":"object",
                "properties":{"id":{"type":"integer"}},
                "required":["id"]
            }),
        },
        ToolSpec {
            name: "workspace.recent",
            description: "List recently used workspaces",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "context.system_map",
            description: "Fetch the current context system map",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "context.launch_packet",
            description: "Generate a launch packet for a node",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "recipe.list",
            description: "List configured launch recipes",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "remote_command.send",
            description: "Execute a remote command against the daemon",
            input_schema: json!({
                "type":"object",
                "properties":{"command":{"type":"string"}},
                "required":["command"]
            }),
        },
        ToolSpec {
            name: "decision.create",
            description: "Create an operator decision request",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "text":{"type":"string"},
                    "node_id":{"type":"string"},
                },
                "required":["text"]
            }),
        },
        ToolSpec {
            name: "decision.list",
            description: "List operator decisions",
            input_schema: json!({"type":"object","properties":{}}),
        },
        ToolSpec {
            name: "decision.inspect",
            description: "Inspect one operator decision",
            input_schema: json!({
                "type":"object",
                "properties":{"decision_id":{"type":"string"}},
                "required":["decision_id"]
            }),
        },
        ToolSpec {
            name: "decision.resolve",
            description: "Resolve an operator decision",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "decision_id":{"type":"string"},
                    "status":{"type":"string","enum":["approved","denied"]},
                },
                "required":["decision_id","status"]
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
        // substrate/harness descriptors (static metadata), artifact refs.
    ];
    if recipe_spawn_is_enabled() {
        tools.push(ToolSpec {
            name: "recipe.spawn",
            description: "Spawn nodes from a configured recipe",
            input_schema: json!({
                "type":"object",
                "properties":{
                    "recipe_id":{"type":"string"},
                    "harness":{"type":"string"},
                    "substrate":{"type":"string"},
                    "workspace":{"type":"string"},
                    "description":{"type":"string"},
                    "role_hint":{"type":"string"},
                },
                "required":["recipe_id","harness","substrate"]
            }),
        });
    }
    tools
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
                Ok(response) => content_result(json!({
                    "url": response.url,
                    "expires_in_seconds": response.expires_in_seconds,
                    "transport": response.transport,
                    "note": response.note,
                })),
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
                .send_request_json::<Value, _>(
                    reqwest::Method::POST,
                    "/api/relationships",
                    Some(&body),
                )
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
        "relationship.remove" => {
            let relationship_id = extract_arg_string(&params.arguments, "relationship_id")
                .or_else(|| extract_arg_string(&params.arguments, "id"))
                .ok_or_else(|| "missing relationship_id".to_string());
            match relationship_id {
                Err(err) => rpc_error(-32602, &err),
                Ok(id) => {
                    let path = format!("/api/relationships/{id}");
                    match client
                        .send_request_no_content_pub(reqwest::Method::DELETE, &path, None::<&()>)
                        .await
                    {
                        Ok(()) => content_result(json!({"ok":true})),
                        Err(err) => {
                            rpc_error(-32000, &format!("relationship.remove failed: {err}"))
                        }
                    }
                }
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
        "channel.inspect" => {
            let channel_id = match parse_channel_id(&params.arguments) {
                Ok(channel_id) => channel_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.inspect_channel(&channel_id).await {
                Ok(response) => content_result(json!({ "channel": response })),
                Err(err) => rpc_error(-32000, &format!("channel.inspect failed: {err}")),
            }
        }
        "channel.create" => {
            let args: ChannelCreateRequest = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("channel.create: invalid args: {err}"));
                }
            };
            match client.create_channel(args).await {
                Ok(response) => content_result(json!({ "channel": response })),
                Err(err) => rpc_error(-32000, &format!("channel.create failed: {err}")),
            }
        }
        "channel.update" => {
            #[derive(Deserialize)]
            struct ChannelUpdateArgs {
                channel_id: String,
                name: Option<String>,
                label: Option<String>,
                detail: Option<String>,
                direction: Option<String>,
                status: Option<String>,
                #[serde(default)]
                config: Option<Value>,
                live: Option<bool>,
            }

            let ChannelUpdateArgs {
                channel_id,
                name,
                label,
                detail,
                direction,
                status,
                config,
                live,
            } = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("channel.update: invalid args: {err}"));
                }
            };
            let request = ChannelUpdateRequest {
                name,
                label,
                detail,
                direction,
                status,
                config,
                live,
            };
            match client.update_channel(&channel_id, request).await {
                Ok(response) => content_result(json!({ "channel": response })),
                Err(err) => rpc_error(-32000, &format!("channel.update failed: {err}")),
            }
        }
        "channel.delete" => {
            let channel_id = match parse_channel_id(&params.arguments) {
                Ok(channel_id) => channel_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.delete_channel(&channel_id).await {
                Ok(()) => content_result(json!({"ok":true})),
                Err(err) => rpc_error(-32000, &format!("channel.delete failed: {err}")),
            }
        }
        "channel.messages" => {
            #[derive(Deserialize)]
            struct ChannelMessagesArgs {
                channel_id: String,
                limit: Option<u32>,
            }
            let args: ChannelMessagesArgs = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("channel.messages: invalid args: {err}"));
                }
            };
            match client.channel_messages(&args.channel_id, args.limit).await {
                Ok(response) => content_result(json!({ "messages": response.messages })),
                Err(err) => rpc_error(-32000, &format!("channel.messages failed: {err}")),
            }
        }
        "channel.test" => {
            #[derive(Deserialize)]
            struct ChannelTestArgs {
                channel_id: String,
                title: String,
                body: String,
            }
            let args: ChannelTestArgs = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("channel.test: invalid args: {err}"));
                }
            };
            let request = ChannelTestRequest {
                title: args.title,
                body: args.body,
            };
            match client.test_channel(&args.channel_id, request).await {
                Ok(response) => content_result(json!({ "sent": response.sent })),
                Err(err) => rpc_error(-32000, &format!("channel.test failed: {err}")),
            }
        }
        "channel.inbound" => {
            #[derive(Deserialize)]
            struct ChannelInboundArgs {
                channel_id: String,
                sender: String,
                subject: String,
                body: String,
                #[serde(default)]
                replies: Vec<String>,
                node_id: Option<String>,
                correlation_token: Option<String>,
            }
            let args: ChannelInboundArgs = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("channel.inbound: invalid args: {err}"));
                }
            };
            let request = ChannelInboundRequest {
                sender: args.sender,
                subject: args.subject,
                body: args.body,
                replies: args.replies,
                node_id: args.node_id,
                correlation_token: args.correlation_token,
            };
            match client.inbound_channel(&args.channel_id, request).await {
                Ok(()) => content_result(json!({"ok":true})),
                Err(err) => rpc_error(-32000, &format!("channel.inbound failed: {err}")),
            }
        }
        "notify.send" => {
            match client
                .send_request_json::<Value, _>(
                    reqwest::Method::POST,
                    "/api/notify/send",
                    Some(&params.arguments),
                )
                .await
            {
                Ok(v) => content_result(v),
                Err(err) => rpc_error(-32000, &format!("notify.send failed: {err}")),
            }
        }
        "notify.list" => match client.list_notifications().await {
            Ok(response) => content_result(json!({ "notifications": response.notifications })),
            Err(err) => rpc_error(-32000, &format!("notify.list failed: {err}")),
        },
        "notify.read" => {
            let notification_id = extract_arg_i64(&params.arguments, "id")
                .or_else(|| extract_arg_i64(&params.arguments, "notification_id"))
                .ok_or_else(|| "missing id".to_string());
            match notification_id {
                Err(err) => rpc_error(-32602, &err),
                Ok(id) => match client.mark_notification_read(id).await {
                    Ok(()) => content_result(json!({"ok":true})),
                    Err(err) => rpc_error(-32000, &format!("notify.read failed: {err}")),
                },
            }
        }
        "workspace.recent" => match client.recent_workspaces().await {
            Ok(response) => content_result(json!({ "workspaces": response })),
            Err(err) => rpc_error(-32000, &format!("workspace.recent failed: {err}")),
        },
        "context.system_map" => match client.system_map().await {
            Ok(response) => content_result(json!({ "system_map": response.graph })),
            Err(err) => rpc_error(-32000, &format!("context.system_map failed: {err}")),
        },
        "context.launch_packet" => {
            let node_id = match parse_node_id(&params.arguments) {
                Ok(node_id) => node_id,
                Err(err) => return rpc_error(-32602, &err),
            };
            match client.launch_packet(node_id).await {
                Ok(response) => content_result(json!({
                    "markdown": response.markdown,
                    "artifact_id": response.artifact_id
                })),
                Err(err) => rpc_error(-32000, &format!("context.launch_packet failed: {err}")),
            }
        }
        "recipe.list" => match client.list_recipes().await {
            Ok(response) => content_result(json!({ "recipes": response.recipes })),
            Err(err) => rpc_error(-32000, &format!("recipe.list failed: {err}")),
        },
        "recipe.spawn" => {
            if !recipe_spawn_is_enabled() {
                return rpc_error(-32601, "recipe.spawn is not supported");
            }
            #[derive(Deserialize)]
            struct SpawnArgs {
                recipe_id: String,
                harness: String,
                substrate: String,
                workspace: Option<String>,
                description: Option<String>,
                role_hint: Option<String>,
            }
            let args: SpawnArgs = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("recipe.spawn: invalid args: {err}"));
                }
            };
            let request = asylum_types::api::RecipeSpawnRequest {
                harness: args.harness,
                substrate: args.substrate,
                workspace: args.workspace,
                description: args.description,
                role_hint: args.role_hint,
            };
            match client.spawn_recipe(&args.recipe_id, request).await {
                Ok(response) => content_result(json!({ "node_ids": response.node_ids })),
                Err(err) => rpc_error(-32000, &format!("recipe.spawn failed: {err}")),
            }
        }
        "remote_command.send" => {
            #[derive(Deserialize)]
            struct RemoteCommandArgs {
                command: String,
            }
            let args: RemoteCommandArgs = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("remote_command.send: invalid args: {err}"));
                }
            };
            let request = asylum_types::api::RemoteCommandRequest {
                command: args.command,
            };
            match client.send_remote_command(request).await {
                Ok(response) => content_result(json!({
                    "kind": response.kind,
                    "status": response.status,
                    "node_id": response.node_id,
                    "result": response.result,
                })),
                Err(err) => rpc_error(-32000, &format!("remote_command.send failed: {err}")),
            }
        }
        "decision.create" => {
            #[derive(Deserialize)]
            struct DecisionCreateArgs {
                text: String,
                node_id: Option<String>,
            }
            let args: DecisionCreateArgs = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("decision.create: invalid args: {err}"));
                }
            };
            let request = asylum_types::api::DecisionCreateRequest {
                text: args.text,
                node_id: args.node_id,
            };
            match client.create_decision(request).await {
                Ok(decision) => content_result(json!({ "decision": decision })),
                Err(err) => rpc_error(-32000, &format!("decision.create failed: {err}")),
            }
        }
        "decision.list" => match client.list_decisions().await {
            Ok(response) => content_result(json!({ "decisions": response.decisions })),
            Err(err) => rpc_error(-32000, &format!("decision.list failed: {err}")),
        },
        "decision.inspect" => {
            let decision_id = extract_arg_string(&params.arguments, "decision_id")
                .or_else(|| extract_arg_string(&params.arguments, "id"))
                .ok_or_else(|| "missing decision_id".to_string());
            match decision_id {
                Err(err) => rpc_error(-32602, &err),
                Ok(id) => match client.get_decision(&id).await {
                    Ok(decision) => content_result(json!({ "decision": decision })),
                    Err(err) => rpc_error(-32000, &format!("decision.inspect failed: {err}")),
                },
            }
        }
        "decision.resolve" => {
            #[derive(Deserialize)]
            struct DecisionResolveArgs {
                decision_id: String,
                status: String,
            }
            let args: DecisionResolveArgs = match serde_json::from_value(params.arguments) {
                Ok(args) => args,
                Err(err) => {
                    return rpc_error(-32602, &format!("decision.resolve: invalid args: {err}"));
                }
            };
            let request = asylum_types::api::DecisionResolveRequest {
                status: args.status,
            };
            match client.resolve_decision(&args.decision_id, request).await {
                Ok(decision) => content_result(json!({ "decision": decision })),
                Err(err) => rpc_error(-32000, &format!("decision.resolve failed: {err}")),
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

fn parse_channel_id(input: &Value) -> Result<String, String> {
    extract_arg_string(input, "channel_id")
        .or_else(|| extract_arg_string(input, "id"))
        .ok_or_else(|| "missing channel_id".to_string())
}

fn extract_arg_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn extract_arg_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64).or_else(|| {
        value
            .get(key)
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<i64>().ok())
    })
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

fn recipe_spawn_is_enabled() -> bool {
    false
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
        assert!(names.contains(&"channel.inspect"));
        assert!(names.contains(&"channel.create"));
        assert!(names.contains(&"channel.update"));
        assert!(names.contains(&"channel.delete"));
        assert!(names.contains(&"channel.messages"));
        assert!(names.contains(&"channel.test"));
        assert!(names.contains(&"channel.inbound"));
        assert!(names.contains(&"channel.list"));
        assert!(names.contains(&"relationship.create"));
        assert!(names.contains(&"relationship.remove"));
        assert!(names.contains(&"health.get"));
        assert!(names.contains(&"notify.list"));
        assert!(names.contains(&"workspace.recent"));
        assert!(names.contains(&"context.system_map"));
        assert!(names.contains(&"context.launch_packet"));
        assert!(names.contains(&"recipe.list"));
        assert!(!names.contains(&"recipe.spawn"));
        assert!(names.contains(&"remote_command.send"));
        assert!(names.contains(&"decision.create"));
        assert!(names.contains(&"decision.list"));
        assert!(names.contains(&"decision.inspect"));
        assert!(names.contains(&"decision.resolve"));
    }

    #[tokio::test]
    async fn recipe_spawn_tool_call_fails_when_disabled() {
        use std::sync::Arc;
        let client = Arc::new(AsylumClient::new(
            "http://127.0.0.1:1",
            Option::<String>::None,
        ));
        let response = handle_tools_call(
            &client,
            Some(json!({
                "name": "recipe.spawn",
                "arguments": {
                    "recipe_id": "start-command-center",
                    "harness": "codex",
                    "substrate": "local",
                },
            })),
        )
        .await;

        assert!(response.error.is_some());
        let error = response.error.unwrap();
        assert_eq!(error.code, -32601);
        assert!(error.message.contains("not supported"));
    }

    #[test]
    fn parse_node_id_works() {
        let parsed = parse_node_id(&json!({"node_id": "00000000-0000-0000-0000-000000000001"}));
        assert!(parsed.is_ok());
    }

    #[tokio::test]
    async fn notification_returns_none() {
        use std::sync::Arc;
        let client = Arc::new(AsylumClient::new(
            "http://127.0.0.1:1",
            Option::<String>::None,
        ));
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
        let client = Arc::new(AsylumClient::new(
            "http://127.0.0.1:1",
            Option::<String>::None,
        ));
        let req = RpcRequest {
            _jsonrpc: Some("2.0".to_string()),
            id: None,
            method: "unknown/method".to_string(),
            params: None,
        };
        let result = handle_request(&client, req).await;
        assert!(
            result.is_none(),
            "unknown notification must not produce a response"
        );
    }
}
