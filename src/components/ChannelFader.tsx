import { useEffect, useRef } from 'react';
import type { PointerEvent } from 'react';
import { DB_MIN, FADER_TICKS, clamp, dbToPos, fmtDb, posToDb, roundTo } from '../lib/db';
import { useMidiControl } from '../hooks/useMidiLearn';
import { LedMeter } from './LedMeter';

interface Props {
  valueDb: number;
  onChange: (db: number) => void;
  color?: string;
  height?: number;
  meter: string | readonly string[];
  controlId?: string;
  dimmed?: boolean;
}

const CAP_H = 18;

export function ChannelFader({ valueDb, onChange, color = 'var(--c-ink)', height = 200, meter, controlId, dimmed }: Props) {
  const trackRef = useRef<HTMLDivElement>(null);
  const valueRef = useRef(valueDb);
  valueRef.current = valueDb;
  const drag = useRef<{ y: number; db: number; shift: boolean; grab: number } | null>(null);

  const commit = (db: number, fine = false) => {
    const nv = db <= DB_MIN + 0.3 ? DB_MIN : clamp(roundTo(db, fine ? 0.1 : 0.5), DB_MIN, 6);
    if (nv !== valueRef.current) onChange(nv);
  };
  const { learnProps } = useMidiControl(controlId, (v01) => commit(posToDb(v01)));

  const travel = height - CAP_H;
  const pos = dbToPos(valueDb);

  const dbAt = (clientY: number, grab: number) => {
    const r = trackRef.current!.getBoundingClientRect();
    const p = 1 - (clientY - grab - r.top - CAP_H / 2) / (r.height - CAP_H);
    return posToDb(clamp(p, 0, 1));
  };

  useEffect(() => {
    const el = trackRef.current;
    if (!el) return;
    const h = (e: WheelEvent) => {
      e.preventDefault();
      const dir = e.deltaY < 0 ? 1 : -1;
      const cur = valueRef.current <= DB_MIN ? -60 : valueRef.current;
      commit(cur + dir * (e.shiftKey ? 0.1 : 0.5), e.shiftKey);
    };
    el.addEventListener('wheel', h, { passive: false });
    return () => el.removeEventListener('wheel', h);
  }, [onChange]);

  const onDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const capCenter = trackRef.current!.getBoundingClientRect().top + CAP_H / 2 + (1 - pos) * travel;
    const onCap = Math.abs(e.clientY - capCenter) <= CAP_H / 2 + 2;
    const grab = onCap ? e.clientY - capCenter : 0;
    drag.current = { y: e.clientY, db: valueRef.current, shift: e.shiftKey, grab };
    if (!onCap) commit(dbAt(e.clientY, 0));
  };
  const onMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    if (e.shiftKey !== d.shift) {
      d.shift = e.shiftKey;
      d.y = e.clientY;
      d.db = valueRef.current;
    }
    if (d.shift) commit(d.db + (d.y - e.clientY) * 0.05, true);
    else commit(dbAt(e.clientY, d.grab));
  };
  const onUp = () => {
    drag.current = null;
  };

  return (
    <div className="flex items-stretch gap-1.5" style={{ height }} {...learnProps}>
      <div className="relative w-7" aria-hidden>
        {FADER_TICKS.map((t) => (
          <span key={t.label} className="num absolute right-0 text-[10px] leading-none text-dim" style={{ top: CAP_H / 2 + (1 - dbToPos(t.db)) * travel - 5, fontWeight: t.db === 0 ? 700 : 400, color: t.db === 0 ? 'var(--c-ink)' : undefined }}>
            {t.label}
          </span>
        ))}
      </div>

      <div
        ref={trackRef}
        role="slider"
        tabIndex={0}
        aria-label="Volume"
        aria-valuemin={DB_MIN}
        aria-valuemax={6}
        aria-valuenow={valueDb}
        aria-valuetext={`${fmtDb(valueDb)} dB`}
        className="relative w-8 touch-none cursor-ns-resize"
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={onUp}
        onLostPointerCapture={onUp}
        onDoubleClick={() => onChange(0)}
        onKeyDown={(e) => {
          const s = e.shiftKey ? 0.1 : 0.5;
          if (e.key === 'ArrowUp') commit((valueDb <= DB_MIN ? -60 : valueDb) + s, e.shiftKey);
          else if (e.key === 'ArrowDown') commit(valueDb - s, e.shiftKey);
          else if (e.key === 'Home') onChange(0);
          else return;
          e.preventDefault();
        }}
      >
        <div className="absolute left-1/2 w-[4px] -translate-x-1/2 rounded-full" style={{ top: CAP_H / 2, bottom: CAP_H / 2, background: 'var(--c-meter-off)' }} />
        <div className="absolute left-1 right-1 h-px" style={{ top: CAP_H / 2 + (1 - dbToPos(0)) * travel, background: 'var(--c-border-strong)' }} />
        <div
          className="absolute left-0 right-0 rounded-[3px]"
          style={{
            top: (1 - pos) * travel,
            height: CAP_H,
            opacity: dimmed ? 0.5 : 1,
            background: 'var(--c-surface)',
            border: '1px solid var(--c-border-strong)',
            boxShadow: '0 1px 2px rgba(0,0,0,0.18)',
          }}
        >
          <div className="absolute inset-x-1.5 top-1/2 h-[2px] -translate-y-1/2" style={{ background: color }} />
        </div>
      </div>

      <LedMeter source={meter} width={13} height={height - CAP_H + 8} />
    </div>
  );
}
