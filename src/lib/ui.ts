import { create } from 'zustand';

export type ModalState =
  | null
  | { type: 'add' }
  | { type: 'settings' }
  | { type: 'speaker'; id: string }
  | { type: 'room'; id?: string };

interface UiStore {
  modal: ModalState;
  open: (m: Exclude<ModalState, null>) => void;
  close: () => void;
}

export const useUi = create<UiStore>((set) => ({
  modal: null,
  open: (modal) => set({ modal }),
  close: () => set({ modal: null }),
}));
