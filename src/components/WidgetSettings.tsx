import { useEffect, useRef, useState } from 'react';
import { Activity, Gauge as GaugeIcon, Layers, Music2, Plus, Sparkles, Trash2 } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { t } from '../lib/i18n';
import type { Widget, WidgetKind, WidgetLayer } from '../lib/types';
import { WIDGET_NAMES } from '../views/WidgetWindow';
import { Toggle } from './Modal';

const KINDS: Array<{ kind: WidgetKind; icon: LucideIcon }> = [
  { kind: 'player', icon: Music2 },
  { kind: 'volume', icon: GaugeIcon },
  { kind: 'rooms', icon: Layers },
  { kind: 'scenes', icon: Sparkles },
  { kind: 'meter', icon: Activity },
];

const LAYERS: Array<{ id: WidgetLayer; label: string; hint: string }> = [
  { id: 'top', label: 'Toujours au premier plan', hint: 'Au-dessus de toutes les fenêtres, même des jeux en fenêtré.' },
  { id: 'normal', label: 'Normal', hint: 'Comme une fenêtre ordinaire : passe devant quand on clique dessus.' },
  { id: 'bottom', label: 'Sous les fenêtres', hint: 'Toujours derrière les autres fenêtres.' },
  { id: 'desktop', label: 'Sur le bureau', hint: 'Posé sur le fond d’écran, derrière les fenêtres, et reste visible avec Win+D (Afficher le bureau).' },
];

const SIZES = [0.8, 1, 1.25, 1.5];

function WidgetCard({ w }: { w: Widget }) {
  const apply = useStore((st) => st.apply);
  const [opacity, setOpacity] = useState(w.opacity);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => setOpacity(w.opacity), [w.opacity]);
  const upd = async (patch: Parameters<typeof api.widgetUpdate>[1]) => apply(await api.widgetUpdate(w.id, patch));
  const layer = LAYERS.find((l) => l.id === w.layer) ?? LAYERS[1];
  const Icon = KINDS.find((k) => k.kind === w.kind)?.icon ?? Music2;
  const sel = 'rounded-[4px] border border-line bg-inset px-2 py-1 text-[12px]';

  return (
    <li className="rounded-md border border-line bg-panel2 p-2.5">
      <div className="flex items-center gap-2">
        <Icon size={15} className="shrink-0 text-mute" />
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold">{t(WIDGET_NAMES[w.kind])}</span>
        <Toggle on={w.visible} onChange={(v) => void upd({ visible: v })} label={t('Afficher le widget {name}', { name: t(WIDGET_NAMES[w.kind]) })} color="var(--c-ok)" />
        <button type="button" className="rounded p-1 text-dim hover:bg-raise hover:text-bad" aria-label={t('Supprimer le widget')} title={t('Supprimer le widget')}
          onClick={async () => apply(await api.widgetRemove(w.id))}>
          <Trash2 size={14} />
        </button>
      </div>
      <div className="mt-2 grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-2 text-[12px]">
        <span className="text-mute">{t("Plan d'affichage")}</span>
        <select className={sel} value={w.layer} onChange={(e) => void upd({ layer: e.target.value as WidgetLayer })}>
          {LAYERS.map((l) => <option key={l.id} value={l.id}>{t(l.label)}</option>)}
        </select>
        <span />
        <span className="-mt-1 text-[10.5px] leading-snug text-dim">{t(layer.hint)}</span>

        <span className="text-mute">{t('Fond')}</span>
        <span className="flex items-center gap-2">
          <input type="range" min={0} max={100} step={1} value={Math.round(opacity * 100)} className="min-w-0 flex-1 accent-[var(--c-ok)]" aria-label={t('Opacité du fond')}
            onChange={(e) => {
              const v = Number(e.target.value) / 100;
              setOpacity(v);
              window.clearTimeout(timer.current);
              timer.current = window.setTimeout(() => void upd({ opacity: v }), 120);
            }} />
          <span className="num w-9 text-right text-mute">{Math.round(opacity * 100)} %</span>
        </span>

        <span className="text-mute">{t('Taille')}</span>
        <select className={sel} value={w.scale} onChange={(e) => void upd({ scale: Number(e.target.value) })}>
          {SIZES.map((z) => <option key={z} value={z}>{Math.round(z * 100)} %</option>)}
          {!SIZES.includes(w.scale) && <option value={w.scale}>{Math.round(w.scale * 100)} %</option>}
        </select>

        <span className="text-mute">{t('Position verrouillée')}</span>
        <span className="flex justify-end"><Toggle on={w.locked} onChange={(v) => void upd({ locked: v })} label={t('Position verrouillée')} /></span>

        <span className="text-mute">{t('Clic traversant')}</span>
        <span className="flex items-center justify-end gap-2">
          <span className="text-[10.5px] text-dim">{t('la souris passe au travers')}</span>
          <Toggle on={w.clickThrough} onChange={(v) => void upd({ clickThrough: v })} label={t('Clic traversant')} />
        </span>
      </div>
    </li>
  );
}

// Settings section: create, place and style the desktop widgets.
export function WidgetSettings() {
  const widgets = useStore((st) => st.s?.widgets ?? []);
  const apply = useStore((st) => st.apply);
  return (
    <section className="py-3">
      <div className="text-[12.5px]">{t('Widgets de bureau')}</div>
      <p className="mb-2 text-[10.5px] leading-snug text-dim">
        {t("Petites fenêtres à laisser sur le bureau, à la manière de Rainmeter. Déverrouillé, un widget se déplace en le faisant glisser ; sa position est retenue. Survolez-le pour le verrouiller, le masquer ou revenir ici.")}
      </p>
      <div className="mb-2 flex flex-wrap gap-1.5">
        {KINDS.map(({ kind, icon: Icon }) => (
          <button key={kind} type="button" className="key flex items-center gap-1.5 !h-[26px] !text-[11.5px]" onClick={async () => apply(await api.widgetAdd(kind))}>
            <Plus size={11} /> <Icon size={12} /> {t(WIDGET_NAMES[kind])}
          </button>
        ))}
      </div>
      {widgets.length > 0 && (
        <ul className="space-y-2">
          {widgets.map((w) => <WidgetCard key={w.id} w={w} />)}
        </ul>
      )}
    </section>
  );
}
