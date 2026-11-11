// UI preferences of this device, kept in localStorage. They are conveniences
// (layout, sorting, what opens at launch), never note data, so a private window
// or cleared storage simply falls back to the defaults.

export type Launch = "last" | "new";
export type Sort = "edited" | "title";

export interface Prefs {
  /** The toolbar hides itself and shows when the pointer nears the top edge. */
  autoHideToolbar: boolean;
  /** At launch: reopen the last note, or start a new one. */
  launch: Launch;
  sort: Sort;
  fontSize: number;
  /** Folders sidebar hidden (3-column layout). */
  sidebarHidden: boolean;
  /** Only the note: sidebar and list hidden. */
  focus: boolean;
  /** Selection restored at launch: a folder id, "all" or "trash". */
  lastFolder: string;
  lastNote: string;
  /** Desktop: look for a newer version on GitHub when the app starts. */
  autoUpdateCheck: boolean;
}

const KEY = "omanote.prefs";
// Before the revamp the text size had its own key.
const OLD_FONT_KEY = "omanote.fontSize";

const defaults: Prefs = {
  autoHideToolbar: false,
  launch: "last",
  sort: "edited",
  fontSize: 14,
  sidebarHidden: false,
  focus: false,
  lastFolder: "all",
  lastNote: "",
  autoUpdateCheck: true,
};

function load(): Prefs {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<Prefs>;
    const font = Number(localStorage.getItem(OLD_FONT_KEY));
    return { ...defaults, ...(font ? { fontSize: font } : {}), ...saved };
  } catch {
    return { ...defaults };
  }
}

export const prefs: Prefs = $state(load());

/** Saves the preferences; call it from an `$effect` that reads them. */
export function savePrefs(p: Prefs) {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    /* storage unavailable: the preferences last for this session only */
  }
}

export const FONT_MIN = 11;
export const FONT_MAX = 24;
export const FONT_DEFAULT = defaults.fontSize;
