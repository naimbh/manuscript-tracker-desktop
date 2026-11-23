/**
 * Profile photo cropper: a square canvas stage with a circular mask. Drag to pan, wheel / pinch / trackpad
 * gesture to zoom (1×–4×), arrow keys to nudge, + and − to zoom, rotate in 90° steps, and export a square PNG or
 * WebP. Plain canvas and pointer events, no library. The same file is used by the web app
 * (resources/js/avatar-cropper.js) and the desktop app (src/lib/avatarCropper.js).
 *
 *   const cropper = new AvatarCropper(canvas, { onChange: (state) => ... });
 *   cropper.setImage(await loadImage(url));
 *   const blob = await cropper.export();   // 512 × 512, re-encoded (no EXIF), WebP when the browser can encode it
 *   cropper.destroy();
 *
 * Orientation: browsers apply the EXIF orientation when they decode an <img>, so the image is drawn upright and
 * the re-encoded output carries no metadata at all.
 */

export const MAX_SOURCE_BYTES = 10 * 1024 * 1024;
export const MIN_ZOOM = 1;
export const MAX_ZOOM = 4;
export const OUTPUT_SIZE = 512;
export const TARGET_BYTES = 200 * 1024;
export const ACCEPT = 'image/png,image/jpeg,image/webp,image/heic,image/heif,.png,.jpg,.jpeg,.webp,.heic,.heif';
export const MESSAGES = {
  type: 'Choose a PNG, JPEG or WebP image.',
  size: 'That image is too large (max 10 MB).',
  decode: 'That image could not be opened. Choose a PNG, JPEG or WebP image.',
};

/** The image format from a file's first bytes: 'png' | 'jpeg' | 'webp' | 'heic' | null. */
export function sniff(bytes) {
  const b = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const ascii = (from, to) => String.fromCharCode(...b.slice(from, to));
  if (b.length >= 8 && b[0] === 0x89 && ascii(1, 4) === 'PNG') return 'png';
  if (b.length >= 3 && b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff) return 'jpeg';
  if (b.length >= 12 && ascii(0, 4) === 'RIFF' && ascii(8, 12) === 'WEBP') return 'webp';
  if (b.length >= 12 && ascii(4, 8) === 'ftyp' && ['heic', 'heix', 'heim', 'heis', 'hevc', 'hevx', 'mif1', 'msf1'].includes(ascii(8, 12))) return 'heic';
  return null;
}

/** Check a browser File before reading it. Resolves to an error message or null. */
export async function checkFile(file) {
  if (!file) return MESSAGES.type;
  if (file.size > MAX_SOURCE_BYTES) return MESSAGES.size;
  const head = new Uint8Array(await file.slice(0, 16).arrayBuffer());
  return sniff(head) ? null : MESSAGES.type;
}

/** Decode an image URL (data:, blob: or same-origin). Rejects with MESSAGES.decode. */
export function loadImage(src) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.decoding = 'async';
    img.onload = () => (img.naturalWidth && img.naturalHeight ? resolve(img) : reject(new Error(MESSAGES.decode)));
    img.onerror = () => reject(new Error(MESSAGES.decode));
    img.src = src;
  });
}

/** Read a Blob as a data: URL. */
export function blobToDataUrl(blob) {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result);
    r.onerror = () => reject(r.error);
    r.readAsDataURL(blob);
  });
}

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** A copy of the image whose longest side is at most `max` px, scaled down in halves for quality. */
function prepare(img, max = 2048) {
  let w = img.naturalWidth || img.width;
  let h = img.naturalHeight || img.height;
  let src = img;
  while (Math.max(w, h) > max) {
    const nw = Math.max(1, Math.round(Math.max(w / 2, (w * max) / Math.max(w, h))));
    const nh = Math.max(1, Math.round((h * nw) / w));
    const c = document.createElement('canvas');
    c.width = nw;
    c.height = nh;
    const g = c.getContext('2d');
    g.imageSmoothingQuality = 'high';
    g.drawImage(src, 0, 0, nw, nh);
    src = c;
    w = nw;
    h = nh;
  }
  return { src, w, h };
}

function toBlob(canvas, type, quality) {
  return new Promise((resolve) => canvas.toBlob(resolve, type, quality));
}

