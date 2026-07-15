# Asylum Agent-Session Control Plane Landscape

*Edited transcript of a Codex research session. Substantive conversation content is retained; obvious copy/paste truncations and spelling errors have been repaired. Tool calls, plans, approval notices, shell output, and agent-status messages have been omitted.*

## User request and context

> I want you to look out there and tell me what other projects/software/tools are being created/exist/whatever that are like Asylum? Things that help/allow people to do what I'm attempting to get Asylum to let me help myself do.
>
> For some extra context, here's a blurb from a conversation I just had about the layers of agents/subagents/orchestration/harnesses/ladder rungs/primitives of Claude Code (though substitute any other coding-agent harness of your choice; in our case, with Asylum, we also included Codex TUI up front too, but ideally would support many others in the future too). It should help explain where Asylum fits in with AI agentic harnesses or whatever like Claude Code and Codex.

## Supplied conversation excerpt: orchestration layers

### User question

> Explain to me the “workflows” feature that I'm seeing in Claude Code. I want a complete understanding of how that and things like agents/subagents, effort levels, model choices (like Opus, Fable, Sonnet, etc.) all come into play.
>
> I am also wondering about how the whole performance of things changes when it comes to letting the model—with utilities/harness/tooling like Ultracode and Claude Code's built-in ability to use agents, subagents, and agent teams—differ between having it do things like using Asylum to stand up and interact with/supervise other agentic sessions. I would assume, on some level, because Claude models are probably trained and optimized around how to do tool calling and agents/subagents/agent teams/Ultracode/etc., and because this probably utilizes tool calling (and maybe is a tool itself), that I might expect different results using Asylum versus using native Claude Code/Claude-model “harnesses” and “parallelization” technologies and tools and utilities.
>
> I'm not asking that question entirely properly, and I'm sure leaving a lot of important questions unasked due to naivete. Fill in for me, read between the lines, understand what I'm trying to get a good understanding of, and really teach me everything I need to know.

### Assistant response

Your instinct is right that these things are related, but the framing needs one correction that makes everything else fall into place: Workflow, subagents, and Asylum are not competing approaches to the same problem. They are four rungs on a ladder, and the thing that changes as you climb is what a “unit of work” is and who owns the control flow.

| Layer | Unit of work | Isolation |
|---|---|---|
| Asylum | A whole agentic session (VM/process) | Kernel |
| Workflow tool | A subagent, orchestrated by a script | Context |
| Agent tool | A subagent, orchestrated by the model | Context |
| Tool calls | One function call | None |

Once you see it as a ladder, “Asylum versus native subagents” stops being a question—you almost certainly want both, nested.

#### The bottom rung: tool calls are the only primitive that actually exists

You guessed this, and you are correct more literally than you probably expected. From the model's point of view, there is no such thing as a subagent. An agent is a tool with a JSON schema. The model emits a tool-use block, something happens somewhere, and a string comes back. It has no privileged access to what happened inside it. The same applies to Workflow. The harness implements that ability and exposes it to the model as a function.

What is real about the model is its priors. Claude is trained on tool-use histories, and Claude Code's tool surface is the most-trodden path in that training distribution. That is not a magic capability—it means the model has a well-calibrated sense of when to reach for a tool, how to write a delegation prompt that a fresh context can act on, and when to stop. This matters enormously for the Asylum question.

#### Subagents: the point is context economy, not speed

The single most misunderstood thing about subagents is that people reach for them to go faster. Speed is a side effect. The actual reason to delegate is that a subagent has its own context window and only its final answer returns to the parent.

A subagent can read forty files, burn 150k tokens, chase three dead ends, and return four hundred tokens of conclusion. The parent pays four hundred tokens. That is the whole trick, and it is why delegation is fundamentally a context-management technique rather than a parallelism technique.

Context—not money—is the scarce resource. Quality degrades as a window fills; compaction is what papers over that. Every time you are tempted to think about token cost, check whether the real question is context.

