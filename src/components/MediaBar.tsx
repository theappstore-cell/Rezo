import { useEffect, useRef, useState } from 'react';
import type { PointerEvent } from 'react';
import { Pause, Play, SkipBack, SkipForward } from 'lucide-react';
import { api } from '../lib/api';
import { t } from '../lib/i18n';
import { isWindows } from '../lib/platform';
import type { MediaState } from '../lib/types';

const POLL_MS = 1500;
const TICK_MS = 250;
export const MEDIA_ACCENT = 'var(--c-ok)';

export const clock = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};

export type MediaCmd = 'toggle' | 'next' | 'previous' | 'seek';

// Polls the current Windows media session while the window is visible and moves the position between polls.
export function useMedia() {
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
    const ext = () => void refresh();
    window.addEventListener('rezo-media-changed', ext);
    return () => {
      window.clearInterval(poll);
      window.clearInterval(tick);
      window.removeEventListener('rezo-media-changed', ext);
    };
  }, []);

  const send = async (cmd: MediaCmd, positionMs?: number) => {
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

// Media keys from the keyboard shortcuts: same command, then every media bar refreshes.
export async function mediaKey(cmd: 'toggle' | 'next' | 'previous') {
  try {
    await api.mediaCommand(cmd);
  } catch {}
  window.setTimeout(() => window.dispatchEvent(new Event('rezo-media-changed')), 350);
}

// Seek bar: thick track, visible thumb, drag to scrub with the target time shown while dragging.
export function Gauge({ pos, duration, canSeek, onSeek, size = 'md', light = false }: {
  pos: number;
  duration: number;
  canSeek: boolean;
  onSeek: (ms: number) => void;
  size?: 'sm' | 'md';
  light?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const [hover, setHover] = useState<number | null>(null);
  const shown = drag ?? pos;
  const pct = duration > 0 ? Math.min(100, (shown / duration) * 100) : 0;
  const at = (e: PointerEvent<HTMLDivElement>) => {
    const r = ref.current!.getBoundingClientRect();
    return Math.round(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * duration);
  };
  const track = size === 'sm' ? 'h-[5px]' : 'h-[6px]';
  const tip = drag ?? hover;
  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={canSeek ? 0 : -1}
      aria-label={t('Position dans le morceau')}
      aria-valuemin={0}
      aria-valuemax={Math.round(duration / 1000)}
      aria-valuenow={Math.round(shown / 1000)}
      aria-valuetext={`${clock(shown)} / ${clock(duration)}`}
      aria-disabled={!canSeek}
      onPointerDown={(e) => {
        if (!canSeek || e.button !== 0 || duration <= 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        setDrag(at(e));
      }}
      onPointerMove={(e) => {
        if (!canSeek || duration <= 0) return;
        const v = at(e);
        if (drag !== null) setDrag(v);
        else setHover(v);
      }}
      onPointerUp={() => {
        if (drag !== null) onSeek(drag);
        setDrag(null);
      }}
      onPointerCancel={() => setDrag(null)}
      onPointerLeave={() => setHover(null)}
      onKeyDown={(e) => {
        if (!canSeek) return;
        if (e.key === 'ArrowRight') onSeek(Math.min(duration, pos + 5000));
        else if (e.key === 'ArrowLeft') onSeek(Math.max(0, pos - 5000));
        else return;
        e.preventDefault();
      }}
      className={`group relative flex w-full touch-none items-center ${size === 'sm' ? 'h-3.5' : 'h-4'} ${canSeek ? 'cursor-pointer' : ''}`}
    >
      <div className={`relative w-full ${track} rounded-full transition-[height] group-hover:h-[8px]`} style={{ background: light ? 'rgba(255,255,255,0.22)' : 'var(--c-meter-off)' }}>
        <div className="absolute inset-y-0 left-0 rounded-full" style={{ width: `${pct}%`, background: MEDIA_ACCENT }} />
        {canSeek && duration > 0 && (
          <div
            className={`absolute top-1/2 h-[12px] w-[12px] -translate-x-1/2 -translate-y-1/2 rounded-full transition-opacity ${drag !== null ? 'opacity-100' : 'opacity-80 group-hover:opacity-100'}`}
            style={{ left: `${pct}%`, background: light ? '#fff' : 'var(--c-ink)', boxShadow: '0 1px 3px rgba(0,0,0,0.35)' }}
          />
        )}
      </div>
      {tip !== null && duration > 0 && (
        <div className="num pointer-events-none absolute -top-6 -translate-x-1/2 rounded bg-ink px-1.5 py-0.5 text-[11px] text-onsig" style={{ left: `${(tip / duration) * 100}%` }}>
          {clock(tip)}
        </div>
      )}
    </div>
  );
}

export function MediaControls({ m, send, size = 'md', light = false }: { m: MediaState | null; send: (c: MediaCmd) => void; size?: 'sm' | 'md'; light?: boolean }) {
  const has = !!m?.available;
  const small = size === 'sm';
  const side = `grid place-items-center rounded-full transition disabled:opacity-35 ${small ? 'h-7 w-7' : 'h-8 w-8'} ${light ? 'text-white hover:bg-white/15' : 'text-ink hover:bg-raise'}`;
  const main = `grid place-items-center rounded-full transition disabled:opacity-40 ${small ? 'h-8 w-8' : 'h-9 w-9'}`;
  return (
    <div className="flex shrink-0 items-center gap-1">
      <button type="button" className={side} disabled={!has || !m!.canPrevious} onClick={() => send('previous')} aria-label={t('Précédent')} title={t('Précédent')}>
        <SkipBack size={small ? 15 : 16} fill="currentColor" />
      </button>
      <button type="button" className={main} style={{ background: light ? '#fff' : 'var(--c-ink)', color: light ? '#111' : 'var(--c-on-ink)' }}
        disabled={!has} onClick={() => send('toggle')} aria-label={m?.playing ? t('Pause') : t('Lecture')} title={m?.playing ? t('Pause') : t('Lecture')}>
        {m?.playing ? <Pause size={small ? 15 : 17} fill="currentColor" /> : <Play size={small ? 15 : 17} fill="currentColor" className="translate-x-[1px]" />}
      </button>
      <button type="button" className={side} disabled={!has || !m!.canNext} onClick={() => send('next')} aria-label={t('Suivant')} title={t('Suivant')}>
        <SkipForward size={small ? 15 : 16} fill="currentColor" />
      </button>
    </div>
  );
}

// Play/pause, previous, next and a seek bar for the music or sound currently playing on the PC.
// `wide`: bottom-centre bar of the Console view; otherwise the compact control at the top right of Rooms and Map.
export function MediaBar({ wide = false }: { wide?: boolean }) {
  const { state, pos, send } = useMedia();
  if (!isWindows) return null;

  const m = state;
  const has = !!m?.available;
  const title = has ? m!.title || m!.app : t('Aucune lecture en cours');
  const artist = has ? m!.artist : '';
  const dur = m?.durationMs ?? 0;
  const gauge = <Gauge pos={pos} duration={dur} canSeek={has && m!.canSeek} onSeek={(ms) => void send('seek', ms)} size={wide ? 'md' : 'sm'} />;
  const times = (
    <span className="num shrink-0 text-[11.5px] font-medium text-mute">{has ? clock(pos) : '0:00'} / {has && dur ? clock(dur) : '0:00'}</span>
  );

  if (wide) {
    return (
      <div className="flex w-full max-w-[560px] items-center gap-3" title={has ? `${[m!.title, m!.artist].filter(Boolean).join(' · ')} — ${m!.app}` : title}>
        <MediaControls m={m} send={(c) => void send(c)} />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="truncate text-[12.5px] font-semibold text-ink">{title}</span>
            {artist && <span className="truncate text-[12px] text-mute">{artist}</span>}
            <span className="ml-auto">{times}</span>
          </div>
          {gauge}
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-w-0 max-w-[320px] items-center gap-2" title={has ? `${[m!.title, m!.artist].filter(Boolean).join(' · ')} — ${m!.app}` : title}>
      <MediaControls m={m} send={(c) => void send(c)} size="sm" />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-1.5">
          <span className="truncate text-[12px] font-semibold leading-tight text-ink">{title}</span>
          <span className="ml-auto">{times}</span>
        </div>
        {gauge}
      </div>
    </div>
  );
}
