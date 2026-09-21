import { countIn, detectMode, evaluate, formatResult, numbersIn } from "../src/lib/calc.ts";
import assert from "node:assert/strict";

const show = (text: string, auto = false) =>
  evaluate(text, auto).map((r) => (r && r.show ? formatResult(r, "it-IT") : null));

// Plain notes: a result shows only on lines ending with "=".
const cases: [string, (string | null)[]][] = [
  ["2 + 3 * 4", [null]],
  ["2 + 3 * 4 =", ["14"]],
  ["2+2=", ["4"]],
  ["(2 + 3) * 4 =\n2^10=", ["20", "1024"]],
  ["Spesa\nlatte 1,50 €\npane 2 €\ntotale =", [null, null, null, "3,50 €"]],
  ["Spesa\nlatte 1,50 €\npane 2 €\ntotale", [null, null, null, null]],
  ["iva = 22%\n100 € + iva =", [null, "122,00 €"]],
  ["20% di 50 =", ["10"]],
  ["caffè: 1,20 €\npranzo: 12 €\nsomma =", [null, null, "13,20 €"]],
  ["x = 10\nx * 3 =\nans + 1 =", [null, "30", "31"]],
  ["sqrt(16) + 1 =", ["5"]],
  ["Ho 3 gatti e 2 cani", [null]],
  ["- [ ] comprare latte\n- [x] fatto", [null, null]],
  ["- [ ] 2 * 3 =", ["6"]],
  ["# Titolo 2024", [null]],
  ["prezzo = 80\nsconto = 15%\nprezzo - sconto =", [null, null, "68"]],
  ["10 / 4 =", ["2,5"]],
  ["3 x 4 =", ["12"]],
  ["1,5k + 500 =", ["2000"]],
  ["10\n20\n\n5\ntotal =", [null, null, null, null, "5"]],
  ["media\n4\n6\nmedia =", [null, null, null, "5"]],
  ["10 €\n5 €\ntotale\ntotale * 2 =", [null, null, null, "30,00 €"]],
  ["// 2 + 2 =\n3 + 3 =", [null, "6"]],
  ["10 €\n5 €\nsumme =", [null, null, "15,00 €"]],
  ["30% von 200 =", ["60"]],
];

// "math" notes: every calculation shows, "=" not needed.
const auto: [string, (string | null)[]][] = [
  ["2 + 3 * 4", ["14"]],
  ["x = 10\nx * 3", ["10", "30"]],
  ["caffè: 1,20 €\npranzo: 12 €\nsomma", ["1,20 €", "12,00 €", "13,20 €"]],
  ["Ho 3 gatti", [null]],
];
for (const [input, expected] of auto) {
  assert.deepEqual(show(input, true), expected, `auto: ${input}`);
}

for (const [input, expected] of cases) {
  assert.deepEqual(show(input), expected, input);
}
assert.deepEqual(detectMode("list: Spesa"), { mode: "list", title: "Spesa" });
assert.deepEqual(detectMode("Lista"), { mode: "list", title: "" });
assert.deepEqual(detectMode("summe"), { mode: "sum", title: "" });
assert.equal(detectMode("Lista della spesa").mode, "plain");
assert.deepEqual(numbersIn("sum\n- [ ] latte 1,50\n2. pane 2\n// 100\nuova x6"), [1.5, 2, 6]);
assert.deepEqual(countIn("count\nuno due\n\ntre\n// no"), { items: 2, lines: 3, words: 3, chars: 10 });

console.log(`calc: ${cases.length + auto.length} casi + modalità ok`);
