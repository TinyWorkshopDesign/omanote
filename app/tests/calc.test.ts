import { buildRates, countIn, detectMode, evaluate, formatResult, numbersIn, type FxRates } from "../src/lib/calc.ts";
import assert from "node:assert/strict";

const show = (text: string, auto = false, locale = "en-US", fx?: FxRates) =>
  evaluate(text, auto, fx).map((r) => (r && r.show ? formatResult(r, locale) : null));

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
  // A name may start with a digit, and the k/M suffix still multiplies.
  ["X1 = 2900\ncoolant = 700\n5axis = 1100\nX1 + coolant + 5axis =", [null, null, null, "4,700"]],
  ["5axis = 1100\n5axis * 2 =", [null, "2,200"]],
  ["2k + 500 =\n500k + 1 =", ["2,500", "500,001"]],
  ["3x4=", ["12"]],
  ["3x4 + 1,234.5 =", ["1,246.5"]],
  // A name may be several words, with either thousands separator.
  ["prezzo unitario = 20\nquantita = 3\nprezzo unitario * quantita =", [null, null, "60"]],
  ["prezzo = 1,234.56 €\nprezzo * 2 =", [null, "2,469.12 €"]],
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
  // A word that only looks like a conversion stays text.
  ["2 in 3", [null]],
  // Two currencies cannot be added: no result (not a silent wrong one).
  ["10 € + 5 $ =", [null]],
  ["10 € + 5 =", ["15.00 €"]],
  ["10 € - 5 $ =", [null]],
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
  ["prezzo = 1.234,56 €\nprezzo * 2 =", [null, "2469,12 €"]],
  ["prezzo totale = 20\nprezzo totale * 2 =", [null, "40"]],
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

// Currency conversion: the rate table the app gets from the ECB (EUR based) and
// the "1 € = 1.08 $" line, which overrides it for one note only.
const fx = buildRates({ USD: 1.08, GBP: 0.85, JPY: 170 });
const conversions: [string, (string | null)[], string, FxRates | undefined][] = [
  ["100 € in $ =", ["108.00 $"], "en-US", fx],
  ["100 € to usd =", ["108.00 $"], "en-US", fx],
  ["100 $ in € =", ["92.59 €"], "en-US", fx],
  ["100 € in ¥ =", ["17,000.00 ¥"], "en-US", fx],
  ["100 € in chf =", [null], "en-US", fx], // not in the table: no result
  ["100 in $ =", [null], "en-US", fx], // nothing to convert
  ["rate = 0\n100 € in $ =", [null, "108.00 $"], "en-US", fx], // "0" is not a rate line
  ["1 € = 1.08 $\n100 € in $ =", [null, "108,00 $"], "it-IT", undefined],
  ["1 € = 1.08 $\n50 € in $ =", [null, "54,00 $"], "it-IT", fx], // the note wins
  ["1 ₿ = 60000 €\n0.5 ₿ in € =", [null, "30,000.00 €"], "en-US", undefined],
  ["1 € = 1.08 $\n100 $ in € =", [null, "92,59 €"], "it-IT", undefined],
];
for (const [input, expected, locale, table] of conversions) {
  assert.deepEqual(show(input, false, locale, table), expected, `fx: ${input}`);
}
// In a "math" note the declared rate shows too.
assert.deepEqual(show("math\n1 € = 1.08 $\n100 € in $", true, "en-US"), [null, "1.08 $", "108.00 $"]);
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
