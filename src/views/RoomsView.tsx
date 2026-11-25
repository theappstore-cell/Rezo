import { Pencil, Plus, SlidersHorizontal, Volume2 } from 'lucide-react';
import { dbToPos, fmtDb, posToDb, roundTo } from '../lib/db';
import { protocolBadge } from '../lib/icons';
import { colorOf, roomLabel, speakersOf, useStore } from '../lib/store';
import { useUi } from '../lib/ui';
import type { Room, Speaker } from '../lib/types';
import { LedMeter } from '../components/LedMeter';
import { eqActive } from '../components/Equalizer';
import { Toggle } from '../components/Modal';
import { Slider } from '../components/Slider';
import { t } from '../lib/i18n';
import { useScrollMemory } from '../lib/layout';

const ZERO_DB = dbToPos(0);

function SpeakerRow({ sp, color }: { sp: Speaker; color: string }) {
  const setSend = useStore((s) => s.setSend);
  const setParam = useStore((s) => s.setParam);
  const setDeviceVolume = useStore((s) => s.setDeviceVolume);
  const open = useUi((u) => u.open);
  const off = !sp.online && !sp.running;
  const hasDevice = sp.deviceVolume !== null && sp.deviceVolume !== undefined;
  const deviceLabel = sp.protocol === 'sonos' || sp.protocol === 'upnp' ? t("Volume de l'enceinte (le même que dans l'app Sonos)") : t('Volume du récepteur');
  const detail = [protocolBadge(sp), sp.group ? t('groupe {name}', { name: sp.group }) : null, off ? t('hors ligne') : null].filter(Boolean).join(' · ');

  return (
    <li className="border-t border-line px-3 py-2.5" style={{ opacity: off ? 0.6 : 1 }}>
      <div className="flex items-center gap-2.5">
        <Toggle on={sp.active} onChange={(v) => void setSend(sp.id, v)} label={t('Envoyer vers {name}', { name: sp.name })} color="var(--c-ok)" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="truncate text-[13.5px] font-medium">{sp.name}</span>
            {sp.streaming && <span className="h-[7px] w-[7px] shrink-0 rounded-full bg-ok" title={t('Reçoit le flux')} />}
          </div>
          <div className="legend truncate">{detail}</div>
        </div>
        <button type="button" className="key !h-[26px] !px-2 !text-[11.5px]" data-on={sp.params.mute} style={{ ['--c' as string]: 'var(--c-bad)' }}
          aria-pressed={sp.params.mute} onClick={() => setParam(sp.id, 'mute', !sp.params.mute)}>Mute</button>
        <button type="button" className="key !h-[26px] !px-2 !text-[11.5px]" data-on={sp.params.solo} style={{ ['--c' as string]: 'var(--c-warn)' }}
          aria-pressed={sp.params.solo} onClick={() => setParam(sp.id, 'solo', !sp.params.solo)}>Solo</button>
        <button type="button" className="key !h-[26px] !px-2 !text-[11.5px]" data-on={eqActive(sp)} style={{ ['--c' as string]: color }}
          title={t('Égaliseur 6 bandes de cette enceinte')} onClick={() => open({ type: 'eq', id: sp.id })}>EQ</button>
        <button type="button" className="key key-icon grid !h-[26px] !w-[26px] place-items-center" aria-label={t("Réglages de l'enceinte")} title={t('Réglages')} onClick={() => open({ type: 'speaker', id: sp.id })}>
          <SlidersHorizontal size={13} />
        </button>
      </div>
      <div className="mt-1 flex items-center gap-2.5 pl-[46px]">
        <div className="min-w-0 flex-1">
          <Slider label={t('Volume {name}', { name: sp.name })} value={dbToPos(sp.params.volumeDb)} color={color} mark={ZERO_DB}
            onChange={(v) => setParam(sp.id, 'volumeDb', roundTo(posToDb(v), 0.5))} onReset={() => setParam(sp.id, 'volumeDb', 0)} controlId={`ch:${sp.id}:volumeDb`} />
          <div className="-mt-0.5"><LedMeter source={sp.id} height={5} orientation="horizontal" /></div>
        </div>
        <span className="num w-[62px] shrink-0 text-right text-[12.5px]">{fmtDb(sp.params.volumeDb)} dB</span>
      </div>
      {hasDevice && (
        <div className="mt-0.5 flex items-center gap-2.5 pl-[20px]" title={deviceLabel}>
          <Volume2 size={14} className="w-[16px] shrink-0 text-dim" aria-hidden />
          <Slider label={`${deviceLabel} : ${sp.name}`} value={(sp.deviceVolume ?? 0) / 100} color={color} step={0.01}
            onChange={(v) => setDeviceVolume(sp.id, Math.round(v * 100))} controlId={`ch:${sp.id}:deviceVolume`} />
          <span className="num w-[62px] shrink-0 text-right text-[12.5px]">{sp.deviceVolume}</span>
        </div>
      )}
      {sp.error && <div className="mt-2 rounded border border-bad/50 bg-bad/10 px-2 py-1 text-[12px] text-bad">{sp.error}</div>}
    </li>
  );
}

