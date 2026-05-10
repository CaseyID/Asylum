pub fn launch_packet_markdown(
    node_id: &str,
    base_url: &str,
    role_hint: &str,
    harness: &str,
    substrate: &str,
    capabilities: &[(&str, bool)],
    graph_summary: &str,
) -> String {
    let mut capabilities_text = String::new();
    for (name, enabled) in capabilities {
        capabilities_text.push_str(&format!("- {name}: {enabled}\n"));
    }
    format!(
        r#"# Launch Packet

Base URL: `{base_url}`
Node: `{node_id}`
Role Hint: `{role_hint}`
Harness/Substrate: `{harness}/{substrate}`

## Available Capabilities
{capabilities_text}

## Current Graph
{graph_summary}
"#,
        base_url = base_url,
        node_id = node_id,
        role_hint = role_hint,
        harness = harness,
        substrate = substrate,
        capabilities_text = capabilities_text,
        graph_summary = graph_summary,
    )
}
