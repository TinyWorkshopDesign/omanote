// The native sketch canvas on iPadOS (crates/tauri-plugin-pencil): a PencilKit view
// laid over the page, driven from the web toolbar. Elsewhere Sketch.svelte draws on
// its own web canvas and none of this is called.
import { addPluginListener, invoke, type PluginListener } from "@tauri-apps/api/core";

export type Ink = "pen" | "pencil" | "marker" | "fountain";
export type SketchTool = Ink | "eraser";
export type EraserMode = "pixel" | "stroke";

export interface ToolState {
  tool: SketchTool;
  /** #rrggbb */
  color: string;
  /** 0, 1 or 2 (thin to thick). */
  size: number;
  eraser: EraserMode;
}

export interface CanvasState {
  empty: boolean;
  canUndo: boolean;
  canRedo: boolean;
}

/** Where the Pencil asked for the palette, in CSS pixels; -1 when it did not say. */
export interface PalettePoint {
  x: number;
  y: number;
}

const frame = (r: DOMRect) => ({ x: r.left, y: r.top, width: r.width, height: r.height });
const call = <T = void>(cmd: string, args?: Record<string, unknown>) => invoke<T>(`plugin:pencil|${cmd}`, args);

export const pencil = {
  open: (r: DOMRect) => call("open", frame(r)),
  setFrame: (r: DOMRect) => call("set_frame", frame(r)),
  setTool: (t: ToolState) => call("set_tool", { ...t }),
  setEnabled: (enabled: boolean) => call("set_enabled", { enabled }),
  undo: () => call("undo"),
  redo: () => call("redo"),
  clear: () => call("clear"),
  close: () => call("close"),
  /** The drawing cropped to its strokes; null when there is nothing drawn. */
  async export(ocr: boolean, background: string): Promise<{ png: Blob; width: number } | null> {
    const r = await call<{ png: string | null; width: number }>("export", { ocr, background });
    if (!r?.png) return null;
    const bytes = Uint8Array.from(atob(r.png), (c) => c.charCodeAt(0));
    return { png: new Blob([bytes], { type: "image/png" }), width: Math.ceil(r.width) };
  },
  onState: (cb: (s: CanvasState) => void): Promise<PluginListener> => addPluginListener("pencil", "state", cb),
  /** The Pencil changed the tool (double tap or squeeze: eraser, previous tool). */
  onTool: (cb: (t: ToolState) => void): Promise<PluginListener> => addPluginListener("pencil", "tool", cb),
  onPalette: (cb: (p: PalettePoint) => void): Promise<PluginListener> => addPluginListener("pencil", "palette", cb),
};
