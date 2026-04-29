import { describe, it, expect, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useUiPrefs, DEFAULT_UI_PREFS } from "./uiPrefs";

beforeEach(() => {
  window.localStorage.clear();
});

describe("useUiPrefs", () => {
  it("returns defaults when nothing is stored", () => {
    const { result } = renderHook(() => useUiPrefs());
    expect(result.current[0]).toEqual(DEFAULT_UI_PREFS);
  });

  it("persists updates to localStorage", () => {
    const { result } = renderHook(() => useUiPrefs());
    act(() => result.current[1]("theme", "light"));
    const stored = JSON.parse(window.localStorage.getItem("asylum.uiPrefs")!);
    expect(stored.theme).toBe("light");
  });

  it("hydrates from existing localStorage value", () => {
    window.localStorage.setItem(
      "asylum.uiPrefs",
      JSON.stringify({ theme: "light", navCollapsed: true, graphLayout: "force" }),
    );
    const { result } = renderHook(() => useUiPrefs());
    expect(result.current[0].theme).toBe("light");
    expect(result.current[0].navCollapsed).toBe(true);
    expect(result.current[0].graphLayout).toBe("force");
  });

  it("ignores unknown keys in stored value", () => {
    window.localStorage.setItem("asylum.uiPrefs", JSON.stringify({ simSpeed: "live" }));
    const { result } = renderHook(() => useUiPrefs());
    expect(result.current[0]).toEqual(DEFAULT_UI_PREFS);
  });
});
