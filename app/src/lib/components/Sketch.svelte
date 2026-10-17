<script lang="ts">
  // Handwriting and sketches: a full-window canvas for a pen (Apple Pencil, a
  // graphics tablet, a Surface pen), a mouse or a finger. The drawing goes into
  // the note as an attachment image or, through OCR, as text.
  //
  // Strokes are kept as points in CSS pixels and redrawn on every change, so a
  // resize or an undo never loses quality. Pens report pressure, which sets the
  // width of each segment; mice and fingers draw at a constant width.
  //
  // Palm rejection: once a pen has touched the canvas, fingers no longer draw
  // (a hand resting on an iPad would otherwise leave marks).
  import { onMount } from "svelte";
  import { MOD, t } from "$lib/i18n.svelte";
  import { icons } from "$lib/icons";

  interface Props {
    /** The drawing as an image; `width` is its size in CSS pixels. */
    onImage: (png: Blob, width: number) => void;
    /** The drawing rendered for OCR (dark ink on white). */
    onText: (png: Blob) => void;
    onClose: () => void;
  }
  let { onImage, onText, onClose }: Props = $props();

  interface Point {
    x: number;
    y: number;
    /** Pen pressure 0..1 (0.5 for mice and fingers). */
    p: number;
  }
  interface Stroke {
    color: string;
    size: number;
    pts: Point[];
  }

  /** Theme colours (CSS variables), then the user's own picks (#rrggbb). */
  const COLORS = ["--text", "--accent", "--danger", "--result"];
  const RECENT_KEY = "omanote.sketchColors";
  const RECENT_MAX = 4;
  const SIZES = [2, 4, 8];
  const ERASER_RADIUS = 12;

  let strokes = $state.raw<Stroke[]>([]);
  let history: Stroke[][] = [];
  let tool = $state<"pen" | "eraser">("pen");
  let color = $state("--text");
  let recent = $state<string[]>(readRecent());
  let size = $state(SIZES[1]);
  let busy = $state(false);

  let box: HTMLElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let ctx: CanvasRenderingContext2D | null = null;
  let live: Stroke | null = null;
  let activePointer: number | null = null;
  let erasing = false;
  /** A pen was used in this session: ignore fingers from now on. */
  let penSeen = false;

  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  /** A swatch as a colour: theme variables follow the current theme. */
  const resolve = (c: string) => (c.startsWith("--") ? css(c) : c);

  function readRecent(): string[] {
    try {
      const list = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
      return Array.isArray(list) ? list.filter((c) => /^#[0-9a-f]{6}$/i.test(c)).slice(0, RECENT_MAX) : [];
    } catch {
      return [];
    }
  }

  function pick(c: string) {
    color = c;
    tool = "pen";
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

  /** The picker starts from the current colour (it only takes #rrggbb). */
  const pickerValue = $derived.by(() => {
    if (!color.startsWith("--")) return color;
    const m = /^#([0-9a-f]{6})$/i.exec(css(color));
    return m ? `#${m[1]}` : "#808080";
  });

  onMount(() => {
    (document.activeElement as HTMLElement | null)?.blur?.();
    const ro = new ResizeObserver(resize);
    if (box) ro.observe(box);
    return () => ro.disconnect();
  });

  function resize() {
    if (!canvas || !box) return;
    const dpr = window.devicePixelRatio || 1;
    const { width, height } = box.getBoundingClientRect();
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    ctx = canvas.getContext("2d");
    ctx?.setTransform(dpr, 0, 0, dpr, 0, 0);
    redraw();
  }

  // ------------------------------------------------------------------ drawing

  const widthAt = (s: Stroke, p: number) => s.size * (0.35 + 1.3 * p);

  /** One segment, smoothed with the midpoints of its neighbours. */
  function segment(g: CanvasRenderingContext2D, s: Stroke, i: number, color = s.color) {
    const pts = s.pts;
    const b = pts[i];
    const a = pts[i - 1] ?? b;
    const z = pts[i - 2] ?? a;
    g.strokeStyle = color;
    g.lineWidth = widthAt(s, a.p);
    g.beginPath();
    g.moveTo((z.x + a.x) / 2, (z.y + a.y) / 2);
    g.quadraticCurveTo(a.x, a.y, (a.x + b.x) / 2, (a.y + b.y) / 2);
    g.stroke();
  }

  function paint(g: CanvasRenderingContext2D, s: Stroke, color = s.color) {
    g.lineCap = "round";
    g.lineJoin = "round";
    if (s.pts.length === 1) {
      const p = s.pts[0];
      g.fillStyle = color;
      g.beginPath();
      g.arc(p.x, p.y, widthAt(s, p.p) / 2, 0, Math.PI * 2);
      g.fill();
      return;
    }
    for (let i = 1; i < s.pts.length; i++) segment(g, s, i, color);
    // The smoothing stops half a segment short: finish at the last point.
    const a = s.pts[s.pts.length - 2];
    const b = s.pts[s.pts.length - 1];
    g.strokeStyle = color;
    g.lineWidth = widthAt(s, b.p);
    g.beginPath();
    g.moveTo((a.x + b.x) / 2, (a.y + b.y) / 2);
    g.lineTo(b.x, b.y);
    g.stroke();
  }

  function redraw() {
    if (!ctx || !canvas) return;
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.restore();
    for (const s of strokes) paint(ctx, s);
    if (live) paint(ctx, live);
  }

  // ------------------------------------------------------------------ input

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
    if (e.pointerType === "pen") penSeen = true;
    else if (e.pointerType === "touch" && penSeen) return;
    if (e.pointerType === "mouse" && e.button !== 0) return;
    e.preventDefault();
    activePointer = e.pointerId;
    canvas!.setPointerCapture(e.pointerId);
    // The eraser end of a pen (buttons bit 32) erases whatever tool is selected.
    erasing = tool === "eraser" || (e.buttons & 32) !== 0;
    if (erasing) {
      history.push(strokes);
      eraseAt(point(e));
      return;
    }
    live = { color: resolve(color) || "#888", size, pts: [point(e)] };
    redraw();
  }

  function move(e: PointerEvent) {
    if (e.pointerId !== activePointer) return;
    e.preventDefault();
    if (erasing) {
      for (const p of points(e)) eraseAt(p);
      return;
    }
    if (!live || !ctx) return;
    for (const p of points(e)) {
      const last = live.pts[live.pts.length - 1];
      if (Math.hypot(p.x - last.x, p.y - last.y) < 0.6) continue;
      live.pts.push(p);
      ctx.lineCap = "round";
      segment(ctx, live, live.pts.length - 1);
    }
  }

  function up(e: PointerEvent) {
    if (e.pointerId !== activePointer) return;
    activePointer = null;
    if (erasing) {
      erasing = false;
      // Nothing was erased: drop the snapshot taken on pointerdown.
      if (history[history.length - 1] === strokes) history.pop();
      return;
    }
    if (live) {
      history.push(strokes);
      strokes = [...strokes, live];
      live = null;
      redraw();
    }
  }

  function eraseAt(p: Point) {
    const hit = (s: Stroke) =>
      s.pts.some((q, i) => {
        const r = ERASER_RADIUS + widthAt(s, q.p) / 2;
        if (i === 0) return Math.hypot(p.x - q.x, p.y - q.y) < r;
        return distToSegment(p, s.pts[i - 1], q) < r;
      });
    const kept = strokes.filter((s) => !hit(s));
    if (kept.length !== strokes.length) {
      strokes = kept;
      redraw();
    }
  }

  function distToSegment(p: Point, a: Point, b: Point) {
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = dx * dx + dy * dy;
    const k = len ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len)) : 0;
    return Math.hypot(p.x - (a.x + k * dx), p.y - (a.y + k * dy));
  }

  function undo() {
    const prev = history.pop();
    if (!prev) return;
    strokes = prev;
    redraw();
  }

  function clear() {
    if (!strokes.length) return;
    history.push(strokes);
    strokes = [];
    redraw();
  }

  // ------------------------------------------------------------------ export

  /** The drawing cropped to its strokes, as a PNG. */
  function render(forOcr: boolean): Promise<{ png: Blob; width: number } | null> {
    if (!strokes.length) return Promise.resolve(null);
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const s of strokes)
      for (const q of s.pts) {
        const w = widthAt(s, q.p) / 2;
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
    const out = document.createElement("canvas");
    out.width = Math.round(w * scale);
    out.height = Math.round(h * scale);
    const g = out.getContext("2d");
    if (!g) return Promise.resolve(null);
    // An image keeps the colours on screen, on the theme background, so it reads
    // the same in any client; OCR gets dark ink on white.
    g.fillStyle = forOcr ? "#ffffff" : css("--bg") || "#ffffff";
    g.fillRect(0, 0, out.width, out.height);
    g.setTransform(scale, 0, 0, scale, (pad - x0) * scale, (pad - y0) * scale);
    for (const s of strokes) paint(g, s, forOcr ? "#000000" : s.color);
    return new Promise((resolve) =>
      out.toBlob((png) => resolve(png ? { png, width: w } : null), "image/png"),
    );
  }

  async function finish(asText: boolean) {
    if (busy) return;
    busy = true;
    const r = await render(asText);
    busy = false;
    if (!r) return onClose();
    if (asText) onText(r.png);
    else onImage(r.png, r.width);
  }

  function key(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    const k = e.key.toLowerCase();
    if (k === "escape") onClose();
    else if (mod && k === "z") undo();
    else if (mod && k === "enter") void finish(false);
    else if (mod || e.altKey) return;
    else if (k === "p") tool = "pen";
    else if (k === "e") tool = "eraser";
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<svelte:window onkeydown={key} />

<div class="sketch" role="dialog" aria-label={t("sketch.title")}>
  <div class="tools">
    <button class="icon" title={`${t("setup.cancel")} (Esc)`} onclick={onClose}>{@html icons.close}</button>
    <span class="sep"></span>
    <button class="icon" class:on={tool === "pen"} title={`${t("sketch.pen")} (P)`} onclick={() => (tool = "pen")}
      >{@html icons.draw}</button
    >
    <button
      class="icon"
      class:on={tool === "eraser"}
      title={`${t("sketch.eraser")} (E)`}
      onclick={() => (tool = "eraser")}>{@html icons.eraser}</button
    >
    <span class="sep"></span>
    {#each [...COLORS, ...recent] as c (c)}
      <button
        class="swatch"
        class:on={color === c && tool === "pen"}
        style="--c: {c.startsWith('--') ? `var(${c})` : c}"
        aria-label={c.startsWith("--") ? c.slice(2) : c}
        onclick={() => pick(c)}
      ></button>
    {/each}
    {#if !COLORS.includes(color) && !recent.includes(color)}
      <button class="swatch on" style="--c: {color}" aria-label={color} onclick={() => pick(color)}></button>
    {/if}
    <label class="swatch custom" title={t("sketch.color")}>
      <input
        type="color"
        value={pickerValue}
        oninput={(e) => picked(e, false)}
        onchange={(e) => picked(e, true)}
      />
    </label>
    <span class="sep"></span>
    {#each SIZES as s (s)}
      <button
        class="size"
        class:on={size === s && tool === "pen"}
        aria-label={`${s}px`}
        onclick={() => ((size = s), (tool = "pen"))}><span style="--s: {s + 2}px"></span></button
      >
    {/each}
    <span class="spacer"></span>
    <button class="icon" title={`${t("sketch.undo")} (${MOD} Z)`} disabled={!strokes.length && !history.length} onclick={undo}
      >{@html icons.undo}</button
    >
    <button class="icon" title={t("sketch.clear")} disabled={!strokes.length} onclick={clear}>{@html icons.trash}</button>
    <span class="sep"></span>
    <button class="icon" title={t("sketch.asText")} disabled={!strokes.length || busy} onclick={() => void finish(true)}
      >{@html icons.ocr}</button
    >
    <button class="done" title={t("sketch.asImage")} disabled={!strokes.length || busy} onclick={() => void finish(false)}
      >{@html icons.check}<span class="label">{t("sketch.insert")}</span></button
    >
  </div>
  <div class="paper" bind:this={box}>
    <canvas
      bind:this={canvas}
      class:erase={tool === "eraser"}
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
    ></canvas>
    {#if !strokes.length}<p class="hint">{t("sketch.hint")}</p>{/if}
  </div>
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
