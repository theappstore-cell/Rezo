import { useRef, useState } from 'react';
import type { PointerEvent } from 'react';

// Drag-to-reorder for a list of items laid out in a row ('x') or a column ('y'). The handle starts the drag; the
// list renders `order` so the items move under the pointer, and `onDrop` gets the new order when it changed.
export function useReorder(ids: string[], axis: 'x' | 'y', onDrop: (ids: string[]) => void) {
  const [order, setOrder] = useState<string[] | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const container = useRef<HTMLElement | null>(null);

  const targetIndex = (id: string, pointer: number, current: string[]) => {
    const els = [...(container.current?.querySelectorAll<HTMLElement>(':scope > [data-reorder]') ?? [])];
    let idx = 0;
    for (const el of els) {
      const other = el.dataset.reorder!;
      if (other === id) continue;
      const r = el.getBoundingClientRect();
      const mid = axis === 'x' ? r.left + r.width / 2 : r.top + r.height / 2;
      if (pointer > mid) idx++;
    }
    const rest = current.filter((x) => x !== id);
    rest.splice(idx, 0, id);
    return rest;
  };

  const handle = (id: string) => ({
    onPointerDown(e: PointerEvent<HTMLElement>) {
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      e.currentTarget.setPointerCapture(e.pointerId);
      setDragging(id);
      setOrder(ids);
    },
    onPointerMove(e: PointerEvent<HTMLElement>) {
      if (dragging !== id || !order) return;
      const next = targetIndex(id, axis === 'x' ? e.clientX : e.clientY, order);
      if (next.join() !== order.join()) setOrder(next);
    },
    onPointerUp() {
      if (dragging === id && order && order.join() !== ids.join()) onDrop(order);
      setDragging(null);
      setOrder(null);
    },
    onPointerCancel() {
      setDragging(null);
      setOrder(null);
    },
    style: { cursor: dragging === id ? 'grabbing' : 'grab', touchAction: 'none' as const },
  });

  return {
    order: order ?? ids,
    dragging,
    container,
    handle,
    item: (id: string) => ({ 'data-reorder': id, style: dragging === id ? { opacity: 0.6 } : undefined }),
  };
}
