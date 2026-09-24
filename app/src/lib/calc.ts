// Inline calculator. Every line of a note is tried
// as an expression; lines that don't parse (plain text) are ignored.
//
//   2 + 3 * 4            → 14
//   vat = 22%            → 0.22        (variables)
//   100 € + vat          → 122 €       (x + y% adds a percentage)
//   20% of 50            → 10
//   coffee: 1.50 €       → 1.50 €      ("label: expr")
//   total                → sum of the block above (also: sum, and the UI languages' words)
//   avg                  → average of the block above (also: average, and translations)
//   sqrt(16) + ans       → uses the previous result

export interface LineResult {
  value: number;
  unit: string;
  /** The value is a percentage (0.2 means 20%). */
  pct: boolean;
  /** Show it in the gutter (plain numbers are collected but not echoed). */
  show: boolean;
}

type Tok =
  | { t: "num"; v: number; pct: boolean; unit: string }
  | { t: "id"; v: string }
  | { t: "op"; v: string }
  | { t: "(" }
  | { t: ")" }
  | { t: "," };

const CURRENCIES = "€$£¥₿";
// Per-line keywords, in every UI language.
const SUM_WORDS = new Set([
  "sum", "total", "tot", "totale", "somma", "suma", "soma", "somme", "summe", "gesamt", "som", "totaal", "razem",
]);
const AVG_WORDS = new Set([
  "avg", "average", "mean", "media", "promedio", "média", "moyenne", "durchschnitt", "gemiddelde", "średnia",
]);
const OF_WORDS = new Set(["of", "on", "di", "del", "della", "dei", "de", "du", "des", "von", "vom", "van", "z", "ze"]);

const FUNCS: Record<string, (...a: number[]) => number> = {
  sqrt: Math.sqrt, abs: Math.abs, round: Math.round, floor: Math.floor, ceil: Math.ceil,
  sin: Math.sin, cos: Math.cos, tan: Math.tan, ln: Math.log, log: Math.log10,
  min: Math.min, max: Math.max, pow: Math.pow,
};
const CONSTS: Record<string, number> = { pi: Math.PI, e: Math.E };

function tokenize(src: string): Tok[] | null {
  const out: Tok[] = [];
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === " " || c === "\t") { i++; continue; }
    if (/[0-9.,]/.test(c) && (c !== "," || /[0-9]/.test(src[i + 1] ?? "") && /[0-9]/.test(src[i - 1] ?? ""))) {
      let j = i;
      let s = "";
      while (j < src.length) {
        const d = src[j];
        if (/[0-9]/.test(d)) s += d;
        else if ((d === "." || d === ",") && /[0-9]/.test(src[j + 1] ?? "")) s += ".";
        else if (d === "_" || d === "'") { /* digit grouping */ }
        else break;
        j++;
      }
      if (s.split(".").length > 2) return null;
      let v = parseFloat(s);
      if (Number.isNaN(v)) return null;
      // suffixes: k / M, %, currency
      let pct = false;
      let unit = "";
      while (j < src.length && src[j] === " " && /[%€$£¥₿]/.test(src[j + 1] ?? "")) j++;
      if (src[j] === "k" && !/[a-z]/i.test(src[j + 1] ?? "")) { v *= 1e3; j++; }
      else if (src[j] === "M" && !/[a-z]/i.test(src[j + 1] ?? "")) { v *= 1e6; j++; }
      if (src[j] === "%") { pct = true; j++; }
      else if (CURRENCIES.includes(src[j] ?? "")) { unit = src[j]; j++; }
      out.push({ t: "num", v, pct, unit });
      i = j;
      continue;
    }
    if (CURRENCIES.includes(c)) {
      // prefix currency: attach to next number
      let j = i + 1;
      while (src[j] === " ") j++;
      const next = tokenize(src.slice(j));
      if (!next || next.length === 0 || next[0].t !== "num") return null;
      (next[0] as { unit: string }).unit = c;
      return [...out, ...next];
    }
    if (/[a-zA-Z_àèéìòù]/.test(c)) {
      let j = i;
      while (j < src.length && /[a-zA-Z0-9_àèéìòù]/.test(src[j])) j++;
      out.push({ t: "id", v: src.slice(i, j).toLowerCase() });
      i = j;
      continue;
    }
    if ("+-*/^×÷".includes(c)) {
      out.push({ t: "op", v: c === "×" ? "*" : c === "÷" ? "/" : c });
      i++;
      continue;
    }
    if (c === "(") { out.push({ t: "(" }); i++; continue; }
    if (c === ")") { out.push({ t: ")" }); i++; continue; }
    if (c === "," || c === ";") { out.push({ t: "," }); i++; continue; }
    return null;
  }
  return out;
}

