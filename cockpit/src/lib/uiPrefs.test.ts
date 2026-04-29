import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useUiPrefs, DEFAULT_UI_PREFS } from "./uiPrefs";

// jsdom localStorage in the vitest worker may not support clear() directly.
// Use a simple in-memory mock so we can test the hook's storage behaviour.
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (k: string) => store[k] ?? null,
    setItem: (k: string, v: string) => { store[k] = v; },
    removeItem: (k: string) => { delete store[k]; },
    clear: () => { store = {}; },
  };
})();

beforeEach(() => {
  localStorageMock.clear();
  vi.stubGlobal("localStorage", localStorageMock);
});

describe("useUiPrefs", () => {
  it("returns defaults when nothing is stored", () => {
    const { result } = renderHook(() => useUiPrefs());
    expect(result.current[0]).toEqual(DEFAULT_UI_PREFS);
  });

  it("persists updates to localStorage", () => {
    const { result } = renderHook(() => useUiPrefs());
    act(() => result.current[1]("theme", "light"));
    const stored = JSON.parse(localStorageMock.getItem("asylum.uiPrefs")!);
    expect(stored.theme).toBe("light");
  });

  it("hydrates from existing localStorage value", () => {
    localStorageMock.setItem(
      "asylum.uiPrefs",
      JSON.stringify({ theme: "light", navCollapsed: true, graphLayout: "force" }),
    );
    const { result } = renderHook(() => useUiPrefs());
    expect(result.current[0].theme).toBe("light");
    expect(result.current[0].navCollapsed).toBe(true);
    expect(result.current[0].graphLayout).toBe("force");
  });

  it("ignores unknown keys in stored value", () => {
    localStorageMock.setItem("asylum.uiPrefs", JSON.stringify({ simSpeed: "live" }));
    const { result } = renderHook(() => useUiPrefs());
    expect(result.current[0]).toEqual(DEFAULT_UI_PREFS);
  });
});
