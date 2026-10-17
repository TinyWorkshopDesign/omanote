// The sketch canvas as a native PencilKit view (see src/lib.rs).
//
// The web toolbar stays where it is; this plugin lays a transparent PKCanvasView
// over the free area of the page, at the frame the web view reports. The web
// view picks tools and colours with `set_tool`; the Pencil's own gestures (double
// tap, and squeeze on Apple Pencil Pro) are handled here and reported back with
// the "tool" and "palette" events, so the toolbar always shows the real tool.
// PencilKit brings the rest: pressure and tilt, barrel roll on the fountain pen,
// palm rejection, stroke smoothing, the pixel and stroke erasers, and the
// system's "only draw with Apple Pencil" setting.

import PencilKit
import Tauri
import UIKit
import WebKit

struct FrameArgs: Decodable {
  let x: Double
  let y: Double
  let width: Double
  let height: Double

  var rect: CGRect { CGRect(x: x, y: y, width: width, height: height) }
}

/// A tool as the web toolbar knows it: also sent back when the Pencil changes it.
struct ToolArgs: Codable, Equatable {
  /// "pen", "pencil", "marker", "fountain" or "eraser".
  var tool: String
  /// "#rrggbb".
  var color: String
  /// 0, 1 or 2 (thin to thick).
  var size: Int
  /// "pixel" or "stroke".
  var eraser: String
}

struct EnabledArgs: Decodable {
  let enabled: Bool
}

struct ExportArgs: Decodable {
  /// Black ink on white for OCR, instead of the colours on the theme background.
  let ocr: Bool
  /// "#rrggbb".
  let background: String
}

struct StateEvent: Encodable {
  let empty: Bool
  let canUndo: Bool
  let canRedo: Bool
}

/// Where the Pencil asked for the palette, in web view (CSS) coordinates; -1 when unknown.
struct PaletteEvent: Encodable {
  let x: Double
  let y: Double
}

/// A canvas with its own undo history, so undo and redo cover exactly this drawing.
final class SketchCanvas: PKCanvasView {
  private let history = UndoManager()
  override var undoManager: UndoManager? { history }
}

class PencilPlugin: Plugin, PKCanvasViewDelegate, UIPencilInteractionDelegate {
  private weak var webview: WKWebView?
  private var canvas: SketchCanvas?

  private var current = ToolArgs(tool: "pen", color: "#000000", size: 1, eraser: "pixel")
  /// The tool before the current one ("switch to previous tool").
  private var previous = ToolArgs(tool: "eraser", color: "#000000", size: 1, eraser: "pixel")
  /// The last inking tool, where "switch to eraser" returns.
  private var lastInk = ToolArgs(tool: "pen", color: "#000000", size: 1, eraser: "pixel")

  override func load(webview: WKWebView) {
    self.webview = webview
  }

  // ---------------------------------------------------------------- commands

  @objc func open(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(FrameArgs.self)
    DispatchQueue.main.async { [self] in
      guard let webview else { return invoke.reject("no web view") }
      canvas?.removeFromSuperview()
      let c = SketchCanvas(frame: place(args.rect, in: webview))
      // Transparent: the page (theme background, hint, palette) shows through.
      c.backgroundColor = .clear
      c.isOpaque = false
      // PencilKit adapts ink colours to dark mode; the theme colours must stay as picked.
      c.overrideUserInterfaceStyle = .light
      c.drawingPolicy = .default
      c.isScrollEnabled = false
      c.minimumZoomScale = 1
      c.maximumZoomScale = 1
      c.contentInsetAdjustmentBehavior = .never
      c.delegate = self
      let pencil = UIPencilInteraction()
      pencil.delegate = self
      c.addInteraction(pencil)
      webview.addSubview(c)
      canvas = c
      c.tool = makeTool(current)
      sendState()
      invoke.resolve()
    }
  }

  @objc func setFrame(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(FrameArgs.self)
    DispatchQueue.main.async { [self] in
      if let webview { canvas?.frame = place(args.rect, in: webview) }
      invoke.resolve()
    }
  }