/** Encode a canvas as described in AvatarCropper.export(). */
export async function encode(canvas, maxBytes = TARGET_BYTES) {
  let last = null;
  for (const q of [0.92, 0.85, 0.78, 0.7, 0.6, 0.5]) {
    const b = await toBlob(canvas, 'image/webp', q);
    if (!b || b.type !== 'image/webp') break; // this browser cannot encode WebP (Safari, WKWebView)
    last = b;
    if (b.size <= maxBytes) break;
  }
  if (last) return stripMetadata(last);
  const png = await toBlob(canvas, 'image/png');
  if (!png) throw new Error('The photo could not be saved.');
  if (png.size > maxBytes) {
    const small = await palettePng(canvas).catch(() => null);
    if (small && small.size < png.size) return small;
  }
  return stripMetadata(png);
}

// ---- Metadata ---------------------------------------------------------------------------------------------

/** PNG chunks kept when stripping metadata (EXIF, text, time and the like are dropped). */
const PNG_KEEP = new Set(['IHDR', 'PLTE', 'tRNS', 'IDAT', 'IEND', 'sRGB', 'gAMA', 'cHRM', 'iCCP', 'sBIT']);

/** Remove EXIF, XMP and text metadata from a PNG or WebP blob (some encoders add a small EXIF block). */
export async function stripMetadata(blob) {
  const b = new Uint8Array(await blob.arrayBuffer());
  const kind = sniff(b.subarray(0, 16));
  const str = (o, n) => String.fromCharCode(...b.subarray(o, o + n));
  if (kind === 'png') {
    const parts = [b.subarray(0, 8)];
    let o = 8;
    while (o + 12 <= b.length) {
      const len = new DataView(b.buffer, b.byteOffset + o, 4).getUint32(0);
      const type = str(o + 4, 4);
      const end = o + 12 + len;
      if (end > b.length) break;
      if (PNG_KEEP.has(type)) parts.push(b.subarray(o, end));
      o = end;
      if (type === 'IEND') break;
    }
    return new Blob(parts, { type: 'image/png' });
  }
  if (kind === 'webp') {
    const parts = [];
    let o = 12;
    while (o + 8 <= b.length) {
      const len = new DataView(b.buffer, b.byteOffset + o + 4, 4).getUint32(0, true);
      const type = str(o, 4);
      const end = Math.min(b.length, o + 8 + len + (len & 1));
      if (type !== 'EXIF' && type !== 'XMP ') {
        const chunk = b.slice(o, end);
        if (type === 'VP8X') chunk[8] &= ~0x0c; // clear the EXIF and XMP flags
        parts.push(chunk);
      }
      o = end;
    }
    const size = parts.reduce((n, p) => n + p.length, 4);
    const head = new Uint8Array(12);
    head.set([0x52, 0x49, 0x46, 0x46]);
    new DataView(head.buffer).setUint32(4, size, true);
    head.set([0x57, 0x45, 0x42, 0x50], 8);
    return new Blob([head, ...parts], { type: 'image/webp' });
  }
  return blob;
}

// ---- 256-colour PNG (for browsers without a WebP encoder) ---------------------------------------------------

/** Median cut: up to 256 colours from a sample of opaque pixels. */
function palette(data, max = 256) {
  const n = data.length / 4;
  const stride = Math.max(1, Math.floor(n / 65536));
  const px = [];
  for (let i = 0; i < n; i += stride) px.push([data[i * 4], data[i * 4 + 1], data[i * 4 + 2]]);
  // Each box remembers its widest channel and a split score (range weighted by size).
  const box = (list) => {
    let axis = 0;
    let range = -1;
    for (let c = 0; c < 3; c++) {
      let lo = 255;
      let hi = 0;
      for (const p of list) { if (p[c] < lo) lo = p[c]; if (p[c] > hi) hi = p[c]; }
      if (hi - lo > range) { range = hi - lo; axis = c; }
    }
    return { list, axis, score: list.length > 1 ? range * Math.sqrt(list.length) : 0 };
  };
  const boxes = [box(px)];
  while (boxes.length < max) {
    let pick = 0;
    boxes.forEach((bx, i) => { if (bx.score > boxes[pick].score) pick = i; });
    if (!boxes[pick].score) break;
    const { list, axis } = boxes[pick];
    list.sort((p, q) => p[axis] - q[axis]);
    const mid = list.length >> 1;
    boxes.splice(pick, 1, box(list.slice(0, mid)), box(list.slice(mid)));
  }
  return boxes.map(({ list }) => {
    const sum = [0, 0, 0];
    for (const p of list) { sum[0] += p[0]; sum[1] += p[1]; sum[2] += p[2]; }
    return sum.map((v) => Math.round(v / list.length));
  });
}

