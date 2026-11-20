import { useRef } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';
import { clamp } from '../lib/db';
import { useMidiControl } from '../hooks/useMidiLearn';

interface Props {
  value: number;
  onChange: (v01: number) => void;
  color?: string;
  label: string;
  step?: number;
  onReset?: () => void;
  mark?: number;
  controlId?: string;
  disabled?: boolean;
}

export function Slider({ value, onChange, color = 'var(--c-ink)', label, step = 0.02, onReset, mark, controlId, disabled }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const down = useRef(false);
  const { learnProps } = useMidiControl(controlId, (v) => onChange(v));

  const setFrom = (clientX: number) => {
    const r = ref.current!.getBoundingClientRect();
    onChange(clamp((clientX - r.left) / r.width, 0, 1));
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') onChange(clamp(value + step, 0, 1));
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') onChange(clamp(value - step, 0, 1));
    else if (e.key === 'Home') onReset?.();
    else return;
    e.preventDefault();
  };

  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={disabled ? -1 : 0}
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value * 100)}
      className="relative h-6 flex-1 touch-none cursor-pointer"
      style={{ opacity: disabled ? 0.45 : 1 }}
      onPointerDown={(e: PointerEvent) => {
        if (disabled || e.button !== 0) return;
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        down.current = true;
        setFrom(e.clientX);
      }}
      onPointerMove={(e) => down.current && setFrom(e.clientX)}
      onPointerUp={() => (down.current = false)}
      onPointerCancel={() => (down.current = false)}
      onDoubleClick={onReset}
      onKeyDown={onKey}
      {...learnProps}
    >
      <div className="absolute inset-x-0 top-1/2 h-[4px] -translate-y-1/2 rounded-full" style={{ background: 'var(--c-meter-off)' }} />
      <div className="absolute left-0 top-1/2 h-[4px] -translate-y-1/2 rounded-full" style={{ width: `${value * 100}%`, background: color }} />
      {mark !== undefined && <div className="absolute top-[14px] h-[4px] w-px" style={{ left: `${mark * 100}%`, background: 'var(--c-border-strong)' }} />}
      <div
        className="absolute top-1/2 h-[16px] w-[8px] -translate-x-1/2 -translate-y-1/2 rounded-[3px]"
        style={{ left: `${value * 100}%`, background: 'var(--c-surface)', border: '1px solid var(--c-border-strong)', boxShadow: '0 1px 2px rgba(0,0,0,0.2)' }}
      />
    </div>
  );
}
