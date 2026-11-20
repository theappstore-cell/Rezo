import { AppWindow, ChevronDown, MonitorSpeaker, Pin, PinOff, Plus, Settings, Minus, SlidersHorizontal } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { api } from '../lib/api';
import { dbToPos, fmtDb, posToDb, roundTo } from '../lib/db';
import { useStore } from '../lib/store';
import { t } from '../lib/i18n';
import { useUi } from '../lib/ui';
import type { AudioApp, ViewId } from '../lib/types';
import { LedMeter } from './LedMeter';
import { Slider } from './Slider';

const ZERO_DB = dbToPos(0);

function AppAvatar({ app }: { app: AudioApp }) {
  if (app.icon) return <img src={app.icon} alt="" width={20} height={20} />;
  return <span className="grid h-5 w-5 place-items-center rounded bg-inset text-[11px] font-semibold text-mute">{app.name.slice(0, 1).toUpperCase()}</span>;
}

function SourcePicker({ className = '' }: { className?: string }) {
  const source = useStore((s) => s.s?.source);
  const setSource = useStore((s) => s.setSource);
  const [open, setOpen] = useState(false);
  const [apps, setApps] = useState<AudioApp[] | null>(null);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    setApps(null);
    api.listApps().then(setApps).catch(() => setApps([]));
    const h = (e: PointerEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    window.addEventListener('pointerdown', h);
    return () => window.removeEventListener('pointerdown', h);
  }, [open]);

  const isSystem = !source || source.kind === 'system';
  const label = isSystem ? t('Tout le son du PC') : source.name;

  const Row = ({ selected, onPick, children }: { selected: boolean; onPick: () => void; children: React.ReactNode }) => (
    <button type="button" role="option" aria-selected={selected} onClick={() => { onPick(); setOpen(false); }}
      className={`flex w-full items-center gap-2.5 px-3 py-1.5 text-left hover:bg-raise ${selected ? 'font-semibold' : ''}`}>
      {children}
      {selected && <span className="ml-auto text-[11px]">✓</span>}
    </button>
  );

  return (
    <div ref={box} className={`relative min-w-0 ${className}`}>
      <div className="legend mb-1">{t('Source du son')}</div>
      <button type="button" aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((o) => !o)} className="key flex w-full items-center gap-2 !px-2 text-left">
        {isSystem ? <MonitorSpeaker size={14} className="shrink-0" /> : <AppWindow size={14} className="shrink-0" />}
        <span className="min-w-0 flex-1 truncate">{label}</span>
        <ChevronDown size={13} className="shrink-0" />
      </button>
      {open && (
        <div role="listbox" className="toast-in absolute left-0 top-full z-30 mt-1 w-[min(320px,calc(100vw-24px))] overflow-hidden rounded-md border border-rule bg-panel" style={{ boxShadow: '0 8px 24px rgba(0,0,0,0.25)' }}>
          <Row selected={isSystem} onPick={() => setSource({ kind: 'system' })}>
            <MonitorSpeaker size={18} className="text-mute" />
            <span className="text-[13px]">{t('Tout le son du PC')}</span>
          </Row>
          <div className="legend border-t border-line px-3 pb-1 pt-2">{t('Applications qui jouent du son')}</div>
          <div className="max-h-52 overflow-y-auto pb-1">
            {apps === null && <div className="px-3 py-2 text-[12px] text-mute">{t('Recherche…')}</div>}
            {apps?.length === 0 && <div className="px-3 py-2 text-[12px] text-mute">{t('Aucune application ne produit de son.')}</div>}
            {apps?.map((a) => (
              <Row key={a.pid} selected={!isSystem && source?.kind === 'app' && source.pid === a.pid} onPick={() => setSource({ kind: 'app', pid: a.pid, name: a.name })}>
                <AppAvatar app={a} />
                <span className="truncate text-[13px]">{a.name}</span>
                {a.active && <span className="h-[7px] w-[7px] rounded-full bg-ok" title={t('Joue en ce moment')} />}
              </Row>
            ))}
          </div>
          <div className="border-t border-line px-3 py-2 text-[11.5px] leading-snug text-dim">{t("Mode application : le reste du son (Discord, notifications) n'est pas diffusé, et l'application reste audible sur le PC.")}</div>
        </div>
      )}
    </div>
  );
}

function MasterBlock({ className = '' }: { className?: string }) {
  const masterDb = useStore((s) => s.s?.masterDb ?? 0);
  const setMaster = useStore((s) => s.setMaster);
  return (
    <div className={className}>
      <div className="mb-0.5 flex items-baseline justify-between">
        <span className="legend">{t('Volume général')}</span>
        <span className="num text-[12.5px] font-semibold">{fmtDb(masterDb)} dB</span>
      </div>
      <Slider label={t('Volume général')} value={dbToPos(masterDb)} mark={ZERO_DB} onChange={(v) => setMaster(roundTo(posToDb(v), 0.5))} onReset={() => setMaster(0)} controlId="master:volumeDb" />
      <div className="-mt-0.5"><LedMeter source="master" height={5} orientation="horizontal" /></div>
    </div>
  );
}