let crcTable = null;
function crc32(bytes) {
  if (!crcTable) {
    crcTable = new Uint32Array(256);
    for (let i = 0; i < 256; i++) {
      let c = i;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crcTable[i] = c >>> 0;
    }
  }
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) c = crcTable[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32(out.subarray(4, 8 + data.length)));
  return out;
}

/** The canvas as an 8-bit palette PNG (median cut + light Floyd–Steinberg dithering). Null when not possible. */
export async function palettePng(canvas) {
  if (typeof CompressionStream === 'undefined') return null;
  const { width: w, height: h } = canvas;
  const data = canvas.getContext('2d').getImageData(0, 0, w, h).data;
  for (let i = 3; i < data.length; i += 4) if (data[i] !== 255) return null; // keep transparency in full colour
  const pal = palette(data);
  const cache = new Map();
  const nearest = (r, g, b) => {
    const key = ((r >> 2) << 12) | ((g >> 2) << 6) | (b >> 2);
    let idx = cache.get(key);
    if (idx === undefined) {
      let best = Infinity;
      for (let i = 0; i < pal.length; i++) {
        const p = pal[i];
        const d = 2 * (p[0] - r) ** 2 + 4 * (p[1] - g) ** 2 + 3 * (p[2] - b) ** 2;
        if (d < best) { best = d; idx = i; }
      }
      cache.set(key, idx);
    }
    return idx;
  };
  // Rows of [filter byte 0, indices...]; errors are diffused at 3/4 strength to limit noise.
  const raw = new Uint8Array(h * (w + 1));
  const err = new Float32Array((w + 2) * 3 * 2);
  for (let y = 0; y < h; y++) {
    const cur = (y & 1) * (w + 2) * 3;
    const next = ((y + 1) & 1) * (w + 2) * 3;
    err.fill(0, next, next + (w + 2) * 3);
    raw[y * (w + 1)] = 0;
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      const e = cur + (x + 1) * 3;
      const r = clamp(Math.round(data[i] + err[e]), 0, 255);
      const g = clamp(Math.round(data[i + 1] + err[e + 1]), 0, 255);
      const b = clamp(Math.round(data[i + 2] + err[e + 2]), 0, 255);
      const idx = nearest(r, g, b);
      raw[y * (w + 1) + 1 + x] = idx;
      const p = pal[idx];
      const d = [(r - p[0]) * 0.75, (g - p[1]) * 0.75, (b - p[2]) * 0.75];
      for (let c = 0; c < 3; c++) {
        err[e + 3 + c] += (d[c] * 7) / 16;
        err[next + x * 3 + c] += (d[c] * 3) / 16;
        err[next + (x + 1) * 3 + c] += (d[c] * 5) / 16;
        err[next + (x + 2) * 3 + c] += d[c] / 16;
      }
    }
  }
  const zlib = new Uint8Array(await new Response(new Blob([raw]).stream().pipeThrough(new CompressionStream('deflate'))).arrayBuffer());
  const ihdr = new Uint8Array(13);
  const v = new DataView(ihdr.buffer);
  v.setUint32(0, w);
  v.setUint32(4, h);
  ihdr.set([8, 3, 0, 0, 0], 8); // 8-bit, palette, deflate, no filter method, no interlace
  const plte = new Uint8Array(pal.length * 3);
  pal.forEach((p, i) => plte.set(p, i * 3));
  return new Blob(
    [new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), pngChunk('IHDR', ihdr), pngChunk('PLTE', plte), pngChunk('IDAT', zlib), pngChunk('IEND', new Uint8Array())],
    { type: 'image/png' },
  );
}

export class AvatarCropper {
  /**
   * @param {HTMLCanvasElement} canvas  the stage (square; its CSS size sets the drawing size)
   * @param {{ onChange?: (state: { zoom: number, rotation: number }) => void, inset?: number }} options
   *   `inset`: gap between the stage edge and the crop circle, in CSS px (default 20)
   */
  constructor(canvas, { onChange = null, inset = 20 } = {}) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.onChange = onChange;
    this.inset = inset;
    this.image = null; // { src, w, h }
    this.zoom = 1;
    this.rotation = 0; // 0, 90, 180, 270
    this.x = 0; // image centre relative to the stage centre, CSS px
    this.y = 0;
    this.size = 0; // stage side, CSS px
    this.pointers = new Map();
    this.gesture = null;
    this.listeners = [];

