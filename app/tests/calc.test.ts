import { countIn, detectMode, evaluate, formatResult, numbersIn } from "../src/lib/calc.ts";
import assert from "node:assert/strict";

const show = (text: string, auto = false, locale = "en-US") =>
  evaluate(text, auto).map((r) => (r && r.show ? formatResult(r, locale) : null));

// Plain notes: a result shows only on lines ending with "=".
const cases: [string, (string | null)[]][] = [
  ["2 + 3 * 4", [null]],
  ["2 + 3 * 4 =", ["14"]],
  ["2+2=", ["4"]],
  ["(2 + 3) * 4 =\n2^10=", ["20", "1,024"]],
  ["Groceries\nmilk 1.50 €\nbread 2 €\ntotal =", [null, null, null, "3.50 €"]],
  ["Groceries\nmilk 1.50 €\nbread 2 €\ntotal", [null, null, null, null]],
  ["vat = 22%\n100 € + vat =", [null, "122.00 €"]],
  ["20% of 50 =", ["10"]],
  ["coffee: 1.20 €\nlunch: 12 €\nsum =", [null, null, "13.20 €"]],
  ["x = 10\nx * 3 =\nans + 1 =", [null, "30", "31"]],
  ["sqrt(16) + 1 =", ["5"]],
  ["I have 3 cats and 2 dogs", [null]],
  ["- [ ] buy milk\n- [x] done", [null, null]],
  ["- [ ] 2 * 3 =", ["6"]],
  ["# Title 2024", [null]],
  ["price = 80\ndiscount = 15%\nprice - discount =", [null, null, "68"]],
  ["10 / 4 =", ["2.5"]],
  ["3 x 4 =", ["12"]],
  ["1.5k + 500 =", ["2,000"]],
  ["10\n20\n\n5\ntotal =", [null, null, null, null, "5"]],
  ["avg\n4\n6\navg =", [null, null, null, "5"]],
  ["10 €\n5 €\ntotal\ntotal * 2 =", [null, null, null, "30.00 €"]],
  ["// 2 + 2 =\n3 + 3 =", [null, "6"]],
];

// Other UI languages: translated keywords ("totale", "somma", "media", "di", "summe",
// "von") and decimal commas, with results formatted for Italian.
const localized: [string, (string | null)[]][] = [
  ["milk 1,50 €\nbread 2 €\ntotale =", [null, null, "3,50 €"]],
  ["vat = 22%\n100 € + vat =", [null, "122,00 €"]],
  ["20% di 50 =", ["10"]],
  ["coffee: 1,20 €\nlunch: 12 €\nsomma =", [null, null, "13,20 €"]],
  ["media\n4\n6\nmedia =", [null, null, null, "5"]],
  ["10 / 4 =", ["2,5"]],
  ["1,5k + 500 =", ["2000"]],
  ["10 €\n5 €\nsumme =", [null, null, "15,00 €"]],
  ["30% von 200 =", ["60"]],
];

// "math" notes: every calculation shows, "=" not needed.
const auto: [string, (string | null)[]][] = [
  ["2 + 3 * 4", ["14"]],
  ["x = 10\nx * 3", ["10", "30"]],
  ["coffee: 1.20 €\nlunch: 12 €\nsum", ["1.20 €", "12.00 €", "13.20 €"]],
  ["I have 3 cats", [null]],
];

for (const [input, expected] of cases) assert.deepEqual(show(input), expected, input);
for (const [input, expected] of localized) assert.deepEqual(show(input, false, "it-IT"), expected, `it: ${input}`);
for (const [input, expected] of auto) assert.deepEqual(show(input, true), expected, `auto: ${input}`);
assert.deepEqual(show("coffee: 1,20 €\nlunch: 12 €\nsomma", true, "it-IT"), ["1,20 €", "12,00 €", "13,20 €"]);

assert.deepEqual(detectMode("list: Groceries"), { mode: "list", title: "Groceries" });
assert.equal(detectMode("List of groceries").mode, "plain");
// Translated keywords.
assert.deepEqual(detectMode("Lista"), { mode: "list", title: "" });
assert.deepEqual(detectMode("summe"), { mode: "sum", title: "" });
assert.equal(detectMode("Lista groceries").mode, "plain");
assert.deepEqual(numbersIn("sum\n- [ ] milk 1,50\n2. bread 2\n// 100\neggs x6"), [1.5, 2, 6]);
assert.deepEqual(countIn("count\none two\n\nsix\n// no"), { items: 2, lines: 3, words: 3, chars: 10 });

console.log(`calc: ${cases.length + localized.length + auto.length + 1} cases + modes ok`);
