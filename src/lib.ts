/** `75` -> `1:15`, `3700` -> `1:01:40`. */
export function formatDuration(totalSeconds: number): string {
  const s = Math.max(0, Math.round(totalSeconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

/** Remaining seconds assuming the pace so far continues; `null` until there is enough data. */
export function estimateRemaining(shapes: number, total: number, elapsedSeconds: number): number | null {
  if (shapes < 5 || elapsedSeconds <= 0 || shapes >= total) return null;
  return ((total - shapes) * elapsedSeconds) / shapes;
}

export function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}

/** Last path segment, for both `/` and `\` separators. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

const IMAGE_EXT = /\.(png|jpe?g|bmp|webp|gif)$/i;

export function isImagePathLike(path: string): boolean {
  return IMAGE_EXT.test(path);
}