function RoomGroup({ room, members }: { room: Room; members: Speaker[] }) {
  const s = useStore((st) => st.s)!;
  const setRoomSend = useStore((st) => st.setRoomSend);
  const setRoomParam = useStore((st) => st.setRoomParam);
  const open = useUi((u) => u.open);
  const activeCount = members.filter((m) => m.active).length;

  return (
    <section className="shrink-0 overflow-hidden rounded-md border border-line bg-panel">
      <header className="flex items-center gap-2.5 bg-panel2 px-3 py-2">
        <span className="h-[10px] w-[10px] shrink-0 rounded-full" style={{ background: room.color }} />
        <h2 className="min-w-0 flex-1 truncate text-[14.5px] font-semibold">{roomLabel(room)}</h2>
        <span className="legend num">{t(activeCount > 1 ? '{n} / {m} actives' : '{n} / {m} active', { n: activeCount, m: members.length })}</span>
        <button type="button" aria-label={t('Modifier la pièce')} title={t('Modifier la pièce')} onClick={() => open({ type: 'room', id: room.id })} className="rounded p-1 text-dim hover:bg-raise hover:text-ink">
          <Pencil size={13} />
        </button>
        <Toggle on={activeCount > 0} onChange={(v) => void setRoomSend(room.id, v)} label={t('Toute la pièce {name}', { name: roomLabel(room) })} color="var(--c-ok)" />
      </header>
      <div className="flex items-center gap-2.5 px-3 py-1.5">
        <span className="legend w-[46px] shrink-0">{t('Pièce')}</span>
        <Slider label={t('Volume de la pièce {name}', { name: roomLabel(room) })} value={dbToPos(room.volumeDb)} color={room.color} mark={ZERO_DB}
          onChange={(v) => setRoomParam(room.id, 'volumeDb', roundTo(posToDb(v), 0.5))} onReset={() => setRoomParam(room.id, 'volumeDb', 0)} controlId={`room:${room.id}:volumeDb`} />
        <span className="num w-[62px] shrink-0 text-right text-[12.5px]">{fmtDb(room.volumeDb)} dB</span>
      </div>
      <ul>
        {members.map((sp) => (
          <SpeakerRow key={sp.id} sp={sp} color={colorOf(s, sp)} />
        ))}
      </ul>
    </section>
  );
}

export function RoomsView() {
  const s = useStore((st) => st.s);
  const scanning = useStore((st) => st.scanning);
  const scan = useStore((st) => st.scan);
  const open = useUi((u) => u.open);
  const scroller = useScrollMemory<HTMLDivElement>('rooms');
  if (!s) return null;

  const groups = s.rooms.map((room) => ({ room, members: speakersOf(s, room.id) })).filter((g) => g.members.length > 0);
  const unassigned = s.speakers.filter((sp) => !s.rooms.some((r) => r.id === sp.roomId));
  const nothing = s.speakers.length === 0;

  return (
    <div ref={scroller} className="flex h-full flex-col gap-3 overflow-y-auto p-2.5 min-[480px]:p-3">
      {nothing && (
        <div className="m-auto flex max-w-[300px] flex-col items-center gap-3 py-10 text-center">
          <h2 className="text-[16px] font-semibold">{t('Aucune enceinte trouvée')}</h2>
          <p className="text-[13px] leading-snug text-mute">
            {t("Rezo cherche les Sonos, les enceintes UPnP/DLNA et les récepteurs Rezo du réseau. Un téléphone peut aussi servir d'enceinte avec un QR code.")}
          </p>
          <div className="flex gap-2">
            <button type="button" onClick={() => void scan()} disabled={scanning} className="key">{scanning ? t('Recherche…') : t('Relancer la recherche')}</button>
            <button type="button" onClick={() => open({ type: 'add' })} className="key key-primary">{t('Ajouter un appareil')}</button>
          </div>
        </div>
      )}
      {groups.map(({ room, members }) => (
        <RoomGroup key={room.id} room={room} members={members} />
      ))}
      {unassigned.length > 0 && (
        <section className="shrink-0 overflow-hidden rounded-md border border-dashed border-rule bg-panel">
          <header className="bg-panel2 px-3 py-2">
            <h2 className="text-[14.5px] font-semibold text-mute">{t('Sans pièce')}</h2>
            <p className="legend">{t("Placez-les dans une pièce depuis l'onglet Plan ou leurs réglages.")}</p>
          </header>
          <ul>
            {unassigned.map((sp) => (
              <SpeakerRow key={sp.id} sp={sp} color="#9a9a93" />
            ))}
          </ul>
        </section>
      )}
      {!nothing && (
        <button type="button" onClick={() => open({ type: 'add' })} className="key mb-1 flex shrink-0 items-center justify-center gap-1.5 !h-9">
          <Plus size={14} /> {t('Ajouter un appareil')}
        </button>
      )}
    </div>
  );
}
