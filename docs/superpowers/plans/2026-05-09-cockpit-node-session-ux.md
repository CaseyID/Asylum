# Cockpit Node Session UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove user-facing attach semantics from Cockpit and make node selection/session focus the normal way users enter and operate nodes.

**Architecture:** Keep daemon attach routes, token plumbing, CLI/MCP compatibility, and Loon adapter internals stable. Change only Cockpit-visible behavior, labels, component props, and current product documentation so "attach" becomes an implementation detail rather than a user task.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, xterm.js, Rust daemon compatibility tests, Playwright CLI for rendered UI validation.

---

## Source Spec

Implement [docs/superpowers/specs/2026-05-09-cockpit-node-session-ux-design.md](../specs/2026-05-09-cockpit-node-session-ux-design.md).

## File Structure

Modify these Cockpit files:

- `cockpit/src/components/NodeSession.tsx`: remove attach/native terminal actions from session chrome, ignore attach-issued history in visible transcript, update Loon/session copy.
- `cockpit/src/components/NodeSession.test.tsx`: replace attach-action tests with session-contract tests.
- `cockpit/src/components/Inspector.tsx`: remove attach action and hide attach capability flags from normal inspector display.
- `cockpit/src/components/Inspector.test.tsx`: add regression test that inspector controls do not expose attach UI.
- `cockpit/src/components/CmdK.tsx`: remove "open attach tab" action from command palette.
- `cockpit/src/components/CmdK.test.tsx`: add regression test that Cmd-K does not expose attach UI.
- `cockpit/src/screens/CockpitScreen.tsx`: stop passing attach/native handlers into `NodeSession`.
- `cockpit/src/screens/ChatScreen.tsx`: stop accepting and passing attach/native handlers.
- `cockpit/src/screens/NodeScreen.tsx`: remove attach/native header actions, stop passing attach/native handlers, hide attach capability rows.
- `cockpit/src/screens/FirstRunScreen.tsx`: replace attach onboarding copy with session language.
- `cockpit/src/screens/ChannelsScreen.tsx`: remove attach from example reply placeholders.
- `cockpit/src/screens/SettingsScreen.tsx`: replace "attach urls" security warning with session URL language.
- `cockpit/src/App.tsx`: remove Cockpit paths that request browser/native attach from UI actions and Cmd-K.
- `cockpit/src/cockpit-copy-regression.test.ts`: add visible-copy regression coverage for Cockpit source files.

Modify docs:

- `docs/specs/asylum-current-product-spec.md`: align Cockpit/product language with node session UX while leaving backend attach contracts described as internal/compatibility details.
- `docs/superpowers/plans/2026-05-09-cockpit-node-session-ux.md`: mark task checkboxes as implementation proceeds.

Backend source files should not change in this plan. Existing backend attach tests are run in Task 5 to prove compatibility stays intact.

## Task 1: NodeSession Stops Exposing Attach

**Files:**
- Modify: `cockpit/src/components/NodeSession.test.tsx`
- Modify: `cockpit/src/components/NodeSession.tsx`
- Modify: `cockpit/src/cockpit.css`

- [ ] **Step 1: Replace NodeSession tests with session UX regression tests**

Edit `cockpit/src/components/NodeSession.test.tsx`.

Use the existing fixture structure, but rename the fixture ID so the test text does not contain `attach`:

```ts
function node(overrides: Partial<AsylumNode> = {}): AsylumNode {
  return {
    id: "node-session-loop",
    harness: "codex",
    substrate: "local",
    role_hint: "worker",
    liveness: "running",
    workspace: "/tmp/asylum",
    description: "worker",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    external_id: null,
    capabilities: caps,
    tokens_in: 0,
    tokens_out: 0,
    tool_calls: 0,
    ctx_pct: 0,
    idle_seconds: 0,
    ...overrides,
  };
}
```

Replace the `describe("NodeSession attach events", ...)` block with:

