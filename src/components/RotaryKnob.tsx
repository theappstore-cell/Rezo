import { useCallback, useEffect, useRef } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';
import { clamp, roundTo } from '../lib/db';
import { useMidiControl } from '../hooks/useMidiLearn';

const START = -135;
const SWEEP = 270;

const polar = (r: number, deg: number): [number, number] => {
  const a = ((deg - 90) * Math.PI) / 180;
  return [50 + r * Math.cos(a), 50 + r * Math.sin(a)];
};

function arc(r: number, a0: number, a1: number): string {
  const [x0, y0] = polar(r, a0);
  const [x1, y1] = polar(r, a1);
  return `M${x0.toFixed(2)} ${y0.toFixed(2)} A${r} ${r} 0 ${Math.abs(a1 - a0) > 180 ? 1 : 0} ${a1 > a0 ? 1 : 0} ${x1.toFixed(2)} ${y1.toFixed(2)}`;
}

export interface RotaryKnobProps {
  label: string;
  value: number;
  min: number;
  max: number;
  defaultValue: number;
  step: number;
  fineStep: number;
  bipolar?: boolean;
  color?: string;
  size?: number;
  format: (v: number) => string;
  onChange: (v: number) => void;
  controlId?: string;
  disabled?: boolean;
}

export function RotaryKnob(p: RotaryKnobProps) {
  const { value, min, max, step, fineStep, onChange } = p;
  const color = p.color ?? 'var(--c-ink)';
  const size = p.size ?? 44;
  const valueRef = useRef(value);
  valueRef.current = value;
  const rootRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; v: number; shift: boolean } | null>(null);

  const commit = useCallback(
    (v: number, fine: boolean) => {
      const s = fine ? fineStep : step;
      const nv = Number(clamp(roundTo(v, s), min, max).toFixed(3));
      if (nv !== valueRef.current) onChange(nv);
    },
    [min, max, step, fineStep, onChange],
  );

  const { learnProps } = useMidiControl(p.controlId, (v01) => commit(min + v01 * (max - min), false));

  useEffect(() => {
    const el = rootRef.current;
    if (!el || p.disabled) return;
    const h = (e: WheelEvent) => {
      e.preventDefault();
      const dir = e.deltaY < 0 ? 1 : -1;
      const s = e.shiftKey ? fineStep : step;
      commit(valueRef.current + dir * s, e.shiftKey);
    };
    el.addEventListener('wheel', h, { passive: false });
    return () => el.removeEventListener('wheel', h);
  }, [commit, step, fineStep, p.disabled]);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || p.disabled) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { x: e.clientX, y: e.clientY, v: valueRef.current, shift: e.shiftKey };
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    if (e.shiftKey !== d.shift) {
      d.shift = e.shiftKey;
      d.x = e.clientX;
      d.y = e.clientY;
      d.v = valueRef.current;
    }
    const delta = d.y - e.clientY + (e.clientX - d.x);
    const v = d.shift ? d.v + (delta / 8) * fineStep : d.v + (delta / 200) * (max - min);
    commit(v, d.shift);
  };
  const onPointerUp = () => {
    drag.current = null;
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const s = e.shiftKey ? fineStep : step;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') commit(value + s, e.shiftKey);
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') commit(value - s, e.shiftKey);
    else if (e.key === 'Home' || e.key === '0') onChange(p.defaultValue);
    else return;
    e.preventDefault();
  };

  const norm = (value - min) / (max - min);
  const angle = START + clamp(norm, 0, 1) * SWEEP;
  const from = p.bipolar ? 0 : START;
  const [px0, py0] = polar(6, angle);
  const [px1, py1] = polar(26, angle);

  return (
    <div className="flex flex-col items-center gap-0.5" {...learnProps}>
      <span className="legend">{p.label}</span>
      <div
        ref={rootRef}
        role="slider"
        tabIndex={p.disabled ? -1 : 0}
        aria-label={p.label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={p.format(value)}
        className="relative touch-none rounded-full"
        style={{ width: size, height: size, cursor: p.disabled ? 'not-allowed' : 'ns-resize', opacity: p.disabled ? 0.4 : 1 }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onLostPointerCapture={onPointerUp}
        onDoubleClick={() => !p.disabled && onChange(p.defaultValue)}
        onKeyDown={onKeyDown}
      >
        <svg viewBox="0 0 100 100" width={size} height={size} aria-hidden>
          <path d={arc(44, START, START + SWEEP)} fill="none" strokeWidth="5" strokeLinecap="round" style={{ stroke: 'var(--c-meter-off)' }} />
          {Math.abs(angle - from) > 0.5 && (
            <path d={arc(44, Math.min(from, angle), Math.max(from, angle))} fill="none" strokeWidth="5" strokeLinecap="round" style={{ stroke: color }} />
          )}
          <circle cx="50" cy="50" r="32" strokeWidth="2" style={{ fill: 'var(--c-surface2)', stroke: 'var(--c-border-strong)' }} />
          <line x1={px0} y1={py0} x2={px1} y2={py1} strokeWidth="5" strokeLinecap="round" style={{ stroke: 'var(--c-ink)' }} />
        </svg>
      </div>
      <div className="num text-[11.5px] text-mute">{p.format(value)}</div>
    </div>
  );
}
