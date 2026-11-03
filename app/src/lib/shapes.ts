// Shape snapping for sketches: draw a line, a triangle, a rectangle, a circle or
// an ellipse, keep the pen still at the end for a moment, and the stroke becomes
// the clean shape. The web canvas (Sketch.svelte) calls `recognize` while the
// pointer rests; the iPad's PencilKit canvas runs the same steps in Swift
// (crates/tauri-plugin-pencil/ios/Sources/ShapeRecognizer.swift): keep the two in step.

export interface Pt {
  x: number;
  y: number;
}

export type ShapeKind = "line" | "triangle" | "rectangle" | "quadrilateral" | "circle" | "ellipse";

export interface Shape {
  kind: ShapeKind;
  /** The outline, evenly spaced (every SPACING px), closed shapes ending where they start. */
  points: Pt[];
}

/** How long the pen must rest at the end of a stroke to snap it, in milliseconds. */
export const HOLD_MS = 450;
/** The pen counts as resting while it stays within this many CSS pixels. */
export const HOLD_RADIUS = 6;

const SPACING = 3;
const MIN_LENGTH = 30;

const dist = (a: Pt, b: Pt) => Math.hypot(a.x - b.x, a.y - b.y);

function distToSegment(p: Pt, a: Pt, b: Pt): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = dx * dx + dy * dy;
  const k = len ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len)) : 0;
  return Math.hypot(p.x - (a.x + k * dx), p.y - (a.y + k * dy));
}

function pathLength(pts: Pt[]): number {
  let l = 0;
  for (let i = 1; i < pts.length; i++) l += dist(pts[i - 1], pts[i]);
  return l;
}

/** The stroke without the points where the pen rested at its end (the last one is kept). */
export function trimHold(pts: Pt[], radius = HOLD_RADIUS): Pt[] {
  if (pts.length < 2) return pts.slice();
  const last = pts[pts.length - 1];
  let i = pts.length - 1;
  while (i > 0 && dist(pts[i - 1], last) < radius) i--;
  return pts.slice(0, i + 1);
}

/** Ramer–Douglas–Peucker simplification of an open polyline. */
function simplify(pts: Pt[], eps: number): Pt[] {
  if (pts.length < 3) return pts.slice();
  let worst = 0;
  let at = 0;
  for (let i = 1; i < pts.length - 1; i++) {
    const d = distToSegment(pts[i], pts[0], pts[pts.length - 1]);
    if (d > worst) {
      worst = d;
      at = i;
    }
  }
  if (worst <= eps) return [pts[0], pts[pts.length - 1]];
  const left = simplify(pts.slice(0, at + 1), eps);
  const right = simplify(pts.slice(at), eps);
  return left.slice(0, -1).concat(right);
}

/** Corners of a closed stroke: split at the point farthest from the start, simplify both halves. */
function corners(pts: Pt[], eps: number): Pt[] {
  let far = 0;
  for (let i = 1; i < pts.length; i++) if (dist(pts[i], pts[0]) > dist(pts[far], pts[0])) far = i;
  const ring = simplify(pts.slice(0, far + 1), eps).concat(simplify(pts.slice(far).concat([pts[0]]), eps).slice(1));
  ring.pop(); // the start again
  // Merge corners that sit close together (a wobble at a corner).
  const merged: Pt[] = [];
  for (const p of ring) if (!merged.length || dist(merged[merged.length - 1], p) > eps * 1.5) merged.push(p);
  while (merged.length > 2 && dist(merged[0], merged[merged.length - 1]) <= eps * 1.5) merged.pop();
  // Drop corners where the outline barely turns (on a straight side).
  return merged.filter((p, i) => {
    const a = merged[(i + merged.length - 1) % merged.length];
    const b = merged[(i + 1) % merged.length];
    return distToSegment(p, a, b) > eps;
  });
}

/** Points every SPACING px along a polyline. */
function resample(poly: Pt[]): Pt[] {
  const out: Pt[] = [poly[0]];
  for (let i = 1; i < poly.length; i++) {
    const a = poly[i - 1];
    const b = poly[i];
    const n = Math.max(1, Math.round(dist(a, b) / SPACING));
    for (let k = 1; k <= n; k++) out.push({ x: a.x + ((b.x - a.x) * k) / n, y: a.y + ((b.y - a.y) * k) / n });
  }
  return out;
}

/** Snaps an angle to the nearest multiple of 45° when it is within `tolerance` degrees. */
function snapAngle(a: Pt, b: Pt, tolerance = 5): Pt {
  const angle = Math.atan2(b.y - a.y, b.x - a.x);
  const step = Math.PI / 4;
  const snapped = Math.round(angle / step) * step;
  if (Math.abs(angle - snapped) > (tolerance * Math.PI) / 180) return b;
  const l = dist(a, b);
  return { x: a.x + l * Math.cos(snapped), y: a.y + l * Math.sin(snapped) };
}