```ts
describe("NodeSession session semantics", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("does not expose attach controls in the session header", () => {
    apiMocks.openNodeObserveSocket.mockReturnValue({ close: vi.fn() });

    const { queryByTitle } = render(<NodeSession node={node()} />);

    expect(queryByTitle("open attach tab")).toBeNull();
    expect(queryByTitle("open attach tab via loon attach")).toBeNull();
    expect(queryByTitle("open in terminal")).toBeNull();
    expect(queryByTitle("interrupt")).toBeNull();
  });

  it("ignores attach-issued history as an internal transport event", async () => {
    let onMessage: ((data: string) => void) | undefined;
    apiMocks.openNodeObserveSocket.mockImplementation((_nodeId: string, options: { onMessage?: (data: string) => void }) => {
      onMessage = options.onMessage;
      return { close: vi.fn() };
    });

    const { container, queryByText, queryByTitle } = render(<NodeSession node={node()} />);

    onMessage?.(JSON.stringify({
      kind: "attach_issued",
      node_id: "node-session-loop",
      body: {
        url: "http://127.0.0.1:7717/attach/token",
        node_id: "node-session-loop",
      },
    }));

    await waitFor(() => expect(apiMocks.openNodeObserveSocket).toHaveBeenCalled());
    expect(queryByText("open a time-limited Cockpit attach view for this node")).toBeNull();
    expect(queryByTitle("open attach tab")).toBeNull();
    expect(container.textContent ?? "").not.toContain("attach tab");
  });

  it("describes Loon live-stream limitations in session language", async () => {
    let onMessage: ((data: string) => void) | undefined;
    apiMocks.openNodeObserveSocket.mockImplementation((_nodeId: string, options: { onMessage?: (data: string) => void }) => {
      onMessage = options.onMessage;
      return { close: vi.fn() };
    });

    const { getByText, queryByText } = render(<NodeSession node={node({ substrate: "loon" })} />);

    onMessage?.("asylum.observe.ws.initialized");
    onMessage?.("asylum.observe.ws.live_stream_unavailable");
    fireEvent.click(getByText("struct"));

    await waitFor(() => {
      expect(getByText("Loon nodes do not stream local PTY-style live observe output; open the node session for an interactive terminal")).toBeDefined();
    });
    expect(queryByText(/use attach/i)).toBeNull();
  });
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
npm --prefix cockpit run test -- cockpit/src/components/NodeSession.test.tsx
```

Expected: FAIL because attach-issued history still renders an attach preview card and the Loon copy still says "use attach".

- [ ] **Step 3: Remove attach UI from `NodeSession.tsx`**

Edit `cockpit/src/components/NodeSession.tsx`.

Change `NodeSessionProps` to:

```ts
export interface NodeSessionProps {
  node: AsylumNode;
  mode?: SessionMode;
  onInterrupt?: (nodeId: string) => void;
  onExpand?: () => void;
}
```

Remove the attach transcript entry variant:

```ts
type TranscriptEntry =
  | { kind: "user"; text: string }
  | { kind: "thought"; text: string }
  | { kind: "text"; text: string; id?: string }
  | { kind: "list"; items: string[] }
  | { kind: "tool"; name: string; args?: Record<string, unknown>; output?: string; state?: "ok" | "pending" | "error" }
  | { kind: "sys-line"; text: string }
  | { kind: "prompt" };
```

Change the initial structured sys line to session language:

```ts
const sysLine = `connected to ${shortNodeId(node.id)} · ${harnessId} · ${node.substrate} · workspace ${node.workspace ?? "~/"} · ${role}`;
```

Change the Loon unavailable message to:

```ts
const message = node.substrate === "loon"
  ? "Loon nodes do not stream local PTY-style live observe output; open the node session for an interactive terminal"
  : "live streaming not supported by this substrate";
```

Change the event switch for attach-issued history to ignore the internal event:

```ts
case "attach_issued": {
  return;
}
```

Remove `onAttach` and `onNativeAttach` from `SessionHeader` props and JSX. The right side should keep only interrupt, view toggle, and expand:

