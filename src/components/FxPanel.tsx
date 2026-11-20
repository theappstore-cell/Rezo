import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { fmtDb } from '../lib/db';
import { useStore } from '../lib/store';
import { t } from '../lib/i18n';
import type { Speaker } from '../lib/types';
import { Pad } from './Pad';
import { RotaryKnob } from './RotaryKnob';

const W = 236;

export function FxButton({ sp, color }: { sp: Speaker; color: string }) {
  const setParam = useStore((s) => s.setParam);
  const p = sp.params;
  const id = `ch:${sp.id}`;
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number }>({ left: 0, top: 0 });
  const active = p.bassDb !== 0 || p.presenceDb !== 0 || p.loudness !== 0 || p.width !== 1 || p.compOn || p.gateOn;

  useLayoutEffect(() => {
    if (!open || !btn.current || !panel.current) return;
    const r = btn.current.getBoundingClientRect();
    const h = panel.current.offsetHeight;
    const left = Math.max(8, Math.min(r.left, window.innerWidth - W - 8));
    const top = Math.max(8, Math.min(r.bottom + 4, window.innerHeight - h - 8));
    setPos({ left, top });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const down = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!panel.current?.contains(t) && !btn.current?.contains(t)) setOpen(false);
    };
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    window.addEventListener('pointerdown', down);
    window.addEventListener('keydown', key);
    return () => {
      window.removeEventListener('pointerdown', down);
      window.removeEventListener('keydown', key);
    };
  }, [open]);

  const knob = (label: string, key: keyof typeof p, min: number, max: number, def: number, step: number, fine: number, format: (v: number) => string, bipolar = false) => (
    <RotaryKnob label={t(label)} value={p[key] as number} min={min} max={max} defaultValue={def} step={step} fineStep={fine} bipolar={bipolar}
      color={color} format={format} onChange={(v) => setParam(sp.id, key, v as never)} controlId={`${id}:${key}`} size={40} />
  );
  const pct = (v: number) => `${Math.round(v * 100)} %`;

  return (
    <>
      <button ref={btn} type="button" className="key !h-[24px] !px-1 !text-[11.5px]" data-on={active} aria-expanded={open} aria-haspopup="dialog"
        style={{ ['--c' as string]: color }} title={t('Bass, présence, loudness, largeur, compresseur, noise gate')}
        onClick={() => setOpen((v) => !v)}>
        {t('Effets')} {open ? '▴' : '▾'}
      </button>
      {open &&
        createPortal(
          <div ref={panel} role="dialog" aria-label={t('Effets de {name}', { name: sp.name })}
            className="toast-in fixed z-50 flex flex-col gap-2.5 rounded-md border border-rule bg-panel p-3"
            style={{ left: pos.left, top: pos.top, width: W, boxShadow: '0 8px 24px rgba(0,0,0,0.3)' }}>
            <div className="flex items-baseline justify-between gap-2">
              <span className="truncate text-[13px] font-semibold">{t('Effets')} · {sp.name}</span>
              <button type="button" className="key !h-6 shrink-0" onClick={() => setOpen(false)}>{t('Fermer')}</button>
            </div>

            <div>
              <div className="legend mb-1">{t('Tonalité')}</div>
              <div className="grid grid-cols-4 gap-x-1">
                {knob('Bass', 'bassDb', 0, 12, 0, 0.5, 0.1, (v) => fmtDb(v))}
                {knob('Prés.', 'presenceDb', -6, 6, 0, 0.5, 0.1, (v) => fmtDb(v), true)}
                {knob('Loud', 'loudness', 0, 1, 0, 0.05, 0.01, pct)}
                {knob('Larg.', 'width', 0, 2, 1, 0.05, 0.01, pct)}
              </div>
            </div>

            <div className="border-t border-line pt-2">
              <Pad label={t('Compresseur')} on={p.compOn} onToggle={() => setParam(sp.id, 'compOn', !p.compOn)} controlId={`${id}:compOn`}
                title={t('Rend le volume plus homogène : les passages forts sont rabaissés')} className="w-full" />
              <div className="mt-2 grid grid-cols-3 gap-x-1" style={{ opacity: p.compOn ? 1 : 0.5 }}>
                {knob('Seuil', 'compThreshDb', -40, 0, -18, 1, 0.5, (v) => fmtDb(v))}
                {knob('Ratio', 'compRatio', 1, 10, 3, 0.5, 0.1, (v) => `${v.toFixed(1)}:1`)}
                {knob('Gain+', 'compMakeupDb', 0, 12, 0, 0.5, 0.1, (v) => fmtDb(v))}
              </div>
            </div>

            <div className="border-t border-line pt-2">
              <Pad label="Noise gate" on={p.gateOn} onToggle={() => setParam(sp.id, 'gateOn', !p.gateOn)} controlId={`${id}:gateOn`}
                title={t('Coupe le son sous le seuil (souffle, bruit de fond)')} className="w-full" />
              <div className="mt-2 grid grid-cols-3 gap-x-1" style={{ opacity: p.gateOn ? 1 : 0.5 }}>
                {knob('Seuil', 'gateThreshDb', -80, -20, -55, 1, 0.5, (v) => fmtDb(v))}
              </div>
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}
