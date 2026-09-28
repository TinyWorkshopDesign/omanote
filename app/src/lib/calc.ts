// Inline calculator. Every line of a note is tried
// as an expression; lines that don't parse (plain text) are ignored.
//
//   2 + 3 * 4            → 14
//   vat = 22%            → 0.22        (variables; the name may start with digits: 5axis = 1100)
//   100 € + vat          → 122 €       (x + y% adds a percentage)
//   20% of 50            → 10
//   coffee: 1.50 €       → 1.50 €      ("label: expr")
//   total                → sum of the block above (also: sum, and the UI languages' words)
//   avg                  → average of the block above (also: average, and translations)
//   sqrt(16) + ans       → uses the previous result
//   100 € in $           → 108 $       (conversion; automatic rates, or "1 € = 1.08 $" in the note)

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
/** The code behind every symbol the calculator knows. */
const SYMBOL_CODES: Record<string, string> = { "€": "EUR", "$": "USD", "£": "GBP", "¥": "JPY", "₿": "BTC" };
/** Shown for a currency: the symbol when there is one, the code otherwise (CHF). */
const CODE_SYMBOLS: Record<string, string> = { EUR: "€", USD: "$", GBP: "£", JPY: "¥", BTC: "₿" };

/** `"EUR>USD"` → rate. */
export type FxRates = ReadonlyMap<string, number>;

/**
 * Every pair, from a table based on one currency (the automatic one comes from
 * the ECB, EUR based): the direct rate, its inverse and the cross rates.
 */
export function buildRates(base: Record<string, number>, baseCode = "EUR"): FxRates {
  const codes = Object.keys(base).map((c) => c.trim().toUpperCase()).filter((c) => c !== baseCode);
  const of = (c: string) => (c === baseCode ? 1 : base[c] ?? base[c.toLowerCase()] ?? 0);
  const rates = new Map<string, number>();
  rates.set(`${baseCode}>${baseCode}`, 1);
  for (const c of codes) {
    const r = of(c);
    if (!r) continue;
    rates.set(`${baseCode}>${c}`, r);
    rates.set(`${c}>${baseCode}`, 1 / r);
  }
  for (const from of codes) {
    for (const to of codes) {
      const rf = of(from);
      const rt = of(to);
      if (from !== to && rf && rt) rates.set(`${from}>${to}`, rt / rf);
    }
  }
  return rates;
}

/** EUR, usd, $, chf → a currency code; null when the text is not a currency. */
function currencyCode(text: string): string | null {
  const t = text.trim();
  if (SYMBOL_CODES[t]) return SYMBOL_CODES[t];
  const up = t.toUpperCase();
  return /^[A-Z]{3}$/.test(up) && up !== "IN" && up !== "TO" ? up : null;
}

const currencySymbol = (code: string) => CODE_SYMBOLS[code] ?? code;
// "1 € = 1.08 $", "1 EUR = 1.08 USD": a rate that only this note uses.
const RATE_DECL = /^([0-9][\d.,]*)\s*(\S{1,4})\s*=\s*([0-9][\d.,]*)\s*(\S{1,4})$/;
// "100 € in $", "100 € to USD", "100 € -> $": a conversion.
const CONVERT = /^(.*?)\s+(?:in|to|->|→)\s+(\S{1,4})$/i;

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

/**
 * Both conventions are read: "1.234,56" and "1,234.56" are 1234.56. With both
 * separators the last one is the decimal one; a repeated separator groups digits.
 */
function parseNumber(raw: string): number | null {
  const dots = raw.split(".").length - 1;
  const commas = raw.split(",").length - 1;
  let s = raw;
  if (dots > 0 && commas > 0) {
    // Both separators: the last one is the decimal separator.
    if (raw.lastIndexOf(".") > raw.lastIndexOf(",")) s = raw.split(",").join("");
    else s = raw.split(".").join("").replace(",", ".");
  } else if (dots > 1) {
    s = raw.split(".").join(""); // repeated separator: digit grouping
  } else if (commas > 1) {
    s = raw.split(",").join("");
  } else if (commas === 1) {
    s = raw.replace(",", "."); // "1,50" → 1.50
  }
  const v = parseFloat(s);
  return Number.isNaN(v) ? null : v;
}

/** "3x4": an x between two numbers multiplies. "12 x 4" already works. */
function isDimensionX(src: string, i: number, prev: Tok | undefined): boolean {
  return (
    (src[i] === "x" || src[i] === "X") &&
    /[0-9]/.test(src[i + 1] ?? "") &&
    prev?.t === "num"
  );
}

