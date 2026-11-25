import { useRef, useState } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';
import { useStore } from '../lib/store';
import { useUi } from '../lib/ui';
import { t } from '../lib/i18n';
import { CUSTOM_ID, EQ_BANDS, EQ_MAX_DB, EQ_PRESETS, FLAT, clampGain, formatGain, presetById } from '../lib/eq';
import type { EqGains } from '../lib/eq';
import type { Speaker } from '../lib/types';
import { Modal, Toggle } from './Modal';

// Graph geometry, in SVG units (the drawing scales with the dialog width).
const W = 560;
const H = 230;
const X0 = 46;
const X1 = W - 14;
const Y0 = 14;
const Y1 = H - 30;
const ACCENT = '#1DB954';

const xOf = (i: number) => X0 + ((i + 0.5) * (X1 - X0)) / EQ_BANDS.length;
const yOf = (db: number) => Y0 + ((EQ_MAX_DB - db) / (2 * EQ_MAX_DB)) * (Y1 - Y0);
const dbOf = (y: number) => EQ_MAX_DB - ((y - Y0) / (Y1 - Y0)) * 2 * EQ_MAX_DB;

// Catmull-Rom spline through the six points, held flat out to both edges, written as cubic Béziers.
function curvePath(gains: readonly number[]): string {
  const pts: [number, number][] = [[X0, yOf(gains[0])], ...gains.map((g, i): [number, number] => [xOf(i), yOf(g)]), [X1, yOf(gains[gains.length - 1])]];
  const clampY = (y: number) => Math.min(Y1, Math.max(Y0, y));
  let d = `M ${pts[0][0]} ${pts[0][1].toFixed(1)}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[Math.min(pts.length - 1, i + 2)];
    const c1 = [p1[0] + (p2[0] - p0[0]) / 6, clampY(p1[1] + (p2[1] - p0[1]) / 6)];
    const c2 = [p2[0] - (p3[0] - p1[0]) / 6, clampY(p2[1] - (p3[1] - p1[1]) / 6)];
    d += ` C ${c1[0].toFixed(1)} ${c1[1].toFixed(1)}, ${c2[0].toFixed(1)} ${c2[1].toFixed(1)}, ${p2[0].toFixed(1)} ${p2[1].toFixed(1)}`;
  }
  return d;
}

interface GraphProps {
  gains: EqGains;
  disabled: boolean;
  onChange: (g: EqGains) => void;
}

function EqGraph({ gains, disabled, onChange }: GraphProps) {
  const svg = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState<{ band: number; offset: number } | null>(null);

  const svgY = (clientY: number) => {
    const r = svg.current!.getBoundingClientRect();
    return ((clientY - r.top) / r.height) * H;
  };
  const setBand = (band: number, db: number) => {
    const v = clampGain(db);
    if (v === gains[band]) return;
    const next = [...gains] as EqGains;
    next[band] = v;
    onChange(next);
  };
  const grab = (e: PointerEvent<SVGCircleElement>, band: number) => {
    if (e.button !== 0) return;
    e.preventDefault();
    svg.current!.setPointerCapture(e.pointerId);
    setDrag({ band, offset: svgY(e.clientY) - yOf(gains[band]) });
  };
  const key = (e: KeyboardEvent<SVGCircleElement>, band: number) => {
    const step = e.shiftKey ? 0.1 : 0.5;
    if (e.key === 'ArrowUp') setBand(band, gains[band] + step);
    else if (e.key === 'ArrowDown') setBand(band, gains[band] - step);
    else if (e.key === 'Home') setBand(band, 0);
    else return;
    e.preventDefault();
  };
  const end = () => setDrag(null);

  const curve = curvePath(gains);
  const tip = drag && { x: xOf(drag.band), y: yOf(gains[drag.band]), text: formatGain(gains[drag.band]) };

  return (
    <svg
      ref={svg}
      viewBox={`0 0 ${W} ${H}`}
      className="block w-full touch-none select-none"
      style={{ cursor: drag ? 'ns-resize' : undefined, opacity: disabled ? 0.4 : 1, pointerEvents: disabled ? 'none' : undefined }}
      onPointerMove={(e) => drag && setBand(drag.band, dbOf(svgY(e.clientY) - drag.offset))}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
      role="group"
      aria-label={t('Égaliseur')}
      aria-disabled={disabled}
    >
      <defs>
        <linearGradient id="rezo-eq-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={ACCENT} stopOpacity="0.38" />
          <stop offset="100%" stopColor={ACCENT} stopOpacity="0" />
        </linearGradient>
      </defs>
      <rect x="0" y="0" width={W} height={H} rx="8" fill="#181818" />
      {[EQ_MAX_DB, 0, -EQ_MAX_DB].map((db) => (
        <g key={db}>
          <line x1={X0} x2={X1} y1={yOf(db)} y2={yOf(db)} stroke="#ffffff" strokeOpacity={db === 0 ? 0.2 : 0.08} strokeDasharray={db === 0 ? '3 5' : undefined} />
          <text x={X0 - 8} y={yOf(db) + 4} textAnchor="end" fontSize="11" fill="#b3b3b3">{db > 0 ? `+${db}dB` : `${db}dB`}</text>
        </g>
      ))}
      {EQ_BANDS.map((b, i) => (
        <g key={b.hz}>
          <line x1={xOf(i)} x2={xOf(i)} y1={Y0} y2={Y1} stroke="#ffffff" strokeOpacity="0.06" />
          <text x={xOf(i)} y={H - 10} textAnchor="middle" fontSize="11" fill="#b3b3b3">{b.label}</text>
        </g>
      ))}
      <path d={`${curve} L ${X1} ${Y1} L ${X0} ${Y1} Z`} fill="url(#rezo-eq-fill)" />
      <path d={curve} fill="none" stroke={ACCENT} strokeWidth="2.5" strokeLinecap="round" />
      {gains.map((g, i) => (
        <g key={EQ_BANDS[i].hz}>
          <circle cx={xOf(i)} cy={yOf(g)} r="7" fill="#ffffff" stroke="#000000" strokeOpacity="0.25" pointerEvents="none" />
          <circle
            cx={xOf(i)}
            cy={yOf(g)}
            r="16"
            fill="transparent"
            style={{ cursor: 'ns-resize', outline: 'none' }}
            tabIndex={disabled ? -1 : 0}
            role="slider"
            aria-label={EQ_BANDS[i].label}
            aria-valuemin={-EQ_MAX_DB}
            aria-valuemax={EQ_MAX_DB}
            aria-valuenow={g}
            aria-valuetext={formatGain(g)}
            onPointerDown={(e) => grab(e, i)}
            onKeyDown={(e) => key(e, i)}
          />
        </g>
      ))}
      {tip && (
        <g pointerEvents="none">
          <rect x={tip.x - 32} y={Math.max(2, tip.y - 34)} width="64" height="20" rx="4" fill="#ffffff" />
          <text x={tip.x} y={Math.max(2, tip.y - 34) + 14} textAnchor="middle" fontSize="11.5" fontWeight="600" fill="#121212">{tip.text}</text>
        </g>
      )}
    </svg>
  );
}

// Per-speaker six-band equaliser, in the spirit of Spotify's: toggle, presets, draggable curve, reset.
export function EqualizerModal({ id }: { id: string }) {
  const close = useUi((u) => u.close);
  const sp = useStore((s) => s.s?.speakers.find((x) => x.id === id));
  const setParam = useStore((s) => s.setParam);
  if (!sp) return null;

  const p = sp.params;
  const gains = (Array.isArray(p.eqGains) && p.eqGains.length === 6 ? p.eqGains : FLAT) as EqGains;
  const presetId = p.eqPreset || 'flat';
  const enabled = p.eqEnabled ?? true;

  const edit = (g: EqGains) => {
    setParam(sp.id, 'eqGains', g);
    if (presetId !== CUSTOM_ID) setParam(sp.id, 'eqPreset', CUSTOM_ID);
  };
  const choose = (pid: string) => {
    const preset = presetById(pid);
    if (!preset) return;
    setParam(sp.id, 'eqGains', [...preset.gains]);
    setParam(sp.id, 'eqPreset', preset.id);
  };

  return (
    <Modal title={sp.name} onClose={close} width={620}>
      <div className="space-y-3">
        <div className="flex items-center justify-between gap-3">
          <h3 className="text-[16px] font-bold">{t('Égaliseur')}</h3>
          <Toggle on={enabled} onChange={(v) => setParam(sp.id, 'eqEnabled', v)} label={t("Activer l'égaliseur")} color={ACCENT} />
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <label htmlFor="rezo-eq-preset" className="text-[12.5px] text-mute">{t('Présélections')}</label>
          <select id="rezo-eq-preset" value={presetId} disabled={!enabled} onChange={(e) => choose(e.target.value)}
            className="min-w-0 rounded-[4px] border border-line bg-inset px-2 py-1.5 text-[12.5px] disabled:opacity-50">
            {EQ_PRESETS.map((pr) => <option key={pr.id} value={pr.id}>{t(pr.label)}</option>)}
            {presetId === CUSTOM_ID && <option value={CUSTOM_ID}>{t('Personnalisé')}</option>}
          </select>
        </div>
        <EqGraph gains={gains} disabled={!enabled} onChange={edit} />
        <div className="flex justify-end">
          <button type="button" onClick={() => choose('flat')} className="rounded-full border border-rule px-4 py-1.5 text-[12.5px] font-semibold transition hover:bg-raise">
            {t('Réinitialiser')}
          </button>
        </div>
      </div>
    </Modal>
  );
}

export function eqActive(sp: Speaker): boolean {
  const g = sp.params.eqGains;
  return (sp.params.eqEnabled ?? true) && Array.isArray(g) && g.some((x) => x !== 0);
}
