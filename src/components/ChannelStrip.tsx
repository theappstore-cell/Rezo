import { memo, useLayoutEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { clamp, fmtDb, fmtMs, fmtPan } from '../lib/db';
import { protocolBadge } from '../lib/icons';
import { useStore } from '../lib/store';
import { t } from '../lib/i18n';
import type { Room, Speaker } from '../lib/types';
import { ChannelFader } from './ChannelFader';
import { FxButton } from './FxPanel';
import { NudgeButton, Pad } from './Pad';
import { RotaryKnob } from './RotaryKnob';

const NEUTRAL = '#9a9a93';

function FaderBay({ valueDb, ...rest }: { valueDb: number } & Omit<React.ComponentProps<typeof ChannelFader>, 'height' | 'valueDb'>) {
  const ref = useRef<HTMLDivElement>(null);
  const [h, setH] = useState(190);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setH(Math.max(110, Math.floor(el.clientHeight))));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return (
    <div className="flex min-h-[130px] flex-1 flex-col items-center gap-1.5">
      <div ref={ref} className="relative min-h-0 w-full flex-1">
        <div className="absolute inset-0 flex justify-center">
          <ChannelFader valueDb={valueDb} height={h} {...rest} />
        </div>
      </div>
      <div className="lcd w-full">{fmtDb(valueDb)} dB</div>
    </div>
  );
}

function Shell({ color, name, sub, width = 112, children }: { color: string; name: string; sub?: string; width?: number; children: ReactNode }) {
  return (
    <div className="flex shrink-0 flex-col gap-2 rounded-md border border-line bg-panel p-2" style={{ width, borderTop: `3px solid ${color}` }}>
      <div className="min-w-0">
        <div className="truncate text-[13px] font-semibold" title={name}>{name}</div>
        {sub && <div className="legend truncate" title={sub}>{sub}</div>}
      </div>
      {children}
    </div>
  );
}

export const SpeakerStrip = memo(function SpeakerStrip({ sp, room }: { sp: Speaker; room?: Room }) {
  const setParam = useStore((s) => s.setParam);
  const setSend = useStore((s) => s.setSend);
  const color = room?.color ?? NEUTRAL;
  const p = sp.params;
  const id = `ch:${sp.id}`;
  const off = !sp.online;
  const state = sp.streaming ? 'var(--c-ok)' : off ? 'var(--c-bad)' : 'var(--c-border-strong)';

  return (
    <Shell color={color} name={sp.name} sub={room?.name ?? t('Sans pièce')}>
      <div className="flex items-center gap-1.5">
        <span className="inline-block h-[8px] w-[8px] rounded-full" style={{ background: state }} title={sp.streaming ? t('Reçoit le flux') : off ? t('Hors ligne') : t('En attente')} />
        <span className="legend truncate">{protocolBadge(sp)}</span>
      </div>

      <div className="grid grid-cols-2 gap-x-1 gap-y-2" style={{ opacity: off ? 0.55 : 1 }}>
        <RotaryKnob label="Gain" value={p.gainDb} min={-12} max={12} defaultValue={0} step={0.5} fineStep={0.1} bipolar color={color}
          format={(v) => fmtDb(v)} onChange={(v) => setParam(sp.id, 'gainDb', v)} controlId={`${id}:gainDb`} size={40} />
        <RotaryKnob label="Pan" value={p.pan} min={-1} max={1} defaultValue={0} step={0.05} fineStep={0.01} bipolar color={color}
          format={fmtPan} onChange={(v) => setParam(sp.id, 'pan', v)} controlId={`${id}:pan`} size={40} />
        <RotaryKnob label="High" value={p.highDb} min={-12} max={12} defaultValue={0} step={0.5} fineStep={0.1} bipolar color={color}
          format={(v) => fmtDb(v)} onChange={(v) => setParam(sp.id, 'highDb', v)} controlId={`${id}:highDb`} size={40} />
        <RotaryKnob label="Mid" value={p.midDb} min={-12} max={12} defaultValue={0} step={0.5} fineStep={0.1} bipolar color={color}
          format={(v) => fmtDb(v)} onChange={(v) => setParam(sp.id, 'midDb', v)} controlId={`${id}:midDb`} size={40} />
        <RotaryKnob label="Low" value={p.lowDb} min={-12} max={12} defaultValue={0} step={0.5} fineStep={0.1} bipolar color={color}
          format={(v) => fmtDb(v)} onChange={(v) => setParam(sp.id, 'lowDb', v)} controlId={`${id}:lowDb`} size={40} />
        <RotaryKnob label="Sync" value={p.delayMs} min={-500} max={500} defaultValue={0} step={1} fineStep={0.5} bipolar
          format={(v) => `${fmtMs(v)} ms`} onChange={(v) => setParam(sp.id, 'delayMs', v)} controlId={`${id}:delayMs`} size={40} />
      </div>
      <div className="flex gap-1">
        <NudgeButton label="−1 ms" title={t('Avancer de 1 ms')} onClick={() => setParam(sp.id, 'delayMs', clamp(p.delayMs - 1, -500, 500))} />
        <NudgeButton label="+1 ms" title={t('Retarder de 1 ms')} onClick={() => setParam(sp.id, 'delayMs', clamp(p.delayMs + 1, -500, 500))} />
      </div>

      <FxButton sp={sp} color={color} />

      <div className="grid grid-cols-2 gap-1">
        <Pad className="col-span-2" label={t('Envoi')} color="var(--c-ok)" on={sp.active} onToggle={() => void setSend(sp.id, !sp.active)}
          title={t('Envoyer le son vers cette enceinte')} controlId={`${id}:send`} disabled={off && !sp.active} />
        <Pad label="Mute" color="var(--c-bad)" on={p.mute} onToggle={() => setParam(sp.id, 'mute', !p.mute)} controlId={`${id}:mute`} />
        <Pad label="Solo" color="var(--c-warn)" on={p.solo} onToggle={() => setParam(sp.id, 'solo', !p.solo)} controlId={`${id}:solo`} />
        <Pad label="Mono" on={p.mode === 'mono'} onToggle={() => setParam(sp.id, 'mode', p.mode === 'mono' ? 'stereo' : 'mono')} controlId={`${id}:mono`} />
        <Pad label="Ø" title={t('Inversion de phase')} on={p.phaseInvert} onToggle={() => setParam(sp.id, 'phaseInvert', !p.phaseInvert)} controlId={`${id}:phase`} />
      </div>

      <FaderBay valueDb={p.volumeDb} onChange={(v) => setParam(sp.id, 'volumeDb', v)} color={color}
        meter={sp.id} controlId={`${id}:volumeDb`} dimmed={p.mute} />
    </Shell>
  );
});