function tokenize(src: string): Tok[] | null {
  const out: Tok[] = [];
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === " " || c === "\t") { i++; continue; }
    if (isDimensionX(src, i, out[out.length - 1])) {
      out.push({ t: "op", v: "*" });
      i++;
      continue;
    }
    // A name may start with digits ("5axis"): digits followed by at least two
    // letters are one identifier, not a number plus a word. One trailing letter
    // stays a multiplier/suffix ("2k", "500k"), and "3x4" is a multiplication.
    if (/[0-9]/.test(c)) {
      const name = /^[0-9]+[A-Za-z_àèéìòù]{2}[A-Za-z0-9_àèéìòù]*/.exec(src.slice(i));
      if (name) {
        out.push({ t: "id", v: name[0].toLowerCase() });
        i += name[0].length;
        continue;
      }
    }
    if (/[0-9.,]/.test(c) && (c !== "," || /[0-9]/.test(src[i + 1] ?? "") && /[0-9]/.test(src[i - 1] ?? ""))) {
      let j = i;
      let raw = "";
      while (j < src.length) {
        const d = src[j];
        if (/[0-9]/.test(d)) raw += d;
        else if ((d === "." || d === ",") && /[0-9]/.test(src[j + 1] ?? "")) raw += d;
        else if (d === "_" || d === "'") { /* digit grouping */ }
        else break;
        j++;
      }
      const parsed = parseNumber(raw);
      if (parsed === null) return null;
      let v = parsed;
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

/**
 * A name may be several words ("prezzo unitario"), stored as "prezzo_unitario"
 * and "prezzounitario": the longest run of words that is a known variable is
 * joined into a single identifier.
 */
function joinWords(toks: Tok[], vars: Map<string, Val>): Tok[] {
  const out: Tok[] = [];
  let i = 0;
  while (i < toks.length) {
    const tk = toks[i];
    if (tk.t === "id" && !vars.has(tk.v)) {
      const words: string[] = [tk.v];
      let found: string | null = null;
      let end = i;
      for (let k = i + 1; k < toks.length && toks[k].t === "id"; k++) {
        words.push((toks[k] as { v: string }).v);
        const spaced = words.join("_");
        if (vars.has(spaced)) { found = spaced; end = k; }
        else if (vars.has(words.join(""))) { found = words.join(""); end = k; }
      }
      if (found) {
        out.push({ t: "id", v: found });
        i = end + 1;
        continue;
      }
    }
    out.push(tk);
    i++;
  }
  return out;
}

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
      if (!left) return null;
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

function apply(op: string, a: Val, b: Val): Val | null {
  // "10 € + 5 $" has no meaning: refuse the line instead of silently keeping
  // the unit on the left. Same for any other combination of two currencies.
  if (a.unit && b.unit && a.unit !== b.unit) return null;
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
  const raw = tokenize(src);
  if (!raw || raw.length === 0) return null;
  const p = new Parser(joinWords(raw, vars), vars);
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
 * `fx` holds the exchange rates (see `buildRates`); a "1 € = 1.08 $" line
 * overrides them for this note only.
 */
export function evaluate(text: string, auto = false, fx?: FxRates): (LineResult | null)[] {
  const vars = new Map<string, Val>();
  const localRates = new Map<string, number>();
  const rateOf = (from: string, to: string): number | null =>
    from === to ? 1 : localRates.get(`${from}>${to}`) ?? fx?.get(`${from}>${to}`) ?? null;
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

    // "1 € = 1.08 $" (or "1 EUR = 1.08 USD"): a rate used by this note only.
    const decl = body.match(RATE_DECL);
    if (decl) {
      const amount = parseNumber(decl[1]);
      const quoted = parseNumber(decl[3]);
      const from = currencyCode(decl[2]);
      const to = currencyCode(decl[4]);
      if (amount && quoted && from && to && from !== to) {
        const rate = quoted / amount;
        localRates.set(`${from}>${to}`, rate);
        localRates.set(`${to}>${from}`, 1 / rate);
        last = { v: rate, unit: currencySymbol(to), pct: false };
        out.push({ value: rate, unit: currencySymbol(to), pct: false, show: auto });
        continue;
      }
    }

    // "100 € in $ =": convert with the rates above (automatic or declared).
    const conv = body.replace(/=\s*$/, "").trim().match(CONVERT);
    if (conv) {
      const to = currencyCode(conv[2]);
      const left = to ? evalExpr(conv[1], vars) : null;
      const from = left?.val.unit ? SYMBOL_CODES[left.val.unit] : null;
      const rate = from && to ? rateOf(from, to) : null;
      if (left && from && to && rate) {
        const val = { v: left.val.v * rate, unit: currencySymbol(to), pct: false };
        last = val;
        block.push(val);
        out.push({ value: val.v, unit: val.unit, pct: val.pct, show: asks || auto });
        continue;
      }
      if (left && to) { out.push(null); continue; } // no rate for that pair
    }

    // "name = expr": the name may start with a digit ("5axis = 1100") but must
    // hold at least one letter, so a plain equation never defines a variable.
    const assign = body.match(/^([a-zA-Z_àèéìòù\d][\wàèéìòù ]{0,30}?)\s*=\s*(.+)$/);
    if (assign && /[a-zA-Z_àèéìòù]/.test(assign[1])) {
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