```tsx
<span className="hright">
  {onInterrupt && (
    <Btn kind="ghost" size="sm" icon="square" iconOnly title="interrupt" onClick={() => onInterrupt(node.id)} />
  )}
  <div className="view-toggle" role="tablist" title="transcript rendering">
    <button className={view === "tui" ? "on" : ""} onClick={() => setView("tui")} title="raw tui - xterm.js terminal">tui</button>
    <button className={view === "structured" ? "on" : ""} onClick={() => setView("structured")} title="structured / semantic">struct</button>
  </div>
  {mode === "cockpit" && onExpand && (
    <Btn kind="ghost" size="sm" icon="maximize-2" iconOnly title="open node workspace" onClick={onExpand} />
  )}
</span>
```

Remove the `if (e.kind === "attach") { ... }` branch from `TermEntry`.

Change the command-center input placeholder from:

```ts
? `send to ${node.id} · try: spawn 2 workers, status, attach to w-9a4f1`
```

to:

```ts
? `send to ${node.id} · try: spawn 2 workers, status, summarize progress`
```

- [ ] **Step 4: Remove unused attach-preview CSS**

Delete the `.attach-preview` block from `cockpit/src/cockpit.css`.

The block starts at the comment:

```css
/* attach preview card */
```

and includes all `.attach-preview`, `.attach-preview .h`, `.attach-preview .body`, `.attach-preview .body::after`, `.attach-preview .body .x`, `.attach-preview .body .b`, and `.attach-preview .foot` rules.

- [ ] **Step 5: Run the focused test and verify it passes**

Run:

