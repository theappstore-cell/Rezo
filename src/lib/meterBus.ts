import { DB_MIN, linToDb } from './db';
import type { Meters } from './types';

interface Level {
  l: number;
  r: number;
  pl: number;
  pr: number;
  plAt: number;
  prAt: number;
}

const FALL_DB_PER_S = 36;
const HOLD_MS = 1200;
const HOLD_FALL_DB_PER_S = 18;

const levels = new Map<string, Level>();
const incoming = new Map<string, [number, number]>();
const subscribers = new Set<() => void>();
let raf = 0;
let last = 0;

export function pushMeters(m: Meters) {
  const put = (id: string, l: number, r: number) => {
    const cur = incoming.get(id);
    incoming.set(id, cur ? [Math.max(cur[0], l), Math.max(cur[1], r)] : [l, r]);
  };
  put('master', m.m[0], m.m[1]);
  for (const id in m.c) put(id, m.c[id][0], m.c[id][1]);
}

function ensure(id: string): Level {
  let lv = levels.get(id);
  if (!lv) {
    lv = { l: DB_MIN, r: DB_MIN, pl: DB_MIN, pr: DB_MIN, plAt: 0, prAt: 0 };
    levels.set(id, lv);
  }
  return lv;
}

function tick(now: number) {
  const dt = Math.min(0.1, (now - last) / 1000 || 0.016);
  last = now;
  for (const id of new Set([...levels.keys(), ...incoming.keys()])) {
    const lv = ensure(id);
    const inc = incoming.get(id);
    const inL = inc ? linToDb(inc[0]) : DB_MIN;
    const inR = inc ? linToDb(inc[1]) : DB_MIN;
    lv.l = Math.max(inL, lv.l - FALL_DB_PER_S * dt);
    lv.r = Math.max(inR, lv.r - FALL_DB_PER_S * dt);
    if (lv.l >= lv.pl) { lv.pl = lv.l; lv.plAt = now; }
    else if (now - lv.plAt > HOLD_MS) lv.pl = Math.max(lv.l, lv.pl - HOLD_FALL_DB_PER_S * dt);
    if (lv.r >= lv.pr) { lv.pr = lv.r; lv.prAt = now; }
    else if (now - lv.prAt > HOLD_MS) lv.pr = Math.max(lv.r, lv.pr - HOLD_FALL_DB_PER_S * dt);
  }
  incoming.clear();
  subscribers.forEach((draw) => draw());
  raf = subscribers.size ? requestAnimationFrame(tick) : 0;
}

export function subscribeMeters(draw: () => void): () => void {
  subscribers.add(draw);
  if (!raf) {
    last = performance.now();
    raf = requestAnimationFrame(tick);
  }
  return () => {
    subscribers.delete(draw);
  };
}

export interface ReadLevel {
  l: number;
  r: number;
  pl: number;
  pr: number;
}

const EMPTY: ReadLevel = { l: DB_MIN, r: DB_MIN, pl: DB_MIN, pr: DB_MIN };

export function readMeter(source: string | readonly string[]): ReadLevel {
  if (typeof source === 'string') return levels.get(source) ?? EMPTY;
  let out = EMPTY;
  for (const id of source) {
    const lv = levels.get(id);
    if (!lv) continue;
    out = out === EMPTY
      ? { l: lv.l, r: lv.r, pl: lv.pl, pr: lv.pr }
      : { l: Math.max(out.l, lv.l), r: Math.max(out.r, lv.r), pl: Math.max(out.pl, lv.pl), pr: Math.max(out.pr, lv.pr) };
  }
  return out;
}