  @objc func setTool(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ToolArgs.self)
    DispatchQueue.main.async { [self] in
      select(args)
      invoke.resolve()
    }
  }

  /// Off while a web palette is open over the canvas, so taps reach the page.
  @objc func setEnabled(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(EnabledArgs.self)
    DispatchQueue.main.async { [self] in
      canvas?.isUserInteractionEnabled = args.enabled
      invoke.resolve()
    }
  }

  @objc func undo(_ invoke: Invoke) {
    DispatchQueue.main.async { [self] in
      canvas?.undoManager?.undo()
      sendState()
      invoke.resolve()
    }
  }

  @objc func redo(_ invoke: Invoke) {
    DispatchQueue.main.async { [self] in
      canvas?.undoManager?.redo()
      sendState()
      invoke.resolve()
    }
  }

  /// Clears the drawing as one undoable step.
  @objc func clear(_ invoke: Invoke) {
    DispatchQueue.main.async { [self] in
      if let c = canvas, !c.drawing.strokes.isEmpty {
        replace(c, with: PKDrawing())
      }
      invoke.resolve()
    }
  }

  /// The drawing cropped to its strokes as a base64 PNG at 2x, with its width in points.
  @objc func export(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ExportArgs.self)
    DispatchQueue.main.async { [self] in
      guard let c = canvas, !c.drawing.strokes.isEmpty else {
        return invoke.resolve(["png": nil, "width": 0])
      }
      var drawing = c.drawing
      if args.ocr {
        drawing = PKDrawing(
          strokes: drawing.strokes.map { stroke in
            var s = stroke
            s.ink = PKInk(s.ink.inkType, color: .black)
            return s
          })
      }
      let pad: CGFloat = args.ocr ? 24 : 12
      let rect = drawing.bounds.insetBy(dx: -pad, dy: -pad).integral
      let scale: CGFloat = 2
      var ink = UIImage()
      // Rendered in light mode, like the canvas, so the colours are the picked ones.
      UITraitCollection(userInterfaceStyle: .light).performAsCurrent {
        ink = drawing.image(from: rect, scale: scale)
      }
      let format = UIGraphicsImageRendererFormat()
      format.scale = scale
      format.opaque = true
      let png = UIGraphicsImageRenderer(size: rect.size, format: format).pngData { g in
        (args.ocr ? UIColor.white : UIColor(hex: args.background) ?? .white).setFill()
        g.fill(CGRect(origin: .zero, size: rect.size))
        ink.draw(at: .zero)
      }
      invoke.resolve(["png": png.base64EncodedString(), "width": Double(rect.width)])
    }
  }

  @objc func close(_ invoke: Invoke) {
    DispatchQueue.main.async { [self] in
      canvas?.removeFromSuperview()
      canvas = nil
      invoke.resolve()
    }
  }

  // ---------------------------------------------------------------- canvas

  /// CSS pixels to web view points. The page starts where the scroll view's content
  /// starts, below the safe-area inset (the status bar), not at the web view's top.
  private func place(_ rect: CGRect, in webview: WKWebView) -> CGRect {
    webview.scrollView.convert(rect, to: webview)
  }

  func canvasViewDrawingDidChange(_ canvasView: PKCanvasView) {
    sendState()
  }

  private func replace(_ c: SketchCanvas, with drawing: PKDrawing) {
    let old = c.drawing
    c.undoManager?.registerUndo(withTarget: c) { [weak self] target in
      if let self { self.replace(target as! SketchCanvas, with: old) }
    }
    c.drawing = drawing
  }

  /// Undo and redo availability changes after the delegate call: report on the next turn.
  private func sendState() {
    DispatchQueue.main.async { [self] in
      guard let c = canvas else { return }
      try? trigger(
        "state",
        data: StateEvent(
          empty: c.drawing.strokes.isEmpty,
          canUndo: c.undoManager?.canUndo ?? false,
          canRedo: c.undoManager?.canRedo ?? false))
    }
  }

  private func select(_ tool: ToolArgs) {
    if tool.tool != current.tool { previous = current }
    current = tool
    if tool.tool != "eraser" { lastInk = tool }
    canvas?.tool = makeTool(tool)
  }

  private func makeTool(_ t: ToolArgs) -> PKTool {
    let i = max(0, min(2, t.size))
    if t.tool == "eraser" {
      let type: PKEraserTool.EraserType = t.eraser == "stroke" ? .vector : .bitmap
      if #available(iOS 16.4, *) {
        return PKEraserTool(type, width: [8, 20, 44][i])
      }
      return PKEraserTool(type)
    }
    var type = PKInkingTool.InkType.pen
    var widths: [CGFloat] = [2.5, 5, 10]
    switch t.tool {
    case "pencil":
      type = .pencil
      widths = [3, 6, 12]
    case "marker":
      type = .marker
      widths = [12, 20, 32]
    case "fountain":
      if #available(iOS 17.0, *) { type = .fountainPen }
      widths = [3, 6, 12]
    default:
      break
    }
    let range = type.validWidthRange
    let width = min(range.upperBound, max(range.lowerBound, widths[i]))
    return PKInkingTool(type, color: UIColor(hex: t.color) ?? .black, width: width)
  }

  // ---------------------------------------------------------------- Apple Pencil

  /// Double tap (Apple Pencil 2 and Pro), following the user's choice in Settings.
  @available(iOS 17.5, *)
  func pencilInteraction(_ interaction: UIPencilInteraction, didReceiveTap tap: UIPencilInteraction.Tap) {
    perform(UIPencilInteraction.preferredTapAction, at: tap.hoverPose?.location, in: interaction.view)
  }

  /// Squeeze (Apple Pencil Pro): acts once the squeeze is released.
  @available(iOS 17.5, *)
  func pencilInteraction(
    _ interaction: UIPencilInteraction, didReceiveSqueeze squeeze: UIPencilInteraction.Squeeze
  ) {
    guard squeeze.phase == .ended else { return }
    perform(UIPencilInteraction.preferredSqueezeAction, at: squeeze.hoverPose?.location, in: interaction.view)
  }

  /// Double tap before iPadOS 17.5 (no position, no squeeze).
  func pencilInteractionDidTap(_ interaction: UIPencilInteraction) {
    perform(UIPencilInteraction.preferredTapAction, at: nil, in: interaction.view)
  }

  private func perform(_ action: UIPencilPreferredAction, at point: CGPoint?, in view: UIView?) {
    switch action {
    case .switchEraser:
      select(current.tool == "eraser" ? lastInk : ToolArgs(
        tool: "eraser", color: current.color, size: current.size, eraser: current.eraser))
      try? trigger("tool", data: current)
    case .switchPrevious:
      select(previous)
      try? trigger("tool", data: current)
    case .showColorPalette, .showInkAttributes:
      showPalette(at: point, in: view)
    default:
      if #available(iOS 17.5, *), action == .showContextualPalette {
        showPalette(at: point, in: view)
      }
    // .ignore, and .runSystemShortcut which the system runs itself.
    }
  }

  private func showPalette(at point: CGPoint?, in view: UIView?) {
    var x = -1.0
    var y = -1.0
    if let point, let view, let webview {
      let p = view.convert(point, to: webview.scrollView)
      x = Double(p.x)
      y = Double(p.y)
    }
    try? trigger("palette", data: PaletteEvent(x: x, y: y))
  }
}

extension UIColor {
  /// "#rrggbb".
  convenience init?(hex: String) {
    var s = hex.trimmingCharacters(in: .whitespaces)
    if s.hasPrefix("#") { s.removeFirst() }
    guard s.count == 6, let v = UInt32(s, radix: 16) else { return nil }
    self.init(
      red: CGFloat((v >> 16) & 0xff) / 255,
      green: CGFloat((v >> 8) & 0xff) / 255,
      blue: CGFloat(v & 0xff) / 255,
      alpha: 1)
  }
}

@_cdecl("init_plugin_pencil")
func initPlugin() -> Plugin {
  return PencilPlugin()
}
