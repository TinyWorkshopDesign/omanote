import { recognize, trimHold, type Pt } from "../src/lib/shapes.ts";
import assert from "node:assert/strict";

// A repeatable wobble, like a hand: deterministic pseudo-random noise.
let seed = 7;
const noise = (amount: number) => {
  seed = (seed * 16807) % 2147483647;
  return ((seed / 2147483647) * 2 - 1) * amount;
};

/** Points along a polyline every `step` px, wobbled by `jitter`, with a rest at the end. */
function hand(poly: Pt[], jitter = 1.5, step = 2, rest = 6): Pt[] {
  const out: Pt[] = [];
  for (let i = 1; i < poly.length; i++) {
    const a = poly[i - 1];
    const b = poly[i];
    const n = Math.max(1, Math.round(Math.hypot(b.x - a.x, b.y - a.y) / step));
    for (let k = 0; k < n; k++) {
      out.push({ x: a.x + ((b.x - a.x) * k) / n + noise(jitter), y: a.y + ((b.y - a.y) * k) / n + noise(jitter) });
    }
  }
  const last = poly[poly.length - 1];
  for (let k = 0; k < rest; k++) out.push({ x: last.x + noise(1), y: last.y + noise(1) });
  return out;
}

function ellipse(cx: number, cy: number, rx: number, ry: number, from = 0, sweep = 2 * Math.PI, n = 120): Pt[] {
  const out: Pt[] = [];
  for (let k = 0; k <= n; k++) {
    const a = from + (sweep * k) / n;
    out.push({ x: cx + rx * Math.cos(a), y: cy + ry * Math.sin(a) });
  }
  return out;
}

const kind = (pts: Pt[]) => recognize(pts)?.kind ?? null;

// Lines, snapped to 45° steps when close.
assert.equal(kind(hand([{ x: 10, y: 10 }, { x: 210, y: 18 }])), "line");
const flat = recognize(hand([{ x: 10, y: 50 }, { x: 210, y: 53 }], 1))!;
assert.equal(flat.kind, "line");
const lastPt = flat.points[flat.points.length - 1];
assert.ok(Math.abs(lastPt.y - flat.points[0].y) < 0.2, "a nearly flat line becomes horizontal");
assert.equal(kind(hand([{ x: 0, y: 0 }, { x: 120, y: 125 }])), "line");

// Closed shapes.
assert.equal(kind(hand([{ x: 10, y: 10 }, { x: 210, y: 10 }, { x: 210, y: 110 }, { x: 10, y: 110 }, { x: 12, y: 12 }])), "rectangle");
assert.equal(kind(hand([{ x: 100, y: 10 }, { x: 190, y: 150 }, { x: 10, y: 150 }, { x: 98, y: 14 }])), "triangle");
assert.equal(
  kind(hand([{ x: 60, y: 0 }, { x: 160, y: 60 }, { x: 100, y: 160 }, { x: 0, y: 100 }, { x: 58, y: 3 }])),
  "quadrilateral",
);
assert.equal(kind(hand(ellipse(100, 100, 60, 60, 0.3, 2 * Math.PI - 0.1))), "circle");
assert.equal(kind(hand(ellipse(100, 100, 120, 50, 1, 2 * Math.PI - 0.1))), "ellipse");

// The rectangle starts at the corner where the pen did and closes.
const rect = recognize(hand([{ x: 210, y: 110 }, { x: 10, y: 110 }, { x: 10, y: 10 }, { x: 210, y: 10 }, { x: 208, y: 108 }]))!;
assert.equal(rect.kind, "rectangle");
assert.deepEqual(rect.points[0], rect.points[rect.points.length - 1]);
assert.ok(Math.hypot(rect.points[0].x - 210, rect.points[0].y - 110) < 4);

// Not shapes: handwriting, an open curve, a U, a tiny scribble.
assert.equal(kind(hand(ellipse(100, 100, 60, 60, 0, Math.PI))), null);
assert.equal(kind(hand([{ x: 0, y: 0 }, { x: 0, y: 100 }, { x: 60, y: 100 }, { x: 60, y: 0 }])), null);
const writing: Pt[] = [];
for (let i = 0; i < 200; i++) writing.push({ x: i * 1.5, y: 20 * Math.sin(i / 4) + noise(2) });
assert.equal(kind(writing), null);
assert.equal(kind(hand([{ x: 0, y: 0 }, { x: 8, y: 5 }])), null);
assert.equal(kind([]), null);

// The rest at the end is trimmed, the last point kept.
const trimmed = trimHold([{ x: 0, y: 0 }, { x: 50, y: 0 }, { x: 51, y: 1 }, { x: 50, y: 2 }]);
assert.deepEqual(trimmed, [{ x: 0, y: 0 }, { x: 50, y: 0 }]);

console.log("shapes ok");