function SceneBar() {
  const scenes = useStore((s) => s.s?.scenes ?? []);
  const applyScene = useStore((s) => s.applyScene);
  const say = useStore((s) => s.say);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');
  const [armedDelete, setArmedDelete] = useState<string | null>(null);

  const save = async () => {
    const n = name.trim();
    setAdding(false);
    setName('');
    if (!n) return;
    await api.saveScene(n);
    useStore.getState().apply(await api.getState());
    say(t('Mix « {name} » enregistré : enceintes actives et tous les réglages.', { name: n }));
  };

  return (
    <div className="flex items-center gap-1.5 overflow-x-auto py-1" role="group" aria-label={t('Scènes')}>
      <span className="legend shrink-0">{t('Scènes')}</span>
      {scenes.map((sc) => (
        <div key={sc.id} className="group relative shrink-0">
          <button type="button" onClick={() => void applyScene(sc.id)} className="key !h-[26px]">{sc.name}</button>
          <button
            type="button"
            aria-label={armedDelete === sc.id ? t('Confirmer la suppression de {name}', { name: sc.name }) : t('Supprimer {name}', { name: sc.name })}
            onClick={async () => {
              if (armedDelete !== sc.id) {
                setArmedDelete(sc.id);
                window.setTimeout(() => setArmedDelete((a) => (a === sc.id ? null : a)), 2500);
                return;
              }
              await api.deleteScene(sc.id);
              useStore.getState().apply(await api.getState());
              setArmedDelete(null);
            }}
            className={`absolute -right-1.5 -top-1.5 grid h-4 w-4 place-items-center rounded-full border border-rule text-[10px] leading-none transition ${armedDelete === sc.id ? 'bg-bad text-white opacity-100' : 'bg-panel opacity-0 group-hover:opacity-100'}`}
          >
            ×
          </button>
        </div>
      ))}
      {adding ? (
        <input autoFocus value={name} maxLength={40} placeholder={t('Nom du mix')} onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') void save(); if (e.key === 'Escape') { setAdding(false); setName(''); } }}
          onBlur={() => void save()} className="field !w-36 shrink-0 !py-[3px]" />
      ) : (
        <button type="button" onClick={() => setAdding(true)} className="key flex shrink-0 items-center gap-1 !h-[26px]" title={t('Enregistrer les enceintes actives et tous les réglages')}>
          <Plus size={12} /> {t('Enregistrer le mix')}
        </button>
      )}
    </div>
  );
}

const TABS: Array<{ id: ViewId; label: string }> = [
  { id: 'rooms', label: 'Pièces' },
  { id: 'plan', label: 'Plan' },
  { id: 'console', label: 'Console' },
];

function Title() {
  return <div data-tauri-drag-region className="pointer-events-none text-[15px] font-semibold">Rezo</div>;
}

const iconBtn = 'key key-icon grid !h-7 !w-7 place-items-center';

function Actions({ compact }: { compact: boolean }) {
  const open = useUi((u) => u.open);
  const [pinned, setPinned] = useState(false);
  return (
    <div className="flex shrink-0 items-center gap-1">
      <button type="button" onClick={() => open({ type: 'add' })} className="key key-primary flex items-center gap-1.5 !h-7" title={t('Ajouter un appareil')}>
        <Plus size={13} /> {compact ? <span className="sr-only">{t('Ajouter un appareil')}</span> : t('Ajouter un appareil')}
      </button>
      <button type="button" aria-label={t('Mixeur audio')} title={t('Mixeur audio : sorties filaires / Bluetooth et volume par application')} onClick={() => void api.openMixer()} className={iconBtn}><SlidersHorizontal size={14} /></button>
      <button type="button" aria-label={t('Réglages')} title={t('Réglages')} onClick={() => open({ type: 'settings' })} className={iconBtn}><Settings size={14} /></button>
      <button type="button" aria-label={pinned ? t('Désépingler la fenêtre') : t('Épingler la fenêtre')} aria-pressed={pinned} data-on={pinned}
        title={t('Épingler : la fenêtre ne se masque plus quand elle perd le focus')}
        onClick={() => { setPinned(!pinned); void api.setPinned(!pinned); }} className={iconBtn}>
        {pinned ? <Pin size={14} /> : <PinOff size={14} />}
      </button>
      <button type="button" aria-label={t('Réduire dans la zone de notification')} title={t('Réduire dans la zone de notification')} onClick={() => void api.hideWindow()} className={iconBtn}><Minus size={14} /></button>
    </div>
  );
}

function Tabs() {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  return (
    <nav className="flex gap-1" role="tablist" aria-label={t('Vues')}>
      {TABS.map(({ id, label }) => (
        <button key={id} type="button" role="tab" aria-selected={view === id} onClick={() => setView(id)}
          className={`border-b-2 px-3 py-2 text-[13.5px] transition-colors ${view === id ? 'border-ink font-semibold' : 'border-transparent text-dim hover:text-ink'}`}>
          {t(label)}
        </button>
      ))}
    </nav>
  );
}

export function TopBar() {
  const wide = useStore((s) => s.view === 'console');

  if (wide) {
    return (
      <header className="shrink-0 border-b border-line bg-panel">
        <div data-tauri-drag-region className="flex items-end gap-5 px-3 pb-1 pt-2">
          <div className="self-center"><Title /></div>
          <SourcePicker className="w-52 shrink-0" />
          <MasterBlock className="w-56 shrink-0" />
          <div className="min-w-0 flex-1"><SceneBar /></div>
          <Tabs />
          <div className="self-center"><Actions compact /></div>
        </div>
      </header>
    );
  }

  return (
    <header className="shrink-0 border-b border-line bg-panel">
      <div data-tauri-drag-region className="flex items-center gap-2 px-3 pb-1 pt-2.5">
        <Title />
        <div data-tauri-drag-region className="flex-1 self-stretch" />
        <Actions compact={false} />
      </div>
      <div className="grid grid-cols-2 items-end gap-4 px-3 pb-1.5 pt-1">
        <SourcePicker />
        <MasterBlock />
      </div>
      <div className="px-3"><SceneBar /></div>
      <div className="px-2"><Tabs /></div>
    </header>
  );
}
