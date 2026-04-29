import { useCallback, useEffect, useState } from "react";
import type { GraphLayout } from "../screens/CockpitScreen";

export interface UiPrefs {
  theme: "dark" | "light";
  navCollapsed: boolean;
  graphLayout: GraphLayout;
}

export const DEFAULT_UI_PREFS: UiPrefs = {
  theme: "dark",
  navCollapsed: false,
  graphLayout: "tree",
};

const STORAGE_KEY = "asylum.uiPrefs";
const VALID_LAYOUTS: GraphLayout[] = ["tree", "free", "force", "swimlanes"];

function readStored(): UiPrefs {
  if (typeof window === "undefined") return DEFAULT_UI_PREFS;
  const raw = window.localStorage.getItem(STORAGE_KEY);
  if (!raw) return DEFAULT_UI_PREFS;
  try {
    const parsed = JSON.parse(raw) as Partial<UiPrefs>;
    return {
      theme: parsed.theme === "light" ? "light" : "dark",
      navCollapsed: Boolean(parsed.navCollapsed),
      graphLayout: VALID_LAYOUTS.includes(parsed.graphLayout as GraphLayout)
        ? (parsed.graphLayout as GraphLayout)
        : "tree",
    };
  } catch {
    return DEFAULT_UI_PREFS;
  }
}

export function useUiPrefs(): [UiPrefs, <K extends keyof UiPrefs>(k: K, v: UiPrefs[K]) => void] {
  const [prefs, setPrefs] = useState<UiPrefs>(readStored);

  useEffect(() => {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  }, [prefs]);

  const setPref = useCallback(<K extends keyof UiPrefs>(k: K, v: UiPrefs[K]) => {
    setPrefs((cur) => ({ ...cur, [k]: v }));
  }, []);

  return [prefs, setPref];
}
