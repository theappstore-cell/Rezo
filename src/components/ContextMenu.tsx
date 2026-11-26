import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { MouseEvent as ReactMouseEvent } from 'react';
import { BellRing, ClipboardPaste, Copy, MapPinOff, Power, RefreshCw, SlidersHorizontal, SlidersVertical } from 'lucide-react';
import { useStore } from '../lib/store';
import { t } from '../lib/i18n';
import { useUi } from '../lib/ui';
import type { MenuItem } from '../lib/ui';
import type { Speaker } from '../lib/types';

// Right-click menu, drawn at the pointer and kept inside the window.
export function ContextMenu() {
  const menu = useUi((u) => u.menu);
  const close = useUi((u) => u.closeMenu);
  const box = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);

  useLayoutEffect(() => {
    if (!menu || !box.current) return setPos(null);
    const r = box.current.getBoundingClientRect();
    setPos({ x: Math.max(4, Math.min(menu.x, window.innerWidth - r.width - 4)), y: Math.max(4, Math.min(menu.y, window.innerHeight - r.height - 4)) });
    box.current.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const down = (e: PointerEvent) => !box.current?.contains(e.target as Node) && close();
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        const items = [...(box.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
        const i = items.indexOf(document.activeElement as HTMLButtonElement);
        items[(i + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus();
        e.preventDefault();
      }
    };
    window.addEventListener('pointerdown', down, true);
    window.addEventListener('keydown', key);
    window.addEventListener('blur', close);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('pointerdown', down, true);
      window.removeEventListener('keydown', key);
      window.removeEventListener('blur', close);
      window.removeEventListener('resize', close);
    };
  }, [menu, close]);

  if (!menu) return null;
  return (
    <div
      ref={box}
      role="menu"
      aria-label={menu.title}
      className="toast-in fixed z-[60] min-w-[210px] max-w-[300px] overflow-hidden rounded-md border border-rule bg-panel py-1"
      style={{ left: pos?.x ?? menu.x, top: pos?.y ?? menu.y, visibility: pos ? 'visible' : 'hidden', boxShadow: '0 10px 28px rgba(0,0,0,0.3)' }}
    >
      {menu.title && <div className="legend truncate px-3 pb-1 pt-1.5 !text-[11.5px] font-semibold">{menu.title}</div>}
      {menu.items.map((it, i) =>
        it === 'sep' ? (
          <div key={i} className="my-1 border-t border-line" />
        ) : (
          <button
            key={i}
            type="button"
            role="menuitem"
            disabled={it.disabled}
            onClick={() => {
              close();
              it.onClick();
            }}
            className={`flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] outline-none hover:bg-raise focus-visible:bg-raise disabled:opacity-40 ${it.danger ? 'text-bad' : ''}`}
          >
            {it.icon ? <it.icon size={14} className="shrink-0 text-mute" /> : <span className="w-[14px]" />}
            <span className="min-w-0 flex-1 truncate">{it.label}</span>
            {it.hint && <span className="legend shrink-0">{it.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}

// Items shared by every place a speaker is shown (Console strip, Rooms row, Map).
export function speakerMenuItems(sp: Speaker, extra: MenuItem[] = []): MenuItem[] {
  const st = useStore.getState();
  const ui = useUi.getState();
  const copied = st.copied;
  const room = st.s?.speakers.filter((x) => x.roomId === sp.roomId && sp.roomId && x.id !== sp.id) ?? [];
  return [
    { label: sp.active ? t('Éteindre') : t('Allumer'), icon: Power, onClick: () => void st.setSend(sp.id, !sp.active), disabled: !sp.online && !sp.active },
    { label: t('Identifier (bip)'), icon: BellRing, onClick: () => void st.identify(sp.id), disabled: !sp.online && !sp.running },
    ...(sp.active ? [{ label: t('Reconnecter'), icon: RefreshCw, onClick: () => void st.reconnect(sp.id) }] : []),
    'sep',
    { label: t('Copier les réglages'), icon: Copy, onClick: () => st.copySettings(sp.id), hint: 'Ctrl+C' },
    {
      label: copied ? t('Coller les réglages de {name}', { name: copied.name }) : t('Coller les réglages'),
      icon: ClipboardPaste,
      onClick: () => st.pasteSettings([sp.id]),
      disabled: !copied || copied.from === sp.id,
      hint: 'Ctrl+V',
    },
    ...(copied && room.length > 0
      ? [{ label: t('Coller sur toute la pièce'), icon: ClipboardPaste, onClick: () => st.pasteSettings([sp.id, ...room.map((x) => x.id)]) }]
      : []),
    'sep',
    { label: t('Égaliseur…'), icon: SlidersVertical, onClick: () => ui.open({ type: 'eq', id: sp.id }) },
    { label: t('Réglages…'), icon: SlidersHorizontal, onClick: () => ui.open({ type: 'speaker', id: sp.id }) },
    ...extra,
  ];
}

export function removeFromPlanItem(sp: Speaker): MenuItem {
  return { label: t('Retirer du plan'), icon: MapPinOff, onClick: () => useStore.getState().setPlanPosition(sp.id, null) };
}

// onContextMenu handler for an element that shows a speaker; also remembers it as the target of Ctrl+C / Ctrl+V.
export function onSpeakerMenu(sp: Speaker, extra?: MenuItem[]) {
  return (e: ReactMouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    useUi.getState().showMenu({ x: e.clientX, y: e.clientY, title: sp.name, items: speakerMenuItems(sp, extra) });
  };
}
