import { useEffect } from 'react';
import { useStore } from '../lib/store';
import { t } from '../lib/i18n';
import { useUi } from '../lib/ui';
import type { ViewId } from '../lib/types';
import { mediaKey } from './MediaBar';
import { Modal } from './Modal';

const VIEWS: ViewId[] = ['rooms', 'plan', 'console'];
export const ZOOM_STEP = 0.1;

let pointer = { x: -1, y: -1 };

// The speaker under the mouse (or holding the focus): the target of Ctrl+C / Ctrl+V and I.
function speakerUnderPointer(): string | null {
  const at = pointer.x >= 0 ? document.elementFromPoint(pointer.x, pointer.y) : null;
  const el = at?.closest<HTMLElement>('[data-speaker]') ?? (document.activeElement as HTMLElement | null)?.closest<HTMLElement>('[data-speaker]');
  return el?.dataset.speaker ?? null;
}

const typing = (el: EventTarget | null) =>
  el instanceof HTMLElement && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName));

const onControl = (el: EventTarget | null) =>
  el instanceof HTMLElement && !!el.closest('button, a, [role="slider"], [role="switch"], [role="menuitem"]');

export function useShortcuts() {
  useEffect(() => {
    const move = (e: PointerEvent) => (pointer = { x: e.clientX, y: e.clientY });
    const key = (e: KeyboardEvent) => {
      if (e.defaultPrevented || typing(e.target)) return;
      const st = useStore.getState();
      const ui = useUi.getState();
      const s = st.s;
      if (!s) return;
      const ctrl = e.ctrlKey || e.metaKey;
      const done = () => e.preventDefault();

      if (ctrl && !e.altKey) {
        const k = e.key.toLowerCase();
        if (k === 'z' && !e.shiftKey) return done(), st.undo();
        if (k === 'y' || (k === 'z' && e.shiftKey)) return done(), st.redo();
        if (k === 'c' && !window.getSelection()?.toString()) {
          const id = speakerUnderPointer();
          if (id) return done(), st.copySettings(id);
          return;
        }
        if (k === 'v') {
          const id = speakerUnderPointer();
          if (id && st.copied) return done(), st.pasteSettings([id]);
          return;
        }
        const tab = ['Digit1', 'Digit2', 'Digit3', 'Numpad1', 'Numpad2', 'Numpad3'].indexOf(e.code);
        if (tab >= 0) return done(), st.setView(VIEWS[tab % 3]);
        if (e.key === '+' || e.key === '=' || e.code === 'NumpadAdd') return done(), void st.setZoom(s.uiZoom + ZOOM_STEP);
        if (e.key === '-' || e.code === 'NumpadSubtract') return done(), void st.setZoom(s.uiZoom - ZOOM_STEP);
        if (e.key === '0' || e.code === 'Digit0' || e.code === 'Numpad0') return done(), void st.setZoom(1);
        if (e.key === 'ArrowRight') return done(), void mediaKey('next');
        if (e.key === 'ArrowLeft') return done(), void mediaKey('previous');
        return;
      }
      if (e.altKey || ui.modal || ui.menu) return;

      if (e.key === '?' || e.key === 'F1') return done(), ui.open({ type: 'shortcuts' });
      if ((e.key === ' ' || e.code === 'Space') && !onControl(e.target)) return done(), void mediaKey('toggle');
      if (e.key.toLowerCase() === 'm') return done(), st.setMasterMute(!s.masterMute);
      if (e.key.toLowerCase() === 'i') {
        const id = speakerUnderPointer();
        if (id) return done(), void st.identify(id);
      }
      if (e.key === '+' || e.code === 'NumpadAdd') return done(), st.setMaster(Math.min(6, Math.round((s.masterDb + 1) * 2) / 2));
      if (e.key === '-' || e.code === 'NumpadSubtract') return done(), st.setMaster(Math.max(-90, Math.round((s.masterDb - 1) * 2) / 2));
      const digit = /^(Digit|Numpad)([1-9])$/.exec(e.code);
      // By key position, so the top row works on AZERTY keyboards too (where the digits need Shift).
      if (digit) {
        const sc = s.scenes[Number(digit[2]) - 1];
        if (sc) return done(), void st.applyScene(sc.id);
      }
    };
    window.addEventListener('pointermove', move, { passive: true });
    window.addEventListener('keydown', key);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('keydown', key);
    };
  }, []);
}

const LIST: Array<[string, string]> = [
  ['Ctrl+Z', 'Annuler la dernière modification'],
  ['Ctrl+Y', 'Rétablir'],
  ['Ctrl+1 / 2 / 3', 'Onglets Pièces, Plan, Console'],
  ['Espace', 'Lecture / pause de la musique du PC'],
  ['Ctrl+→ / Ctrl+←', 'Piste suivante / précédente'],
  ['M', 'Couper / rétablir toutes les enceintes'],
  ['+ / −', 'Volume général ±1 dB'],
  ['1 … 9', 'Rappeler la scène n°1 à 9'],
  ['I', "Identifier l'enceinte sous la souris (bip)"],
  ['Ctrl+C / Ctrl+V', "Copier / coller les réglages de l'enceinte sous la souris"],
  ['Ctrl++ / Ctrl+− / Ctrl+0', "Taille de l'interface"],
  ['Clic droit', "Menu d'une enceinte"],
  ['? ou F1', 'Cette aide'],
];

export function ShortcutsModal() {
  const close = useUi((u) => u.close);
  return (
    <Modal title={t('Raccourcis clavier')} onClose={close} width={460}>
      <table className="w-full text-[12.5px]">
        <tbody>
          {LIST.map(([k, what]) => (
            <tr key={k} className="border-t border-line first:border-t-0">
              <td className="whitespace-nowrap py-1.5 pr-4"><kbd className="rounded border border-rule bg-panel2 px-1.5 py-0.5 font-mono text-[11.5px]">{t(k)}</kbd></td>
              <td className="py-1.5 text-mute">{t(what)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="mt-3 text-[11px] leading-snug text-dim">{t("Les raccourcis agissent quand la fenêtre de Rezo a le focus, pas dans un champ de saisie.")}</p>
    </Modal>
  );
}
