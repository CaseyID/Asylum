/// The supervisor operating manual, static across every node (no
/// interpolation): exact MCP tool names and parameters for running a fleet on
/// today's real surface. Every tool name and parameter named here must exist
/// verbatim in `crates/asylum-cli/src/mcp.rs` (`tool_definitions`) -- this file
/// is checked against that list in tests. Keep it factual and short; this is a
/// reference sheet a harness reads once, not prose.
const FLEET_OPERATING_MANUAL: &str = r#"## Running a fleet

You reach Asylum only through the injected MCP tools (`mcp__asylum__*`). The
names and parameters below are exact -- no other names or params exist.

### Spawning workers

- `node.spawn_peer(harness, substrate, role_hint?, workspace?, description?, prompt?, relationship_kind?, relationship_label?, model?, effort?)`
  creates a peer and records a graph edge from you to it (`relationship_kind`
  defaults to `spawned_for`). `harness` is `claude_code` or `codex`; `substrate`
  is `local` or `loon`. `node_id` (source) is optional -- it defaults to you
  (`ASYLUM_NODE_ID`) when omitted.
  - `description` is framing folded into the worker's opening message.
  - `prompt` is the worker's actual first instruction, delivered as its
    opening submitted turn. Set `prompt` -- not just `description` -- when you
    want the worker to act immediately on a concrete task; it saves a
    follow-up `send_input` round trip.
  - `model` and `effort` are optional launch-profile overrides passed verbatim
    to the harness (no Asylum catalog or validation). The peer does NOT inherit
    your profile; omit them for the harness default. Unsupported options fail
    with an explicit error.
- `node.create(harness, substrate, role_hint?, workspace?, description?, prompt?, created_by?, model?, effort?)`
  creates a node without a graph edge back to you. Prefer `node.spawn_peer`
  when you are the one doing the spawning. `model`/`effort` are the same
  verbatim launch-profile overrides as on `node.spawn_peer`.

### Watching workers without streaming their output

Do not poll `node.inspect` / `node.events` in a loop -- register hooks once;
they fire on real events, and every extra tool call costs a harness turn.

- `hook.create(name, event, filter?, actions[], enabled?)`. `event` must be one
  of the 14 events that can actually fire: `graph.spawn`,
  `node.session_started`, `node.turn_complete`, `node.awaiting_input`,
  `node.idle`, `node.ctx_pressure`, `node.tool_call`, `node.session_end`,
  `node.exited`, `node.errored`, `node.resumed`, `channel.inbound`,
  `schedule.5m`, `schedule.30m`. `node.idle` is a real native signal for local Claude workers
  (Notification hook); for Codex it is a daemon quiescence timer (~120s of no
  output), so treat Codex idle timing as approximate.
- `filter` is a small expression over the event payload: `key==value`,
  `key!=value`, `key~value` (substring), `key>=/<=/>/<value` (numeric),
  joined with `&&` / `||`. Empty or `"any"` matches every occurrence. Use
  `node.id==<uuid>` to scope a hook to one worker.
- Actions (a hook runs its action list in order when it fires):
  - `{{"kind":"send_input","target":"<node-uuid>|event|node|event.node|","template":"<text>"}}` --
    types `template` (rendered against the event payload, e.g. `{{message}}`)
    into the target node and submits it in one call. An empty target or
    `event`/`node`/`event.node` means "the node the event is about."
  - `{{"kind":"spawn","target":"","args":{{"harness":"claude_code|codex","substrate":"local|loon","role":"worker","workspace":"<path?>","description":"<str?>","prompt":"<str?>"}}}}` --
    launches a brand-new node when the hook fires, e.g. to replace a worker
    that just errored.
  - `{{"kind":"channel","target":"<channel-id>","args":{{"title":"<str>"}},"template":"<body>"}}` --
    sends a message through a notification channel. On the seeded `ntfy`
    channel (`ntfy-default`, live only if ntfy server/topic are configured)
    this both pushes to the human's phone AND -- because the event payload
    carries the node id -- sets up reply correlation, so a phone reply routes
    back to that exact node. This is the ONLY path that gets a node-correlated
    reply; see Escalating below.
  - `{{"kind":"pause_node","target":""}}` / `{{"kind":"archive","target":""}}` --
    interrupt or archive the node the event is about.
