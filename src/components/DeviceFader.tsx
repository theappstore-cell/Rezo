import { useEffect, useRef } from 'react';
import type { PointerEvent } from 'react';
import { clamp } from '../lib/db';
import { useMidiControl } from '../hooks/useMidiLearn';

interface Props {
  value: number;
  onChange: (v: number) => void;
  height: number;
  color?: string;
  label: string;
  controlId?: string;
}

const CAP_H = 14;

// Volume of the speaker itself (0-100), as set in the Sonos app or on the receiver; independent of the mix.
export function DeviceFader({ value, onChange, height, color = 'var(--c-ink)', label, controlId }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const valueRef = useRef(value);
  valueRef.current = value;
  const dragging = useRef(false);
  const { learnProps } = useMidiControl(controlId, (v01) => commit(v01 * 100));

  const commit = (v: number) => {
    const nv = Math.round(clamp(v, 0, 100));
    if (nv !== valueRef.current) onChange(nv);
  };
  const travel = height - CAP_H;
  const pos = clamp(value, 0, 100) / 100;

  const at = (clientY: number) => {
    const r = ref.current!.getBoundingClientRect();
    return (1 - (clientY - r.top - CAP_H / 2) / (r.height - CAP_H)) * 100;
  };

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const h = (e: WheelEvent) => {
      e.preventDefault();
      commit(valueRef.current + (e.deltaY < 0 ? 1 : -1) * (e.shiftKey ? 1 : 2));
    };
    el.addEventListener('wheel', h, { passive: false });
    return () => el.removeEventListener('wheel', h);
  }, [onChange]);

  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={0}
      aria-label={label}
      title={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value}
      className="relative w-6 touch-none cursor-ns-resize"
      style={{ height }}
      onPointerDown={(e: PointerEvent<HTMLDivElement>) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        dragging.current = true;
        commit(at(e.clientY));
      }}
      onPointerMove={(e) => dragging.current && commit(at(e.clientY))}
      onPointerUp={() => (dragging.current = false)}
      onPointerCancel={() => (dragging.current = false)}
      onLostPointerCapture={() => (dragging.current = false)}
      onKeyDown={(e) => {
        if (e.key === 'ArrowUp') commit(value + 1);
        else if (e.key === 'ArrowDown') commit(value - 1);
        else if (e.key === 'PageUp') commit(value + 10);
        else if (e.key === 'PageDown') commit(value - 10);
        else return;
        e.preventDefault();
      }}
      {...learnProps}
    >
      <div className="absolute left-1/2 w-[4px] -translate-x-1/2 rounded-full" style={{ top: CAP_H / 2, bottom: CAP_H / 2, background: 'var(--c-meter-off)' }} />
      <div className="absolute left-1/2 w-[4px] -translate-x-1/2 rounded-full" style={{ bottom: CAP_H / 2, height: pos * travel, background: color, opacity: 0.55 }} />
      <div
        className="absolute left-0 right-0 rounded-[3px]"
        style={{ top: (1 - pos) * travel, height: CAP_H, background: 'var(--c-surface)', border: '1px solid var(--c-border-strong)', boxShadow: '0 1px 2px rgba(0,0,0,0.18)' }}
      >
        <div className="absolute inset-x-1 top-1/2 h-[2px] -translate-y-1/2" style={{ background: color }} />
      </div>
    </div>
  );
}
