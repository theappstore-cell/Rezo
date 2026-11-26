import { useEffect, useRef, useState } from 'react';
import type { PointerEvent } from 'react';
import { ImagePlus, Trash2 } from 'lucide-react';
import { api } from '../lib/api';
import { speakerIcon } from '../lib/icons';
import { readMeter, subscribeMeters } from '../lib/meterBus';
import { colorOf, useStore } from '../lib/store';
import { t } from '../lib/i18n';
import type { Speaker } from '../lib/types';
import { onSpeakerMenu, removeFromPlanItem } from './ContextMenu';

const MAX_SIDE = 1600;
const DEFAULT_RATIO = 4 / 3;

// Reads a picture, scales it down to 1600 px on its longest side and returns it as a JPEG data URL.
async function shrink(file: File): Promise<string> {
  const url = URL.createObjectURL(file);
  try {
    const img = await new Promise<HTMLImageElement>((res, rej) => {
      const i = new Image();
      i.onload = () => res(i);
      i.onerror = () => rej(new Error(t('Image illisible.')));
      i.src = url;
    });
    const k = Math.min(1, MAX_SIDE / Math.max(img.naturalWidth, img.naturalHeight));
    const c = document.createElement('canvas');
    c.width = Math.max(1, Math.round(img.naturalWidth * k));
    c.height = Math.max(1, Math.round(img.naturalHeight * k));
    const ctx = c.getContext('2d')!;
    ctx.fillStyle = '#fff';
    ctx.fillRect(0, 0, c.width, c.height);
    ctx.drawImage(img, 0, 0, c.width, c.height);
    return c.toDataURL('image/jpeg', 0.85);
  } finally {
    URL.revokeObjectURL(url);
  }
}

// A glow around each speaker that follows its level, drawn without re-rendering React.
function Halo({ id, color }: { id: string; color: string }) {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => subscribeMeters(() => {
    const el = ref.current;
    if (!el) return;
    const lv = readMeter(id);
    const db = Math.max(lv.l, lv.r);
    const k = Math.max(0, Math.min(1, (db + 50) / 50));
    el.style.transform = `translate(-50%, -50%) scale(${1 + k * 1.6})`;
    el.style.opacity = String(k * 0.55);
  }), [id]);
  return <span ref={ref} className="pointer-events-none absolute left-1/2 top-1/2 h-10 w-10 rounded-full" style={{ background: color, opacity: 0, transform: 'translate(-50%, -50%)', filter: 'blur(6px)' }} />;
}

interface Drag {
  id: string;
  x: number;
  y: number;
  moved: boolean;
  fromTray: boolean;
}

