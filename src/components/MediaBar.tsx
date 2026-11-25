import { useEffect, useRef, useState } from 'react';
import type { PointerEvent } from 'react';
import { Pause, Play, SkipBack, SkipForward } from 'lucide-react';
import { api } from '../lib/api';
import { t } from '../lib/i18n';
import { isWindows } from '../lib/platform';
import type { MediaState } from '../lib/types';

const POLL_MS = 1500;
const TICK_MS = 250;

const clock = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};

// Polls the current Windows media session while the window is visible and moves the position between polls.
function useMedia() {
  const [state, setState] = useState<MediaState | null>(null);
  const [pos, setPos] = useState(0);
  const base = useRef({ at: 0, ms: 0, playing: false, max: 0 });

  const refresh = async () => {
    try {
      const m = await api.mediaState();
      base.current = { at: performance.now(), ms: m.positionMs, playing: m.playing, max: m.durationMs };
      setState(m);
      setPos(m.positionMs);
    } catch {
      setState(null);
    }
  };

  useEffect(() => {
    if (!isWindows) return;
    void refresh();
    const poll = window.setInterval(() => document.visibilityState === 'visible' && void refresh(), POLL_MS);
    const tick = window.setInterval(() => {
      const b = base.current;
      if (b.playing) setPos(Math.min(b.max || Infinity, b.ms + (performance.now() - b.at)));
    }, TICK_MS);
    return () => {
      window.clearInterval(poll);
      window.clearInterval(tick);
    };
  }, []);

  const send = async (cmd: 'toggle' | 'next' | 'previous' | 'seek', positionMs?: number) => {
    if (cmd === 'toggle') base.current = { ...base.current, playing: !base.current.playing, ms: pos, at: performance.now() };
    if (cmd === 'seek' && positionMs !== undefined) {
      base.current = { ...base.current, ms: positionMs, at: performance.now() };
      setPos(positionMs);
    }
    try {
      await api.mediaCommand(cmd, positionMs);
    } catch {}
    window.setTimeout(() => void refresh(), 350);
  };

  return { state, pos, send };
}

function Gauge({ pos, duration, canSeek, onSeek, thin }: { pos: number; duration: number; canSeek: boolean; onSeek: (ms: number) => void; thin?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const pct = duration > 0 ? Math.min(100, (pos / duration) * 100) : 0;
  const seek = (e: PointerEvent<HTMLDivElement>) => {
    if (!canSeek || !ref.current) return;
    const r = ref.current.getBoundingClientRect();
    onSeek(Math.round(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * duration));
  };
  return (
    <div
      ref={ref}
      role="progressbar"
      aria-label={t('Position dans le morceau')}
      aria-valuemin={0}
      aria-valuemax={Math.round(duration / 1000)}
      aria-valuenow={Math.round(pos / 1000)}
      onPointerDown={seek}
      className={`group relative flex w-full items-center ${thin ? 'h-2' : 'h-3'} ${canSeek ? 'cursor-pointer' : ''}`}
      title={canSeek ? t('Cliquez pour avancer ou reculer') : undefined}
    >
      <div className="h-[4px] w-full overflow-hidden rounded-full" style={{ background: 'var(--c-meter-off)' }}>
        <div className="h-full rounded-full bg-ink group-hover:bg-ok" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

// Play/pause, previous, next and a seek gauge for the music or sound currently playing on the PC.
// `wide`: bottom-centre bar of the Console view; otherwise the compact control at the top right of Rooms and Map.
export function MediaBar({ wide = false }: { wide?: boolean }) {
  const { state, pos, send } = useMedia();
  if (!isWindows) return null;

  const m = state;
  const has = !!m?.available;
  const label = has ? [m!.title, m!.artist].filter(Boolean).join(' · ') || m!.app : t('Aucune lecture en cours');
  const btn = 'grid place-items-center rounded-full text-ink transition hover:bg-raise disabled:opacity-35';
  const controls = (
    <div className="flex shrink-0 items-center gap-0.5">
      <button type="button" className={`${btn} h-7 w-7`} disabled={!has || !m!.canPrevious} onClick={() => void send('previous')} aria-label={t('Précédent')} title={t('Précédent')}>
        <SkipBack size={14} />
      </button>
      <button type="button" className={`${btn} h-8 w-8 border border-rule`} disabled={!has} onClick={() => void send('toggle')} aria-label={m?.playing ? t('Pause') : t('Lecture')} title={m?.playing ? t('Pause') : t('Lecture')}>
        {m?.playing ? <Pause size={15} /> : <Play size={15} />}
      </button>
      <button type="button" className={`${btn} h-7 w-7`} disabled={!has || !m!.canNext} onClick={() => void send('next')} aria-label={t('Suivant')} title={t('Suivant')}>
        <SkipForward size={14} />
      </button>
    </div>
  );

  if (wide) {
    return (
      <div className="flex w-full max-w-[460px] items-center gap-2.5" title={has ? `${label} — ${m!.app}` : label}>
        {controls}
        <span className="num w-9 shrink-0 text-right text-[11px] text-dim">{has ? clock(pos) : '0:00'}</span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[11.5px] leading-tight text-ink">{label}</div>
          <Gauge pos={pos} duration={m?.durationMs ?? 0} canSeek={has && m!.canSeek} onSeek={(ms) => void send('seek', ms)} thin />
        </div>
        <span className="num w-9 shrink-0 text-[11px] text-dim">{has && m!.durationMs ? clock(m!.durationMs) : '0:00'}</span>
      </div>
    );
  }

  return (
    <div className="flex min-w-0 max-w-[230px] items-center gap-1.5" title={has ? `${label} — ${m!.app} (${clock(pos)} / ${clock(m!.durationMs)})` : label}>
      {controls}
      <div className="min-w-0 flex-1">
        <div className="truncate text-[11px] leading-tight text-mute">{label}</div>
        <Gauge pos={pos} duration={m?.durationMs ?? 0} canSeek={has && m!.canSeek} onSeek={(ms) => void send('seek', ms)} thin />
      </div>
    </div>
  );
}
