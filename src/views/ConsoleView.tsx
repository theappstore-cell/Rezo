import { useState } from 'react';
import type { RefObject } from 'react';
import { Keyboard, Link2, Trash2 } from 'lucide-react';
import { useMidiLearn } from '../hooks/useMidiLearn';
import { roomLabel, speakersOf, useStore } from '../lib/store';
import { MasterStrip, RoomStrip, SpeakerStrip } from '../components/ChannelStrip';
import { t } from '../lib/i18n';
import { useScrollMemory } from '../lib/layout';
import { useReorder } from '../lib/useReorder';
import type { Room, Speaker } from '../lib/types';

// The strips of one room (or of the speakers without a room), reordered by dragging their grip.
function StripGroup({ room, members, allIds }: { room?: Room; members: Speaker[]; allIds: string[] }) {
  const reorderSpeakers = useStore((st) => st.reorderSpeakers);
  const ids = members.map((m) => m.id);
  const r = useReorder(ids, 'x', (next) => {
    const full = [...allIds];
    const slots = full.map((id, i) => (ids.includes(id) ? i : -1)).filter((i) => i >= 0);
    slots.forEach((slot, k) => (full[slot] = next[k]));
    reorderSpeakers(full);
  });
  const byId = new Map(members.map((m) => [m.id, m]));
  return (
    <div ref={r.container as RefObject<HTMLDivElement>} className={`flex shrink-0 gap-1.5 ${room ? 'border-r border-line pr-2' : ''}`} aria-label={room ? t('Pièce {name}', { name: roomLabel(room) }) : undefined}>
      {r.order.map((id) => {
        const sp = byId.get(id);
        if (!sp) return null;
        const it = r.item(id);
        return (
          <div key={id} data-reorder={it['data-reorder']} className="flex" style={it.style}>
            <SpeakerStrip sp={sp} room={room} handle={members.length > 1 ? r.handle(id) : undefined} />
          </div>
        );
      })}
      {room && <RoomStrip room={room} members={members} />}
    </div>
  );
}
import { tint } from '../lib/style';

export function ConsoleView() {
  const s = useStore((st) => st.s);
  const sync = useStore((st) => st.sync);
  const [syncing, setSyncing] = useState(false);
  const scroller = useScrollMemory<HTMLDivElement>('console');
  const midi = useMidiLearn();
  if (!s) return null;
  const grouped = s.speakers.some((sp) => sp.syncedTo);
  const activeCount = s.speakers.filter((sp) => sp.running).length;

  const loose = s.speakers.filter((sp) => !s.rooms.some((r) => r.id === sp.roomId));
  const allIds = s.speakers.map((sp) => sp.id);
  const groups = s.rooms.map((room) => ({ room, members: speakersOf(s, room.id) })).filter((g) => g.members.length > 0);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-line px-3 py-2">
        <button type="button" className="key flex items-center gap-1.5" data-on={midi.learn} aria-pressed={midi.learn} onClick={midi.toggleLearn} disabled={!midi.supported}>
          <Keyboard size={14} />
          MIDI Learn
        </button>
        <button type="button" className="key flex items-center gap-1.5" style={tint('var(--c-ok)')} data-on={grouped} aria-pressed={grouped} disabled={syncing || (!grouped && activeCount < 2)}
          onClick={async () => { setSyncing(true); try { await sync(); } finally { setSyncing(false); } }}
          title={grouped
            ? t('Dissoudre le groupe Sonos : chaque enceinte reprend son propre flux.')
            : t("Synchroniser les enceintes actives : les Sonos sont groupées comme dans l'app Sonos (aucun décalage entre elles), les autres sont retardées pour jouer en même temps.")}>
          <Link2 size={14} />
          {syncing ? t('Sync…') : 'Sync'}
        </button>
        <div className="legend min-w-0 flex-1 truncate !text-[12px]">
          {!midi.supported && t('Web MIDI indisponible dans cette fenêtre.')}
          {midi.supported && midi.error && <span className="text-bad">{t(midi.error)}</span>}
          {midi.supported && !midi.error && midi.learn && t('Cliquez un contrôle, puis bougez un potard, fader ou bouton du contrôleur. Clic droit : délier.')}
          {midi.supported && !midi.error && !midi.learn && (!midi.ready ? t('MIDI inactif : les contrôleurs restent libres pour vos autres logiciels (MIDI Learn pour lier).') : midi.inputs.length > 0 ? t('Contrôleur : {list}', { list: midi.inputs.join(', ') }) : t('Aucun contrôleur MIDI détecté. Branchez-en un en USB.'))}
        </div>
        <span className="legend num">{t(midi.bindings > 1 ? '{n} assignations' : '{n} assignation', { n: midi.bindings })}</span>
        {midi.learn && midi.bindings > 0 && (
          <button type="button" onClick={midi.clearAll} className="key flex items-center gap-1">
            <Trash2 size={12} /> {t('Tout effacer')}
          </button>
        )}
      </div>

      <div className="flex min-h-0 flex-1">
        <div ref={scroller} className="flex min-w-0 flex-1 gap-2 overflow-x-auto overflow-y-auto p-3">
          {s.speakers.length === 0 && (
            <div className="m-auto flex flex-col items-center gap-2 text-center text-[13px] text-mute">
              {t('Aucune enceinte pour le moment.')}
              <button type="button" className="key key-primary" onClick={() => useStore.getState().setView('rooms')}>{t('Voir comment en trouver')}</button>
            </div>
          )}
          {loose.length > 0 && <StripGroup members={loose} allIds={allIds} />}
          {groups.map(({ room, members }) => (
            <StripGroup key={room.id} room={room} members={members} allIds={allIds} />
          ))}
        </div>
        <div className="flex shrink-0 border-l border-line p-3">
          <MasterStrip />
        </div>
      </div>
    </div>
  );
}