export const RoomStrip = memo(function RoomStrip({ room, members }: { room: Room; members: Speaker[] }) {
  const setRoomParam = useStore((s) => s.setRoomParam);
  const setRoomSend = useStore((s) => s.setRoomSend);
  const anyOn = members.some((m) => m.active);
  const id = `room:${room.id}`;
  return (
    <div className="flex shrink-0 flex-col gap-2 rounded-md border border-rule bg-panel2 p-2" style={{ width: 96, borderTop: `3px solid ${room.color}` }}>
      <div className="min-w-0">
        <div className="truncate text-[13px] font-semibold" title={room.name}>{room.name}</div>
        <div className="legend">Bus · {members.filter((m) => m.active).length}/{members.length}</div>
      </div>
      <div className="mt-auto grid grid-cols-1 gap-1">
        <Pad label={t('Pièce')} color="var(--c-ok)" on={anyOn} onToggle={() => void setRoomSend(room.id, !anyOn)} title={t('Tout allumer / éteindre dans la pièce')} controlId={`${id}:send`} />
        <Pad label="Mute" color="var(--c-bad)" on={room.mute} onToggle={() => setRoomParam(room.id, 'mute', !room.mute)} controlId={`${id}:mute`} />
        <Pad label="Solo" color="var(--c-warn)" on={room.solo} onToggle={() => setRoomParam(room.id, 'solo', !room.solo)} controlId={`${id}:solo`} />
      </div>
      <FaderBay valueDb={room.volumeDb} onChange={(v) => setRoomParam(room.id, 'volumeDb', v)} color={room.color}
        meter={members.map((m) => m.id)} controlId={`${id}:volumeDb`} dimmed={room.mute} />
    </div>
  );
});

export function MasterStrip() {
  const masterDb = useStore((s) => s.s?.masterDb ?? 0);
  const localPc = useStore((s) => s.s?.localPc ?? false);
  const pcMuted = useStore((s) => s.s?.pcMuted ?? false);
  const setMaster = useStore((s) => s.setMaster);
  const setLocalPc = useStore((s) => s.setLocalPc);
  return (
    <div className="flex shrink-0 flex-col gap-2 rounded-md border border-rule bg-panel2 p-2" style={{ width: 112, borderTop: '3px solid var(--c-ink)' }}>
      <div className="min-w-0">
        <div className="text-[13px] font-semibold">Master</div>
        <div className="legend">{t('Sortie, avec limiteur')}</div>
      </div>
      <div className="mt-auto grid grid-cols-1 gap-1">
        <Pad label="PC" on={localPc} onToggle={() => setLocalPc(!localPc)} controlId="master:localPc"
          title={t('Haut-parleurs du PC : garder le son local en plus des enceintes')} />
        <div className="legend text-center">{pcMuted ? t('PC coupé') : t('PC audible')}</div>
      </div>
      <FaderBay valueDb={masterDb} onChange={setMaster} meter="master" controlId="master:volumeDb" />
    </div>
  );
}
