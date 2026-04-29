use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Recipe {
    pub id: &'static str,
    pub title: &'static str,
    pub prompt_template: &'static str,
}

pub fn starter_recipes() -> Vec<Recipe> {
    vec![
        Recipe {
            id: "start-command-center",
            title: "Start Command Center",
            prompt_template: "Start a command-center node. Use role hints as guidance, not state-machine.
Call node.create, node.observe, and relationship.create as needed.",
        },
        Recipe {
            id: "spawn-worker-nodes",
            title: "Spawn Worker Nodes",
            prompt_template: "Spawn explicit worker nodes for the active graph.
Use node.create and relationship.create; do not invent inferred relationships.",
        },
        Recipe {
            id: "observe-and-summarize-system",
            title: "Observe System",
            prompt_template: "Observe all command nodes, summarize status and capability deltas,
and post findings as notifications.",
        },
        Recipe {
            id: "run-plan-to-completion",
            title: "Run Plan To Completion",
            prompt_template: "Pick a concrete end goal and execute a chain of node interactions until completion.
Keep role hints as context only.",
        },
        Recipe {
            id: "checkpoint-or-handoff-node",
            title: "Checkpoint Or Handoff",
            prompt_template: "Capture state, persist launch packet, then handoff to a new role-hinted node.",
        },
        Recipe {
            id: "parallel-exploration",
            title: "Parallel Exploration",
            prompt_template: "Launch parallel worker nodes for orthogonal investigation threads and stitch results.",
        },
    ]
}

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
    let mut recipes = String::new();
    for recipe in starter_recipes() {
        recipes.push_str(&format!("- `{}`\n", recipe.id));
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

## Starter Recipes
{recipes}
"#,
        base_url = base_url,
        node_id = node_id,
        role_hint = role_hint,
        harness = harness,
        substrate = substrate,
        capabilities_text = capabilities_text,
        graph_summary = graph_summary,
        recipes = recipes,
    )
}

pub fn starter_recipe_map() -> BTreeMap<&'static str, &'static str> {
    starter_recipes()
        .into_iter()
        .map(|recipe| (recipe.id, recipe.prompt_template))
        .collect()
}