function round(p: Pt): Pt {
  return { x: Math.round(p.x * 10) / 10, y: Math.round(p.y * 10) / 10 };
}

/** The clean shape a stroke was meant to be, or null when it is none of them. */
export function recognize(input: Pt[]): Shape | null {
  const pts = trimHold(input);
  if (pts.length < 3) return null;
  // Measured on the stroke without its tremor, which would otherwise make it look longer.
  const length = pathLength(simplify(pts, 2));
  if (length < MIN_LENGTH) return null;
  const start = pts[0];
  const end = pts[pts.length - 1];
  const chord = dist(start, end);

  // A line: the stroke goes straight from start to end without coming back.
  if (chord > 0.9 * length) {
    let worst = 0;
    for (const p of pts) worst = Math.max(worst, distToSegment(p, start, end));
    if (worst < Math.max(4, 0.06 * chord)) {
      return { kind: "line", points: resample([start, snapAngle(start, end)]).map(round) };
    }
    return null;
  }

  // Everything else must close on itself.
  if (chord > Math.max(12, 0.18 * length)) return null;
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const w = maxX - minX;
  const h = maxY - minY;
  const diag = Math.hypot(w, h);
  if (w < 8 || h < 8) return null;

  // Polygon: its corners, and how far the stroke strays from its sides.
  const poly = corners(pts, 0.06 * diag);
  let polyError = Infinity;
  if (poly.length === 3 || poly.length === 4) {
    let sum = 0;
    for (const p of pts) {
      let d = Infinity;
      for (let i = 0; i < poly.length; i++) d = Math.min(d, distToSegment(p, poly[i], poly[(i + 1) % poly.length]));
      sum += d;
    }
    polyError = sum / pts.length / (diag / 2);
  }

  // Ellipse in the bounding box: how far each point is from it, relative to the radius.
  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;
  const rx = w / 2;
  const ry = h / 2;
  let sum = 0;
  for (const p of pts) sum += Math.abs(Math.hypot((p.x - cx) / rx, (p.y - cy) / ry) - 1);
  const ellipseError = sum / pts.length;

  if (polyError < 0.07 && polyError <= ellipseError) {
    if (poly.length === 3) return { kind: "triangle", points: resample([...poly, poly[0]]).map(round) };
    // Sides close to horizontal and vertical: an upright rectangle.
    const upright = poly.every((p, i) => {
      const q = poly[(i + 1) % 4];
      const angle = (Math.abs(Math.atan2(q.y - p.y, q.x - p.x)) * 180) / Math.PI;
      return Math.min(angle % 90, 90 - (angle % 90)) < 12;
    });
    if (upright) {
      const rect = [
        { x: minX, y: minY },
        { x: maxX, y: minY },
        { x: maxX, y: maxY },
        { x: minX, y: maxY },
      ];
      // Start at the corner nearest the stroke's start, so the outline begins where the pen did.
      let first = 0;
      for (let i = 1; i < 4; i++) if (dist(rect[i], start) < dist(rect[first], start)) first = i;
      const ordered = [0, 1, 2, 3].map((k) => rect[(first + k) % 4]);
      return { kind: "rectangle", points: resample([...ordered, ordered[0]]).map(round) };
    }
    return { kind: "quadrilateral", points: resample([...poly, poly[0]]).map(round) };
  }

  if (ellipseError < 0.12) {
    const circle = Math.abs(rx - ry) / Math.max(rx, ry) < 0.15;
    const ax = circle ? (rx + ry) / 2 : rx;
    const ay = circle ? (rx + ry) / 2 : ry;
    // Begin at the start point's angle, going the way the pen went.
    const a0 = Math.atan2((start.y - cy) / ay, (start.x - cx) / ax);
    const mid = pts[Math.floor(pts.length / 4)];
    const a1 = Math.atan2((mid.y - cy) / ay, (mid.x - cx) / ax);
    let turn = a1 - a0;
    while (turn > Math.PI) turn -= 2 * Math.PI;
    while (turn < -Math.PI) turn += 2 * Math.PI;
    const dir = turn < 0 ? -1 : 1;
    const perimeter = Math.PI * (3 * (ax + ay) - Math.sqrt((3 * ax + ay) * (ax + 3 * ay)));
    const n = Math.max(24, Math.round(perimeter / SPACING));
    const outline: Pt[] = [];
    for (let k = 0; k <= n; k++) {
      const a = a0 + (dir * 2 * Math.PI * k) / n;
      outline.push(round({ x: cx + ax * Math.cos(a), y: cy + ay * Math.sin(a) }));
    }
    return { kind: circle ? "circle" : "ellipse", points: outline };
  }

  return null;
}
