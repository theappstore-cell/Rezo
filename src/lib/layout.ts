import { useEffect, useRef, useState } from 'react';

// Window width at which the header fits on one row; below it the stacked header is used, whatever the tab.
export const WIDE_HEADER_PX = 1100;
// Below this width the header buttons drop their text labels.
export const NARROW_PX = 540;

export function useWidth(): number {
  const [w, setW] = useState(() => window.innerWidth);
  useEffect(() => {
    const h = () => setW(window.innerWidth);
    window.addEventListener('resize', h);
    return () => window.removeEventListener('resize', h);
  }, []);
  return w;
}

const scrolls = new Map<string, { x: number; y: number }>();

// Gives a scrolling container back its position when its tab is shown again, like tabs of a single-page app.
export function useScrollMemory<T extends HTMLElement>(key: string) {
  const ref = useRef<T | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const saved = scrolls.get(key);
    if (saved) {
      el.scrollLeft = saved.x;
      el.scrollTop = saved.y;
    }
    const save = () => scrolls.set(key, { x: el.scrollLeft, y: el.scrollTop });
    el.addEventListener('scroll', save, { passive: true });
    return () => {
      save();
      el.removeEventListener('scroll', save);
    };
  }, [key]);
  return ref;
}