    const on = (el, type, fn, opts) => {
      el.addEventListener(type, fn, opts);
      this.listeners.push(() => el.removeEventListener(type, fn, opts));
    };
    on(canvas, 'pointerdown', (e) => this.onPointerDown(e));
    on(canvas, 'pointermove', (e) => this.onPointerMove(e));
    on(canvas, 'pointerup', (e) => this.onPointerUp(e));
    on(canvas, 'pointercancel', (e) => this.onPointerUp(e));
    on(canvas, 'wheel', (e) => this.onWheel(e), { passive: false });
    on(canvas, 'keydown', (e) => this.onKey(e));
    // Safari / WKWebView trackpad pinch.
    on(canvas, 'gesturestart', (e) => { e.preventDefault(); this.gesture = { zoom: this.zoom }; }, { passive: false });
    on(canvas, 'gesturechange', (e) => {
      e.preventDefault();
      if (!this.gesture) return;
      const p = this.local(e);
      this.setZoom(this.gesture.zoom * e.scale, p.x, p.y);
    }, { passive: false });
    on(canvas, 'gestureend', (e) => { e.preventDefault(); this.gesture = null; }, { passive: false });

    if (typeof ResizeObserver !== 'undefined') {
      this.observer = new ResizeObserver(() => this.resize());
      this.observer.observe(canvas);
    }
    this.resize();
  }

  destroy() {
    this.listeners.forEach((off) => off());
    this.listeners = [];
    this.observer?.disconnect();
    this.image = null;
  }

  /** Diameter of the crop circle (CSS px). */
  get diameter() {
    return Math.max(1, this.size - 2 * this.inset);
  }

  /** Image size after rotation (source px). */
  get rotated() {
    const { w, h } = this.image ?? { w: 1, h: 1 };
    return this.rotation % 180 ? { w: h, h: w } : { w, h };
  }

  /** CSS px per source px: at zoom 1 the image just covers the circle. */
  get scale() {
    const r = this.rotated;
    return (this.diameter / Math.min(r.w, r.h)) * this.zoom;
  }

  setImage(img) {
    this.image = prepare(img);
    this.reset();
  }

  reset() {
    this.zoom = 1;
    this.rotation = 0;
    this.x = 0;
    this.y = 0;
    this.changed();
  }

  rotate() {
    // Keep the point under the circle's centre where it is.
    const [x, y] = [this.x, this.y];
    this.rotation = (this.rotation + 90) % 360;
    this.x = -y;
    this.y = x;
    this.changed();
  }

  /** Zoom to `z`, keeping the point (px, py) (relative to the stage centre) still. */
  setZoom(z, px = 0, py = 0) {
    const next = clamp(Number(z) || 1, MIN_ZOOM, MAX_ZOOM);
    const f = next / this.zoom;
    this.x = px - (px - this.x) * f;
    this.y = py - (py - this.y) * f;
    this.zoom = next;
    this.changed();
  }

  zoomBy(step) {
    this.setZoom(this.zoom + step);
  }

  pan(dx, dy) {
    this.x += dx;
    this.y += dy;
    this.changed();
  }

  /** Keep the circle covered by the image. */
  clamp() {
    const r = this.rotated;
    const s = this.scale;
    const mx = Math.max(0, (r.w * s - this.diameter) / 2);
    const my = Math.max(0, (r.h * s - this.diameter) / 2);
    this.x = clamp(this.x, -mx, mx);
    this.y = clamp(this.y, -my, my);
  }

  changed() {
    this.clamp();
    this.draw();
    this.onChange?.({ zoom: this.zoom, rotation: this.rotation });
  }

  resize() {
    const rect = this.canvas.getBoundingClientRect();
    const size = Math.round(Math.min(rect.width, rect.height || rect.width));
    if (!size) return;
    const dpr = window.devicePixelRatio || 1;
    if (this.size && size !== this.size) {
      const f = (size - 2 * this.inset) / this.diameter;
      this.x *= f;
      this.y *= f;
    }
    this.size = size;
    this.canvas.width = Math.round(size * dpr);
    this.canvas.height = Math.round(size * dpr);
    this.dpr = dpr;
    this.changed();
  }

  /** Draw the image centred on a square of side `side`; `k` = output px per stage px. */
  paint(g, side, k = 1) {
    const { src, w, h } = this.image;
    const s = this.scale * k;
    g.save();
    g.translate(side / 2 + this.x * k, side / 2 + this.y * k);
    g.rotate((this.rotation * Math.PI) / 180);
    g.imageSmoothingEnabled = true;
    g.imageSmoothingQuality = 'high';
    g.drawImage(src, (-w * s) / 2, (-h * s) / 2, w * s, h * s);
    g.restore();
  }

  draw() {
    const g = this.ctx;
    const { size, dpr } = this;
    if (!size) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, size, size);
    if (!this.image) return;
    this.paint(g, size);
    // Dim everything outside the circle, then outline it.
    const r = this.diameter / 2;
    g.save();
    g.beginPath();
    g.rect(0, 0, size, size);
    g.arc(size / 2, size / 2, r, 0, Math.PI * 2, true);
    g.fillStyle = 'rgba(9, 9, 11, 0.55)';
    g.fill('evenodd');
    g.beginPath();
    g.arc(size / 2, size / 2, r, 0, Math.PI * 2);
    g.lineWidth = 2;
    g.strokeStyle = 'rgba(255, 255, 255, 0.9)';
    g.stroke();
    g.restore();
  }

  /**
   * The crop as a square Blob without metadata: WebP (≤ about `maxBytes`) where the browser encodes WebP;
   * otherwise PNG, reduced to a 256-colour PNG when the full-colour one is larger than `maxBytes`.
   */
  async export({ size = OUTPUT_SIZE, maxBytes = TARGET_BYTES } = {}) {
    if (!this.image) throw new Error(MESSAGES.decode);
    // Render larger and halve down when the source is much bigger than the output (sharper result).
    const k = size / this.diameter;
    const ratio = 1 / (this.scale * k); // source px per output px
    const steps = clamp(Math.ceil(Math.log2(Math.max(1, ratio))) - 1, 0, 2);
    let side = size * 2 ** steps;
    let c = document.createElement('canvas');
    c.width = side;
    c.height = side;
    this.paint(c.getContext('2d'), side, k * 2 ** steps);
    while (side > size) {
      const half = side / 2;
      const n = document.createElement('canvas');
      n.width = half;
      n.height = half;
      const g = n.getContext('2d');
      g.imageSmoothingQuality = 'high';
      g.drawImage(c, 0, 0, half, half);
      c = n;
      side = half;
    }
    return encode(c, maxBytes);
  }

  // ---- Input ------------------------------------------------------------------------------------------

  local(e) {
    const rect = this.canvas.getBoundingClientRect();
    return { x: e.clientX - rect.left - rect.width / 2, y: e.clientY - rect.top - rect.height / 2 };
  }

  onPointerDown(e) {
    if (!this.image || (e.pointerType === 'mouse' && e.button !== 0)) return;
    this.canvas.setPointerCapture?.(e.pointerId);
    this.canvas.focus({ preventScroll: true });
    this.pointers.set(e.pointerId, this.local(e));
    this.pinch = null;
    e.preventDefault();
  }

  onPointerMove(e) {
    const prev = this.pointers.get(e.pointerId);
    if (!prev) return;
    const p = this.local(e);
    if (this.pointers.size === 1) {
      this.pointers.set(e.pointerId, p);
      this.pan(p.x - prev.x, p.y - prev.y);
      return;
    }
    this.pointers.set(e.pointerId, p);
    const [a, b] = [...this.pointers.values()];
    const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const dist = Math.hypot(a.x - b.x, a.y - b.y) || 1;
    if (this.pinch) {
      this.x += mid.x - this.pinch.mid.x;
      this.y += mid.y - this.pinch.mid.y;
      this.setZoom(this.zoom * (dist / this.pinch.dist), mid.x, mid.y);
    }
    this.pinch = { mid, dist };
  }

  onPointerUp(e) {
    this.pointers.delete(e.pointerId);
    this.pinch = null;
    this.canvas.releasePointerCapture?.(e.pointerId);
  }

  onWheel(e) {
    if (!this.image) return;
    e.preventDefault();
    const p = this.local(e);
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1;
    // Trackpad pinch arrives as ctrl + wheel in Chromium; it needs a stronger factor.
    const f = Math.exp(-e.deltaY * unit * (e.ctrlKey ? 0.01 : 0.0015));
    this.setZoom(this.zoom * f, p.x, p.y);
  }

  onKey(e) {
    if (!this.image || e.metaKey || e.ctrlKey || e.altKey) return;
    const step = e.shiftKey ? 20 : 4;
    const moves = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
    if (moves[e.key]) {
      this.pan(...moves[e.key]);
    } else if (e.key === '+' || e.key === '=') {
      this.zoomBy(0.1);
    } else if (e.key === '-' || e.key === '_') {
      this.zoomBy(-0.1);
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  }
}
