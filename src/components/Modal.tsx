import { X } from 'lucide-react';
import { useEffect } from 'react';
import type { ReactNode } from 'react';
import { t } from '../lib/i18n';

interface Props {
  title: string;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}

export function Modal({ title, onClose, children, width = 400 }: Props) {
  useEffect(() => {
    const h = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-40 flex items-end justify-center bg-black/45 p-3 sm:items-center" onPointerDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        role="dialog"
        aria-label={title}
        className="toast-in flex max-h-full w-full flex-col overflow-hidden rounded-md border border-rule bg-panel"
        style={{ maxWidth: width, boxShadow: '0 8px 24px rgba(0,0,0,0.25)' }}
      >
        <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
          <h2 className="text-[14px] font-semibold">{title}</h2>
          <button type="button" aria-label={t('Fermer')} onClick={onClose} className="rounded p-1 text-mute hover:bg-raise hover:text-ink">
            <X size={16} />
          </button>
        </div>
        <div className="overflow-y-auto p-4">{children}</div>
      </div>
    </div>
  );
}

export function Toggle({ on, onChange, label, color = 'var(--c-ink)' }: { on: boolean; onChange: (v: boolean) => void; label: string; color?: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={() => onChange(!on)}
      className="relative h-[20px] w-[36px] shrink-0 rounded-full border transition-colors"
      style={{ background: on ? color : 'var(--c-meter-off)', borderColor: on ? color : 'var(--c-border-strong)' }}
    >
      <span className="absolute top-[2px] h-[14px] w-[14px] rounded-full transition-all" style={{ left: on ? 18 : 2, background: on ? 'var(--c-on-ink)' : 'var(--c-surface)', boxShadow: '0 1px 2px rgba(0,0,0,0.3)' }} />
    </button>
  );
}