The cost of that compression is that it is lossy and unauditable. If a subagent investigates the wrong question, the parent gets a confident, well-written, wrong summary, and the evidence that would have let it catch the error is gone. Delegation trades auditability for context.

#### Workflow: the difference is who owns the control flow

This is the newest piece and the one worth internalizing properly.

When the model uses the Agent tool, it is the orchestrator. It decides, turn by turn, in natural-language reasoning, what to spawn next. That means its orchestration is stochastic, burns its context, and inherits every failure mode of an LLM. Ask it to review three hundred files by spawning agents and what will actually happen is that it may sample a plausible-looking subset, tell you what it found, and quietly never mention that it looked at forty.

When the model uses Workflow, it writes a JavaScript program and the harness runs it. `for (const item of threeHundredItems)` runs three hundred times. Every time. No drift, no fatigue, no sampling. Workflow exists because LLM-driven control flow does not scale reliably, and code does.

Everything else in the Workflow tool follows from that:

- **`pipeline()` versus `parallel()`.** `parallel()` is a barrier: it waits for every item to finish. `pipeline()` runs each item through all stages independently—item A can be in stage three while item B is still in stage one. Barriers are the most common expensive mistake in workflow authoring: if the slowest agent takes three times as long as the fastest, a barrier idles the whole fleet. They are justified only when a later stage genuinely needs all prior results at once, such as deduplicating across a complete finding set or early-exiting on a zero count. “I need to flatten a list first” is not a reason; do that inside a stage.
- **Structured output.** Passing a JSON schema forces a subagent to produce structured data instead of prose, with retries on mismatch handled at the tool layer. This is what makes agent-to-agent handoff composable. The difference between “the agent said something about three bugs” and “I have a typed array of three finding objects” is the difference between a demo and a system.
- **Resume.** A workflow run can be resumed; the longest unchanged prefix of agent calls returns cached results instantly and only the first edited call onward reruns. That is also why scripts cannot call `Date.now()` or random functions: they would break replay.

#### Effort and model are independent dials

Effort (`low`, `medium`, `high`, `xhigh`, `max`) controls thinking depth, and it visibly changes behavior: low effort means fewer, more consolidated tool calls, less preamble, and terser confirmations. High effort means more exploration and more verification.

Two things about effort are counterintuitive and commonly misunderstood:

- **Effort is non-monotonic on total cost.** Higher effort often produces fewer turns on agentic work because the model plans better up front instead of thrashing. On Opus 4.8 specifically, the guidance is to compare medium/high/xhigh on your own evaluation set rather than reflexively using xhigh. The intelligence ceiling is high enough that xhigh is not automatically right, and `max` can tip into overthinking.
- **Effort is per-agent inside a workflow.** A thirty-agent workflow running everything at `max` is absurd. The useful shape is cheap, low-effort agents for mechanical fan-out—grep this, enumerate call sites—and high or xhigh reserved for stages requiring judgment: synthesis, adversarial verification, and final ranking.

Model is the orthogonal dial. Sonnet 5 at xhigh will beat Opus 4.8 at low on plenty of tasks. Roughly: Fable 5 is the most capable and most expensive, with thinking always on, for genuinely hard long-horizon work. Opus is the high-end workhorse. Sonnet 5 now lands close to Opus on coding and agentic tasks at a fraction of the price, which makes it the natural choice for fleet work. Haiku is for genuinely mechanical grunt work.

The tempting move is to downgrade aggressively to save money. Resist it. Reflexive downgrading usually costs more in retries, wrong answers that survive into synthesis, and extra rounds. The default in a workflow should be to let agents inherit the session model, and downgrade only where the task is clearly mechanical.

#### Why multi-agent work works at all—and when it is theater

Adversarial verification—spawn *N* skeptics, each prompted to refute a finding, and kill it if a majority refute it—does not work because the verifiers are smarter. It works because they start with a different framing.

