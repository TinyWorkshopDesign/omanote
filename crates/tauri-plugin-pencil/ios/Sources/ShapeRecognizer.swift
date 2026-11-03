// Shape snapping for the PencilKit canvas: the same steps as app/src/lib/shapes.ts
// (which has the tests), keep the two in step. A stroke that ends with the pen
// resting for a moment and looks like a line, a triangle, a rectangle, a circle or
// an ellipse becomes the clean shape.

import CoreGraphics
import Foundation

enum ShapeRecognizer {
  /// How long the pen must rest at the end of a stroke to snap it.
  static let holdSeconds: TimeInterval = 0.45
  /// The pen counts as resting while it stays within this many points.
  static let holdRadius: CGFloat = 6

  private static let spacing: CGFloat = 3
  private static let minLength: CGFloat = 30

  struct Shape {
    let kind: String
    /// The outline, evenly spaced; closed shapes end where they start.
    let points: [CGPoint]
  }

  private static func dist(_ a: CGPoint, _ b: CGPoint) -> CGFloat { hypot(a.x - b.x, a.y - b.y) }

  private static func distToSegment(_ p: CGPoint, _ a: CGPoint, _ b: CGPoint) -> CGFloat {
    let dx = b.x - a.x
    let dy = b.y - a.y
    let len = dx * dx + dy * dy
    let k = len > 0 ? max(0, min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len)) : 0
    return hypot(p.x - (a.x + k * dx), p.y - (a.y + k * dy))
  }

  private static func pathLength(_ pts: [CGPoint]) -> CGFloat {
    var l: CGFloat = 0
    for i in pts.indices.dropFirst() { l += dist(pts[i - 1], pts[i]) }
    return l
  }

  /// Index of the first point of the rest at the end of the stroke (where the pen
  /// arrived and stayed); the last index when it did not rest.
  static func restStart(_ pts: [CGPoint], radius: CGFloat = holdRadius) -> Int {
    guard let last = pts.last else { return 0 }
    var i = pts.count - 1
    while i > 0 && dist(pts[i - 1], last) < radius { i -= 1 }
    return i
  }

  /// Ramer–Douglas–Peucker simplification of an open polyline.
  private static func simplify(_ pts: [CGPoint], _ eps: CGFloat) -> [CGPoint] {
    guard pts.count >= 3 else { return pts }
    var worst: CGFloat = 0
    var at = 0
    for i in 1..<(pts.count - 1) {
      let d = distToSegment(pts[i], pts[0], pts[pts.count - 1])
      if d > worst {
        worst = d
        at = i
      }
    }
    if worst <= eps { return [pts[0], pts[pts.count - 1]] }
    let left = simplify(Array(pts[0...at]), eps)
    let right = simplify(Array(pts[at...]), eps)
    return Array(left.dropLast()) + right
  }

  /// Corners of a closed stroke: split at the point farthest from the start, simplify both halves.
  private static func corners(_ pts: [CGPoint], _ eps: CGFloat) -> [CGPoint] {
    var far = 0
    for i in pts.indices where dist(pts[i], pts[0]) > dist(pts[far], pts[0]) { far = i }
    var ring = simplify(Array(pts[0...far]), eps) + simplify(Array(pts[far...]) + [pts[0]], eps).dropFirst()
    ring.removeLast()  // the start again
    var merged: [CGPoint] = []
    for p in ring where merged.isEmpty || dist(merged[merged.count - 1], p) > eps * 1.5 { merged.append(p) }
    while merged.count > 2 && dist(merged[0], merged[merged.count - 1]) <= eps * 1.5 { merged.removeLast() }
    return merged.enumerated().filter { i, p in
      let a = merged[(i + merged.count - 1) % merged.count]
      let b = merged[(i + 1) % merged.count]
      return distToSegment(p, a, b) > eps
    }.map { $0.element }
  }

  private static func resample(_ poly: [CGPoint]) -> [CGPoint] {
    guard let first = poly.first else { return [] }
    var out = [first]
    for i in poly.indices.dropFirst() {
      let a = poly[i - 1]
      let b = poly[i]
      let n = max(1, Int((dist(a, b) / spacing).rounded()))
      for k in 1...n {
        let t = CGFloat(k) / CGFloat(n)
        out.append(CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t))
      }
    }
    return out
  }

  /// Snaps an angle to the nearest multiple of 45° when it is within `tolerance` degrees.
  private static func snapAngle(_ a: CGPoint, _ b: CGPoint, tolerance: CGFloat = 5) -> CGPoint {
    let angle = atan2(b.y - a.y, b.x - a.x)
    let step = CGFloat.pi / 4
    let snapped = (angle / step).rounded() * step
    if abs(angle - snapped) > tolerance * .pi / 180 { return b }
    let l = dist(a, b)
    return CGPoint(x: a.x + l * cos(snapped), y: a.y + l * sin(snapped))
  }

  /// The clean shape a stroke was meant to be, or nil when it is none of them.
  static func recognize(_ input: [CGPoint]) -> Shape? {
    let pts = Array(input.prefix(restStart(input) + 1))
    guard pts.count >= 3 else { return nil }
    // Measured on the stroke without its tremor, which would otherwise make it look longer.
    let length = pathLength(simplify(pts, 2))
    guard length >= minLength else { return nil }
    let start = pts[0]
    let end = pts[pts.count - 1]
    let chord = dist(start, end)

    // A line: the stroke goes straight from start to end without coming back.
    if chord > 0.9 * length {
      let worst = pts.map { distToSegment($0, start, end) }.max() ?? 0
      if worst < max(4, 0.06 * chord) {
        return Shape(kind: "line", points: resample([start, snapAngle(start, end)]))
      }
      return nil
    }

    // Everything else must close on itself.
    guard chord <= max(12, 0.18 * length) else { return nil }
    let minX = pts.map(\.x).min()!
    let maxX = pts.map(\.x).max()!
    let minY = pts.map(\.y).min()!
    let maxY = pts.map(\.y).max()!
    let w = maxX - minX
    let h = maxY - minY
    let diag = hypot(w, h)
    guard w >= 8, h >= 8 else { return nil }

    let poly = corners(pts, 0.06 * diag)
    var polyError = CGFloat.infinity
    if poly.count == 3 || poly.count == 4 {
      var sum: CGFloat = 0
      for p in pts {
        var d = CGFloat.infinity
        for i in poly.indices { d = min(d, distToSegment(p, poly[i], poly[(i + 1) % poly.count])) }
        sum += d
      }
      polyError = sum / CGFloat(pts.count) / (diag / 2)
    }

    let cx = (minX + maxX) / 2
    let cy = (minY + maxY) / 2
    let rx = w / 2
    let ry = h / 2
    let ellipseError =
      pts.reduce(CGFloat(0)) { $0 + abs(hypot(($1.x - cx) / rx, ($1.y - cy) / ry) - 1) } / CGFloat(pts.count)

    if polyError < 0.07 && polyError <= ellipseError {
      if poly.count == 3 { return Shape(kind: "triangle", points: resample(poly + [poly[0]])) }
      let upright = poly.indices.allSatisfy { i in
        let p = poly[i]
        let q = poly[(i + 1) % 4]
        let angle = abs(atan2(q.y - p.y, q.x - p.x)) * 180 / .pi
        let r = angle.truncatingRemainder(dividingBy: 90)
        return min(r, 90 - r) < 12
      }
      if upright {
        let rect = [
          CGPoint(x: minX, y: minY), CGPoint(x: maxX, y: minY),
          CGPoint(x: maxX, y: maxY), CGPoint(x: minX, y: maxY),
        ]
        var first = 0
        for i in 1..<4 where dist(rect[i], start) < dist(rect[first], start) { first = i }
        let ordered = (0..<4).map { rect[(first + $0) % 4] }
        return Shape(kind: "rectangle", points: resample(ordered + [ordered[0]]))
      }
      return Shape(kind: "quadrilateral", points: resample(poly + [poly[0]]))
    }

    if ellipseError < 0.12 {
      let circle = abs(rx - ry) / max(rx, ry) < 0.15
      let ax = circle ? (rx + ry) / 2 : rx
      let ay = circle ? (rx + ry) / 2 : ry
      let a0 = atan2((start.y - cy) / ay, (start.x - cx) / ax)
      let mid = pts[pts.count / 4]
      var turn = atan2((mid.y - cy) / ay, (mid.x - cx) / ax) - a0
      while turn > .pi { turn -= 2 * .pi }
      while turn < -.pi { turn += 2 * .pi }
      let dir: CGFloat = turn < 0 ? -1 : 1
      let perimeter = CGFloat.pi * (3 * (ax + ay) - sqrt((3 * ax + ay) * (ax + 3 * ay)))
      let n = max(24, Int((perimeter / spacing).rounded()))
      let outline = (0...n).map { k -> CGPoint in
        let a = a0 + dir * 2 * .pi * CGFloat(k) / CGFloat(n)
        return CGPoint(x: cx + ax * cos(a), y: cy + ay * sin(a))
      }
      return Shape(kind: circle ? "circle" : "ellipse", points: outline)
    }

    return nil
  }
}
