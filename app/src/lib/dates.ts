// Dates in the note list: a short date on each row and the section the row
// belongs to (Today, Yesterday, Previous 7 Days, Previous 30 Days, then months).

export type Section =
  | { kind: "today" | "yesterday" | "week" | "month" }
  | { kind: "older"; label: string };

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

const DAY = 86_400_000;

/** The section a note edited at `ms` falls in, relative to `now`. */
export function sectionOf(ms: number, lang: string, now = Date.now()): Section {
  const today = startOfDay(new Date(now));
  if (ms >= today) return { kind: "today" };
  if (ms >= today - DAY) return { kind: "yesterday" };
  if (ms >= today - 7 * DAY) return { kind: "week" };
  if (ms >= today - 30 * DAY) return { kind: "month" };
  const d = new Date(ms);
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  const label = d.toLocaleDateString(lang, sameYear ? { month: "long" } : { month: "long", year: "numeric" });
  return { kind: "older", label: label.charAt(0).toUpperCase() + label.slice(1) };
}

export function sectionKey(s: Section): string {
  return s.kind === "older" ? `older:${s.label}` : s.kind;
}

/** Row date: the time today, the weekday this week, otherwise the date. */
export function rowDate(ms: number, lang: string, now = Date.now()): string {
  const d = new Date(ms);
  const today = startOfDay(new Date(now));
  if (ms >= today) return d.toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });
  if (ms >= today - 6 * DAY) return d.toLocaleDateString(lang, { weekday: "short" });
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  return d.toLocaleDateString(lang, { day: "numeric", month: "short", year: sameYear ? undefined : "2-digit" });
}

/** "Edited …" line above the note. */
export function fullDate(ms: number, lang: string): string {
  return new Date(ms).toLocaleString(lang, { dateStyle: "long", timeStyle: "short" });
}