- `hook.list`, `hook.delete(hook_id)`, `hook.firings` manage and audit rules.

A practical monitor pair for a supervisor: one hook on `node.awaiting_input`
(`channel` action to `ntfy-default`, escalates and enables reply routing) and
one on `node.errored` (`send_input` to nudge, or `spawn` to replace).

### Feeding a stalled worker

- `node.send_input(node_id, text)` (or a `send_input` hook action) delivers
  and submits text in one call -- use it for a plain nudge.
- If the worker's harness explicitly asked a question
  (`node.awaiting_input` fired), Asylum already created a pending decision for
  it -- resolve that instead of free-handing another `send_input` for the same
  turn, so the record stays honest.

### The decision loop (a worker asks, a human or you answer)

- `node.awaiting_input` (permission prompt / elicitation / `agent_needs_input`)
  auto-creates a pending decision on that node (deduplicated: one pending
  decision per node at a time; a repeat awaiting-input refreshes its text
  instead of stacking a second one).
- `decision.list(pending=true)` lists only unresolved decisions -- check this
  instead of guessing which workers are stuck.
- `decision.resolve(decision_id, status, answer?)`: `status` is `approved` or
  `denied`; a non-empty `answer` is delivered to the node verbatim, overriding
  the yes/no derived from `status`. This injects straight into the worker's
  input and closes out the decision record in one call.
- `decision.create(text, node_id?)` lets you raise a question yourself instead
  of waiting for a worker's harness to ask one.
- A phone reply to a `channel`-hook ntfy escalation is correlated by node and
  routes through this same `decision.resolve` path automatically -- no extra
  tool call from you once the hook is set up.

### Escalating to the human

- `notify.send(title, body, topic?)` sends a plain ntfy push with NO node
  correlation -- use it for FYI/status only.
- To reach a specific worker and get a reply routed back to it, you must go
  through a hook: a `node.awaiting_input` or `node.errored` hook with a
  `channel` action targeting `ntfy-default`. Direct `notify.send` /
  `channel.test` calls never carry a node id, so they cannot be replied-to-a-
  node; only the hook path can.

### When work is done

- `node.stop(node_id)` stops a worker gracefully. `node.archive(node_id)` stops
  and marks it archived -- use this once a worker's output is no longer
  needed. Do not leave finished workers running: a stopped/archived node is
  honest liveness, not an abandoned process.

### Etiquette

**Choosing the right layer**, in order of preference:
1. Work you can do directly in your own session: do it. No delegation.
2. Fine-grained fan-out inside one body of work (many files, many checks,
   draft alternatives): use your harness's own subagents, agent teams, or
   scripted workflows -- cheap, fast, share your workspace.
3. Work needing independent lifetime, isolation, separate supervision, or a
   different workspace/harness/substrate/launch profile: spawn a node with
   `node.spawn_peer` and a concrete assignment and completion criteria.

**Verifying substantial results**: verify in a fresh context with a distinct
adversarial framing -- an evaluator peer node, or your harness's in-harness
equivalent primed to refute rather than confirm. Same-context self-review is
weak: an agent that just finished the work is an anchored judge of it. This
is a recommendation, not a gate -- Asylum does not block completion on
verification.

- Prefer hooks over polling `node.list` / `node.inspect` in a loop.
- Call `graph.get` once to orient yourself, not repeatedly.
- Give a worker a concrete `prompt` at spawn time rather than a vague
  `description` plus a follow-up `send_input`.
- Never simulate a worker in your own transcript. Real fan-out is either a
  real in-harness subagent or a real node.
