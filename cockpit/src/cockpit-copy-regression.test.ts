import { describe, expect, it } from "vitest";
// @ts-ignore
import fs from "fs";
// @ts-ignore
import path from "path";
// @ts-ignore
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const SOURCE_FILES = [
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
] as const;

const FORBIDDEN_PATTERNS: Record<string, RegExp> = {
  "open attach": /\bopen attach\b/i,
  "browser attach": /\bbrowser attach\b/i,
  "native attach": /\bnative attach\b/i,
  "attach tab": /\battach tab\b/i,
  "attach url": /\battach url\b/i,
  "attach link": /\battach link\b/i,
  "terminal attach": /\bterminal attach\b/i,
  "use attach": /\buse attach\b/i,
  "attached to": /\battached to\b/i,
};

describe("copied copy regression guard", () => {
  it("rejects deprecated attach copy in cockpit visible surface files", () => {
    const violations: string[] = [];

    for (const relPath of SOURCE_FILES) {
      const absPath = path.join(__dirname, relPath);
      const raw = fs.readFileSync(absPath, "utf8").toLowerCase();

      for (const [label, pattern] of Object.entries(FORBIDDEN_PATTERNS)) {
        if (pattern.test(raw)) {
          violations.push(`${relPath}: found "${label}"`);
        }
        pattern.lastIndex = 0;
      }
    }

    expect(violations).toEqual([]);
  });
});