A model that spent 50k tokens convincing itself a bug is real is anchored. A fresh model told “try to refute this; default to refuted if uncertain” is not anchored. What you are buying, statistically, is decorrelated errors.

That tells you exactly when multi-agent work is worthless: if *N* agents share the same prompt, context, and framing, their errors are correlated, and you have spent five times the tokens to get the same answer five times. Diversity of framing is the entire product. “Give each verifier a distinct lens—correctness, security, does it actually reproduce?” beats “run three identical refuters.”

This is why the highest-leverage part of any workflow is not the fan-out but the verify stage. Most bad agent output is plausible but wrong, and only a fresh, hostile reader catches plausible but wrong. A five-agent adversarial verify stage beats a fifty-agent workflow that fans out and concatenates.

#### Asylum

Here is my honest read, and you should correct me where I have Asylum wrong—I'm reconstructing this from the repository context and prior sessions rather than reading the code fresh. As I understand it, Asylum stands up agentic harnesses—Claude Code, Codex, and others—inside Firecracker microVMs via Loon, routes bytes to and from their PTYs, and gives you a cockpit to supervise them.

If that is right, Asylum is not an alternative to Workflow. It is a layer above Claude Code entirely. It does not replace subagents; it supervises the process that has subagents.

| Layer | Unit of work | Isolation | Lifetime | Failure semantics | Coordination cost |
|---|---|---|---|---|---|
| Native harness | Context window | Shared filesystem, process tree, machine, and credentials | Seconds to minutes | A null comes back and you filter it | Nearly free: a function call |
| Asylum | Session | Kernel-level when using a VM; nothing shared unless mounted | Hours to days | You have a VM to reason about—more power and more failure surface | Seconds to minutes: provision a VM, authenticate a harness, clone a repository |

The granularity at which each layer is economical differs by orders of magnitude. Do not use Asylum to grep a file. Do not use a subagent to run an unattended overnight refactor of a repository you do not fully trust.

That last point deserves emphasis, because you have a scar here: an orchestration script's `pkill -f` with an empty pattern took out the desktop. That is the isolation argument in miniature. Native subagents share your machine. There is no boundary. Asylum's entire value proposition is that a mission that goes wrong destroys a VM instead of your desktop or host state.

#### Will results actually differ?

Yes, but not primarily because Asylum makes the model worse. Asylum hosts the harness. If the thing running inside the VM is a real Claude Code process, then the inner model still sees the same tool surface, prompt shape, delegation patterns, Workflow, and subagents. Its post-training priors remain intact. Asylum is orthogonal to the model's competence, not subtractive from it.

The risk boundary is precisely where your “dumb plumbing” principle already puts it. The moment you start interpreting agent output in Rust—parsing intent, matching keywords, or reimplementing an agent loop against a bespoke tool set—you leave the well-trodden path, and you will feel it. Not because you would be doing anything conceptually wrong, but because you would be handing the model a tool surface it has far less experience with and re-deriving from scratch the loop that Anthropic has already built and optimized. Your instinct to route raw bytes to real harnesses and keep intelligence in the agents is not merely an architectural preference—it is what lets you inherit all that post-training for free. Mechanistically, it is the right call.

Where you will see a real difference is **steering bandwidth**. The native layer has a typed channel: JSON schemas, validated and retried at the tool layer. Asylum's channel is a PTY—bytes in, bytes out—and whatever structure exists is whatever the harness happened to print. That makes programmatic composition—fan out fifty items and collect fifty typed findings—radically easier natively and quite painful at the Asylum layer. Conversely, supervision, interruption, snapshotting, and rollback are natural at the Asylum layer and do not exist natively at all.

#### The synthesis: nest them

```text
Asylum: fleet of long-lived, isolated, heterogeneous missions
└── each node runs a real Claude Code or Codex session
    └── the harness uses Workflow/Agent internally for its own fan-out
        └── which are, at bottom, tool calls
```