"#;

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
    let header = format!(
        r#"# Asylum Launch Packet

Node: `{node_id}`
Role hint: `{role_hint}`
Harness/Substrate: `{harness}/{substrate}`
Daemon base URL: `{base_url}`

## This node's own capabilities
{capabilities_text}
## Current graph
{graph_summary}

"#,
        node_id = node_id,
        role_hint = role_hint,
        harness = harness,
        substrate = substrate,
        base_url = base_url,
        capabilities_text = capabilities_text,
        graph_summary = graph_summary,
    );
    format!("{header}{FLEET_OPERATING_MANUAL}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_includes_node_header_fields() {
        let markdown = launch_packet_markdown(
            "node-1",
            "http://127.0.0.1:7717",
            "supervisor",
            "claude_code",
            "local",
            &[("send_input", true), ("interrupt", true), ("stop", true)],
            "1 nodes with 0 explicit edges",
        );
        assert!(markdown.contains("node-1"));
        assert!(markdown.contains("http://127.0.0.1:7717"));
        assert!(markdown.contains("supervisor"));
        assert!(markdown.contains("claude_code/local"));
        assert!(markdown.contains("- send_input: true"));
        assert!(markdown.contains("1 nodes with 0 explicit edges"));
    }

    /// The operating manual must name only real MCP tools/events/action kinds
    /// (checked against the actual catalogs in mcp.rs / hooks::event_catalog).
    #[test]
    fn markdown_names_only_real_tools_events_and_actions() {
        let markdown = launch_packet_markdown(
            "node-1",
            "http://127.0.0.1:7717",
            "supervisor",
            "claude_code",
            "local",
            &[],
            "no graph",
        );
        for tool in [
            "node.spawn_peer",
            "node.create",
            "node.send_input",
            "node.stop",
            "node.archive",
            "hook.create",
            "hook.list",
            "hook.delete",
            "hook.firings",
            "decision.list",
            "decision.resolve",
            "decision.create",
            "notify.send",
            "graph.get",
        ] {
            assert!(markdown.contains(tool), "missing tool reference: {tool}");
        }
        for event in [
            "graph.spawn",
            "node.session_started",
            "node.turn_complete",
            "node.awaiting_input",
            "node.idle",
            "node.ctx_pressure",
            "node.tool_call",
            "node.session_end",
            "node.exited",
            "node.errored",
            "node.resumed",
            "channel.inbound",
            "schedule.5m",
            "schedule.30m",
        ] {
            assert!(markdown.contains(event), "missing event reference: {event}");
        }

        // n1 drift guard: the manual must name EVERY catalog event, so a newly
        // added hookable event cannot be silently under-advertised again.
        for entry in crate::hooks::event_catalog() {
            assert!(
                markdown.contains(&entry.id),
                "manual omits catalog event: {}",
                entry.id
            );
        }
        for action_kind in ["send_input", "spawn", "channel", "pause_node", "archive"] {
            assert!(
                markdown.contains(&format!("\"kind\":\"{action_kind}\"")),
                "missing action kind reference: {action_kind}"
            );
        }
        assert!(markdown.contains("ntfy-default"));
        assert!(!markdown.contains("recipe"));
        assert!(!markdown.contains("transcript.checkpoint"));
    }

    /// LAYER-003/LAYER-004 drift guard: the manual must keep the layer-choice
    /// and verification etiquette markers so the doctrine can't silently
    /// erode back to "spawn a node for everything" or a same-context review.
    #[test]
    fn markdown_includes_layer_choice_and_verification_etiquette() {
        let markdown = launch_packet_markdown(
            "node-1",
            "http://127.0.0.1:7717",
            "supervisor",
            "claude_code",
            "local",
            &[],
            "no graph",
        );

        assert!(markdown.contains("Choosing the right layer"));
        assert!(markdown.contains("Work you can do directly in your own session: do it. No delegation."));
        assert!(markdown.contains(
            "use your harness's own subagents, agent teams, or\n   scripted workflows"
        ));
        assert!(markdown.contains("spawn a node with\n   `node.spawn_peer`"));

        assert!(markdown.contains("Verifying substantial results"));
        assert!(markdown.contains("verify in a fresh context with a distinct\nadversarial framing"));
        assert!(markdown.contains("Same-context self-review is\nweak"));
        assert!(markdown.contains("This\nis a recommendation, not a gate"));

        assert!(markdown.contains("Never simulate a worker in your own transcript. Real fan-out is either a\n  real in-harness subagent or a real node."));
        // Point 5 must ban fiction, not in-harness parallelism.
        assert!(!markdown.contains("Do not simulate worker nodes inside your own harness session"));
    }
}
