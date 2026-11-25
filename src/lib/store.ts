import { create } from 'zustand';
import { api, on } from './api';
import { loadMidiMap } from './midi';
import { pushMeters } from './meterBus';
import { t } from './i18n';
import type { ChannelParams, FullState, Meters, Room, Source, Speaker, ViewId } from './types';

const pending = new Map<string, () => Promise<unknown>>();
let scheduled = false;
function enqueue(key: string, fn: () => Promise<unknown>) {
  pending.set(key, fn);
  if (scheduled) return;
  scheduled = true;
  requestAnimationFrame(() => {
    scheduled = false;
    const items = [...pending.values()];
    pending.clear();
    for (const f of items) f().catch((e) => console.warn('IPC', e));
  });
}

let lastLocalEdit = 0;
const touch = () => {
  lastLocalEdit = Date.now();
};

interface Store {
  s: FullState | null;
  view: ViewId;
  toast: string | null;
  scanning: boolean;
  init: () => Promise<void>;
  apply: (next: FullState) => void;
  setView: (v: ViewId) => void;
  say: (msg: string) => void;

  setParam: <K extends keyof ChannelParams>(id: string, key: K, value: ChannelParams[K]) => void;
  setRoomParam: <K extends 'volumeDb' | 'mute' | 'solo'>(id: string, key: K, value: Room[K]) => void;
  setMaster: (db: number) => void;
  setLocalPc: (on: boolean) => void;
  setSource: (src: Source) => void;
  setSend: (id: string, on: boolean) => Promise<void>;
  setRoomSend: (id: string, on: boolean) => Promise<void>;
  setDeviceVolume: (id: string, volume: number) => void;
  setUseLineIn: (id: string, on: boolean) => Promise<void>;
  sync: () => Promise<void>;
  applyScene: (id: string) => Promise<void>;
  scan: () => Promise<void>;
  moveSpeaker: (id: string, roomId: string) => void;
}

const isView = (v: string | null): v is ViewId => v === 'rooms' || v === 'plan' || v === 'console';

const savedView = (): ViewId => {
  const h = location.hash.replace('#', '');
  if (isView(h)) return h;
  const q = new URLSearchParams(location.search).get('view');
  if (q === 'plan' || q === 'console' || q === 'rooms') return q;
  try {
    const v = localStorage.getItem('rezo.view');
    if (v === 'plan' || v === 'console' || v === 'rooms') return v;
  } catch {}
  return 'rooms';
};

