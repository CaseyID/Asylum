# Handoff: Port of playwright-cli UI validation from Claude Code to Codex CLI

**Date:** 2026-05-09
**Audience:** Codex CLI
**Goal:** Verify the port works end-to-end and fix anything broken.

## What this is

We have a working UI-validation setup in Claude Code that drives a real browser via the `playwright-cli` shell binary (microsoft/playwright-cli). It supports snapshot-driven ARIA navigation, persistent login profiles, video recording with chapter markers and overlays, and Playwright traces. We just tried to port it to Codex CLI. We need you to verify the port is correct and functional.

We are **not** porting the older playwright-MCP-based variant — only the CLI-based one.

## What was already in Claude Code (the source of truth for behavior)

| Piece | Path | Role |
|---|---|---|
| Subagent | `~/.claude/agents/playwright-cli-validator.md` | Sonnet worker that the orchestrator delegates UI work to. Full operating discipline lives here. |
| Slash command | `~/.claude/commands/validate-ui-cli.md` | Thin wrapper that briefs and invokes the subagent. |
| Reference skill | `~/.claude/skills/playwright-cli/` | Upstream microsoft/playwright-cli skill — `SKILL.md` plus 10 reference docs (video-recording, tracing, storage-state, session-management, request-mocking, running-code, spec-driven-testing, test-generation, playwright-tests, element-attributes). |
| Config | `~/.claude/playwright-cli.config.json` | `{ "browser": { "browserName": "chromium", "launchOptions": { "channel": "chromium" } } }` |
| Cache dirs | `~/.cache/claude-playwright-cli/{output,recordings,profile}` | Scratch + persistent browser profile + video deliverables. |
| Browser binary | `~/.cache/ms-playwright/chromium-1223/...` | Pre-installed; subagent is forbidden from reinstalling. |

The cost-saving trick in Claude Code: orchestrator (Opus) runs the slash command, which spawns a Sonnet subagent. The subagent does all the snapshot/click loops in its own context and returns a short markdown report. Per-step ARIA dumps never reach the orchestrator's context.

## What we ported to Codex (and the rationale)

Codex doesn't support user-defined slash commands (built-ins only — see `~/.agents/knowledge/codex/upstream/codex/developers.openai.com/codex/cli/slash-commands.md`). Codex's subagent story is also weaker / more like persona-switching than fork-and-return. So the port shape is: **two skills, no slash command, no custom agent.** Skills can be implicitly invoked by description match, or explicitly via `$skill-name`.

### Files created / copied

```
~/.agents/skills/playwright-cli/                          # copied from ~/.claude/skills/playwright-cli/
  SKILL.md                                                # upstream microsoft/playwright-cli reference
  references/
    element-attributes.md
    playwright-tests.md
    request-mocking.md
    running-code.md
    session-management.md
    spec-driven-testing.md
    storage-state.md
    test-generation.md
    tracing.md
    video-recording.md

~/.agents/skills/validate-ui-cli/                         # NEW — the validator workflow
  SKILL.md                                                # adapted from the Claude subagent body

~/.codex/playwright-cli.config.json                       # copied from ~/.claude/playwright-cli.config.json

~/.cache/codex-playwright-cli/                            # NEW — isolated from Claude's cache
  output/
  recordings/
  profile/
```

### Adaptations made (vs the Claude version)

1. **Cache path renamed** — `claude-playwright-cli` → `codex-playwright-cli` everywhere. Profiles/cookies are isolated so the two CLIs don't share login state.
2. **Config path renamed** — `~/.claude/playwright-cli.config.json` → `~/.codex/playwright-cli.config.json`. Referenced in the skill's command examples.
3. **No subagent delegation** — the Claude subagent body said "you are a worker, the orchestrator handed you a task." The Codex skill is rewritten as instructions to the *active* agent ("you are validating … by driving …"). All operating principles preserved verbatim: snapshot-first, named persistent session `validator`, persistent profile, video recording with chapters/overlays, tracing for deep-debug, scratch cleanup discipline, never install browsers, etc.
4. **Reference path updated** — the skill points at `/home/casey/.agents/skills/playwright-cli/SKILL.md` (and `references/video-recording.md` etc.) for the canonical command list.
5. **Sandbox note added** — a Codex-specific paragraph telling the agent to ask the user for write access to `~/.cache/codex-playwright-cli/` (or use `--sandbox workspace-write` with that path as an extra writable root) if the active sandbox blocks writes there. Don't silently fall back to writing into the project workspace.
6. **No code-edit tool restriction** — the Claude subagent had `disallowedTools: [Write, Edit, NotebookEdit]` in frontmatter; Codex skills don't have an equivalent field, so the skill body says "this skill validates and reports; it does not patch bugs" instead. (If the user explicitly asks for a fix after validation, that's a separate step outside the skill's scope.)
7. **Frontmatter `allowed-tools` field on the upstream playwright-cli skill** is Claude-specific. Left in place — Codex should ignore unknown fields, but flag if it doesn't.

### What was intentionally **not** ported

