import type { ReactNode } from 'react';
import { useMidiControl } from '../hooks/useMidiLearn';

interface Props {
  label: ReactNode;
  on: boolean;
  onToggle: () => void;
  color?: string;
  title?: string;
  controlId?: string;
  disabled?: boolean;
  className?: string;
}

export function Pad({ label, on, onToggle, color = 'var(--c-ink)', title, controlId, disabled, className = '' }: Props) {
  const { learnProps } = useMidiControl(controlId, (v) => {
    if (v > 0.5) onToggle();
  });
  return (
    <button
      type="button"
      className={`key !h-[26px] !px-1 !text-[11.5px] ${className}`}
      data-on={on}
      style={{ ['--c' as string]: color }}
      title={title}
      aria-pressed={on}
      disabled={disabled}
      onClick={onToggle}
      {...learnProps}
    >
      {label}
    </button>
  );
}

export function NudgeButton({ label, onClick, title }: { label: string; onClick: () => void; title?: string }) {
  return (
    <button type="button" title={title} onClick={onClick} className="key num flex-1 !h-[22px] !px-0 !text-[11px]">
      {label}
    </button>
  );
}
