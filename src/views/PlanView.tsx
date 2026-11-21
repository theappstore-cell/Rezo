import { Pencil, Plus, Trash2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import type { PointerEvent } from 'react';
import { roomLabel, speakersOf, useStore } from '../lib/store';
import { api } from '../lib/api';
import { useUi } from '../lib/ui';
import type { Room, Speaker } from '../lib/types';
import { t } from '../lib/i18n';

interface DragState {
  id: string;
  name: string;
  x: number;
  y: number;
}

const NONE = '__none';

export function PlanView() {
  const s = useStore((st) => st.s);
  const moveSpeaker = useStore((st) => st.moveSpeaker);
  const setRoomSend = useStore((st) => st.setRoomSend);
  const open = useUi((u) => u.open);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const [confirmPrune, setConfirmPrune] = useState(false);
  useEffect(() => {
    if (!confirmPrune) return;
    const h = setTimeout(() => setConfirmPrune(false), 4000);
    return () => clearTimeout(h);
  }, [confirmPrune]);
  const start = useRef<{ id: string; x: number; y: number; moved: boolean } | null>(null);

  if (!s) return null;

  const dropTarget = (x: number, y: number) =>
    document.elementFromPoint(x, y)?.closest('[data-room-drop]')?.getAttribute('data-room-drop') ?? null;

  const onDown = (e: PointerEvent<HTMLButtonElement>, sp: Speaker) => {
    if (e.button !== 0) return;
    try { e.currentTarget.setPointerCapture(e.pointerId); } catch {}
    start.current = { id: sp.id, x: e.clientX, y: e.clientY, moved: false };
    setDrag({ id: sp.id, name: sp.name, x: e.clientX, y: e.clientY });
  };
  const onMove = (e: PointerEvent<HTMLButtonElement>) => {
    const st = start.current;
    if (!st) return;
    if (!st.moved && Math.hypot(e.clientX - st.x, e.clientY - st.y) > 5) st.moved = true;
    if (st.moved) {
      setDrag((d) => d && { ...d, x: e.clientX, y: e.clientY });
      setOver(dropTarget(e.clientX, e.clientY));
    }
  };
  const onUp = (e: PointerEvent<HTMLButtonElement>) => {
    const st = start.current;
    start.current = null;
    setDrag(null);
    setOver(null);
    if (!st) return;
    if (st.moved) {
      const target = dropTarget(e.clientX, e.clientY);
      if (target !== null) moveSpeaker(st.id, target === NONE ? '' : target);
    } else {
      open({ type: 'speaker', id: st.id });
    }
  };

  const chip = (sp: Speaker) => (
    <button
      key={sp.id}
      type="button"
      onPointerDown={(e) => onDown(e, sp)}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={() => { start.current = null; setDrag(null); setOver(null); }}
      title={t('Glisser vers une autre pièce · cliquer pour régler')}
      className="flex max-w-full cursor-grab touch-none items-center gap-1.5 rounded border border-line bg-panel2 px-2 py-1 text-[12.5px] active:cursor-grabbing"
      style={{ opacity: drag?.id === sp.id ? 0.35 : sp.online || sp.running ? 1 : 0.55 }}
    >
      <span className="h-[8px] w-[8px] shrink-0 rounded-full" style={{ background: sp.streaming ? 'var(--c-ok)' : sp.active ? 'var(--c-warn)' : 'var(--c-border-strong)' }} />
      <span className="truncate">{sp.name}</span>
    </button>
  );

  const box = (room: Room | null, members: Speaker[]) => {
    const id = room?.id ?? NONE;
    const activeN = members.filter((m) => m.active).length;
    const anyOn = activeN > 0;
    const hot = over === id && drag !== null;
    return (
      <div
        key={id}
        data-room-drop={id}
        className={`flex min-h-[120px] flex-col gap-2.5 rounded-md bg-panel p-2.5 ${hot ? 'border-2 border-ink bg-raise' : room ? 'border border-line' : 'border border-dashed border-rule'}`}
      >
        <div className="flex items-center gap-2">
          {room && <span className="h-[10px] w-[10px] shrink-0 rounded-full" style={{ background: room.color }} />}
          <h2 className="min-w-0 flex-1 truncate text-[14px] font-semibold">{room ? roomLabel(room) : t('Sans pièce')}</h2>
          {room && (
            <button type="button" aria-label={t('Modifier la pièce')} title={t('Modifier ou supprimer la pièce')} onClick={() => open({ type: 'room', id: room.id })}
              className="rounded p-1 text-dim hover:bg-raise hover:text-ink">
              <Pencil size={13} />
            </button>
          )}
          {room && (
            <button type="button" className="key !h-[24px] !px-2 !text-[11.5px]" data-on={anyOn} style={{ ['--c' as string]: 'var(--c-ok)' }}
              disabled={members.length === 0} aria-pressed={anyOn} aria-label={t('Allumer ou éteindre {name}', { name: roomLabel(room) })}
              onClick={() => void setRoomSend(room.id, !anyOn)}>{anyOn ? t('Allumée') : t('Éteinte')}</button>
          )}
        </div>
        <div className="flex flex-1 flex-wrap content-start gap-1.5">
          {members.map(chip)}
          {members.length === 0 && <span className="legend m-auto">{t('Déposez une enceinte ici')}</span>}
        </div>
        <div className="legend num">{t(activeN > 1 ? '{n} / {m} actives' : '{n} / {m} active', { n: activeN, m: members.length })}</div>
      </div>
    );
  };

  const unassigned = s.speakers.filter((sp) => !s.rooms.some((r) => r.id === sp.roomId));
  const empty = s.rooms.filter((r) => speakersOf(s, r.id).length === 0);
  const prune = async () => {
    if (!confirmPrune) return setConfirmPrune(true);
    setConfirmPrune(false);
    for (const r of empty) await api.deleteRoom(r.id);
    useStore.getState().apply(await api.getState());
  };

  return (
    <div className="h-full overflow-y-auto p-3">
      {empty.length > 0 && (
        <div className="mb-3 flex items-center gap-2">
          <p className="legend min-w-0 flex-1 truncate">{t('Glissez les enceintes entre les pièces ; le crayon modifie ou supprime une pièce.')}</p>
          <button type="button" onClick={() => void prune()} data-on={confirmPrune} style={{ ['--c' as string]: 'var(--c-bad)' }}
            className="key flex shrink-0 items-center gap-1.5 !h-7 !text-[12px]" title={t('Supprime les pièces sans enceinte, dont celles créées au premier lancement (Salon, Cuisine…). Les enceintes ne sont pas touchées.')}>
            <Trash2 size={13} />
            {confirmPrune ? t('Supprimer {n} pièce(s) ?', { n: empty.length }) : t('Supprimer les pièces vides')}
          </button>
        </div>
      )}
      <div className="grid grid-cols-2 gap-3 [@media(min-width:900px)]:grid-cols-3">
        {s.rooms.map((room) => box(room, speakersOf(s, room.id)))}
        {box(null, unassigned)}
        <button type="button" onClick={() => open({ type: 'room' })}
          className="flex min-h-[120px] flex-col items-center justify-center gap-1.5 rounded-md border border-dashed border-rule text-dim transition hover:bg-raise hover:text-ink">
          <Plus size={18} />
          <span className="text-[12.5px]">{t('Nouvelle pièce')}</span>
        </button>
      </div>
      {drag && (
        <div className="pointer-events-none fixed z-50 rounded bg-ink px-2 py-1 text-[12px] font-medium text-onsig" style={{ left: drag.x + 12, top: drag.y + 12 }}>
          {drag.name}
        </div>
      )}
    </div>
  );
}
