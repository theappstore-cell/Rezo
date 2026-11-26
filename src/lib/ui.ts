import { create } from 'zustand';
import type { LucideIcon } from 'lucide-react';

export type ModalState =
  | null
  | { type: 'add' }
  | { type: 'settings' }
  | { type: 'shortcuts' }
  | { type: 'speaker'; id: string }
  | { type: 'eq'; id: string }
  | { type: 'room'; id?: string };

export type MenuItem =
  | 'sep'
  | { label: string; icon?: LucideIcon; onClick: () => void; disabled?: boolean; danger?: boolean; hint?: string };

export interface MenuState {
  x: number;
  y: number;
  title?: string;
  items: MenuItem[];
}

interface UiStore {
  modal: ModalState;
  menu: MenuState | null;
  open: (m: Exclude<ModalState, null>) => void;
  close: () => void;
  showMenu: (m: MenuState) => void;
  closeMenu: () => void;
}

export const useUi = create<UiStore>((set) => ({
  modal: null,
  menu: null,
  open: (modal) => set({ modal, menu: null }),
  close: () => set({ modal: null }),
  showMenu: (menu) => set({ menu }),
  closeMenu: () => set({ menu: null }),
}));