interface Val { v: number; unit: string; pct: boolean }

class Parser {
  i = 0;
  usedOperator = false;
  toks: Tok[];
  vars: Map<string, Val>;
  constructor(toks: Tok[], vars: Map<string, Val>) {
    this.toks = toks;
    this.vars = vars;
  }

  peek(): Tok | undefined { return this.toks[this.i]; }
  next(): Tok | undefined { return this.toks[this.i++]; }

  parse(): Val | null {
    const v = this.expr(0);
    if (!v || this.i !== this.toks.length) return null;
    return v;
  }

  prec(op: string): number {
    return op === "+" || op === "-" ? 1 : op === "*" || op === "/" ? 2 : op === "^" ? 3 : op === "of" ? 2 : 0;
  }

  expr(minPrec: number): Val | null {
    let left = this.unary();
    if (!left) return null;
    for (;;) {
      const tk = this.peek();
      let op: string | null = null;
      if (tk?.t === "op") op = tk.v;
      else if (tk?.t === "id" && OF_WORDS.has(tk.v) && left.pct) op = "of";
      else if (tk?.t === "id" && tk.v === "x") op = "*";
      if (!op) break;
      const p = this.prec(op);
      if (p < minPrec) break;
      this.next();
      this.usedOperator = true;
      const right = this.expr(op === "^" ? p : p + 1);
      if (!right) return null;
      left = apply(op, left, right);
    }
    return left;
  }

  unary(): Val | null {
    const tk = this.peek();
    if (tk?.t === "op" && (tk.v === "-" || tk.v === "+")) {
      this.next();
      const v = this.unary();
      if (!v) return null;
      return tk.v === "-" ? { ...v, v: -v.v } : v;
    }
    return this.primary();
  }

  primary(): Val | null {
    const tk = this.next();
    if (!tk) return null;
    if (tk.t === "num") return { v: tk.pct ? tk.v / 100 : tk.v, unit: tk.unit, pct: tk.pct };
    if (tk.t === "(") {
      const v = this.expr(0);
      if (!v || this.next()?.t !== ")") return null;
      this.usedOperator = true;
      return v;
    }
    if (tk.t === "id") {
      if (this.peek()?.t === "(" && FUNCS[tk.v]) {
        this.next();
        const args: number[] = [];
        let unit = "";
        if (this.peek()?.t !== ")") {
          for (;;) {
            const a = this.expr(0);
            if (!a) return null;
            args.push(a.v);
            unit ||= a.unit;
            const sep = this.next();
            if (sep?.t === ")") break;
            if (sep?.t !== ",") return null;
          }
        } else this.next();
        this.usedOperator = true;
        return { v: FUNCS[tk.v](...args), unit, pct: false };
      }
      if (tk.v in CONSTS) { this.usedOperator = true; return { v: CONSTS[tk.v], unit: "", pct: false }; }
      const val = this.vars.get(tk.v);
      if (val) { this.usedOperator = true; return val; }
    }
    return null;
  }
}