Asylum's job is durability, isolation, supervision, and heterogeneity—concerns that only make sense at session granularity. Claude Code's job is everything inside one mission.

Trying to make Asylum do fine-grained fan-out means reimplementing Workflow badly at a hundred times the coordination cost. Trying to make Workflow run long-horizon isolated missions means reimplementing Asylum badly. The layers are complements because they isolate different things: Workflow isolates context; Asylum isolates blast radius.

#### Further implications

- **Ultracode is a policy, not a capability.** It does not unlock anything. It flips the default from “work solo unless the user opts into orchestration” to “author a Workflow for every substantive task unless it is trivial,” and then enforces that as a constraint. Every tool it uses is available without it; Ultracode changes when the model reaches for them. It was off again, incidentally, after you set effort back to xhigh.
- **Ask for a workflow explicitly when you want one.** Because Workflow can spawn dozens of agents and burn substantial tokens, it should not be launched merely by inference.
- **The shape of the script matters more than the agent count.** “Agent count” is the easiest number to reach for and nearly meaningless. What determines whether a workflow produces something trustworthy is whether it has a verify stage with decorrelated framing, and whether it converges—looping until two consecutive rounds find nothing new—rather than stopping at an arbitrary number.
- **Silent truncation is the failure mode to watch for.** A workflow that samples, skips retries, or drops results without saying so can look like comprehensive coverage. If Asylum builds orchestration features, it should make dropped or incomplete work loud.

The next concrete design question would be what Asylum should look like when it is explicitly designed to let an inner Claude Code session use Workflow well: what the VM needs—CPU count caps concurrency at `min(16, cores - 2)`, so an under-provisioned VM silently serializes fan-out—what the cockpit should surface, and where the PTY boundary costs you structure that you would rather have kept.

---

## Research result: the Asylum landscape

### Bottom line

Yes—this is now a real, rapidly forming product category. A useful name for it is:

> **Multi-harness agent-session control plane**

As of July 12, 2026, Asylum is not alone, but the research did not find another project combining its entire intended boundary:

- Real Claude Code, Codex, and future harness sessions—not a replacement agent loop.
- Local processes and remote microVM-backed nodes under one model.
- Durable session identity, context, events, results, and artifacts.
- Full-fidelity live session intervention.
- Typed agent-to-agent coordination and explicit relationships.
- Supervision by exception.
- An always-on, single-user daemon.
- No mandatory Asylum-owned task/workflow state machine.

