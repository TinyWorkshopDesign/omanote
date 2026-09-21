// Monochrome line icons (Omarchy look: square joins, currentColor, no emoji).
// Used both in Svelte ({@html}) and in CodeMirror widgets.

const svg = (body: string) =>
  `<svg class="ico" viewBox="0 0 24 24" width="1em" height="1em" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="square" stroke-linejoin="miter" aria-hidden="true">${body}</svg>`;

export const icons = {
  /** Picture: frame, sun, mountain. */
  image: svg('<rect x="3" y="4" width="18" height="16"/><circle cx="8.5" cy="9.5" r="1.6"/><path d="M3 17l5-5 4 4 3-3 6 6"/>'),
  /** Text from an image: scan corners around lines of text. */
  ocr: svg('<path d="M3 8V3h5M16 3h5v5M21 16v5h-5M8 21H3v-5"/><path d="M7.5 9h9M7.5 12.5h9M7.5 16h5"/>'),
  /** Encrypted note. */
  lock: svg('<rect x="5" y="11" width="14" height="10"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/><path d="M12 15v2"/>'),
};