function apply(op: string, a: Val, b: Val): Val {
  const unit = a.unit || b.unit;
  switch (op) {
    case "+": return { v: b.pct && !a.pct ? a.v * (1 + b.v) : a.v + b.v, unit, pct: a.pct && b.pct };
    case "-": return { v: b.pct && !a.pct ? a.v * (1 - b.v) : a.v - b.v, unit, pct: a.pct && b.pct };
    case "*": return { v: a.v * b.v, unit, pct: false };
    case "/": return { v: a.v / b.v, unit: a.unit && b.unit ? "" : unit, pct: false };
    case "^": return { v: Math.pow(a.v, b.v), unit, pct: false };
    case "of": return { v: a.v * b.v, unit: b.unit, pct: false };
  }
  return a;
}

function evalExpr(src: string, vars: Map<string, Val>): { val: Val; computed: boolean } | null {
  const toks = tokenize(src);
  if (!toks || toks.length === 0) return null;
  const p = new Parser(toks, vars);
  const val = p.parse();
  if (!val || !Number.isFinite(val.v)) return null;
  return { val, computed: p.usedOperator || val.pct };
}

const SKIP = /^\s*(#|>|```|\/\/|- \[[ xX]\]\s*$)/;
const LIST_PREFIX = /^\s*(?:[-*+•]\s+(?:\[[ xX]\]\s+)?|\d+[.)]\s+)/;

/**
 * Evaluates a whole note; returns one entry per line (null = no result).
 * Every line is computed (variables, totals), but a result is *shown* only when
 * the line ends with "=" ("2+2=", "x * 2 =", "totale ="). With `auto` (notes
 * whose first line is "math") every calculation shows.
 */
export function evaluate(text: string, auto = false): (LineResult | null)[] {
  const vars = new Map<string, Val>();
  const out: (LineResult | null)[] = [];
  let block: Val[] = [];
  let last: Val | null = null;

  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line === "") { block = []; out.push(null); continue; }
    if (SKIP.test(raw)) { out.push(null); continue; }

    let body = raw.replace(LIST_PREFIX, "").trim();
    let label = "";
    const colon = body.match(/^([^:=]{1,40}):\s*(.+)$/);
    if (colon && !/^\d/.test(colon[1])) { label = colon[1]; body = colon[2]; }
    const word = body.toLowerCase().replace(/[:=]\s*$/, "").trim();
    const asks = /=\s*$/.test(line);

    if (last) vars.set("ans", last); else vars.delete("ans");

    if (SUM_WORDS.has(word) || AVG_WORDS.has(word)) {
      if (block.length === 0) { out.push(null); continue; }
      const total = block.reduce((s, x) => s + x.v, 0);
      const v = SUM_WORDS.has(word) ? total : total / block.length;
      const unit = block.find((x) => x.unit)?.unit ?? "";
      last = { v, unit, pct: false };
      for (const w of [...SUM_WORDS, ...AVG_WORDS]) vars.set(w, last);
      out.push({ value: v, unit, pct: false, show: auto || asks });
      block = [];
      continue;
    }

    const assign = body.match(/^([a-zA-Z_àèéìòù][\wàèéìòù ]{0,30}?)\s*=\s*(.+)$/);
    if (assign) {
      const r = evalExpr(assign[2], vars);
      if (r) {
        vars.set(assign[1].trim().toLowerCase().replace(/\s+/g, "_"), r.val);
        const name = assign[1].trim().toLowerCase();
        if (name.includes(" ")) vars.set(name.replace(/\s+/g, ""), r.val);
        last = r.val;
        out.push({ value: r.val.v, unit: r.val.unit, pct: r.val.pct, show: auto });
        continue;
      }
    }

    // A trailing "=" asks for the result: "3 * 4 ="
    const r = evalExpr(body.replace(/=\s*$/, ""), vars);
    if (r) {
      last = r.val;
      block.push(r.val);
      out.push({
        value: r.val.v,
        unit: r.val.unit,
        pct: r.val.pct,
        show: asks || (auto && (r.computed || label !== "")),
      });
      continue;
    }

    // "latte 2,50 €" → collect the trailing amount for totals, don't echo it.
    const tail = body.match(/(?:^|\s)([€$£]?\s?-?\d[\d.,]*\s?[€$£%]?)\s*$/);
    if (tail) {
      const t = evalExpr(tail[1], vars);
      if (t && !t.val.pct) block.push(t.val);
    }
    out.push(null);
  }
  return out;
}