```bash
npm --prefix cockpit run test -- cockpit/src/components/NodeSession.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit Task 1**

Run:

```bash
git add cockpit/src/components/NodeSession.test.tsx cockpit/src/components/NodeSession.tsx cockpit/src/cockpit.css
git commit -m "cockpit: make node session direct"
```

## Task 2: Remove Attach Actions From Cockpit Wiring

**Files:**
- Modify: `cockpit/src/components/Inspector.test.tsx`
- Modify: `cockpit/src/components/Inspector.tsx`
- Create: `cockpit/src/components/CmdK.test.tsx`
- Modify: `cockpit/src/components/CmdK.tsx`
- Modify: `cockpit/src/screens/CockpitScreen.tsx`
- Modify: `cockpit/src/screens/ChatScreen.tsx`
- Modify: `cockpit/src/screens/NodeScreen.tsx`
- Modify: `cockpit/src/App.tsx`

- [ ] **Step 1: Add Inspector regression coverage**

Append this test to `cockpit/src/components/Inspector.test.tsx`:

```ts
describe("Inspector node session controls", () => {
  it("does not expose attach or native terminal actions", () => {
    const node = makeNode("worker-abc123");
    const { container, queryByText } = render(
      <Inspector
        node={node}
        onAction={vi.fn()}
        onOpen={vi.fn()}
      />
    );

    expect(queryByText("attach")).toBeNull();
    expect(queryByText("open in terminal")).toBeNull();
    expect(container.textContent ?? "").not.toContain("browser_attach");
    expect(container.textContent ?? "").not.toContain("native_attach");
  });
});
```

- [ ] **Step 2: Add Cmd-K regression coverage**

Create `cockpit/src/components/CmdK.test.tsx`:

```ts
import { describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { CmdK } from "./CmdK";
import type { AsylumNode } from "../types";

const node: AsylumNode = {
  id: "node-session-1",
  harness: "codex",
  substrate: "local",
  role_hint: "worker",
  liveness: "running",
  workspace: "/tmp/asylum",
  description: "worker",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  external_id: null,
  capabilities: {
    browser_attach: true,
    native_attach: true,
    send_input: true,
    interrupt: true,
    stop: true,
    resume: false,
    structured_events: false,
    transcript_export: false,
  },
  tokens_in: 0,
  tokens_out: 0,
  tool_calls: 0,
  ctx_pct: 0,
  idle_seconds: 0,
};

describe("CmdK node session actions", () => {
  it("does not expose attach actions", () => {
    const { container } = render(
      <CmdK
        onClose={vi.fn()}
        onPick={vi.fn()}
        onLaunch={vi.fn()}
        onPickNode={vi.fn()}
        onSendRemoteCommand={vi.fn()}
        nodes={[node]}
      />
    );

    expect(container.textContent ?? "").not.toMatch(/attach/i);
  });
});
```

- [ ] **Step 3: Run focused tests and verify they fail**

Run:

```bash
npm --prefix cockpit run test -- cockpit/src/components/Inspector.test.tsx cockpit/src/components/CmdK.test.tsx
```

Expected: FAIL because Inspector still renders `attach`, capability grids still show `browser_attach` and `native_attach`, and `CmdK` still requires and renders `onAttachInBrowser`.

- [ ] **Step 4: Remove attach from Inspector**

Edit `cockpit/src/components/Inspector.tsx`.

Change the action type to remove `attach`:

```ts
export type InspectorAction =
  | "send"
  | "interrupt"
  | "fork"
  | "stop"
  | "terminate"
  | "archive";
```

Change capability keys to hide backend attach flags from normal UI:

```ts
const CAPABILITY_KEYS: (keyof CapabilitySnapshot)[] = [
  "send_input",
  "interrupt",
  "stop",
  "resume",
  "structured_events",
  "transcript_export",
];
```

Delete the primary `attach` button from the controls block. The first controls should be `send input`, `interrupt`, and `fork`.

- [ ] **Step 5: Remove attach from Cmd-K**

Edit `cockpit/src/components/CmdK.tsx`.

Change props to:

```ts
export interface CmdKProps {
  onClose: () => void;
  onPick: (screen: ScreenId) => void;
  onLaunch: () => void;
  onPickNode: (node: AsylumNode) => void;
  onSendRemoteCommand: () => void;
  nodes: AsylumNode[];
}
```

Remove `onAttachInBrowser` from destructuring.

Delete this base item:

```ts
{
  sec: "actions",
  label: "open attach tab…",
  icon: "external-link",
  action: () => onAttachInBrowser(),
},
```

- [ ] **Step 6: Remove attach/native props from screens**

Edit `cockpit/src/screens/CockpitScreen.tsx`.

Change the action prop type to remove native attach:

```ts
onAction: (action: InspectorAction, payload?: string) => void;
```

Render `NodeSession` without attach/native props:

```tsx
<NodeSession
  key={panelNode.id}
  node={panelNode}
  mode="cockpit"
  onInterrupt={() => onAction("interrupt")}
  onExpand={() => onExpandToChat(panelNode.id)}
/>
```

Edit `cockpit/src/screens/ChatScreen.tsx`.

Remove these props from `ChatScreenProps` and destructuring:

```ts
onAttach?: (node: AsylumNode) => void;
onNativeAttach?: (node: AsylumNode) => void;
```

Render `NodeSession` as:

```tsx
<NodeSession
  key={active.id}
  node={active}
  mode="fullscreen"
  onInterrupt={onInterrupt ? () => onInterrupt(active) : undefined}
/>
```

Edit `cockpit/src/screens/NodeScreen.tsx`.

Change `NodeScreenAction` to:

```ts
export type NodeScreenAction =
  | "send"
  | "interrupt"
  | "fork"
  | "stop"
  | "terminate"
  | "archive";
```

Delete the header buttons for `open attach tab` and `open in terminal`.

Render `NodeSession` as:

```tsx
<NodeSession
  key={node.id}
  node={node}
  mode="fullscreen"
  onInterrupt={() => fire("interrupt", "sigint sent · paused")}
/>
```

Change `CapsView` rows to hide backend attach flags:

```ts
const rows: [string, boolean][] = [
  ["send_input", caps.send_input],
  ["interrupt", caps.interrupt],
  ["stop", caps.stop],
  ["resume", caps.resume],
  ["structured_events", caps.structured_events],
  ["transcript_export", caps.transcript_export],
];
```

- [ ] **Step 7: Remove attach/native action handling from App**

Edit `cockpit/src/App.tsx`.

Remove imports:

```ts
requestBrowserAttach,
requestNativeTarget,
```

Remove the `attach` and `native-attach` branches from `handleNodeAction`. The first branch should now be:

```ts
if (action === "send") {
  setSelectedNode(target.id);
  setChatNodeId(target.id);
  setScreen("chat");
  setLocalNotice("opened session input");
} else if (action === "interrupt") {
  await interruptNode(target.id);
  setLocalNotice("interrupt sent");
}
```

Change `inspectorAction` to:

```ts
const inspectorAction = useCallback(
  (action: InspectorAction, payload?: string) => {
    void handleNodeAction(selectedNode, action, payload);
  },
  [selectedNode],
);
```

Render `ChatScreen` without attach/native props:

```tsx
<ChatScreen
  nodes={graph.nodes}
  chatNodeId={chatNodeId ?? ccNode?.id}
  onSelectChat={setChatNodeId}
  onInterrupt={(node) => void handleNodeAction(node, "interrupt")}
  onLaunch={() => setScreen("create")}
/>
```

Render `CmdK` without `onAttachInBrowser`.

Change the remote command prompt to remove the attach example:

```ts
const raw = window.prompt(
  `remote command:\n  status token=…\n  ${example}\n  interrupt token=… node=…\n  stop token=… node=…`,
  "",
);
```

- [ ] **Step 8: Run focused tests and verify they pass**

Run:

```bash
npm --prefix cockpit run test -- cockpit/src/components/Inspector.test.tsx cockpit/src/components/CmdK.test.tsx
```

Expected: PASS.

- [ ] **Step 9: Run typecheck/build and fix compile fallout**

Run:

```bash
npm --prefix cockpit run build
```

Expected: PASS. If TypeScript reports prop or union mismatches, update the call site rather than reintroducing attach actions.

- [ ] **Step 10: Commit Task 2**

Run:

```bash
git add cockpit/src/components/Inspector.test.tsx cockpit/src/components/Inspector.tsx cockpit/src/components/CmdK.test.tsx cockpit/src/components/CmdK.tsx cockpit/src/screens/CockpitScreen.tsx cockpit/src/screens/ChatScreen.tsx cockpit/src/screens/NodeScreen.tsx cockpit/src/App.tsx
git commit -m "cockpit: remove attach actions"
```

## Task 3: Clean Remaining Cockpit Copy And Add Copy Regression Test

**Files:**
- Create: `cockpit/src/cockpit-copy-regression.test.ts`
- Modify: `cockpit/src/screens/FirstRunScreen.tsx`
- Modify: `cockpit/src/screens/ChannelsScreen.tsx`
- Modify: `cockpit/src/screens/SettingsScreen.tsx`
- Modify: `cockpit/src/App.test.tsx`

- [ ] **Step 1: Add visible-copy regression test**

Create `cockpit/src/cockpit-copy-regression.test.ts`:

```ts
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const visibleSurfaceFiles = [
  "components/NodeSession.tsx",
  "components/Inspector.tsx",
  "components/CmdK.tsx",
  "screens/CockpitScreen.tsx",
  "screens/ChatScreen.tsx",
  "screens/NodeScreen.tsx",
  "screens/FirstRunScreen.tsx",
  "screens/ChannelsScreen.tsx",
  "screens/SettingsScreen.tsx",
  "App.tsx",
];

const forbiddenVisibleCopy = [
  /open attach/i,
  /browser attach/i,
  /native attach/i,
  /attach tab/i,
  /attach url/i,
  /attach link/i,
  /terminal attach/i,
  /use attach/i,
  /attached to/i,
];

describe("Cockpit user-facing node session copy", () => {
  it("does not expose attach terminology in visible surfaces", () => {
    const hits: string[] = [];

    for (const file of visibleSurfaceFiles) {
      const abs = resolve(__dirname, file);
      const text = readFileSync(abs, "utf8");
      for (const pattern of forbiddenVisibleCopy) {
        if (pattern.test(text)) {
          hits.push(`${file}: ${pattern}`);
        }
      }
    }

    expect(hits).toEqual([]);
  });
});
```

- [ ] **Step 2: Run the copy regression test and verify it fails**

Run:

```bash
npm --prefix cockpit run test -- cockpit/src/cockpit-copy-regression.test.ts
```

Expected: FAIL because FirstRun, Settings, Channels, App, and NodeSession/NodeScreen still contain visible attach phrases until the remaining cleanup lands.

- [ ] **Step 3: Update first-run onboarding copy**

Edit `cockpit/src/screens/FirstRunScreen.tsx`.

Change:

```ts
["open attach tab", "real harness ui at any time, no extra install required"],
["receive ntfy", "remote command channel — reply with `approve`, `attach`, `retry`"],
```

to:

```ts
["open node session", "real harness ui or terminal from the node workspace"],
["receive ntfy", "remote command channel - reply with `approve`, `status`, `retry`"],
```

- [ ] **Step 4: Update channel reply placeholder**

Edit `cockpit/src/screens/ChannelsScreen.tsx`.

Change the reply placeholder:

```tsx
<input value={replies} onChange={(e) => setReplies(e.target.value)} placeholder="approve, deny, attach" />
```

to:

```tsx
<input value={replies} onChange={(e) => setReplies(e.target.value)} placeholder="approve, deny, status" />
```

- [ ] **Step 5: Update settings exposure warning**

Edit `cockpit/src/screens/SettingsScreen.tsx`.

Change:

```tsx
exposing asylum beyond localhost reveals attach urls and node transcripts. require pairing + tailscale.
```

to:

```tsx
exposing asylum beyond localhost reveals session URLs and node transcripts. require pairing + tailscale.
```

- [ ] **Step 6: Update App tests to stop mocking removed UI actions**

Edit `cockpit/src/App.test.tsx`.

Remove these hoisted mocks:

```ts
requestBrowserAttach: vi.fn(),
requestNativeTarget: vi.fn(),
```

The goal is to avoid Cockpit UI tests reinforcing removed user-facing actions.

- [ ] **Step 7: Run copy regression and app tests**

Run:

```bash
npm --prefix cockpit run test -- cockpit/src/cockpit-copy-regression.test.ts cockpit/src/App.test.tsx
```

Expected: PASS.

- [ ] **Step 8: Commit Task 3**

Run:

```bash
git add cockpit/src/cockpit-copy-regression.test.ts cockpit/src/screens/FirstRunScreen.tsx cockpit/src/screens/ChannelsScreen.tsx cockpit/src/screens/SettingsScreen.tsx cockpit/src/App.test.tsx
git commit -m "cockpit: guard session terminology"
```

## Task 4: Align Current Product Spec

**Files:**
- Modify: `docs/specs/asylum-current-product-spec.md`
- Modify: `docs/superpowers/plans/2026-05-09-cockpit-node-session-ux.md`

- [ ] **Step 1: Update product definition and goals**

Edit `docs/specs/asylum-current-product-spec.md`.

Change the product definition sentence to use session language:

```md
Asylum is a single-user, always-on control plane for real agent harness sessions. It does not replace Codex, Claude Code, Pi, Hermes, Loon, or any other harness/substrate. It launches harnesses, gives them shared context and capabilities, observes them, lets humans open live node sessions and intervene, and lets harnesses coordinate with each other through a common daemon-owned capability surface.
```

Change the goals bullet:

```md
- Let humans open node sessions, inspect, send input, interrupt, stop, archive, fork, and relate nodes.
```

- [ ] **Step 2: Update Cockpit requirements**

In the Cockpit requirements table, change `COCKPIT-006` acceptance to:

```md
Selecting a graph/table/chat rail node focuses its real session and can show metadata, events, capabilities, and relationships without a separate attach action.
```

Change `COCKPIT-010` to:

```md
| COCKPIT-010 | Cockpit controls call real capabilities. | Send input, interrupt, stop, fork, archive, relationship actions, and recipe actions call daemon APIs and surface errors. Cockpit does not expose attach as a normal user workflow. |
```

Change `COCKPIT-015` acceptance to:

```md
Cmd-K can navigate screens, find nodes, launch nodes, and send remote commands without fake action paths.
```

Change `COCKPIT-017` acceptance to:

```md
No Tweaks panel, `simSpeed`, canned `runResponse`, hardcoded demo nodes, fake settings, fake logs, fake attach preview output, visible attach workflow, or no-op buttons ship in `cockpit/src`.
```

- [ ] **Step 3: Update prototype workflow intent**

Change `PROTO-002` acceptance to:

```md
Launch command center, observe graph, inspect nodes, open node sessions, remote notifications, channels, hooks, fleet table, and settings are real workflows.
```

Leave backend transport references such as `/api/attach/{token}/ws`, signed attach URLs, `node.attach.browser`, `node.attach.native_target`, and Loon `attach` contract names in place when the spec is describing daemon/API compatibility or security internals.

- [ ] **Step 4: Mark completed plan checkboxes**

As each task lands, update this plan's checkboxes from `- [ ]` to `- [x]` for the completed steps in `docs/superpowers/plans/2026-05-09-cockpit-node-session-ux.md`.

- [ ] **Step 5: Commit Task 4**

Run:

```bash
git add docs/specs/asylum-current-product-spec.md docs/superpowers/plans/2026-05-09-cockpit-node-session-ux.md
git commit -m "docs: align cockpit session language"
```

## Task 5: Full Verification And Browser Smoke

**Files:**
- Modify: `docs/superpowers/plans/2026-05-09-cockpit-node-session-ux.md`

- [ ] **Step 1: Run Cockpit tests**

Run:

```bash
npm --prefix cockpit run test
```

Expected: PASS.

- [ ] **Step 2: Run Cockpit build**

Run:

```bash
npm --prefix cockpit run build
```

Expected: PASS.

- [ ] **Step 3: Run backend attach compatibility tests**

Run:

```bash
cargo test -p asylum-daemon attach
```

Expected: PASS. Backend attach token plumbing should remain stable even though Cockpit no longer exposes attach as normal UX.

- [ ] **Step 4: Start the dev stack for rendered validation**

Run in a long-lived terminal:

```bash
cargo dev
```

Expected: daemon and Cockpit become available at `http://127.0.0.1:7788/`.

- [ ] **Step 5: Open Cockpit with Playwright CLI**

Run from the Playwright cache output directory:

```bash
playwright-cli -s=validator open http://127.0.0.1:7788/ --config=/home/casey/.codex/playwright-cli.config.json --persistent --profile=/home/casey/.cache/codex-playwright-cli/profile
```

Expected: browser opens Cockpit. If auth is required, provide the owner token through the rendered prompt before continuing.

- [ ] **Step 6: Validate rendered node/session UX**

Run:

```bash
playwright-cli -s=validator snapshot
playwright-cli -s=validator console error
playwright-cli -s=validator requests
```

Expected:

- Graph/fleet/main Cockpit views show node/session terminology, not attach terminology.
- Selecting a node focuses the visible session panel.
- Expanding the session opens the node workspace/session page.
- The node workspace defaults to the session tab.
- No visible button says `attach`, `open attach tab`, `browser attach`, `native attach`, `attach URL`, or `open in terminal`.
- Console errors output is empty.
- Failed network requests do not include failed Cockpit API calls.

Use element refs from the snapshot to click a graph node, a fleet row, and the session expand button. Record the exact refs and observations in this plan under the verification notes before committing.

- [ ] **Step 7: Close and clean Playwright scratch**

Run:

```bash
playwright-cli -s=validator close
rm -rf /home/casey/.cache/codex-playwright-cli/output/.playwright-cli
```

Expected: browser session closes and scratch snapshots are removed from the Playwright output directory.

- [ ] **Step 8: Record verification notes**

Append a short verification note under this task with:

- test commands run,
- browser URL,
- graph/fleet/node interactions checked,
- console/network result,
- any known residual backend attach terminology that remains intentionally internal.

- [ ] **Step 9: Commit verification notes**

Run:

```bash
git add docs/superpowers/plans/2026-05-09-cockpit-node-session-ux.md
git commit -m "docs: record session ux verification"
```

## Release Status

Planned implementation, not released. When implementation lands, update this section to one of:

- `Released as vX.Y.Z` with a GitHub release link and shipped platforms.
- `On main, not released - awaiting authorization. Last release: v0.1.10 (2026-05-07).`
- `Doc-only / internal - no release needed. Last release: v0.1.10 (2026-05-07).`

See [RELEASES.md](../../RELEASES.md) for the release ledger.
