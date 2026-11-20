export const DB_MIN = -90;

const MARKS: ReadonlyArray<readonly [number, number]> = [
  [DB_MIN, 0],
  [-48, 0.1],
  [-24, 0.3],
  [-12, 0.5],
  [-6, 0.64],
  [0, 0.78],
  [6, 1],
];

export const FADER_TICKS: ReadonlyArray<{ db: number; label: string }> = [
  { db: 6, label: '+6' },
  { db: 0, label: '0' },
  { db: -6, label: '-6' },
  { db: -12, label: '-12' },
  { db: -24, label: '-24' },
  { db: -48, label: '-48' },
  { db: DB_MIN, label: '-∞' },
];

export function dbToPos(db: number): number {
  if (db <= DB_MIN) return 0;
  if (db >= 6) return 1;
  for (let i = 1; i < MARKS.length; i++) {
    const [d1, p1] = MARKS[i];
    if (db <= d1) {
      const [d0, p0] = MARKS[i - 1];
      return p0 + ((db - d0) / (d1 - d0)) * (p1 - p0);
    }
  }
  return 1;
}

export function posToDb(pos: number): number {
  if (pos <= 0) return DB_MIN;
  if (pos >= 1) return 6;
  for (let i = 1; i < MARKS.length; i++) {
    const [d1, p1] = MARKS[i];
    if (pos <= p1) {
      const [d0, p0] = MARKS[i - 1];
      return d0 + ((pos - p0) / (p1 - p0)) * (d1 - d0);
    }
  }
  return 6;
}

export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export const linToDb = (x: number) => (x <= 1e-5 ? DB_MIN : 20 * Math.log10(x));

export function fmtDb(db: number, digits = 1): string {
  if (db <= DB_MIN + 0.05) return '-∞';
  return `${db > 0 ? '+' : ''}${db.toFixed(digits)}`;
}

export function fmtPan(p: number): string {
  const v = Math.round(Math.abs(p) * 100);
  if (v === 0) return 'C';
  return p < 0 ? `L${v}` : `R${v}`;
}

export function fmtMs(ms: number): string {
  return `${ms > 0 ? '+' : ''}${ms.toFixed(1)}`;
}

export function roundTo(v: number, step: number): number {
  return Math.round(v / step) * step;
}
