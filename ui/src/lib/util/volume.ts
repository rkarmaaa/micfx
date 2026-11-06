// Slider 0–100 % ↔ dB del core. Curva cubica (−18 dB a metà corsa): segue l'orecchio, non la matematica.
export const MIN_DB = -60;

export function percentToDb(p: number): number {
  if (p <= 0) return MIN_DB;
  return Math.max(MIN_DB, 60 * Math.log10(Math.min(p, 100) / 100));
}

export function dbToPercent(db: number): number {
  if (db <= MIN_DB + 0.01) return 0;
  return Math.max(0, Math.min(100, Math.round(100 * 10 ** (db / 60))));
}

export function formatDb(db: number): string {
  if (db <= MIN_DB + 0.01) return '−∞ dB';
  const r = Math.round(db * 10) / 10;
  return `${r > 0 ? '+' : r < 0 ? '−' : ''}${Math.abs(r).toFixed(1)} dB`;
}

export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 100) / 10);
  if (s < 10) return `${s.toFixed(1)} s`;
  const total = Math.round(s);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
}
