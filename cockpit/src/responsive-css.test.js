import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(resolve(here, "cockpit.css"), "utf8");

describe("Cockpit responsive layout guardrails", () => {
  it("keeps the mobile shell stacked instead of fixed-width rail driven", () => {
    expect(css).toContain("@media (max-width: 760px)");
    expect(css).toContain(".body.nav-collapsed");
    expect(css).toContain("grid-template-columns: 1fr");
    expect(css).toContain(".nav");
    expect(css).toContain("border-bottom: 1px solid var(--border)");
  });

  it("keeps wide operational tables and node relationship controls contained on narrow screens", () => {
    expect(css).toContain(".panel { overflow-x: auto; }");
    expect(css).toContain(".panel .table { width: max-content; min-width: 760px; }");
    expect(css).toContain(".node-page");
    expect(css).toContain("grid-template-columns: minmax(0, 1fr)");
    expect(css).toContain(".rel-create-grid");
    expect(css).toContain("grid-template-columns: minmax(0, 1fr) minmax(0, 1fr)");
  });

  it("keeps the notifications toolbar wrapped and readable on narrow screens", () => {
    expect(css).toContain(".logs-toolbar");
    expect(css).toContain("flex-wrap: wrap");
    expect(css).toContain(".logs-toolbar .search");
    expect(css).toContain("flex: 1 1 100% !important");
    expect(css).toContain(".logs-count");
    expect(css).toContain("margin-left: 0");
  });
});