- The `~/.claude/commands/validate-ui-cli.md` slash command — Codex doesn't support user-defined slash commands.
- The `ui-validator` (MCP-based) variant — out of scope; we only wanted the CLI flavor. (Codex already has a separate `~/.agents/skills/ui-validation/` skill that uses MCP; we did not modify it.)

## What we need you to verify

Please check, in roughly this order, and **fix anything that's wrong** rather than just reporting it:

1. **Skill discovery.** From a fresh Codex CLI session, run `/skills` (or type `$` in the composer). Confirm both `playwright-cli` and `validate-ui-cli` appear with their descriptions intact. If they don't appear, debug — check `~/.codex/config.toml` for any `[[skills.config]]` `enabled = false` entries that would suppress them, check directory layout matches the spec in `developers.openai.com/codex/skills.md`, and check for SKILL.md frontmatter parse errors.

2. **Implicit invocation matches.** Skim the `validate-ui-cli` description and confirm the trigger words (validate UI, click through, record demo, walkthrough video, trace slow page, console errors in browser) line up with how a user would naturally phrase the request. Sharpen the description if you see obvious gaps. Don't expand it past ~2 sentences — Codex truncates skill descriptions in the initial context budget.

3. **Frontmatter compatibility.** Read `~/.agents/skills/playwright-cli/SKILL.md` line 1-5. The `allowed-tools` field is Claude-Code-specific. Verify Codex's skill loader ignores unknown fields gracefully. If it errors or warns, remove that line.

4. **Path correctness.** Grep `~/.agents/skills/validate-ui-cli/SKILL.md` for any leftover references to `claude-playwright-cli` or `~/.claude/`. There should be zero. Also confirm every absolute path mentioned in the skill (config, cache subdirs, reference docs) actually exists on disk.

5. **Binary + browser sanity.** Confirm `playwright-cli --version` runs and the bundled chromium at `~/.cache/ms-playwright/` is reachable. Do **not** install or reinstall anything — if it's missing, that's an infrastructure note for the user, not something to fix here.

6. **Smoke test the workflow.** Pick a trivial public URL (e.g., `https://example.com`) and run the full happy path the skill describes:
   - `cd ~/.cache/codex-playwright-cli/output`
   - Open with the documented flags including `--config=/home/casey/.codex/playwright-cli.config.json --persistent --profile=...`
   - Take one snapshot
   - Close the session
   - Verify scratch was cleaned and `recordings/` + `profile/` were preserved.

   If the sandbox blocks the cache writes, surface that as a finding with the exact command the user should run to grant access — don't silently work around it.

7. **Video recording smoke test (optional but valuable).** Same target, but wrap in `video-start` / `video-stop` and confirm a `.webm` lands at `~/.cache/codex-playwright-cli/recordings/`. Delete the test recording when done.

8. **Sandbox guidance correctness.** The skill currently says: ask the user to grant write access to `~/.cache/codex-playwright-cli/` or run with `--sandbox workspace-write` plus the cache dir as an extra writable root. Confirm that's the right phrasing for the current Codex version (`codex --version`, then check `developers.openai.com/codex/agent-approvals-security.md` and `cli.md`). Update the skill if there's a more precise CLI flag or config-key incantation.

9. **Coexistence with the existing `ui-validation` skill.** `~/.agents/skills/ui-validation/SKILL.md` already exists and uses the playwright **MCP** server. Confirm the new `validate-ui-cli` skill doesn't conflict (descriptions should be distinguishable enough that Codex picks the right one — MCP-flavored asks → `ui-validation`, CLI-flavored / video-recording / trace asks → `validate-ui-cli`). Tighten descriptions if Codex would plausibly pick the wrong one.

## Deliverable

A short report under `## Findings` covering:
- Each numbered check above: PASS / FAIL / FIXED
- Any edits you made (file path + one-line summary of the change)
- Any blockers you couldn't fix yourself, with the exact next step for the user
- A go / no-go for using `validate-ui-cli` in real Codex sessions

Keep the report tight — bullet points, no chain-of-thought, no DOM dumps.

## Reference material on disk

- `~/.agents/knowledge/codex/upstream/codex/developers.openai.com/codex/skills.md` — skill format, locations, discovery
- `~/.agents/knowledge/codex/upstream/codex/developers.openai.com/codex/cli/slash-commands.md` — confirmation that user slash commands aren't a thing
- `~/.agents/knowledge/codex/upstream/codex/developers.openai.com/codex/agent-approvals-security.md` — sandbox / approvals model
- `~/.claude/agents/playwright-cli-validator.md` — original subagent (source of truth for operating principles)
- `~/.agents/skills/playwright-cli/references/video-recording.md` — overlay/screencast API for polished walkthroughs

## What "good to go" looks like

A user types `validate the UI at localhost:3000 and record a demo of the signup flow` in Codex and Codex implicitly picks up the `validate-ui-cli` skill, runs the workflow, produces a `.webm` under `~/.cache/codex-playwright-cli/recordings/`, cleans up scratch, and emits the documented markdown report — without the user having to know any of the paths or flags above.
