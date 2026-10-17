// Editable sketches. A drawing is saved as a plain PNG attachment (every Joplin
// client shows it); its strokes travel inside that same PNG, in a compressed iTXt
// chunk with the keyword below, which other programs ignore. Reopening the PNG in
// Omanote brings the strokes back. Coordinates are CSS pixels from the PNG's
// top-left corner (the drawing's crop), so a reopened sketch sits at the top-left
// of the paper.
//
// `strokes` is the common format both canvases read. The iPad canvas also stores
// its own PencilKit drawing (`pencilkit`, base64), which it prefers when present:
// the fountain pen, PencilKit's inks and its pixel eraser come back exactly. On the
// web canvas a PencilKit drawing comes back from `strokes`, an approximation.

const KEYWORD = "omanote-sketch";

export interface StrokeData {
  /** "pen", "pencil", "marker", "fountain", or "erase" (a pixel-eraser pass). */
  kind: string;
  /** #rrggbb */
  color: string;
  /** Width in CSS pixels at medium pressure. */
  width: number;
  /** Pressure is real (a pen), not simulated from speed. */
  pen: boolean;
  /** [x, y, pressure 0..1] */
  pts: number[][];
}

export interface SketchData {
  v: 1;
  strokes: StrokeData[];
  pencilkit?: string;
}

// ------------------------------------------------------------------ PNG chunks

const SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

async function deflate(bytes: Uint8Array, inflate = false): Promise<Uint8Array> {
  const stream = new Blob([bytes as Uint8Array<ArrayBuffer>])
    .stream()
    .pipeThrough(inflate ? new DecompressionStream("deflate") : new CompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

interface Chunk {
  type: string;
  /** Offset of the chunk's length field. */
  at: number;
  data: Uint8Array;
}

function chunks(png: Uint8Array): Chunk[] | null {
  if (png.length < 8 || SIGNATURE.some((b, i) => png[i] !== b)) return null;
  const view = new DataView(png.buffer, png.byteOffset, png.byteLength);
  const out: Chunk[] = [];
  for (let at = 8; at + 12 <= png.length; ) {
    const len = view.getUint32(at);
    const type = String.fromCharCode(...png.subarray(at + 4, at + 8));
    out.push({ type, at, data: png.subarray(at + 8, at + 8 + len) });
    at += 12 + len;
    if (type === "IEND") break;
  }
  return out;
}

/** The PNG with the sketch's strokes added (before IEND, replacing an older copy). */
export async function withSketchData(png: Blob, data: SketchData): Promise<Blob> {
  const bytes = new Uint8Array(await png.arrayBuffer());
  const list = chunks(bytes);
  const end = list?.find((c) => c.type === "IEND");
  if (!list || !end) return png;
  const enc = new TextEncoder();
  // iTXt: keyword, NUL, compressed (1), method (0 = zlib), language, NUL, translated keyword, NUL, text.
  const text = await deflate(enc.encode(JSON.stringify(data)));
  const head = enc.encode(KEYWORD);
  const body = new Uint8Array(head.length + 5 + text.length);
  body.set(head, 0);
  body.set([0, 1, 0, 0, 0], head.length);
  body.set(text, head.length + 5);
  const typed = new Uint8Array(4 + body.length);
  typed.set(enc.encode("iTXt"), 0);
  typed.set(body, 4);
  const chunk = new Uint8Array(12 + body.length);
  const view = new DataView(chunk.buffer);
  view.setUint32(0, body.length);
  chunk.set(typed, 4);
  view.setUint32(8 + body.length, crc32(typed));
  const parts: Uint8Array[] = [bytes.subarray(0, 8)];
  for (const c of list) {
    if (c.type === "iTXt" && keyword(c.data) === KEYWORD) continue;
    if (c === end) parts.push(chunk);
    parts.push(bytes.subarray(c.at, c.at + 12 + c.data.length));
  }
  return new Blob(parts as Uint8Array<ArrayBuffer>[], { type: "image/png" });
}

function keyword(data: Uint8Array): string {
  const nul = data.indexOf(0);
  return nul < 0 ? "" : new TextDecoder().decode(data.subarray(0, nul));
}

/** The strokes saved in a sketch PNG, or null (not a sketch, or one made before editing). */
export async function readSketchData(png: Blob): Promise<SketchData | null> {
  const bytes = new Uint8Array(await png.arrayBuffer());
  const chunk = chunks(bytes)?.find((c) => c.type === "iTXt" && keyword(c.data) === KEYWORD);
  if (!chunk) return null;
  try {
    const d = chunk.data;
    let i = d.indexOf(0) + 1;
    const compressed = d[i] === 1;
    i += 2;
    i = d.indexOf(0, i) + 1; // language
    i = d.indexOf(0, i) + 1; // translated keyword
    const raw = d.subarray(i);
    const json = new TextDecoder().decode(compressed ? await deflate(raw, true) : raw);
    const data = JSON.parse(json) as SketchData;
    return data?.v === 1 && Array.isArray(data.strokes) ? data : null;
  } catch {
    return null;
  }
}

/** Points rounded for storage: tenths of a pixel, hundredths of pressure. */
export const roundPoint = (x: number, y: number, p: number) => [
  Math.round(x * 10) / 10,
  Math.round(y * 10) / 10,
  Math.round(p * 100) / 100,
];
