import { useEffect, useLayoutEffect, useRef } from 'react';
import type { MouseEvent, ReactNode } from 'react';
import { Lock, LockOpen, Settings2, VolumeX, X } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, isTauri } from '../lib/api';
import { dbToPos, fmtDb, posToDb, roundTo } from '../lib/db';
import { roomIcon } from '../lib/icons';
import { roomLabel, speakersOf, useStore } from '../lib/store';
import { t } from '../lib/i18n';
import { tint } from '../lib/style';
import type { FullState, Widget } from '../lib/types';
import { LedMeter } from '../components/LedMeter';
import { Gauge, MediaControls, clock, useMedia } from '../components/MediaBar';
import { Toggle } from '../components/Modal';
import { Slider } from '../components/Slider';

export const WIDGET_NAMES: Record<Widget['kind'], string> = {
  player: 'Lecteur',
  volume: 'Volume général',
  rooms: 'Pièces',
  scenes: 'Scènes',
  meter: 'Vu-mètre',
};

const WIDTH: Record<Widget['kind'], number> = { player: 320, volume: 260, rooms: 230, scenes: 250, meter: 240 };

function Player() {
  const { state: m, pos, send } = useMedia();
  const has = !!m?.available;
  const dur = m?.durationMs ?? 0;
  return (
    <div className="flex flex-col gap-1.5">
      <div className="min-w-0" data-drag>
        <div className="truncate text-[13.5px] font-semibold leading-tight" data-drag>{has ? m!.title || m!.app : t('Aucune lecture en cours')}</div>
        <div className="truncate text-[11.5px] opacity-75" data-drag>{has ? [m!.artist, m!.app].filter(Boolean).join(' · ') : 'Rezo'}</div>
      </div>
      <div className="flex items-center gap-2">
        <MediaControls m={m} send={(c) => void send(c)} size="sm" light />
        <div className="min-w-0 flex-1">
          <Gauge pos={pos} duration={dur} canSeek={has && m!.canSeek} onSeek={(ms) => void send('seek', ms)} size="sm" light />
          <div className="num flex justify-between text-[10.5px] opacity-75" data-drag>
            <span>{has ? clock(pos) : '0:00'}</span>
            <span>{has && dur ? clock(dur) : '0:00'}</span>
          </div>
        </div>
      </div>
    </div>
  );
}

function Volume({ s }: { s: FullState }) {
  const setMaster = useStore((st) => st.setMaster);
  const setMasterMute = useStore((st) => st.setMasterMute);
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2" data-drag>
        <span className="flex-1 text-[12px] font-semibold" data-drag>{t('Volume général')}</span>
        <span className={`num text-[12.5px] font-semibold ${s.masterMute ? 'line-through opacity-60' : ''}`} data-drag>{fmtDb(s.masterDb)} dB</span>
        <button type="button" className="key key-icon grid !h-[24px] !w-[26px] place-items-center" data-on={s.masterMute} style={tint('var(--c-bad)')}
          aria-pressed={s.masterMute} aria-label={t('Couper toutes les enceintes')} title={t('Couper toutes les enceintes')} onClick={() => setMasterMute(!s.masterMute)}>
          <VolumeX size={13} />
        </button>
      </div>
      <Slider label={t('Volume général')} value={dbToPos(s.masterDb)} mark={dbToPos(0)} color="var(--c-ok)" onChange={(v) => setMaster(roundTo(posToDb(v), 0.5))} onReset={() => setMaster(0)} />
      <LedMeter source="master" height={5} orientation="horizontal" />
    </div>
  );
}

function Rooms({ s }: { s: FullState }) {
  const setRoomSend = useStore((st) => st.setRoomSend);
  const rooms = s.rooms.filter((r) => speakersOf(s, r.id).length > 0);
  return (
    <div className="flex flex-col gap-1">
      <div className="text-[12px] font-semibold" data-drag>{t('Pièces')}</div>
      {rooms.length === 0 && <div className="text-[12px] opacity-70" data-drag>{t('Aucune enceinte dans une pièce.')}</div>}
      {rooms.map((r) => {
        const members = speakersOf(s, r.id);
        const on = members.some((m) => m.active);
        const Icon = roomIcon(r.icon);
        return (
          <div key={r.id} className="flex items-center gap-2 py-0.5" data-drag>
            <Icon size={14} style={{ color: r.color }} />
            <span className="min-w-0 flex-1 truncate text-[12.5px]" data-drag>{roomLabel(r)}</span>
            <span className="num text-[10.5px] opacity-60" data-drag>{members.filter((m) => m.active).length}/{members.length}</span>
            <Toggle on={on} onChange={(v) => void setRoomSend(r.id, v)} label={t('Toute la pièce {name}', { name: roomLabel(r) })} color="var(--c-ok)" />
          </div>
        );
      })}
    </div>
  );
}

