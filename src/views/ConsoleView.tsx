import { useState } from 'react';
import { Keyboard, Link2, Trash2 } from 'lucide-react';
import { useMidiLearn } from '../hooks/useMidiLearn';
import { roomLabel, speakersOf, useStore } from '../lib/store';
import { MasterStrip, RoomStrip, SpeakerStrip } from '../components/ChannelStrip';
import { t } from '../lib/i18n';
import { useScrollMemory } from '../lib/layout';

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
  const groups = s.rooms.map((room) => ({ room, members: speakersOf(s, room.id) })).filter((g) => g.members.length > 0);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-line px-3 py-2">
        <button type="button" className="key flex items-center gap-1.5" data-on={midi.learn} aria-pressed={midi.learn} onClick={midi.toggleLearn} disabled={!midi.supported}>
          <Keyboard size={14} />
          MIDI Learn
        </button>
        <button type="button" className="key flex items-center gap-1.5" style={{ ['--c' as string]: 'var(--c-ok)' }} data-on={grouped} aria-pressed={grouped} disabled={syncing || (!grouped && activeCount < 2)}
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
          {s.speakers.length === 0 && <div className="m-auto text-center text-[13px] text-mute">{t('Aucune enceinte. Ajoutez-en depuis la barre du haut.')}</div>}
          {loose.map((sp) => (
            <SpeakerStrip key={sp.id} sp={sp} />
          ))}
          {groups.map(({ room, members }) => (
            <div key={room.id} className="flex shrink-0 gap-1.5 border-r border-line pr-2" aria-label={t('Pièce {name}', { name: roomLabel(room) })}>
              {members.map((sp) => (
                <SpeakerStrip key={sp.id} sp={sp} room={room} />
              ))}
              <RoomStrip room={room} members={members} />
            </div>
          ))}
        </div>
        <div className="flex shrink-0 border-l border-line p-3">
          <MasterStrip />
        </div>
      </div>
    </div>
  );
}
