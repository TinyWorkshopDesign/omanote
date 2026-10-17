<script lang="ts">
  // Handwriting and sketches: a full-window canvas for a pen (Apple Pencil, a
  // graphics tablet, a Surface pen), a mouse or a finger. The drawing goes into
  // the note as an attachment image or, through OCR, as text.
  //
  // Two engines behind one toolbar:
  // - `native` (iPadOS): a PencilKit view laid over the paper area by
  //   tauri-plugin-pencil (`lib/pencil.ts`). It brings what pointer events cannot:
  //   the Pencil's double tap and squeeze (which open the palette below or switch
  //   tools, as set in the system settings), barrel roll, haptics and Apple's inks.
  // - the web canvas everywhere else. Strokes are kept as points in CSS pixels and
  //   drawn as filled outlines by perfect-freehand, which steadies the input and
  //   varies the width with pen pressure (or with speed for mice and fingers).
  //   The pixel eraser is a stroke that cuts out what is under it.
  //
  // Palm rejection: once a pen has touched the canvas, fingers no longer draw
  // (a hand resting on an iPad would otherwise leave marks).
  import { onDestroy, onMount } from "svelte";
  import { getStroke } from "perfect-freehand";
  import type { PluginListener } from "@tauri-apps/api/core";
  import { MOD, t, type Key } from "$lib/i18n.svelte";
  import { icons } from "$lib/icons";
  import { pencil, type EraserMode, type Ink, type SketchTool, type ToolState } from "$lib/pencil";
  import { roundPoint, withSketchData, type SketchData, type StrokeData } from "$lib/sketchData";

  interface Props {
    /** Draw on the native PencilKit canvas (iPadOS) instead of the web one. */
    native?: boolean;
    /** The strokes of a sketch being edited (see lib/sketchData.ts). */
    initial?: SketchData | null;
    /** The drawing as an image; `width` is its size in CSS pixels. */
    onImage: (png: Blob, width: number) => void;
    /** The drawing rendered for OCR (dark ink on white). */
    onText: (png: Blob) => void;
    onClose: () => void;
  }
  let { native = false, initial = null, onImage, onText, onClose }: Props = $props();

  interface Point {
    x: number;
    y: number;
    /** Pen pressure 0..1 (0.5 for mice and fingers). */
    p: number;
  }
  interface Stroke {
    /** An ink, or "erase": a pixel-eraser pass that cuts out what lies under it. */
    kind: Ink | "erase";
    color: string;
    /** Width in CSS pixels at medium pressure. */
    width: number;
    /** Drawn with a pen: pressure is real, not simulated from speed. */
    pen: boolean;
    pts: Point[];
  }

  /** Theme colours (CSS variables), then the user's own picks (#rrggbb). */
  const COLORS = ["--text", "--accent", "--danger", "--result"];
  const RECENT_KEY = "omanote.sketchColors";
  const RECENT_MAX = 4;

  interface InkLook {
    icon: string;
    label: Key;
    /** Widths for the three sizes, in CSS pixels. */
    widths: [number, number, number];
    /** How much pressure thins the line (perfect-freehand). */
    thinning: number;
    alpha: number;
  }
  const INKS: Record<Ink, InkLook> = {
    pen: { icon: icons.pen, label: "sketch.ballpoint", widths: [2.5, 5, 10], thinning: 0.6, alpha: 1 },
    pencil: { icon: icons.pencil, label: "sketch.pencil", widths: [2, 4, 8], thinning: 0.35, alpha: 0.8 },
    marker: { icon: icons.marker, label: "sketch.marker", widths: [12, 20, 32], thinning: 0, alpha: 0.35 },
    fountain: { icon: icons.fountainPen, label: "sketch.fountain", widths: [3, 6, 12], thinning: 0.7, alpha: 1 },
  };
  /** The fountain pen needs the nib angle and barrel roll: PencilKit only. */
  const inks = $derived((Object.keys(INKS) as Ink[]).filter((k) => native || k !== "fountain"));
  const ERASER_WIDTHS = [12, 28, 56];

  let tool = $state<SketchTool>("pen");
  let lastInk: Ink = "pen";
  let eraserMode = $state<EraserMode>("pixel");
  /** The size index (0..2) of each tool. */
  let sizes = $state<Record<SketchTool, number>>({ pen: 1, pencil: 1, marker: 1, fountain: 1, eraser: 1 });
  let color = $state("--text");
  let recent = $state<string[]>(readRecent());
  let busy = $state(false);
  /** The contextual palette (Pencil squeeze or double tap), in CSS pixels. */
  let palette = $state<{ x: number; y: number } | null>(null);

  // Web engine.
  let strokes = $state.raw<Stroke[]>([]);
  let undone = $state.raw<Stroke[][]>([]);
  let history = $state.raw<Stroke[][]>([]);
  // Native engine: what PencilKit reports.
  let nativeEmpty = $state(true);
  let nativeUndo = $state(false);
  let nativeRedo = $state(false);

  const empty = $derived(native ? nativeEmpty : !strokes.some((s) => s.kind !== "erase"));
  const canUndo = $derived(native ? nativeUndo : history.length > 0);
  const canRedo = $derived(native ? nativeRedo : undone.length > 0);

  let box: HTMLElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let ctx: CanvasRenderingContext2D | null = null;
  /** The finished strokes, drawn once: a frame only adds the live stroke on top. */
  let base: HTMLCanvasElement | null = null;
  let live: Stroke | null = null;
  let frame = 0;
  let activePointer: number | null = null;
  /** Stroke-eraser pass in progress (pixel erasing is a live stroke). */
  let erasing = false;
  /** A pen was used in this session: ignore fingers from now on. */
  let penSeen = false;
  let listeners: Promise<PluginListener>[] = [];

  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  /** A swatch as a colour: theme variables follow the current theme. */
  const resolve = (c: string) => (c.startsWith("--") ? css(c) : c);

  /** Any CSS colour as #rrggbb (PencilKit and the colour picker take only that). */
  function hex(c: string): string {
    const g = document.createElement("canvas").getContext("2d");
    if (!g) return "#808080";
    g.fillStyle = "#808080";
    g.fillStyle = c;
    const v = g.fillStyle;
    return /^#[0-9a-f]{6}$/i.test(v) ? v : "#808080";
  }

  function readRecent(): string[] {
    try {
      const list = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
      return Array.isArray(list) ? list.filter((c) => /^#[0-9a-f]{6}$/i.test(c)).slice(0, RECENT_MAX) : [];
    } catch {
      return [];
    }
  }

  function choose(next: SketchTool) {
    if (next === "eraser" && tool === "eraser") eraserMode = eraserMode === "pixel" ? "stroke" : "pixel";
    tool = next;
    if (next !== "eraser") lastInk = next;
  }

  function pick(c: string) {
    color = c;
    if (tool === "eraser") tool = lastInk;
  }

  /** The system colour picker: preview while choosing, remember on confirm. */
  function picked(e: Event, done: boolean) {
    const c = (e.currentTarget as HTMLInputElement).value.toLowerCase();
    pick(c);
    if (!done) return;
    recent = [c, ...recent.filter((r) => r !== c)].slice(0, RECENT_MAX);
    try {
      localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
    } catch {
      /* not remembered: fine */
    }
  }

  /** The picker starts from the current colour. */
  const pickerValue = $derived(hex(resolve(color)));

  /** The native canvas follows every toolbar change. */
  $effect(() => {
    if (!native) return;
    void pencil.setTool({ tool, color: hex(resolve(color)), size: sizes[tool], eraser: eraserMode });
  });

  /** Saved strokes as web-canvas strokes (unknown inks draw as a pen). */
  const fromData = (list: StrokeData[]): Stroke[] =>
    list
      .filter((d) => d.pts?.length)
      .map((d) => ({
        kind: d.kind === "erase" || d.kind in INKS ? (d.kind as Stroke["kind"]) : "pen",
        color: d.color,
        width: d.width,
        pen: d.pen,
        pts: d.pts.map(([x, y, p]) => ({ x, y, p: p ?? 0.5 })),
      }));

  onMount(() => {
    (document.activeElement as HTMLElement | null)?.blur?.();
    if (initial && !native) strokes = fromData(initial.strokes);
    const ro = new ResizeObserver(resize);
    if (box) ro.observe(box);
    if (native && box) {
      listeners = [
        pencil.onState((s) => {
          nativeEmpty = s.empty;
          nativeUndo = s.canUndo;
          nativeRedo = s.canRedo;
        }),
        pencil.onTool((s: ToolState) => {
          tool = s.tool;
          eraserMode = s.eraser;
          if (s.tool !== "eraser") lastInk = s.tool;
        }),
        pencil.onPalette((p) => openPalette(p.x, p.y)),
      ];
      void pencil.open(box.getBoundingClientRect(), initial);
    }
    return () => ro.disconnect();
  });

  onDestroy(() => {
    if (!native) return;
    void pencil.close();
    for (const l of listeners) void l.then((x) => x.unregister());
  });

  function resize() {
    if (!box) return;
    if (native) {
      void pencil.setFrame(box.getBoundingClientRect());
      return;
    }
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const { width, height } = box.getBoundingClientRect();
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    ctx = canvas.getContext("2d");
    base = document.createElement("canvas");
    base.width = canvas.width;
    base.height = canvas.height;
    rebuild();
  }

  // ------------------------------------------------------------------ palette

  function openPalette(x: number, y: number) {
    const r = box?.getBoundingClientRect();
    if (!r) return;
    // Unknown position (older Pencils): near the top of the paper, centred.
    const px = x < 0 ? r.left + r.width / 2 : x;
    const py = y < 0 ? r.top + 80 : y;
    palette = { x: px, y: py };
    if (native) void pencil.setEnabled(false);
  }

  function closePalette() {
    if (!palette) return;
    palette = null;
    if (native) void pencil.setEnabled(true);
  }

  /** The palette beside the pen tip, kept on screen. */
  function place(node: HTMLElement, at: { x: number; y: number }) {
    const put = (p: { x: number; y: number }) => {
      const { width, height } = node.getBoundingClientRect();
      const m = 8;
      let left = p.x + 24;
      if (left + width > window.innerWidth - m) left = p.x - 24 - width;
      const top = Math.min(Math.max(m, p.y - height / 2), window.innerHeight - height - m);
      node.style.left = `${Math.max(m, left)}px`;
      node.style.top = `${top}px`;
    };
    put(at);
    return { update: put };
  }

  // ------------------------------------------------------------------ drawing (web)

  function outline(s: Stroke, last: boolean): Path2D {
    const look = s.kind === "erase" ? null : INKS[s.kind];
    const pts = getStroke(
      s.pts.map((q) => [q.x, q.y, q.p]),
      {
        size: s.width,
        thinning: look?.thinning ?? 0,
        smoothing: 0.6,
        streamline: 0.55,
        // Mice and fingers: thinner when fast, like ink.
        simulatePressure: !s.pen && (look?.thinning ?? 0) > 0,
        last,
      },
    );
    const path = new Path2D();
    if (pts.length < 2) return path;
    // A closed quadratic curve through the midpoints of the outline.
    path.moveTo(pts[0][0], pts[0][1]);
    for (let i = 0; i < pts.length; i++) {
      const [x0, y0] = pts[i];
      const [x1, y1] = pts[(i + 1) % pts.length];
      path.quadraticCurveTo(x0, y0, (x0 + x1) / 2, (y0 + y1) / 2);
    }
    path.closePath();
    return path;
  }

  function paint(g: CanvasRenderingContext2D, s: Stroke, last: boolean, ink?: string) {
    const path = outline(s, last);
    g.save();
    if (s.kind === "erase") {
      g.globalCompositeOperation = "destination-out";
      g.fillStyle = "#000";
    } else {
      g.globalAlpha = INKS[s.kind].alpha;
      g.fillStyle = ink ?? s.color;
    }
    g.fill(path);
    g.restore();
  }

  /** Redraws the finished strokes into the cache, then the screen. */
  function rebuild() {
    const g = base?.getContext("2d");
    if (!g || !base) return;
    const dpr = window.devicePixelRatio || 1;
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, base.width, base.height);
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    for (const s of strokes) paint(g, s, true);
    show();
  }

  function show() {
    if (!ctx || !canvas || !base) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(base, 0, 0);
    if (live) {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      paint(ctx, live, false);
    }
  }

  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      show();
    });
  }

  /** A new state of the drawing, undoable. */
  function commit(next: Stroke[]) {
    history = [...history, strokes];
    undone = [];
    strokes = next;
    rebuild();
  }

  // ------------------------------------------------------------------ input (web)

  function point(e: PointerEvent): Point {
    const r = canvas!.getBoundingClientRect();
    const p = e.pointerType === "pen" && e.pressure > 0 ? e.pressure : 0.5;
    return { x: e.clientX - r.left, y: e.clientY - r.top, p };
  }

  /** Coalesced events carry the points the pen passed between two frames. */
  function points(e: PointerEvent): Point[] {
    const list = typeof e.getCoalescedEvents === "function" ? e.getCoalescedEvents() : [];
    return (list.length ? list : [e]).map(point);
  }

  function down(e: PointerEvent) {
    if (activePointer !== null || busy) return;
    if (palette) return closePalette();
    if (e.pointerType === "pen") penSeen = true;
    else if (e.pointerType === "touch" && penSeen) return;
    if (e.pointerType === "mouse" && e.button !== 0) return;
    e.preventDefault();
    activePointer = e.pointerId;
    canvas!.setPointerCapture(e.pointerId);
    // The eraser end of a pen (buttons bit 32) erases whatever tool is selected.
    const eraser = tool === "eraser" || (e.buttons & 32) !== 0;
    const pen = e.pointerType === "pen";
    if (eraser && eraserMode === "stroke") {
      erasing = true;
      history = [...history, strokes];
      eraseAt(point(e));
      return;
    }
    if (eraser) {
      live = { kind: "erase", color: "", width: ERASER_WIDTHS[sizes.eraser], pen, pts: [point(e)] };
    } else {
      const ink = tool === "eraser" ? lastInk : tool;
      const width = INKS[ink].widths[sizes[ink]];
      live = { kind: ink, color: resolve(color) || "#888", width, pen, pts: [point(e)] };
    }
    schedule();
  }

  function move(e: PointerEvent) {
    if (e.pointerId !== activePointer) return;
    e.preventDefault();
    if (erasing) {
      for (const p of points(e)) eraseAt(p);
      return;
    }
    if (!live) return;
    for (const p of points(e)) {
      const last = live.pts[live.pts.length - 1];
      if (Math.hypot(p.x - last.x, p.y - last.y) < 0.5) continue;
      live.pts.push(p);
    }
    schedule();
  }

  function up(e: PointerEvent) {
    if (e.pointerId !== activePointer) return;
    activePointer = null;
    if (erasing) {
      erasing = false;
      // Nothing was erased: drop the snapshot taken on pointerdown.
      if (history[history.length - 1] === strokes) history = history.slice(0, -1);
      else undone = [];
      return;
    }
    if (live) {
      const done = live;
      live = null;
      commit([...strokes, done]);
    }
  }

  /** Stroke eraser: removes every ink stroke the eraser touches. */
  function eraseAt(p: Point) {
    const r = ERASER_WIDTHS[sizes.eraser] / 2;
    const hit = (s: Stroke) =>
      s.kind !== "erase" &&
      s.pts.some((q, i) => {
        const d = r + s.width / 2;
        if (i === 0) return Math.hypot(p.x - q.x, p.y - q.y) < d;
        return distToSegment(p, s.pts[i - 1], q) < d;
      });
    const kept = strokes.filter((s) => !hit(s));
    if (kept.length !== strokes.length) {
      strokes = kept;
      rebuild();
    }
  }

  function distToSegment(p: Point, a: Point, b: Point) {
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = dx * dx + dy * dy;
    const k = len ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len)) : 0;
    return Math.hypot(p.x - (a.x + k * dx), p.y - (a.y + k * dy));
  }

  // ------------------------------------------------------------------ actions

  function undo() {
    if (native) return void pencil.undo();
    const prev = history[history.length - 1];
    if (!prev) return;
    history = history.slice(0, -1);
    undone = [...undone, strokes];
    strokes = prev;
    rebuild();
  }

  function redo() {
    if (native) return void pencil.redo();
    const next = undone[undone.length - 1];
    if (!next) return;
    undone = undone.slice(0, -1);
    history = [...history, strokes];
    strokes = next;
    rebuild();
  }

  function clear() {
    if (native) return void pencil.clear();
    if (strokes.length) commit([]);
  }

  // ------------------------------------------------------------------ export

  /** The drawing cropped to its strokes, as a PNG, with its strokes from the PNG's corner. */
  function render(forOcr: boolean): Promise<{ png: Blob; width: number; data: SketchData } | null> {
    const ink = strokes.filter((s) => s.kind !== "erase");
    if (!ink.length) return Promise.resolve(null);
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const s of ink)
      for (const q of s.pts) {
        const w = s.width;
        x0 = Math.min(x0, q.x - w);
        y0 = Math.min(y0, q.y - w);
        x1 = Math.max(x1, q.x + w);
        y1 = Math.max(y1, q.y + w);
      }
    const pad = forOcr ? 24 : 12;
    const w = Math.ceil(x1 - x0 + pad * 2);
    const h = Math.ceil(y1 - y0 + pad * 2);
    // Sharp on Retina screens without storing huge files.
    const scale = forOcr ? 2 : Math.min(2, Math.max(1, window.devicePixelRatio || 1));
    const make = () => {
      const c = document.createElement("canvas");
      c.width = Math.round(w * scale);
      c.height = Math.round(h * scale);
      return c;
    };
    const out = make();
    const layer = make();
    const g = out.getContext("2d");
    const l = layer.getContext("2d");
    if (!g || !l) return Promise.resolve(null);
    // The strokes on a transparent layer first, so the pixel eraser cuts the ink
    // and not the background.
    l.setTransform(scale, 0, 0, scale, (pad - x0) * scale, (pad - y0) * scale);
    for (const s of strokes) paint(l, s, true, forOcr ? "#000000" : undefined);
    // An image keeps the colours on screen, on the theme background, so it reads
    // the same in any client; OCR gets dark ink on white.
    g.fillStyle = forOcr ? "#ffffff" : css("--bg") || "#ffffff";
    g.fillRect(0, 0, out.width, out.height);
    g.drawImage(layer, 0, 0);
    const dx = pad - x0;
    const dy = pad - y0;
    const data: SketchData = {
      v: 1,
      strokes: strokes.map((s) => ({
        kind: s.kind,
        color: s.kind === "erase" ? "" : hex(s.color),
        width: Math.round(s.width * 10) / 10,
        pen: s.pen,
        pts: s.pts.map((q) => roundPoint(q.x + dx, q.y + dy, q.p)),
      })),
    };
    return new Promise((resolve) =>
      out.toBlob((png) => resolve(png ? { png, width: w, data } : null), "image/png"),
    );
  }

  async function finish(asText: boolean) {
    if (busy) return;
    busy = true;
    const r = native ? await pencil.export(asText, hex(css("--bg") || "#ffffff")) : await render(asText);
    busy = false;
    if (!r) return onClose();
    if (asText) return onText(r.png);
    // The strokes travel inside the PNG, so the sketch can be reopened and edited.
    onImage(await withSketchData(r.png, r.data), r.width);
  }

  function key(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    const k = e.key.toLowerCase();
    if (k === "escape") palette ? closePalette() : onClose();
    else if (mod && k === "z" && e.shiftKey) redo();
    else if (mod && k === "z") undo();
    else if (mod && k === "enter") void finish(false);
    else if (mod || e.altKey) return;
    else if (k === "p") choose("pen");
    else if (k === "b") choose("pencil");
    else if (k === "h") choose("marker");
    else if (k === "e") choose("eraser");
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  const KEYS: Partial<Record<SketchTool, string>> = { pen: "P", pencil: "B", marker: "H", eraser: "E" };
  const keyHint = (k: SketchTool) => (KEYS[k] ? ` (${KEYS[k]})` : "");
  const eraserLabel = $derived(t(eraserMode === "stroke" ? "sketch.eraserStroke" : "sketch.eraser"));
  /** The dot shown on a size button: the real width, kept within the button. */
  const dot = (i: number) => {
    const w = tool === "eraser" ? ERASER_WIDTHS[i] : INKS[tool].widths[i];
    return Math.min(26, Math.max(4, w));
  };
</script>

<svelte:window onkeydown={key} />

{#snippet inkButtons()}
  {#each inks as k (k)}
    <button class="icon" class:on={tool === k} title={t(INKS[k].label) + keyHint(k)} onclick={() => choose(k)}
      >{@html INKS[k].icon}</button
    >
  {/each}
  <button class="icon" class:on={tool === "eraser"} title={eraserLabel + keyHint("eraser")} onclick={() => choose("eraser")}
    >{@html eraserMode === "stroke" ? icons.eraserStroke : icons.eraser}</button
  >
{/snippet}

{#snippet colorButtons()}
  {#each [...COLORS, ...recent] as c, i (c)}
    <button
      class="swatch"
      class:extra={i >= COLORS.length + 2}
      class:on={color === c && tool !== "eraser"}
      style="--c: {c.startsWith('--') ? `var(${c})` : c}"
      aria-label={c.startsWith("--") ? c.slice(2) : c}
      onclick={() => pick(c)}
    ></button>
  {/each}
  {#if !COLORS.includes(color) && !recent.includes(color)}
    <button class="swatch on" style="--c: {color}" aria-label={color} onclick={() => pick(color)}></button>
  {/if}
  <label class="swatch custom" title={t("sketch.color")}>
    <input type="color" value={pickerValue} oninput={(e) => picked(e, false)} onchange={(e) => picked(e, true)} />
  </label>
{/snippet}

{#snippet sizeButtons()}
  {#each [0, 1, 2] as i (i)}
    <button class="size" class:on={sizes[tool] === i} aria-label={`${i + 1}`} onclick={() => (sizes[tool] = i)}
      ><span style="--s: {dot(i)}px"></span></button
    >
  {/each}
{/snippet}

<div class="sketch" role="dialog" aria-label={t("sketch.title")}>
  <div class="tools">
    <button class="icon" title={`${t("setup.cancel")} (Esc)`} onclick={onClose}>{@html icons.close}</button>
    <span class="sep"></span>
    {@render inkButtons()}
    <span class="sep"></span>
    {@render colorButtons()}
    <span class="sep"></span>
    {@render sizeButtons()}
    <span class="spacer"></span>
    <button class="icon" title={`${t("sketch.undo")} (${MOD} Z)`} disabled={!canUndo} onclick={undo}
      >{@html icons.undo}</button
    >
    <button class="icon" title={`${t("sketch.redo")} (${MOD} ⇧ Z)`} disabled={!canRedo} onclick={redo}
      >{@html icons.redo}</button
    >
    <button class="icon" title={t("sketch.clear")} disabled={empty} onclick={clear}>{@html icons.trash}</button>
    <span class="sep"></span>
    <button class="icon" title={t("sketch.asText")} disabled={empty || busy} onclick={() => void finish(true)}
      >{@html icons.ocr}</button
    >
    <button class="done" title={t("sketch.asImage")} disabled={empty || busy} onclick={() => void finish(false)}
      >{@html icons.check}<span class="label">{t("sketch.insert")}</span></button
    >
  </div>
  <div class="paper" bind:this={box}>
    {#if !native}
      <canvas
        bind:this={canvas}
        class:erase={tool === "eraser"}
        onpointerdown={down}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}
      ></canvas>
    {/if}
    {#if empty}<p class="hint">{t("sketch.hint")}</p>{/if}
  </div>
  {#if palette}
    <!-- A tap outside closes the palette (the native canvas lets taps through meanwhile). -->
    <div class="veil" role="presentation" onpointerdown={closePalette}></div>
    <div class="palette" role="toolbar" aria-label={t("sketch.palette")} use:place={palette}>
      <div class="row">{@render inkButtons()}</div>
      <div class="row">{@render colorButtons()}</div>
      <div class="row">{@render sizeButtons()}</div>
    </div>
  {/if}
</div>

<style>
  .sketch {
    position: fixed;
    inset: 0;
    z-index: 25;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: env(safe-area-inset-top) 10px 0;
    min-height: calc(var(--bar-h) + env(safe-area-inset-top));
    background: var(--bar);
    border-bottom: 1px solid var(--line);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .icon {
    line-height: 1;
    font-size: var(--ico-l, 24px);
    min-width: 46px;
    min-height: 48px;
    padding: 4px 8px;
    color: var(--muted);
    flex: none;
  }
  .icon:hover:not(:disabled),
  .icon.on {
    color: var(--accent);
  }
  .icon:disabled {
    opacity: 0.3;
  }
  .sep {
    width: 1px;
    align-self: stretch;
    margin: 12px 6px;
    background: var(--line);
    flex: none;
  }
  .spacer {
    flex: 1;
  }
  .swatch {
    flex: none;
    width: 30px;
    height: 30px;
    margin: 0 4px;
    padding: 0;
    background: var(--c);
    border: 2px solid var(--bar);
    outline: 1px solid var(--line);
  }
  .swatch.on {
    outline: 2px solid var(--accent);
  }
  /* "Any colour": a hue wheel that opens the system picker. */
  .swatch.custom {
    position: relative;
    display: block;
    cursor: pointer;
    background: conic-gradient(#f44, #fd4, #4d4, #4dd, #44f, #d4d, #f44);
    border-radius: 50%;
  }
  .swatch.custom input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
    border: 0;
    padding: 0;
  }
  .size {
    flex: none;
    width: 40px;
    height: 44px;
    display: grid;
    place-items: center;
    padding: 0;
  }
  .size span {
    width: var(--s);
    height: var(--s);
    border-radius: 50%;
    background: var(--muted);
  }
  .size.on span {
    background: var(--accent);
  }
  .done {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: 4px;
    padding: 6px 12px;
    background: var(--accent);
    color: var(--bg);
    font-weight: 700;
    font-size: var(--ico-l, 24px);
    line-height: 1;
  }
  .done .label {
    font-size: 0.9rem;
  }
  .done:disabled {
    opacity: 0.35;
  }
  .paper {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  canvas {
    position: absolute;
    inset: 0;
    display: block;
    /* The page must not scroll or zoom under the pen. */
    touch-action: none;
    cursor: crosshair;
    -webkit-user-select: none;
    user-select: none;
    -webkit-touch-callout: none;
  }
  canvas.erase {
    cursor: cell;
  }
  .hint {
    position: absolute;
    top: 40%;
    left: 0;
    right: 0;
    margin: 0;
    text-align: center;
    color: var(--muted);
    pointer-events: none;
  }
  .veil {
    position: fixed;
    inset: 0;
    z-index: 1;
  }
  .palette {
    position: fixed;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    background: var(--bar);
    border: 1px solid var(--line);
    box-shadow: 0 8px 28px rgb(0 0 0 / 0.35);
  }
  .palette .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    max-width: 300px;
  }
  /* Narrow windows (an iPad in portrait): a tighter row, "Insert" as its icon only. */
  @media (max-width: 1000px) {
    .tools {
      padding-left: 4px;
      padding-right: 4px;
    }
    .icon {
      min-width: 36px;
      padding: 4px 4px;
    }
    .sep {
      margin: 12px 3px;
    }
    .swatch {
      width: 26px;
      height: 26px;
      margin: 0 2px;
    }
    /* Only the two latest picks fit; all of them stay in the Pencil's palette. */
    .tools .swatch.extra {
      display: none;
    }
    .size {
      width: 30px;
    }
    .done .label {
      display: none;
    }
  }
  /* Phones: two rows rather than an action scrolled out of sight. */
  @media (max-width: 560px) {
    .tools {
      flex-wrap: wrap;
      row-gap: 0;
      padding-bottom: 4px;
    }
    .sep {
      display: none;
    }
    .done .label {
      display: none;
    }
  }
</style>
