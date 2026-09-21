import { evaluate, formatResult } from "../src/lib/calc.ts";
import assert from "node:assert/strict";

const show = (text: string) => evaluate(text).map((r) => (r && r.show ? formatResult(r, "it-IT") : null));
const cases: [string, (string | null)[]][] = [
  ["2 + 3 * 4", ["14"]],
  ["(2 + 3) * 4\n2^10", ["20", "1024"]],
  ["Spesa\nlatte 1,50 €\npane 2 €\ntotale", [null, null, null, "3,50 €"]],
  ["iva = 22%\n100 € + iva", ["22%", "122,00 €"]],
  ["20% di 50", ["10"]],
  ["caffè: 1,20 €\npranzo: 12 €\nsomma", ["1,20 €", "12,00 €", "13,20 €"]],
  ["x = 10\nx * 3\nans + 1", ["10", "30", "31"]],
  ["sqrt(16) + 1", ["5"]],
  ["Ho 3 gatti e 2 cani", [null]],
  ["- [ ] comprare latte\n- [x] fatto", [null, null]],
  ["# Titolo 2024", [null]],
  ["prezzo = 80\nsconto = 15%\nprezzo - sconto", ["80", "15%", "68"]],
  ["10 / 4 =", ["2,5"]],
  ["3 x 4", ["12"]],
  ["1,5k + 500", ["2000"]],
  ["10\n20\n\n5\ntotal", [null, null, null, null, "5"]],
  ["media\n4\n6\nmedia", [null, null, null, "5"]],
  ["10 €\n5 €\ntotale\ntotale * 2", [null, null, "15,00 €", "30,00 €"]],
];
for (const [input, expected] of cases) {
  assert.deepEqual(show(input), expected, input);
}
console.log(`calc: ${cases.length} casi ok`);
