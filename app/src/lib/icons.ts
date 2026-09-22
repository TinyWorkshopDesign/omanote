// Interface icons, drawn the way Omarchy draws the bar, the menu and the panels:
// monochrome glyphs from a Nerd Font (see Omarchy's manual/38-fonts.md and
// shell/Ui/OpticalGlyph.qml — its menu names glyphs such as U+F035C for "menu").
//
// The glyphs come from Nerd Fonts' Symbols Nerd Font (MIT) and live in
// app/static/fonts/omanote-symbols.woff2 as a subset: `tools/gen-symbol-font.py`
// cuts it from the pinned upstream release and writes symbols.generated.ts with
// the name -> codepoint map. Add an icon by naming its glyph there and rerunning
// that script.
//
// They use currentColor, so they follow the theme. Used both in Svelte ({@html})
// and in CodeMirror widgets.

import { CODEPOINTS, type IconName } from "./symbols.generated";

/**
 * One icon as HTML. The text is a private-use glyph drawn by the bundled font
 * (the `.ico` rule in app.css sizes and centres it); `aria-hidden` because every
 * caller labels the control itself.
 */
function glyph(name: IconName): string {
  return `<span class="ico" aria-hidden="true">${String.fromCodePoint(CODEPOINTS[name])}</span>`;
}

/**
 * Sets `--ico-s` / `--ico-l` (small and large icon sizes, in CSS px). Glyphs are
 * text: whole device pixels keep them crisp at any display scale (1.25 on many
 * Linux laptops, 2 on Retina).
 */
export function fitIconsToScreen() {
  const apply = () => {
    const dpr = window.devicePixelRatio || 1;
    const size = (target: number) => Math.round(target * dpr) / dpr;
    const root = document.documentElement.style;
    root.setProperty("--ico-s", `${size(15)}px`);
    root.setProperty("--ico-l", `${size(22)}px`);
    matchMedia(`(resolution: ${dpr}dppx)`).addEventListener("change", apply, { once: true });
  };
  apply();
}

export const icons = {
  /** Folder tree panel. */
  menu: glyph("menu"),
  /** New note. */
  plus: glyph("plus"),
  /** Close a panel or remove something. */
  close: glyph("close"),
  /** Settings panel. */
  settings: glyph("settings"),
  /** Search notes. */
  search: glyph("search"),
  /** Keep the window on top. */
  pin: glyph("pin"),
  /** OCR of a screen region. */
  capture: glyph("capture"),
  /** Sync: two arrows chasing each other, coloured by state. */
  sync: glyph("sync"),
  /** Folder options. */
  more: glyph("more"),
  /** Previous note in the stack. */
  prev: glyph("prev"),
  /** Next note in the stack. */
  next: glyph("next"),
  /** Collapsed tree branch. */
  expand: glyph("expand"),
  /** Open tree branch. */
  collapse: glyph("collapse"),
  /** Conflicting copy of a note. */
  warn: glyph("warn"),
  /** Picture: a framed landscape. */
  image: glyph("image"),
  /** Text from an image. */
  ocr: glyph("ocr"),
  /** Encrypted note. */
  lock: glyph("lock"),
  /** New folder. */
  folderAdd: glyph("folderAdd"),
  /** A note in the tree. */
  note: glyph("note"),
  /** Joplin's trash. */
  trash: glyph("trash"),
  /** Take back out of the trash: an arrow turning back. */
  restore: glyph("restore"),
};