function Scenes({ s }: { s: FullState }) {
  const applyScene = useStore((st) => st.applyScene);
  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-[12px] font-semibold" data-drag>{t('Scènes')}</div>
      {s.scenes.length === 0 && <div className="text-[12px] opacity-70" data-drag>{t('Aucun mix enregistré.')}</div>}
      <div className="flex flex-wrap gap-1.5">
        {s.scenes.map((sc) => (
          <button key={sc.id} type="button" className="key !h-[26px]" onClick={() => void applyScene(sc.id)}>{sc.name}</button>
        ))}
      </div>
    </div>
  );
}

function Meter() {
  return (
    <div className="flex flex-col gap-1" data-drag>
      <div className="text-[11px] font-semibold uppercase tracking-wide opacity-75" data-drag>{t('Sortie')}</div>
      <LedMeter source="master" height={12} orientation="horizontal" />
    </div>
  );
}

function ToolButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" aria-label={label} title={label} onClick={onClick} className="grid h-[20px] w-[20px] place-items-center rounded text-white/80 hover:bg-white/20 hover:text-white">
      {children}
    </button>
  );
}

// One desktop widget, in its own transparent window. Unlocked, it moves by dragging any part that is not a control.
export function WidgetWindow({ id }: { id: string }) {
  const s = useStore((st) => st.s);
  const root = useRef<HTMLDivElement>(null);
  const w = s?.widgets.find((x) => x.id === id);

  useEffect(() => {
    void useStore.getState().initLite();
  }, []);

  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const fit = () => {
      const r = el.getBoundingClientRect();
      void api.widgetFit(id, r.width, r.height);
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [id, w?.kind, w?.scale]);

  if (!s || !w) return null;

  const drag = (e: MouseEvent) => {
    if (w.locked || e.button !== 0 || !isTauri) return;
    const target = e.target as HTMLElement;
    if (target.closest('button, input, [role="slider"], [role="switch"]')) return;
    void getCurrentWindow().startDragging();
  };

  const content = {
    player: <Player />,
    volume: <Volume s={s} />,
    rooms: <Rooms s={s} />,
    scenes: <Scenes s={s} />,
    meter: <Meter />,
  }[w.kind];

  return (
    <div ref={root} className="inline-block p-[3px]" style={{ zoom: w.scale }}>
      <div
        onMouseDown={drag}
        className={`widget-dark group relative rounded-[12px] px-3 py-2.5 text-[#f3f3ef] ${w.locked ? '' : 'cursor-move'}`}
        style={{
          width: WIDTH[w.kind],
          background: `rgba(20, 20, 22, ${w.opacity})`,
          border: `1px solid rgba(255,255,255,${Math.min(0.18, w.opacity * 0.25)})`,
          textShadow: w.opacity < 0.5 ? '0 1px 2px rgba(0,0,0,0.75)' : undefined,
        }}
      >
        <div className="absolute right-1.5 top-1.5 z-10 flex gap-0.5 rounded-md bg-black/55 p-0.5 opacity-0 transition-opacity group-hover:opacity-100">
          <ToolButton label={w.locked ? t('Déverrouiller (permet de déplacer le widget)') : t('Verrouiller la position')} onClick={() => void api.widgetUpdate(id, { locked: !w.locked })}>
            {w.locked ? <Lock size={12} /> : <LockOpen size={12} />}
          </ToolButton>
          <ToolButton label={t('Réglages des widgets')} onClick={() => void api.showSettings()}>
            <Settings2 size={12} />
          </ToolButton>
          <ToolButton label={t('Masquer ce widget')} onClick={() => void api.widgetUpdate(id, { visible: false })}>
            <X size={12} />
          </ToolButton>
        </div>
        {content}
      </div>
    </div>
  );
}
