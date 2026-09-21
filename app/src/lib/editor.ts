// CodeMirror 6 setup for Omanote: Joplin Markdown notes that behave like
// Scratchpad behaviour — first-line keywords (list, math, sum, avg, count, code), live
// checkboxes, inline math, timers, image paste/drop — with a live preview:
// real Markdown (headings, emphasis, code, quotes, links), Joplin's rich-text
// HTML (coloured <span>s, <img>, &nbsp;) and attachments, whose raw syntax
// reappears on the lines the cursor is on.

import {
  EditorSelection,
  EditorState,
  type ChangeSpec,
  type Extension,
  type Range,
  type Text,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  WidgetType,
  keymap,
  placeholder,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentLess, indentMore } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { tags } from "@lezer/highlight";
import { countIn, detectMode, evaluate, formatResult, numbersIn, type Mode } from "./calc";
import { t } from "./i18n.svelte";
import { icons } from "./icons";

const TASK = /^(\s*)([-*+]) \[( |x|X)\] /;
const BARE_TASK = /^(\s*)\[( |x|X)?\] /; // "[] " / "[x] " shorthand, normalised to "- [ ] "
const BULLET = /^(\s*)([-*+]) (?!\[[ xX]\] )/;
const NUMBERED = /^(\s*)(\d+)([.)]) /;
const HEADING = /^(#{1,6}) /;
const COMMENT = /^\s*\/\//;
const FENCE = /^\s*```/;
const CHECK_TRIGGER = "/x";

function modeOf(doc: Text): Mode {
  return detectMode(doc.line(1).text).mode;
}

function hasMarker(text: string) {
  return TASK.test(text) || BULLET.test(text) || NUMBERED.test(text);
}

/**
 * In "list" notes a line becomes an item only if it continues the list: it
 * follows the keyword line, an item, a heading or a comment. A blank line
 * (double Enter) ends the list, and what comes after stays plain text.
 */
function continuesList(doc: Text, n: number, marked: Set<number>): boolean {
  if (n - 1 <= 1) return true;
  const prev = doc.line(n - 1).text;
  return marked.has(n - 1) || hasMarker(prev) || BARE_TASK.test(prev) || HEADING.test(prev) || COMMENT.test(prev);
}

// ---------------------------------------------------------------------------
// Widgets
// ---------------------------------------------------------------------------

class CheckboxWidget extends WidgetType {
  constructor(readonly checked: boolean, readonly pos: number) {
    super();
  }
  eq(o: CheckboxWidget) {
    return o.checked === this.checked && o.pos === this.pos;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = "cm-task" + (this.checked ? " cm-task-checked" : "");
    el.setAttribute("role", "checkbox");
    el.setAttribute("aria-checked", String(this.checked));
    el.dataset.pos = String(this.pos);
    return el;
  }
  ignoreEvent() {
    return false;
  }
}

class TextWidget extends WidgetType {
  constructor(readonly text: string, readonly cls: string, readonly copy = false) {
    super();
  }
  eq(o: TextWidget) {
    return o.text === this.text && o.cls === this.cls;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = this.cls;
    el.textContent = this.text;
    if (this.copy) el.dataset.copy = "1";
    return el;
  }
  ignoreEvent() {
    return false;
  }
}

const bullet = Decoration.replace({ widget: new TextWidget("•", "cm-bullet") });
const doneLine = Decoration.line({ class: "cm-task-done" });
const titleLine = Decoration.line({ class: "cm-title-line" });
const resultLine = Decoration.line({ class: "cm-has-result" });
const commentLine = Decoration.line({ class: "cm-comment" });
const codeLine = Decoration.line({ class: "cm-code" });
const keywordMark = Decoration.mark({ class: "cm-keyword" });

// ---------------------------------------------------------------------------
// Decorations
// ---------------------------------------------------------------------------

interface Built {
  all: DecorationSet;
  atomic: DecorationSet;
}

function build(view: EditorView): Built {
  const doc = view.state.doc;
  const text = doc.toString();
  const first = doc.line(1).text;
  const { mode } = detectMode(first);
  // Math works in lists too; only code notes stay raw.
  // Results show after a trailing "=" (every calculation in "math" notes).
  const results = mode === "code" ? [] : evaluate(text, mode === "math");
  const deco: Range<Decoration>[] = [];
  const atomic: Range<Decoration>[] = [];
  let inFence = false;

  for (const { from, to } of view.visibleRanges) {
    let pos = from;
    while (pos <= to) {
      const line = doc.lineAt(pos);
      const s = line.text;
      pos = line.to + 1;

      if (line.number === 1) {
        if (s.trim() !== "") deco.push(titleLine.range(line.from));
        if (mode !== "plain") {
          const kw = /^\s*[\p{L}]+/u.exec(s)!;
          deco.push(keywordMark.range(line.from, line.from + kw[0].length));
          const summary = modeSummary(mode, text);
          if (summary) {
            deco.push(resultLine.range(line.from));
            deco.push(Decoration.widget({ widget: new TextWidget(summary, "cm-result", true), side: 1 }).range(line.to));
          }
        } else {
          // A quick note is often just "2+2" or "x = 5": the first line computes too.
          const res = results[0];
          if (res?.show) deco.push(inlineResult(s, line.from, formatResult(res)));
        }
        continue;
      }

      if (FENCE.test(s)) inFence = !inFence;
      if (mode === "code" || inFence || FENCE.test(s)) {
        deco.push(codeLine.range(line.from));
        continue;
      }
      if (COMMENT.test(s)) {
        deco.push(commentLine.range(line.from));
        continue;
      }

      const task = TASK.exec(s);
      const res = results[line.number - 1];
      if (task && task[3] !== " ") deco.push(doneLine.range(line.from));

      if (task) {
        const start = line.from + task[1].length;
        const r = Decoration.replace({ widget: new CheckboxWidget(task[3] !== " ", start) }).range(
          start,
          line.from + task[0].length,
        );
        deco.push(r);
        atomic.push(r);
      } else {
        const bl = BULLET.exec(s);
        if (bl) {
          const r = bullet.range(line.from + bl[1].length, line.from + bl[1].length + 1);
          deco.push(r);
          atomic.push(r);
        }
      }

      if (res?.show) {
        deco.push(inlineResult(s, line.from, formatResult(res)));
      }
    }
  }
  return { all: Decoration.set(deco, true), atomic: Decoration.set(atomic, true) };
}

/** The result sits right after the "=" (or after the text in "math" notes). */
function inlineResult(text: string, lineFrom: number, value: string): Range<Decoration> {
  const eq = text.search(/=\s*$/);
  const at = eq >= 0 ? lineFrom + eq + 1 : lineFrom + text.trimEnd().length;
  const shown = eq >= 0 ? value : `= ${value}`;
  return Decoration.widget({ widget: new TextWidget(shown, "cm-result-inline", true), side: 1 }).range(at);
}

function modeSummary(mode: Mode, text: string): string | null {
  if (mode === "sum" || mode === "avg") {
    const nums = numbersIn(text);
    if (!nums.length) return null;
    const total = nums.reduce((a, b) => a + b, 0);
    const value = mode === "sum" ? total : total / nums.length;
    return formatResult({ value, unit: "", pct: false, show: true });
  }
  if (mode === "count") {
    const c = countIn(text);
    return t("mode.count", { items: c.items, lines: c.lines, words: c.words, chars: c.chars });
  }
  return null;
}

const decorations = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    atomic: DecorationSet;
    constructor(view: EditorView) {
      ({ all: this.decorations, atomic: this.atomic } = build(view));
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged) ({ all: this.decorations, atomic: this.atomic } = build(u.view));
    }
  },
  {
    decorations: (v) => v.decorations,
    provide: (p) => EditorView.atomicRanges.of((view) => view.plugin(p)?.atomic ?? Decoration.none),
    eventHandlers: {
      mousedown(e, view) {
        const el = e.target as HTMLElement;
        if (el.classList.contains("cm-task")) {
          toggleTaskAt(view, Number(el.dataset.pos));
          e.preventDefault();
          return true;
        }
        if (el.dataset.copy) {
          void navigator.clipboard?.writeText(el.textContent ?? "");
          el.classList.add("cm-result-copied");
          setTimeout(() => el.classList.remove("cm-result-copied"), 700);
          e.preventDefault();
          return true;
        }
        return false;
      },
    },
  },
);

// ---------------------------------------------------------------------------
// Automatic lists: "list" keyword, "[] " shorthand, "/x" check trigger
// ---------------------------------------------------------------------------

const autoLists = EditorState.transactionFilter.of((tr) => {
  if (!tr.docChanged) return tr;
  const doc = tr.newDoc;
  const mode = modeOf(doc);
  const becameList = mode === "list" && modeOf(tr.startState.doc) !== "list";
  const touched = new Set<number>();
  if (becameList) {
    for (let n = 2; n <= doc.lines; n++) touched.add(n);
  } else {
    tr.changes.iterChangedRanges((_fa, _ta, fb, tb) => {
      const a = doc.lineAt(fb).number;
      const b = doc.lineAt(Math.min(tb, doc.length)).number;
      for (let n = a; n <= b; n++) if (n > 1) touched.add(n);
    });
  }

  const changes: ChangeSpec[] = [];
  const marked = new Set<number>();
  for (const n of [...touched].sort((a, b) => a - b)) {
    const line = doc.line(n);
    const s = line.text;
    if (mode === "code" || FENCE.test(s)) continue;

    const bare = BARE_TASK.exec(s);
    const task = TASK.exec(s);
    const endsWithTrigger = s.trimEnd().endsWith(CHECK_TRIGGER);
    const needsMarker =
      mode === "list" &&
      !bare &&
      s.trim() !== "" &&
      !hasMarker(s) &&
      !HEADING.test(s) &&
      !COMMENT.test(s) &&
      continuesList(doc, n, marked);
    if (needsMarker) marked.add(n);
    const end = line.from + s.trimEnd().length;

    if (bare) {
      // "[] item" → "- [ ] item": plain Markdown, rendered as a checkbox by Joplin too.
      const isChecked = bare[2] === "x" || bare[2] === "X";
      const checked = endsWithTrigger ? !isChecked : isChecked;
      changes.push({ from: line.from + bare[1].length, to: line.from + bare[0].length, insert: checked ? "- [x] " : "- [ ] " });
      if (endsWithTrigger) changes.push({ from: end - CHECK_TRIGGER.length, to: end });
    } else if (needsMarker) {
      const indent = /^\s*/.exec(s)![0].length;
      changes.push({ from: line.from + indent, insert: endsWithTrigger ? "- [x] " : "- [ ] " });
      if (endsWithTrigger) changes.push({ from: end - CHECK_TRIGGER.length, to: end });
    } else if (task && endsWithTrigger) {
      const at = line.from + task[1].length + 3;
      changes.push({ from: at, to: at + 1, insert: task[3] === " " ? "x" : " " });
      changes.push({ from: end - CHECK_TRIGGER.length, to: end });
    }
  }
  return changes.length ? [tr, { changes, sequential: true }] : tr;
});

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

function toggleTaskAt(view: EditorView, pos: number) {
  const line = view.state.doc.lineAt(pos);
  const m = TASK.exec(line.text);
  if (!m) return;
  const at = line.from + m[1].length + 3;
  view.dispatch({ changes: { from: at, to: at + 1, insert: m[3] === " " ? "x" : " " } });
}

function selectedLines(state: EditorState) {
  const seen = new Map<number, ReturnType<Text["line"]>>();
  for (const r of state.selection.ranges) {
    const a = state.doc.lineAt(r.from).number;
    const b = state.doc.lineAt(r.to).number;
    for (let n = a; n <= b; n++) seen.set(n, state.doc.line(n));
  }
  return [...seen.values()];
}

/** Mod-Shift-K: check off the current line (making it a checkbox if needed). */
export function checkLine(view: EditorView): boolean {
  const changes: ChangeSpec[] = [];
  for (const line of selectedLines(view.state)) {
    const task = TASK.exec(line.text);
    if (task) {
      const at = line.from + task[1].length + 3;
      changes.push({ from: at, to: at + 1, insert: task[3] === " " ? "x" : " " });
    } else {
      const bl = BULLET.exec(line.text) ?? NUMBERED.exec(line.text);
      const indent = /^\s*/.exec(line.text)![0].length;
      changes.push({ from: line.from + indent, to: bl ? line.from + bl[0].length : undefined, insert: "- [x] " });
    }
  }
  view.dispatch({ changes });
  return true;
}

/** Mod-Shift-M: checkbox → bullet → numbered → checkbox. */
function cycleMarker(view: EditorView): boolean {
  const changes: ChangeSpec[] = [];
  let n = 0;
  for (const line of selectedLines(view.state)) {
    const indent = /^\s*/.exec(line.text)![0].length;
    const task = TASK.exec(line.text);
    const num = NUMBERED.exec(line.text);
    const bl = BULLET.exec(line.text);
    const cur = task ?? num ?? bl;
    const next = task ? "- " : bl ? `${++n}. ` : "- [ ] ";
    changes.push({ from: line.from + indent, to: cur ? line.from + cur[0].length : line.from + indent, insert: next });
  }
  view.dispatch({ changes });
  return true;
}

/** Wraps the selection (or the word at the cursor) in a Markdown marker; toggles. */
function wrap(marker: string) {
  return (view: EditorView): boolean => {
    const { state } = view;
    const k = marker.length;
    const tr = state.changeByRange((range) => {
      let { from, to } = range;
      if (from === to) {
        const w = state.wordAt(from);
        if (w) ({ from, to } = w);
      }
      if (state.sliceDoc(from - k, from) === marker && state.sliceDoc(to, to + k) === marker) {
        return {
          changes: [
            { from: from - k, to: from },
            { from: to, to: to + k },
          ],
          range: EditorSelection.range(from - k, to - k),
        };
      }
      return {
        changes: [
          { from, insert: marker },
          { from: to, insert: marker },
        ],
        range: EditorSelection.range(from + k, to + k),
      };
    });
    view.dispatch(tr);
    return true;
  };
}

/** Mod-Shift-H: # → ## → ### → no heading. */
function cycleHeading(view: EditorView): boolean {
  const changes: ChangeSpec[] = [];
  for (const line of selectedLines(view.state)) {
    const h = HEADING.exec(line.text);
    const level = h ? h[1].length : 0;
    const next = level >= 3 ? "" : "#".repeat(level + 1) + " ";
    changes.push({ from: line.from, to: line.from + (h ? h[0].length : 0), insert: next });
  }
  view.dispatch({ changes });
  return true;
}

/** Mod-/: toggle "// " at the start of the selected lines. */
function toggleComment(view: EditorView): boolean {
  const lines = selectedLines(view.state);
  const allCommented = lines.every((l) => COMMENT.test(l.text));
  const changes: ChangeSpec[] = lines.map((l) => {
    const indent = /^\s*/.exec(l.text)![0].length;
    if (allCommented) {
      const m = /^\s*\/\/ ?/.exec(l.text)!;
      return { from: l.from + indent, to: l.from + m[0].length };
    }
    return { from: l.from + indent, insert: "// " };
  });
  view.dispatch({ changes });
  return true;
}

/** Enter: continue lists/checklists; on an empty item, end the list. */
function continueList(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  if (!sel.empty || modeOf(state.doc) === "code") return false;
  const line = state.doc.lineAt(sel.head);
  const task = TASK.exec(line.text);
  const bl = BULLET.exec(line.text);
  const num = NUMBERED.exec(line.text);
  const m = task ?? bl ?? num;
  if (!m || line.number === 1) return false;
  if (sel.head < line.from + m[0].length) return false;

  if (line.text.trim() === m[0].trim()) {
    if (modeOf(state.doc) === "list") {
      // Double Enter ends the list: blank line, then plain text below.
      view.dispatch({
        changes: { from: line.from, to: line.to, insert: "\n" },
        selection: { anchor: line.from + 1 },
        scrollIntoView: true,
      });
    } else {
      view.dispatch({ changes: { from: line.from, to: line.to, insert: "" } });
    }
    return true;
  }
  let marker: string;
  if (task) marker = `${task[1]}${task[2]} [ ] `;
  else if (num) marker = `${num[1]}${Number(num[2]) + 1}${num[3]} `;
  else marker = `${bl![1]}${bl![2]} `;
  view.dispatch({
    changes: { from: sel.head, insert: "\n" + marker },
    selection: { anchor: sel.head + 1 + marker.length },
    scrollIntoView: true,
  });
  return true;
}

// ---------------------------------------------------------------------------
// Live preview
// ---------------------------------------------------------------------------

type ResolveImage = (id: string) => Promise<string | null>;

/** An attachment shown in place of `![alt](:/id)` or `<img src=":/id">`. */
class ImageWidget extends WidgetType {
  constructor(
    readonly id: string,
    readonly alt: string,
    readonly width: number,
    readonly len: number,
    readonly resolve: ResolveImage,
  ) {
    super();
  }
  eq(o: ImageWidget) {
    return o.id === this.id && o.alt === this.alt && o.width === this.width && o.len === this.len;
  }
  toDOM(view: EditorView) {
    const wrap = document.createElement("span");
    wrap.className = "cm-image";
    const img = document.createElement("img");
    img.alt = this.alt;
    if (this.width) img.style.width = `min(${this.width}px, 100%)`;
    const remove = document.createElement("button");
    remove.className = "cm-image-remove";
    remove.innerHTML = icons.close;
    remove.title = this.alt;
    remove.onmousedown = (e) => {
      // Removes the reference only; the attachment stays in Joplin.
      e.preventDefault();
      const pos = view.posAtDOM(wrap);
      const doc = view.state.doc.toString();
      // Replacing widget: pos is the start; trailing widget: pos is the end.
      const from = doc.slice(pos, pos + this.len).toLowerCase().includes(this.id) ? pos : pos - this.len;
      view.dispatch({ changes: { from, to: from + this.len } });
    };
    wrap.append(img, remove);
    const missing = () => {
      img.remove();
      const ph = document.createElement("span");
      ph.className = "cm-image-missing";
      ph.innerHTML = icons.image;
      ph.append(` ${this.alt}`);
      wrap.prepend(ph);
    };
    this.resolve(this.id)
      .then((src) => (src ? (img.src = src) : missing()))
      .catch(missing);
    return wrap;
  }
  ignoreEvent() {
    return true;
  }
}

const hide = Decoration.replace({});
const htmlDim = Decoration.mark({ class: "cm-html" });
const underline = Decoration.mark({ class: "cm-underline" });
const ENTITIES: Record<string, string> = { nbsp: " ", amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", "#39": "'" };

const RE_IMAGE_MD = /!\[([^\]\n]*)\]\(:\/([0-9a-fA-F]{32})\)/g;
const RE_IMAGE_HTML = /<img\b[^>\n]*?\bsrc=["']:\/([0-9a-fA-F]{32})["'][^>\n]*>/g;
const RE_SPAN = /<span\b([^>\n]*)>([\s\S]*?)<\/span>/g;
const RE_TAG = /<\/?(?:u|b|strong|i|em|s|mark|sup|sub|div|p|font|br)\b[^>\n]*\/?>/gi;
const RE_ENTITY = /&(nbsp|amp|lt|gt|quot|apos|#39);/g;
const RE_ESCAPE = /\\([\\`*_{}[\]()#+\-.!|$<>~])/g;
const RE_INS = /\+\+(?=\S)([^+\n]+?)\+\+/g;

/** Lines touched by the selection: there the raw syntax stays visible. */
function activeLines(state: EditorState): Set<number> {
  const lines = new Set<number>();
  if (!state.selection) return lines;
  for (const r of state.selection.ranges) {
    const a = state.doc.lineAt(r.from).number;
    const b = state.doc.lineAt(r.to).number;
    for (let n = a; n <= b; n++) lines.add(n);
  }
  return lines;
}

function buildPreview(view: EditorView, resolve: ResolveImage, focused: boolean): DecorationSet {
  const { state } = view;
  const doc = state.doc;
  const active = focused ? activeLines(state) : new Set<number>();
  const mode = modeOf(doc);
  const out: Range<Decoration>[] = [];
  const touchesActive = (from: number, to: number) => {
    const a = doc.lineAt(from).number;
    const b = doc.lineAt(to).number;
    for (let n = a; n <= b; n++) if (active.has(n)) return true;
    return false;
  };

  if (mode === "code") return Decoration.none;

  for (const { from, to } of view.visibleRanges) {
    // Markdown markers (# ** ` > ~~) disappear off the cursor lines.
    syntaxTree(state).iterate({
      from,
      to,
      enter: (node) => {
        const name = node.name;
        if (name === "FencedCode" || name === "CodeBlock") return false;
        const markers = ["HeaderMark", "EmphasisMark", "StrikethroughMark", "CodeMark"];
        if (!markers.includes(name)) return;
        if (touchesActive(node.from, node.to)) return;
        if (doc.lineAt(node.from).number === 1) return; // the Joplin title is plain text
        let end = node.to;
        if (name === "HeaderMark" && doc.sliceString(end, end + 1) === " ") end++;
        out.push(hide.range(node.from, end));
      },
    });

    // Joplin's rich-text HTML and attachments (regexes over the visible text).
    const start = doc.lineAt(from).from;
    const text = doc.sliceString(start, to);
    const at = (i: number) => start + i;

    const images: [number, number, string, string, number][] = [];
    for (const m of text.matchAll(RE_IMAGE_MD)) images.push([m.index!, m.index! + m[0].length, m[2], m[1], 0]);
    for (const m of text.matchAll(RE_IMAGE_HTML)) {
      const alt = /\balt=["']([^"']*)["']/.exec(m[0])?.[1] ?? "";
      const width = Number(/\bwidth=["']?(\d+)/.exec(m[0])?.[1] ?? 0);
      images.push([m.index!, m.index! + m[0].length, m[1], alt, width]);
    }
    for (const [a, b, id, alt, width] of images) {
      const widget = new ImageWidget(id.toLowerCase(), alt, width, b - a, resolve);
      // On the cursor line the source stays editable and the picture follows it.
      if (touchesActive(at(a), at(b))) out.push(Decoration.widget({ widget, side: 1 }).range(at(b)));
      else out.push(Decoration.replace({ widget }).range(at(a), at(b)));
    }

    for (const m of text.matchAll(RE_SPAN)) {
      const a = m.index!;
      const open = m[0].indexOf(">") + 1;
      const close = m[0].length - "</span>".length;
      const color = /(?:^|[;\s"])color:\s*([^;"]+)/i.exec(m[1])?.[1]?.trim();
      const bg = /background(?:-color)?:\s*([^;"]+)/i.exec(m[1])?.[1]?.trim();
      const style = [color && `color: ${color}`, bg && `background-color: ${bg}`].filter(Boolean).join("; ");
      if (open < close && style) {
        out.push(Decoration.mark({ attributes: { style } }).range(at(a + open), at(a + close)));
      }
      const raw = touchesActive(at(a), at(a + m[0].length));
      out.push((raw ? htmlDim : hide).range(at(a), at(a + open)));
      out.push((raw ? htmlDim : hide).range(at(a + close), at(a + m[0].length)));
    }

    for (const m of text.matchAll(RE_TAG)) {
      const a = at(m.index!);
      const b = a + m[0].length;
      out.push((touchesActive(a, b) ? htmlDim : hide).range(a, b));
    }

    for (const m of text.matchAll(RE_ENTITY)) {
      const a = at(m.index!);
      const b = a + m[0].length;
      if (touchesActive(a, b)) continue;
      out.push(Decoration.replace({ widget: new TextWidget(ENTITIES[m[1]] ?? "", "cm-entity") }).range(a, b));
    }

    for (const m of text.matchAll(RE_ESCAPE)) {
      const a = at(m.index!);
      if (!touchesActive(a, a + 2)) out.push(hide.range(a, a + 1));
    }

    // "++text++" is Joplin's underline.
    for (const m of text.matchAll(RE_INS)) {
      const a = at(m.index!);
      const b = a + m[0].length;
      out.push(underline.range(a + 2, b - 2));
      if (!touchesActive(a, b)) out.push(hide.range(a, a + 2), hide.range(b - 2, b));
    }
  }
  return Decoration.set(out, true);
}

/** The user is in the editor (even if the whole window is momentarily unfocused). */
function editing(view: EditorView): boolean {
  return view.hasFocus || document.activeElement === view.contentDOM;
}

function preview(resolve: ResolveImage) {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = buildPreview(view, resolve, editing(view));
      }
      update(u: ViewUpdate) {
        if (u.docChanged || u.viewportChanged || u.selectionSet || u.focusChanged || syntaxTree(u.startState) !== syntaxTree(u.state)) {
          this.decorations = buildPreview(u.view, resolve, editing(u.view));
        }
      }
    },
    { decorations: (v) => v.decorations },
  );
}

/** Markdown look (Omarchy palette): headings scale, marks stay muted. */
const markdownStyle = HighlightStyle.define([
  { tag: tags.heading1, fontSize: "1.6em", fontWeight: "700", color: "var(--accent)" },
  { tag: tags.heading2, fontSize: "1.35em", fontWeight: "700", color: "var(--accent)" },
  { tag: tags.heading3, fontSize: "1.15em", fontWeight: "700", color: "var(--accent)" },
  { tag: [tags.heading4, tags.heading5, tags.heading6], fontWeight: "700", color: "var(--accent)" },
  { tag: tags.strong, fontWeight: "700" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through", color: "var(--muted)" },
  { tag: tags.link, color: "var(--accent)" },
  { tag: tags.url, color: "var(--muted)", textDecoration: "underline" },
  { tag: tags.monospace, color: "var(--result)" },
  { tag: tags.quote, color: "var(--muted)", fontStyle: "italic" },
  { tag: tags.contentSeparator, color: "var(--muted)" },
  { tag: tags.processingInstruction, color: "var(--muted)" },
  { tag: tags.escape, color: "var(--muted)" },
]);

/**
 * Inserts an attachment reference on a line of its own at the cursor. Line 1 is
 * the Joplin title, so the image never goes there: on the title it lands just
 * below it, and an empty note gets `title` as its first line.
 */
export function insertBlock(view: EditorView, text: string, title = "") {
  const doc = view.state.doc;
  let { from, to } = view.state.selection.main;
  let insert: string;
  if (doc.length === 0) {
    insert = `${title}\n${text}\n`;
  } else if (doc.lineAt(from).number === 1) {
    from = to = doc.line(1).to;
    insert = `\n${text}` + (doc.lines === 1 ? "\n" : "");
  } else {
    const line = doc.lineAt(from);
    insert = (from > line.from ? "\n" : "") + text + "\n";
  }
  view.dispatch({
    changes: { from, to, insert },
    selection: { anchor: from + insert.length },
    scrollIntoView: true,
  });
  view.focus();
}

// ---------------------------------------------------------------------------

export interface EditorOptions {
  parent: HTMLElement;
  doc: string;
  onChange: (text: string) => void;
  /** A line starting with "timer" was entered; resolve true if it was a timer command. */
  onTimer?: (line: string) => Promise<boolean>;
  /** Images were pasted or dropped: the page decides (attachment or OCR) and inserts them. */
  onImages?: (images: File[]) => void;
  /** URL of an attachment (`:/id`) to display, or null if unavailable. */
  resolveImage?: ResolveImage;
  extraKeys?: { key: string; run: () => boolean }[];
}

/** Inserts text at the cursor (used for OCR results). */
export function insertText(view: EditorView, text: string) {
  const { from, to } = view.state.selection.main;
  view.dispatch({
    changes: { from, to, insert: text },
    selection: { anchor: from + text.length },
    scrollIntoView: true,
  });
  view.focus();
}

const theme = EditorView.theme({
  "&": { height: "100%", backgroundColor: "transparent" },
  ".cm-scroller": { fontFamily: "var(--font)", lineHeight: "1.65" },
  ".cm-content": { caretColor: "var(--accent)" },
});

export function createEditor(o: EditorOptions): EditorView {
  const timerEnter = (view: EditorView): boolean => {
    if (!o.onTimer) return false;
    const line = view.state.doc.lineAt(view.state.selection.main.head);
    // In list notes the line may already carry a "- [ ] " marker.
    const command = line.text.replace(/^\s*(?:[-*+]\s+(?:\[[ xX]\]\s+)?)?/, "");
    if (!/^timer\b/i.test(command)) return false;
    const text = line.text;
    void o.onTimer(command).then((handled) => {
      if (!handled) {
        view.dispatch(view.state.replaceSelection("\n"));
        return;
      }
      // The command line is consumed.
      const cur = view.state.doc.line(Math.min(line.number, view.state.doc.lines));
      if (cur.text !== text) return;
      const from = cur.number > 1 ? cur.from - 1 : cur.from;
      const to = cur.number > 1 ? cur.to : Math.min(cur.to + 1, view.state.doc.length);
      view.dispatch({ changes: { from, to } });
    });
    return true;
  };

  const extensions: Extension[] = [
    markdown({ base: markdownLanguage }),
    syntaxHighlighting(markdownStyle),
    preview(o.resolveImage ?? (async () => null)),
    theme,
    history(),
    EditorView.lineWrapping,
    EditorState.allowMultipleSelections.of(true),
    placeholder(t("note.placeholder")),
    decorations,
    autoLists,
    keymap.of([
      ...(o.extraKeys ?? []),
      { key: "Enter", run: (v) => timerEnter(v) || continueList(v) },
      { key: "Mod-Shift-k", run: checkLine },
      { key: "Mod-Enter", run: checkLine },
      { key: "Mod-Shift-m", run: cycleMarker },
      { key: "Mod-b", run: wrap("**") },
      { key: "Mod-i", run: wrap("*") },
      { key: "Mod-u", run: wrap("++") }, // Joplin's underline
      { key: "Mod-Shift-x", run: wrap("~~") },
      { key: "Mod-Shift-h", run: cycleHeading },
      { key: "Mod-/", run: toggleComment },
      { key: "Tab", run: indentMore, shift: indentLess },
      ...defaultKeymap,
      ...historyKeymap,
    ]),
    EditorView.updateListener.of((u) => {
      if (u.docChanged) o.onChange(u.state.doc.toString());
    }),
    EditorView.domEventHandlers({
      paste(e) {
        const files = [...(e.clipboardData?.files ?? [])].filter((f) => f.type.startsWith("image/"));
        if (!files.length || !o.onImages) return false;
        e.preventDefault();
        o.onImages(files);
        return true;
      },
      // Images dragged from Finder, a browser, Photos… arrive as files (Tauri's
      // native drop interception is off, see tauri.conf.json).
      dragover(e) {
        if (!e.dataTransfer?.types.includes("Files")) return false;
        e.preventDefault();
        return true;
      },
      drop(e, view) {
        const files = [...(e.dataTransfer?.files ?? [])].filter((f) => f.type.startsWith("image/"));
        if (!files.length || !o.onImages) return false;
        e.preventDefault();
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        if (pos !== null) view.dispatch({ selection: { anchor: pos } });
        o.onImages(files);
        return true;
      },
    }),
    EditorView.contentAttributes.of({ autocapitalize: "sentences", spellcheck: "true" }),
  ];
  const view = new EditorView({
    parent: o.parent,
    state: EditorState.create({ doc: o.doc, extensions }),
  });
  // A "list" note written elsewhere (Joplin, CLI, an AI agent) may lack the
  // "- [ ] " markers: add them so every app sees a real checklist.
  if (modeOf(view.state.doc) === "list") {
    const changes: ChangeSpec[] = [];
    const marked = new Set<number>();
    const doc = view.state.doc;
    for (let n = 2; n <= doc.lines; n++) {
      const line = doc.line(n);
      const s = line.text;
      const plain = !hasMarker(s) && !BARE_TASK.test(s) && !HEADING.test(s) && !COMMENT.test(s) && !FENCE.test(s);
      if (s.trim() !== "" && plain && continuesList(doc, n, marked)) {
        marked.add(n);
        changes.push({ from: line.from + /^\s*/.exec(s)![0].length, insert: "- [ ] " });
      }
    }
    if (changes.length) view.dispatch({ changes });
  }
  return view;
}