// Free floor plan: an optional picture of the home with the speakers placed on it. Click a speaker to turn it on or
// off, drag it to move it, right-click for its menu.
export function FloorPlan() {
  const s = useStore((st) => st.s)!;
  const setSend = useStore((st) => st.setSend);
  const setPlanPosition = useStore((st) => st.setPlanPosition);
  const say = useStore((st) => st.say);
  const area = useRef<HTMLDivElement>(null);
  const file = useRef<HTMLInputElement>(null);
  const [image, setImage] = useState<string | null>(null);
  const [ratio, setRatio] = useState(DEFAULT_RATIO);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!s.hasPlanImage) return setImage(null);
    let alive = true;
    api.getPlanImage().then((d) => alive && setImage(d)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [s.hasPlanImage]);

  useEffect(() => {
    if (!image) return setRatio(DEFAULT_RATIO);
    const i = new Image();
    i.onload = () => setRatio(i.naturalWidth / Math.max(1, i.naturalHeight));
    i.src = image;
  }, [image]);

  const placed = s.speakers.filter((sp) => sp.planX != null && sp.planY != null);
  const tray = s.speakers.filter((sp) => sp.planX == null || sp.planY == null);

  const relative = (x: number, y: number) => {
    const r = area.current!.getBoundingClientRect();
    return { x: (x - r.left) / r.width, y: (y - r.top) / r.height, inside: x >= r.left && x <= r.right && y >= r.top && y <= r.bottom };
  };

  const down = (e: PointerEvent<HTMLElement>, sp: Speaker, fromTray: boolean) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag({ id: sp.id, x: e.clientX, y: e.clientY, moved: false, fromTray });
  };
  const move = (e: PointerEvent<HTMLElement>) => {
    setDrag((d) => (d ? { ...d, x: e.clientX, y: e.clientY, moved: d.moved || Math.hypot(e.clientX - d.x, e.clientY - d.y) > 4 } : d));
  };
  const up = (e: PointerEvent<HTMLElement>, sp: Speaker) => {
    const d = drag;
    setDrag(null);
    if (!d) return;
    if (!d.moved) {
      if (!d.fromTray) void setSend(sp.id, !sp.active);
      return;
    }
    const p = relative(e.clientX, e.clientY);
    if (p.inside) setPlanPosition(sp.id, { x: Math.min(1, Math.max(0, p.x)), y: Math.min(1, Math.max(0, p.y)) });
  };

  const pick = async (f: File | undefined) => {
    if (!f) return;
    setBusy(true);
    try {
      await api.setPlanImage(await shrink(f));
      useStore.getState().apply(await api.getState());
    } catch (err) {
      say(String(err));
    } finally {
      setBusy(false);
      if (file.current) file.current.value = '';
    }
  };

  const liveDrag = drag?.moved ? drag : null;
  const liveSp = liveDrag ? s.speakers.find((x) => x.id === liveDrag.id) : null;

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="legend min-w-0 flex-1">{t('Clic : allumer / éteindre · glisser : déplacer · clic droit : menu. Le halo suit le niveau de chaque enceinte.')}</p>
        <input ref={file} type="file" accept="image/png,image/jpeg,image/webp" className="hidden" onChange={(e) => void pick(e.target.files?.[0])} />
        <button type="button" className="key flex items-center gap-1.5 !h-7 !text-[12px]" disabled={busy} onClick={() => file.current?.click()}>
          <ImagePlus size={13} /> {image ? t("Changer l'image") : t('Importer une image du plan')}
        </button>
        {image && (
          <button type="button" className="key flex items-center gap-1.5 !h-7 !text-[12px]" onClick={async () => { await api.setPlanImage(null); useStore.getState().apply(await api.getState()); }}>
            <Trash2 size={13} /> {t("Retirer l'image")}
          </button>
        )}
      </div>

      <div
        ref={area}
        className="relative mx-auto overflow-hidden rounded-md border border-line bg-panel"
        style={{
          aspectRatio: String(ratio),
          width: `min(100%, ${(70 * ratio).toFixed(1)}vh)`,
          backgroundImage: image ? undefined : 'linear-gradient(var(--c-border) 1px, transparent 1px), linear-gradient(90deg, var(--c-border) 1px, transparent 1px)',
          backgroundSize: image ? undefined : '32px 32px',
        }}
      >
        {image && <img src={image} alt={t('Plan du logement')} className="pointer-events-none absolute inset-0 h-full w-full select-none object-fill" draggable={false} />}
        {!image && placed.length === 0 && (
          <div className="absolute inset-0 grid place-items-center p-6 text-center text-[13px] text-mute">
            {t('Importez une photo ou un plan du logement, puis glissez les enceintes dessus depuis la liste ci-dessous.')}
          </div>
        )}
        {placed.map((sp) => {
          const color = colorOf(s, sp);
          const Icon = speakerIcon(sp);
          const dragging = liveDrag?.id === sp.id;
          return (
            <button
              key={sp.id}
              type="button"
              data-speaker={sp.id}
              onPointerDown={(e) => down(e, sp, false)}
              onPointerMove={move}
              onPointerUp={(e) => up(e, sp)}
              onPointerCancel={() => setDrag(null)}
              onContextMenu={onSpeakerMenu(sp, ['sep', removeFromPlanItem(sp)])}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  void setSend(sp.id, !sp.active);
                }
              }}
              title={`${sp.name} — ${sp.active ? t('cliquez pour éteindre') : t('cliquez pour allumer')}`}
              aria-pressed={sp.active}
              className="group absolute flex -translate-x-1/2 -translate-y-1/2 touch-none flex-col items-center gap-0.5"
              style={{ left: `${sp.planX! * 100}%`, top: `${sp.planY! * 100}%`, opacity: dragging ? 0.3 : sp.online || sp.running ? 1 : 0.5 }}
            >
              <span className="relative grid h-9 w-9 place-items-center">
                {sp.running && <Halo id={sp.id} color={color} />}
                <span className="relative grid h-9 w-9 place-items-center rounded-full border-2 shadow-md transition"
                  style={{ borderColor: color, background: sp.active ? color : 'var(--c-surface)', color: sp.active ? '#fff' : color }}>
                  <Icon size={17} />
                </span>
              </span>
              <span className="max-w-[120px] truncate rounded bg-panel/90 px-1.5 text-[11.5px] font-medium shadow-sm">{sp.name}</span>
            </button>
          );
        })}
      </div>

      <div>
        <div className="legend mb-1.5">{tray.length ? t('À placer : glissez sur le plan') : t('Toutes les enceintes sont placées.')}</div>
        <div className="flex flex-wrap gap-1.5">
          {tray.map((sp) => (
            <button key={sp.id} type="button" data-speaker={sp.id}
              onPointerDown={(e) => down(e, sp, true)} onPointerMove={move} onPointerUp={(e) => up(e, sp)} onPointerCancel={() => setDrag(null)}
              onContextMenu={onSpeakerMenu(sp)}
              className="flex cursor-grab touch-none items-center gap-1.5 rounded border border-line bg-panel2 px-2 py-1 text-[12.5px] active:cursor-grabbing"
              style={{ opacity: liveDrag?.id === sp.id ? 0.35 : 1 }}>
              <span className="h-[8px] w-[8px] rounded-full" style={{ background: colorOf(s, sp) }} />
              {sp.name}
            </button>
          ))}
        </div>
      </div>

      {liveDrag && liveSp && (
        <div className="pointer-events-none fixed z-50 rounded bg-ink px-2 py-1 text-[12px] font-medium text-onsig" style={{ left: liveDrag.x + 12, top: liveDrag.y + 12 }}>
          {liveSp.name}
        </div>
      )}
    </div>
  );
}
