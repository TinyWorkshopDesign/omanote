// Theming. On Omarchy the palette comes from the system (and follows
// `omarchy-theme-set` live); elsewhere the user picks one of the bundled
// Omarchy themes, remembered per device.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import bundled from "./themes.json";

export interface OmarchyTheme {
  name: string;
  mode: string;
  colors: Record<string, string>;
}

export interface BundledTheme {
  name: string;
  mode: string;
  accent: string;
  bg: string;
}

export const bundledThemes: BundledTheme[] = Object.entries(
  bundled as Record<string, { mode: string; accent: string; bg: string }>,
).map(([name, t]) => ({ name, ...t }));

const STORAGE_KEY = "omanote.theme";
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Maps an Omarchy palette onto the app's CSS variables. */
function applySystem(t: OmarchyTheme) {
  const c = t.colors;
  const pick = (...keys: string[]) => keys.map((k) => c[k]).find(Boolean) ?? "";
  const vars: Record<string, string> = {
    "--bg": pick("background"),
    "--bar": pick("dark_background", "background"),
    "--bg-elev": pick("lighter_background", "background"),
    "--panel": pick("selection", "lighter_background", "background"),
    "--text": pick("foreground"),
    "--muted": pick("dark_foreground", "muted", "foreground"),
    "--line": pick("selection", "muted", "foreground"),
    "--accent": pick("accent", "blue"),
    "--result": pick("green", "accent"),
    "--danger": pick("red", "accent"),
  };
  const root = document.documentElement;
  root.removeAttribute("data-theme");
  root.style.colorScheme = t.mode === "light" ? "light" : "dark";
  for (const [k, v] of Object.entries(vars)) if (v) root.style.setProperty(k, v);
}

function applyBundled(name: string) {
  const root = document.documentElement;
  for (const k of ["--bg", "--bar", "--bg-elev", "--panel", "--text", "--muted", "--line", "--accent", "--result", "--danger"]) {
    root.style.removeProperty(k);
  }
  root.style.removeProperty("color-scheme");
  root.setAttribute("data-theme", name);
}

export interface ThemeState {
  /** Name of the theme in use. */
  current: string;
  /** True when the palette is provided by Omarchy itself. */
  fromSystem: boolean;
}

/**
 * Applies the right theme and keeps following the system one.
 * Returns the current state plus an unsubscribe function.
 */
export async function initTheme(
  onChange: (s: ThemeState) => void,
): Promise<() => void> {
  const stored = localStorage.getItem(STORAGE_KEY);

  let system: OmarchyTheme | null = null;
  if (inTauri) {
    try {
      system = await invoke<OmarchyTheme | null>("get_system_theme");
    } catch {
      system = null;
    }
  }

  if (system) {
    applySystem(system);
    onChange({ current: system.name, fromSystem: true });
    if (!inTauri) return () => {};
    const off = await listen<OmarchyTheme>("theme-changed", (e) => {
      applySystem(e.payload);
      onChange({ current: e.payload.name, fromSystem: true });
    });
    return off;
  }

  const name = stored && bundledThemes.some((t) => t.name === stored) ? stored : "tokyo-night";
  applyBundled(name);
  onChange({ current: name, fromSystem: false });
  return () => {};
}

export function setBundledTheme(name: string) {
  applyBundled(name);
  localStorage.setItem(STORAGE_KEY, name);
}
