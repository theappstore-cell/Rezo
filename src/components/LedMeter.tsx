import { useEffect, useRef, useState } from 'react';
import { readMeter, subscribeMeters } from '../lib/meterBus';

interface Props {
  source: string | readonly string[];
  width?: number;
  height: number;
  orientation?: 'vertical' | 'horizontal';
  floor?: number;
  ceil?: number;
}

const GREEN = '#3d9a5b';
const AMBER = '#d1a017';
const RED = '#d6453d';

export function useSchemeTick() {
  const [n, setN] = useState(0);
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const h = () => setN((x) => x + 1);
    mq.addEventListener('change', h);
    return () => mq.removeEventListener('change', h);
  }, []);
  return n;
}

export function LedMeter({ source, width, height, orientation = 'vertical', floor = -60, ceil = 3 }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const ref = useRef<HTMLCanvasElement>(null);
  const key = typeof source === 'string' ? source : source.join('|');
  const scheme = useSchemeTick();

  useEffect(() => {
    const cv = ref.current;
    const box = wrap.current;
    if (!cv || !box) return;
    const ctx = cv.getContext('2d');
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const vertical = orientation === 'vertical';
    const src = key.includes('|') ? key.split('|') : key;
    const css = getComputedStyle(cv);
    const off = css.getPropertyValue('--c-meter-off').trim() || '#ddd';
    const ink = css.getPropertyValue('--c-ink').trim() || '#000';
    let w = width ?? Math.max(10, Math.floor(box.clientWidth));
    const h = height;

    const setSize = () => {
      if (width === undefined) w = Math.max(10, Math.floor(box.clientWidth));
      cv.width = Math.round(w * dpr);
      cv.height = Math.round(h * dpr);
      cv.style.width = `${w}px`;
      cv.style.height = `${h}px`;
    };
    setSize();
    const ro = width === undefined ? new ResizeObserver(setSize) : null;
    ro?.observe(box);

    const frac = (db: number) => Math.min(1, Math.max(0, (db - floor) / (ceil - floor)));
    const gate12 = frac(-12);
    const gate3 = frac(-3);

    const draw = () => {
      const lv = readMeter(src);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      const span = vertical ? h : w;
      const lane = ((vertical ? w : h) - 1) / 2;
      ([[lv.l, lv.pl], [lv.r, lv.pr]] as const).forEach(([db, hold], ch) => {
        const o = ch * (lane + 1);
        const len = frac(db) * span;
        const rect = (from: number, to: number, color: string) => {
          if (to <= from) return;
          ctx.fillStyle = color;
          if (vertical) ctx.fillRect(o, h - to, lane, to - from);
          else ctx.fillRect(from, o, to - from, lane);
        };
        rect(0, span, off);
        rect(0, Math.min(len, gate12 * span), GREEN);
        rect(gate12 * span, Math.min(len, gate3 * span), AMBER);
        rect(gate3 * span, len, RED);
        if (hold > floor + 1) {
          const p = frac(hold) * span;
          ctx.fillStyle = ink;
          if (vertical) ctx.fillRect(o, h - p - 1, lane, 1.5);
          else ctx.fillRect(p - 1, o, 1.5, lane);
        }
      });
    };
    draw();
    const unsub = subscribeMeters(draw);
    return () => {
      unsub();
      ro?.disconnect();
    };
  }, [key, width, height, orientation, floor, ceil, scheme]);

  return (
    <div ref={wrap} style={width === undefined ? { width: '100%', height } : { width, height }} aria-hidden>
      <canvas ref={ref} />
    </div>
  );
}