const fmtCache = new Map<string, Intl.NumberFormat>();
export function formatResult(r: LineResult, locale?: string): string {
  if (r.pct) {
    const p = new Intl.NumberFormat(locale, { maximumFractionDigits: 4 }).format(r.value * 100);
    return `${p}%`;
  }
  const abs = Math.abs(r.value);
  const money = r.unit !== "" && CURRENCIES.includes(r.unit);
  const digits = money ? 2 : abs !== 0 && abs < 0.001 ? 8 : 6;
  const key = `${locale ?? ""}|${digits}|${money}`;
  let f = fmtCache.get(key);
  if (!f) {
    f = new Intl.NumberFormat(locale, { maximumFractionDigits: digits, minimumFractionDigits: money ? 2 : 0 });
    fmtCache.set(key, f);
  }
  const n = f.format(r.value);
  return r.unit ? `${n} ${r.unit}` : n;
}

// ---------------------------------------------------------------------------
// Note modes (keywords on the first line: "list: Groceries")
// ---------------------------------------------------------------------------

export type Mode = "plain" | "list" | "math" | "sum" | "avg" | "count" | "code";

export const MODE_KEYWORDS: Record<Exclude<Mode, "plain">, string[]> = {
  list: ["list", "lista", "liste", "lijst", "todo"],
  math: ["math", "calc", "maths", "mate", "matematica", "mathe", "matemática", "wiskunde", "matematyka"],
  sum: ["sum", "somma", "suma", "soma", "somme", "summe", "som"],
  avg: ["avg", "media", "promedio", "média", "moyenne", "durchschnitt", "gemiddelde", "średnia"],
  count: ["count", "conta", "contar", "compter", "zählen", "tel", "licz"],
  code: ["code", "codice", "código", "codigo", "kod"],
};

const KEYWORD_OF = new Map<string, Mode>(
  Object.entries(MODE_KEYWORDS).flatMap(([mode, words]) => words.map((w) => [w, mode as Mode])),
);

/** Reads the note mode from its first line ("list" or "list: Title"). */
export function detectMode(firstLine: string): { mode: Mode; title: string } {
  const m = /^\s*([\p{L}]+)\s*(?::\s*(.*))?$/u.exec(firstLine);
  const mode = m ? KEYWORD_OF.get(m[1].toLowerCase()) : undefined;
  return mode ? { mode, title: (m![2] ?? "").trim() } : { mode: "plain", title: firstLine };
}

const NUMBER_RE = /-?\d+(?:[.,]\d+)?/g;

/** Every number in the note body (line 1 and // comments excluded), for sum/avg. */
export function numbersIn(text: string): number[] {
  const out: number[] = [];
  for (const line of text.split("\n").slice(1)) {
    if (/^\s*\/\//.test(line)) continue;
    for (const n of line.replace(/^\s*(?:[-*+]\s+(?:\[[ xX]\]\s+)?|\d+[.)]\s+)/, "").matchAll(NUMBER_RE)) {
      out.push(parseFloat(n[0].replace(",", ".")));
    }
  }
  return out;
}

export interface Counts {
  items: number;
  lines: number;
  words: number;
  chars: number;
}

/** "count" mode: items (non-empty lines after the first), lines, words, characters. */
export function countIn(text: string): Counts {
  const body = text.split("\n").slice(1).filter((l) => !/^\s*\/\//.test(l));
  const joined = body.join("\n");
  return {
    items: body.filter((l) => l.trim() !== "").length,
    lines: body.length,
    words: (joined.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu) ?? []).length,
    chars: [...joined.replace(/\n/g, "")].length,
  };
}
