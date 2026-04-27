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

        let response = match serde_json::from_str::<RpcRequest>(trimmed) {
            Ok(request) => handle_request(&client, request).await,
            Err(err) => RpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError {
                    code: -32700,
                    message: format!("invalid JSON-RPC request: {err}"),
                    data: None,
                }),
            },
        };

        let response = serde_json::to_string(&response)?;
        writer.write_all(response.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }

    Ok(())
}

fn tool_definitions() -> Vec<ToolSpec> {
    vec![
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
            description: "Interrupt a node",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "node.stop",
            description: "Stop a node",
            input_schema: json!({
                "type":"object",
                "properties":{"node_id":{"type":"string"}},
                "required":["node_id"]
            }),
        },
        ToolSpec {
            name: "graph.get",
            description: "Fetch current graph",
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
    ]
}

async fn handle_request(client: &AsylumClient, request: RpcRequest) -> RpcResponse {
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
        _ => RpcResponse {
            jsonrpc: "2.0",
            id: Value::Null,
            result: None,
            error: Some(RpcError {
                code: -32601,
                message: format!("method not found: {}", request.method),
                data: None,
            }),
        },
    };
    response.id = request_id;
    response
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
        "graph.get" => match client.graph().await {
            Ok(response) => content_result(json!({ "graph": response.graph })),
            Err(err) => rpc_error(-32000, &format!("graph.get failed: {err}")),
        },
        "attach_url.issue" => {
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
    }

    #[test]
    fn parse_node_id_works() {
        let parsed = parse_node_id(&json!({"node_id": "00000000-0000-0000-0000-000000000001"}));
        assert!(parsed.is_ok());
    }
}