That boundary is explicit in [Asylum's current product specification](specs/asylum-current-product-spec.md) (line 53 at the time of the research session) and [orchestration-layer documentation](concepts/orchestration-layers.md) (line 31 at the time of the session). The closest projects occupy different slices of the intersection.

### Closest actual peers

| Project | Why it is close | Important divergence from Asylum |
|---|---|---|
| [Warp Oz](https://www.warp.dev/blog/multi-harness-cloud-agent-orchestration) | The strongest commercial category match. Oz explicitly sits above Claude Code, Codex, and Warp Agent and provides unified launch, tracking, governance, steering, automatic parallel orchestration, APIs, audit, session handoff, schedules, and hosted or self-hosted execution. | Enterprise/cloud/workflow oriented and commercially controlled. Its primary object is an agent run or automation, not Asylum's durable, graph-related session node. Cross-harness memory was still a research preview. |
| [OpenHands Agent Canvas](https://www.openhands.dev/product/canvas) | The strongest open-source platform competitor. It launches real Claude Code, Codex, Gemini CLI, OpenHands, or custom ACP agents. It supports parallel worktrees, local/Docker/remote-VM/cloud backends, persistent conversations, branching, automations, and remote/mobile access. | The unit is an ACP conversation. It steers a structured conversation rather than opening the harness's native TUI/PTY. No clearly documented cross-conversation relationship graph, coordinator-to-peer capability plane, or fleet-wide attention inbox. It was also very young and labeled beta. |
| [Daintree](https://daintree.org/) | The closest analogue to Cockpit. It runs real CLI harnesses in real terminal panels, detects working/waiting/done state, provides an inbox, worktrees, review/merge, fleet broadcasting, and an MCP server. Its Assistant is itself a real Claude/Codex/etc. session that can spawn, inspect, redirect, restart, and monitor other terminals. | Local desktop and worktree isolation only. No kernel boundary, remote substrate abstraction, or documented daemon/reboot durability. It reads and interprets terminal output for coordination, whereas Asylum's doctrine prefers harness-native structured reporting and treats the PTY as opaque, full-fidelity transport. |
| [Agent Deck](https://github.com/asheshgoplani/agent-deck) | The closest terminal-native open-source session manager. Real Claude, Codex, Gemini, OpenCode, and other sessions run under tmux. It has TUI and web attachment, worktrees, optional Docker, archive/fork, transcript preservation, state detection, notifications, and persistent “conductor” sessions that supervise workers. | It lacks Asylum's first-class substrate abstraction, Loon-style VM boundary, durable graph, work envelope/result model, and one shared daemon capability surface across Cockpit/API/CLI/MCP. |
| [Gas Town](https://github.com/gastownhall/gastown) | The richest coordination overlap: real multi-harness workers, persistent identities, mailboxes, handoffs, a Mayor coordinator, work graphs, watchdogs, escalation, capacity scheduling, merge queues, and activity feeds. | Architecturally almost the inverse of Asylum. Its durable objects are work items, convoys, roles, workflow templates, and merge state. Asylum deliberately keeps the session as the primary object and refuses to own a mandatory workflow engine. |
| [Docker Sandboxes](https://docs.docker.com/ai/sandboxes/) | The closest analogue to one isolated Asylum node. It launches Claude Code, Codex, Gemini, Copilot, OpenCode, and others inside persistent microVMs. Users can stop, restart, reattach, open a shell, inspect resources, and use private cloned repositories. | It manages isolated computers and harness launches, not a coordinated fleet. No session graph, work envelope, results, peer spawning, agent authority, hooks, inbox, or fleet Cockpit. |
| [GitHub Agent HQ / Mission Control](https://github.blog/news-insights/company-news/welcome-home-agents/) | A major competitive surface for the same human job. Users launch concurrent agent sessions, view live logs, steer, stop, archive, continue locally, and review pull requests. GitHub offers Copilot plus Claude and Codex partner agents. | Repository/task/PR-centric and cloud-managed. It does not host arbitrary locally installed harness TUIs or expose a general coordinator-controlled peer graph. Claude and Codex support was public preview. |

The two projects to watch most closely are **OpenHands Agent Canvas** and **Daintree**. Together they cover a surprising amount of Asylum's territory:

- Agent Canvas approaches Asylum from the structured, multi-backend side.
- Daintree approaches it from the real-terminal, local-Cockpit side.
- Oz approaches it from commercial cloud and governance.
- Agent Deck approaches it from durable session plumbing.
- Gas Town approaches it from durable coordination.

### Vendor-owned command centers

These compete for the user's workflow even though they do not preserve Asylum's harness neutrality.

- The [OpenAI Codex app](https://openai.com/index/introducing-the-codex-app/) is explicitly a “command center for agents”: parallel threads, worktrees, long-running background work, diff review, skills, automations, remote devboxes, and mobile steering. See also [Work with Codex from anywhere](https://openai.com/index/work-with-codex-from-anywhere/). But every session is Codex.
- [Claude Code Desktop](https://code.claude.com/docs/en/desktop) provides local, Anthropic-cloud, and SSH sessions; automatic worktrees; terminal/editor/browser/diff panes; session filtering; interruption and steering; scheduled work; pull-request monitoring; and cloud continuation. But every session is Claude Code.
- [Claude Code Agent Teams](https://code.claude.com/docs/en/agent-teams) are genuinely coordinated Claude sessions with shared tasks and direct messaging, but they remain harness-internal, experimental, and subject to documented resume and coordination limitations. They belong inside an Asylum node according to Asylum's layer model.
- [Cursor Cloud Agents](https://docs.cursor.com/background-agent) provide parallel isolated VMs, background execution, browser/desktop takeover, automations, mobile/web control, and [self-hosted workers](https://cursor.com/blog/self-hosted-cloud-agents). The harness remains Cursor's, even when different underlying models are used.
- [Devin advanced orchestration](https://docs.devin.ai/work-with-devin/advanced-capabilities) lets a coordinator Devin create, message, budget, sleep, and terminate child Devins, each in its own VM.
- Factory Missions and [Droid Computers](https://docs.factory.ai/cli/features/droid-computers) combine specialist worker Droids, persistent development computers, local/cloud/on-prem execution, streaming, interruption, schedules, and automation.

These products validate the central premise of Asylum: the scarce human activity is increasingly directing, supervising, unblocking, and reviewing fleets—not typing into one agent.

### The local session-manager explosion

A large cluster of smaller tools now addresses: “I have ten agent terminals and cannot tell which one needs me.”

- [Conductor](https://www.conductor.build/) — polished macOS worktree manager for Claude Code, Codex, Cursor, and OpenCode.
- [Nimbalyst](https://github.com/nimbalyst/nimbalyst) — cross-platform visual session/task/workstream manager and successor to Crystal.
- [CodeFleet](https://codefleet.app/) — macOS worktree grid, chats, terminals, live tool calls, comparisons, Kanban tasks, and scheduled routines.
- [Herdr](https://github.com/ogulcancelik/herdr) — durable terminal panes with detach/reattach, SSH, state detection, and an agent-accessible socket API.
- [dmux](https://github.com/standardagents/dmux) and [workmux](https://github.com/raine/workmux) — tmux/worktree launch, dashboards, notifications, review, merge, and multi-harness support.
- [Claude Squad](https://github.com/smtg-ai/claude-squad) — a thinner tmux/worktree TUI for multiple real harnesses.
- [cmux](https://cmux.com/) — a programmable terminal and notification surface for many CLI agents.
- [OpenGUI](https://github.com/akemmanuel/OpenGUI) — persistent multi-project UI over real Claude Code, Codex, OpenCode, and Pi adapters.
- [Vibe Kanban](https://github.com/BloopAI/vibe-kanban) was important prior art, but [its company shut down in April 2026](https://www.vibekanban.com/blog/shutdown); only the community-local open-source project remains.

These products are making tabs, worktrees, diff review, status badges, and notifications commodity features. Asylum should not expect those features alone to distinguish it.

### Workflow, provenance, and security systems

[h5i](https://github.com/h5i-dev/h5i) is especially interesting. It runs real Claude/Codex sessions in Landlock/seccomp/namespaces/container-confined worktrees, records prompts and commands in Git, supports peer review, and replays candidate patches in a neutral verifier.

It overlaps Asylum's security, durability, evidence, and verification concerns. But it imposes a sealed-candidate → review → verify → winner workflow. That makes it more naturally a layer inside or beneath Asylum than Asylum's product model.

Similarly:

- [LangGraph](https://docs.langchain.com/oss/python/langgraph/persistence), [AutoGen](https://microsoft.github.io/autogen/stable/index.html), and [CrewAI AMP](https://docs.crewai.com/en/enterprise/introduction) provide durable graph execution, multi-agent workflows, deployment, and observability.
- They build or host agent loops. They do not supervise arbitrary vendor-harness sessions as opaque, independent processes.
- They live closer to Claude Workflow/Agent tools on the ladder than to Asylum.

### Sandbox and substrate projects

The substrate layer is becoming extremely crowded:

- [OpenSandbox](https://github.com/alibaba/OpenSandbox)
- [Daytona](https://www.daytona.io/docs/sandboxes)
- [E2B](https://e2b.dev/docs/sandbox)
- [Fly.io Sprites](https://sprites.dev/)
- [CodeSandbox SDK](https://codesandbox.io/sdk)
- [Kubernetes Agent Sandbox](https://github.com/kubernetes-sigs/agent-sandbox)
- [Vercel Sandbox](https://vercel.com/sandbox)
- [Microsandbox](https://github.com/superradcompany/microsandbox)
- [Cloudflare Sandbox SDK](https://developers.cloudflare.com/sandbox/)
- [Modal Sandboxes](https://modal.com/docs/guide/sandboxes)

These manage computers, processes, filesystems, snapshots, volumes, PTYs, or warm pools. They are possible alternatives or future adapters beneath an Asylum node. They are not, by themselves, Asylum competitors.

This reinforces an important correction to the supplied conversation: Asylum does not mean every node is a Firecracker VM. **Local is also a first-class substrate.** Loon is where Asylum gains the stronger microVM blast-radius boundary.

### Where Asylum is still meaningfully different

1. **Session-first, not task-first.** A node remains a real session even when its objective, process, VM, or current turn changes.
2. **Harness-neutral without replacing the harness.** Claude Code remains Claude Code. Codex remains Codex. Their native subagents, workflows, permissions, and post-training remain intact.
3. **Two channels per node.** Structured coordination enables composition; raw live session transport enables full-fidelity human intervention. Agent Canvas mostly chooses the first. Daintree mostly chooses the second. Asylum intends both.
4. **Local and isolated remote capacity under one contract.** Most competitors are either local/worktree tools or managed-cloud systems. Asylum aims to keep the operator model constant while making the isolation difference explicit.
5. **Agent-created peers without a platform-owned workflow engine.** Gas Town, h5i, Kanban tools, and cloud systems generally make their task model central. Asylum lets an ordinary coordinator harness decide what organization is appropriate.
6. **Durable graph, authority, results, and evidence.** Most local tools have a list of terminals or worktrees. A durable graph of explicit responsibility, least-privilege peer authority, work envelopes, decisions, results, and artifact provenance is still unusual.
7. **Single-user, local-first ownership.** Oz, GitHub, Cursor, Devin, and Factory increasingly optimize for organizations, governance, billing, and cloud services. Asylum's one-owner appliance model remains differentiated.

### What to study and borrow

- **Agent Canvas:** ACP, backend switching, conversation forking, local/VM/cloud continuity, and automation UX.
- **Daintree:** status presentation, notification inbox, fleet broadcasting, coordinator-as-real-harness, and permission-tier UX.
- **Agent Deck:** durable tmux ownership, archive/fork behavior, conductor/watchers, and remote channels.
- **Gas Town:** mailboxes, handoffs, watchdogs, escalation, and coordination-failure visibility—without importing its workflow hierarchy.
- **Docker Sandboxes:** named microVM reattachment, clone-mode isolation, credential handling, and local-node resource cards.
- **Oz:** local/cloud handoff, APIs, return values, audit, and cross-harness governance.
- **h5i:** Git-native provenance, replayable evidence, isolation-policy display, and adversarial verification.
- **ACP:** it is becoming an LSP-like interoperability layer for harnesses. An optional ACP adapter could give Asylum structured tool events, permissions, and conversation state without parsing terminal output, while PTY transport remains available for the native session.

### Candid conclusion

Asylum should not position itself as “a way to run multiple coding agents.” That space is already crowded.

Its sharper position is:

> **A local-first, multi-harness control plane for durable, separately supervisable agent sessions—across local and isolated substrates—with both structured coordination and direct live intervention.**

That remains distinct. But the window is moving quickly: Oz, Agent Canvas, Daintree, Agent Deck, GitHub, OpenAI, and Anthropic are all converging on parts of it during 2026.