export const useStore = create<Store>((set, get) => {
  const upd = (fn: (s: FullState) => FullState) => set((st) => (st.s ? { s: fn(st.s) } : {}));
  return {
  s: null,
  view: savedView(),
  toast: null,
  scanning: false,

  async init() {
    get().apply(await api.getState());
    loadMidiMap(get().s?.midiMap ?? []);
    await on<FullState>('state', (st) => get().apply(st));
    await on<Meters>('meters', pushMeters);
    // Tabs behave like routes of a single-page app: the address follows the tab and Back/Forward switch tabs.
    if (location.hash.replace('#', '') !== get().view) history.replaceState(null, '', `#${get().view}`);
    window.addEventListener('hashchange', () => {
      const h = location.hash.replace('#', '');
      if (isView(h) && h !== get().view) get().setView(h);
    });
  },

  apply(next) {
    const cur = get().s;
    if (cur && Date.now() - lastLocalEdit < 600) {
      next = {
        ...next,
        masterDb: cur.masterDb,
        speakers: next.speakers.map((sp) => {
          const old = cur.speakers.find((x) => x.id === sp.id);
          return old ? { ...sp, params: old.params, deviceVolume: old.deviceVolume, receiverVolume: old.receiverVolume } : sp;
        }),
        rooms: next.rooms.map((r) => {
          const o = cur.rooms.find((x) => x.id === r.id);
          return o ? { ...r, volumeDb: o.volumeDb, mute: o.mute, solo: o.solo } : r;
        }),
      };
    }
    if (cur) {
      for (const sp of next.speakers) {
        const old = cur.speakers.find((x) => x.id === sp.id);
        if (sp.error && sp.error !== old?.error) set({ toast: `${sp.name} : ${sp.error}` });
      }
    }
    set({ s: next });
  },

  setView(v) {
    try { localStorage.setItem('rezo.view', v); } catch {}
    set({ view: v });
    if (location.hash !== `#${v}`) location.hash = v;
  },

  say(msg) {
    set({ toast: msg });
  },

  setParam(id, key, value) {
    touch();
    upd((s) => ({ ...s, speakers: s.speakers.map((sp: Speaker) => (sp.id === id ? { ...sp, params: { ...sp.params, [key]: value } } : sp)) }));
    enqueue(`p:${id}:${key}`, () => api.setParam(id, key, value));
  },

  setRoomParam(id, key, value) {
    touch();
    upd((s) => ({ ...s, rooms: s.rooms.map((r) => (r.id === id ? { ...r, [key]: value } : r)) }));
    enqueue(`r:${id}:${key}`, () => api.setRoomParam(id, key, value));
  },

  setMaster(db) {
    touch();
    upd((s) => ({ ...s, masterDb: db }));
    enqueue('master', () => api.setMaster(db));
  },

  setLocalPc(on) {
    upd((s) => ({ ...s, localPc: on }));
    void api.setLocalPc(on);
  },

  setSource(src) {
    upd((s) => ({ ...s, source: src }));
    void api.setSource(src);
  },

  async setSend(id, on) {
    upd((s) => ({ ...s, speakers: s.speakers.map((sp) => (sp.id === id ? { ...sp, active: on } : sp)) }));
    try {
      get().apply(await api.setSend(id, on));
    } catch (e) {
      get().say(String(e));
    }
  },

  async setRoomSend(id, on) {
    try {
      get().apply(await api.setRoomSend(id, on));
    } catch (e) {
      get().say(String(e));
    }
  },

  setDeviceVolume(id, volume) {
    touch();
    upd((s) => ({ ...s, speakers: s.speakers.map((sp) => (sp.id === id ? { ...sp, deviceVolume: volume, receiverVolume: volume } : sp)) }));
    enqueue(`v:${id}`, () => api.setDeviceVolume(id, volume));
  },

  async setUseLineIn(id, on) {
    upd((s) => ({ ...s, speakers: s.speakers.map((sp) => (sp.id === id ? { ...sp, useLineIn: on } : sp)) }));
    try {
      get().apply(await api.setUseLineIn(id, on));
    } catch (e) {
      get().say(String(e));
    }
  },

  async sync() {
    try {
      const { report, state } = await api.syncSpeakers();
      get().apply(state);
      const parts: string[] = [];
      if (report.ungrouped.length) parts.push(t('Groupe Sonos dissous : {list}', { list: report.ungrouped.join(', ') }));
      if (report.grouped.length) parts.push(t('Groupe Sonos : {list}', { list: report.grouped.join(' + ') }));
      if (report.delayed.length) parts.push(report.delayed.map(([n, d]) => t('{name} retardée de {ms} ms', { name: n, ms: d })).join(', '));
      get().say(parts.length ? parts.join(' · ') : t('Rien à synchroniser : activez au moins deux enceintes.'));
    } catch (e) {
      get().say(String(e));
    }
  },

  async applyScene(id) {
    try {
      get().apply(await api.applyScene(id));
    } catch (e) {
      get().say(String(e));
    }
  },

  async scan() {
    set({ scanning: true });
    try {
      get().apply(await api.scanDevices());
    } catch (e) {
      get().say(String(e));
    } finally {
      set({ scanning: false });
    }
  },

  moveSpeaker(id, roomId) {
    upd((s) => ({ ...s, speakers: s.speakers.map((sp) => (sp.id === id ? { ...sp, roomId } : sp)) }));
    void api.setSpeakerRoom(id, roomId);
  },
  };
});

export function speakersOf(s: FullState, roomId: string): Speaker[] {
  return s.speakers.filter((sp) => sp.roomId === roomId);
}

// The rooms created on first launch keep their French name in the config: show it translated until renamed.
const DEFAULT_ROOMS: Record<string, string> = { salon: 'Salon', cuisine: 'Cuisine', bureau: 'Bureau', chambre: 'Chambre', terrasse: 'Terrasse' };
export function roomLabel(r: { id: string; name: string }): string {
  return DEFAULT_ROOMS[r.id] === r.name ? t(r.name) : r.name;
}

export function colorOf(s: FullState, sp: Speaker): string {
  return s.rooms.find((r) => r.id === sp.roomId)?.color ?? 'var(--c-ink3)';
}
