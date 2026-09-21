// CodeMirror 6 setup for Omanote: plain-text notes (Joplin Markdown) with
// live checkboxes, bullets, headings and inline calculation results.

import { EditorState, RangeSetBuilder, type Extension } from "@codemirror/state";
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
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { evaluate, formatResult } from "./calc";

const TASK = /^(\s*)([-*+]) \[( |x|X)\] /;
const BULLET = /^(\s*)([-*+]) (?!\[[ xX]\] )/;
const NUMBERED = /^(\s*)(\d+)([.)]) /;
const HEADING = /^(#{1,3}) /;

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

class BulletWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = "cm-bullet";
    el.textContent = "•";
    return el;
  }
}

class ResultWidget extends WidgetType {
  constructor(readonly text: string) {
    super();
  }
  eq(o: ResultWidget) {
    return o.text === this.text;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = "cm-result";
    el.textContent = this.text;
    el.title = "Clic per copiare";
    return el;
  }
  ignoreEvent() {
    return false;
  }
}

const bullet = Decoration.replace({ widget: new BulletWidget() });
const doneLine = Decoration.line({ class: "cm-task-done" });
const titleLine = Decoration.line({ class: "cm-title-line" });
const resultLine = Decoration.line({ class: "cm-has-result" });
const headingLines = [1, 2, 3].map((n) => Decoration.line({ class: `cm-h${n}` }));
const dimMark = Decoration.mark({ class: "cm-dim" });

function buildDecorations(view: EditorView): DecorationSet {
  const doc = view.state.doc;
  const results = evaluate(doc.toString());
  const b = new RangeSetBuilder<Decoration>();

  for (const { from, to } of view.visibleRanges) {
    let pos = from;
    while (pos <= to) {
      const line = doc.lineAt(pos);
      const text = line.text;
      const res = results[line.number - 1];

      if (line.number === 1 && text.trim() !== "") b.add(line.from, line.from, titleLine);
      const h = HEADING.exec(text);
      const task = TASK.exec(text);
      if (h) b.add(line.from, line.from, headingLines[h[1].length - 1]);
      if (task && task[3] !== " ") b.add(line.from, line.from, doneLine);
      if (res?.show) b.add(line.from, line.from, resultLine);

      if (h) {
        b.add(line.from, line.from + h[0].length, dimMark);
      } else if (task) {
        const start = line.from + task[1].length;
        b.add(
          start,
          line.from + task[0].length,
          Decoration.replace({ widget: new CheckboxWidget(task[3] !== " ", start) }),
        );
      } else {
        const bl = BULLET.exec(text);
        if (bl) b.add(line.from + bl[1].length, line.from + bl[1].length + 1, bullet);
      }

      if (res?.show) {
        b.add(line.to, line.to, Decoration.widget({ widget: new ResultWidget(formatResult(res)), side: 1 }));
      }
      pos = line.to + 1;
    }
  }
  return b.finish();
}

const decorations = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged) this.decorations = buildDecorations(u.view);
    }
  },
  {
    decorations: (v) => v.decorations,
    provide: (p) =>
      EditorView.atomicRanges.of((view) => view.plugin(p)?.decorations ?? Decoration.none),
    eventHandlers: {
      mousedown(e, view) {
        const t = e.target as HTMLElement;
        if (t.classList.contains("cm-task")) {
          toggleTaskAt(view, Number(t.dataset.pos));
          e.preventDefault();
          return true;
        }
        if (t.classList.contains("cm-result")) {
          void navigator.clipboard?.writeText(t.textContent ?? "");
          t.classList.add("cm-result-copied");
          setTimeout(() => t.classList.remove("cm-result-copied"), 700);
          e.preventDefault();
          return true;
        }
        return false;
      },
    },
  },
);

function toggleTaskAt(view: EditorView, pos: number) {
  const line = view.state.doc.lineAt(pos);
  const m = TASK.exec(line.text);
  if (!m) return;
  const at = line.from + m[1].length + 3;
  view.dispatch({ changes: { from: at, to: at + 1, insert: m[3] === " " ? "x" : " " } });
}

/** Mod-Enter: toggle the checkbox of the current line (creating one if needed). */
export function toggleTask(view: EditorView): boolean {
  const { state } = view;
  const changes = [];
  const seen = new Set<number>();
  for (const r of state.selection.ranges) {
    const line = state.doc.lineAt(r.head);
    if (seen.has(line.number)) continue;
    seen.add(line.number);
    const task = TASK.exec(line.text);
    if (task) {
      const at = line.from + task[1].length + 3;
      changes.push({ from: at, to: at + 1, insert: task[3] === " " ? "x" : " " });
    } else {
      const bl = BULLET.exec(line.text);
      const indent = /^\s*/.exec(line.text)![0];
      if (bl) changes.push({ from: line.from + bl[0].length, insert: "[ ] " });
      else changes.push({ from: line.from + indent.length, insert: "- [ ] " });
    }
  }
  view.dispatch({ changes });
  return true;
}

/** Enter: continue lists/checklists; on an empty item, end the list. */
function continueList(view: EditorView): boolean {
  const { state } = view;
  const sel = state.selection.main;
  if (!sel.empty) return false;
  const line = state.doc.lineAt(sel.head);
  const task = TASK.exec(line.text);
  const bl = BULLET.exec(line.text);
  const num = NUMBERED.exec(line.text);
  const m = task ?? bl ?? num;
  if (!m) return false;
  if (sel.head < line.from + m[0].length) return false;

  if (line.text.trim() === m[0].trim()) {
    // Empty item → remove the marker.
    view.dispatch({ changes: { from: line.from, to: line.to, insert: "" } });
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

export interface EditorOptions {
  parent: HTMLElement;
  doc: string;
  onChange: (text: string) => void;
  extraKeys?: { key: string; run: () => boolean }[];
}

const theme = EditorView.theme({
  "&": { height: "100%", backgroundColor: "transparent" },
  ".cm-scroller": { fontFamily: "var(--font)", lineHeight: "1.65" },
  ".cm-content": { caretColor: "var(--accent)" },
});

export function createEditor(o: EditorOptions): EditorView {
  const extensions: Extension[] = [
    theme,
    history(),
    EditorView.lineWrapping,
    EditorState.allowMultipleSelections.of(true),
    placeholder("Scrivi qualcosa…  (la prima riga è il titolo)"),
    decorations,
    keymap.of([
      ...(o.extraKeys ?? []),
      { key: "Enter", run: continueList },
      { key: "Mod-Enter", run: toggleTask },
      indentWithTab,
      ...defaultKeymap,
      ...historyKeymap,
    ]),
    EditorView.updateListener.of((u) => {
      if (u.docChanged) o.onChange(u.state.doc.toString());
    }),
    EditorView.contentAttributes.of({ autocapitalize: "sentences", spellcheck: "true" }),
  ];
  return new EditorView({
    parent: o.parent,
    state: EditorState.create({ doc: o.doc, extensions }),
  });
}
