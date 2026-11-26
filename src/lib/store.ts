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

// ---- Undo / redo: every edit made in this window records how to put it back ----

interface HistEntry {
  key: string;
  at: number;
  label: string;
  undo: () => void;
  redo: () => void;
}

const HISTORY_MAX = 100;
// Repeated edits of the same control within this time (a dragged fader) are one step.
const COALESCE_MS = 1000;
const past: HistEntry[] = [];
const future: HistEntry[] = [];
let replaying = false;
let grouping: HistEntry[] | null = null;

function record(key: string, label: string, undo: () => void, redo: () => void) {
  if (replaying) return;
  const now = Date.now();
  if (grouping) {
    grouping.push({ key, at: now, label, undo, redo });
    return;
  }
  const last = past[past.length - 1];
  if (last && last.key === key && now - last.at < COALESCE_MS) {
    last.redo = redo;
    last.at = now;
  } else {
    past.push({ key, at: now, label, undo, redo });
    if (past.length > HISTORY_MAX) past.shift();
  }
  future.length = 0;
}

// Runs several edits as one undo step.
function group(label: string, fn: () => void) {
  if (replaying || grouping) return fn();
  grouping = [];
  try {
    fn();
  } finally {
    const items = grouping;
    grouping = null;
    if (items.length) {
      past.push({
        key: `g:${Date.now()}`,
        at: Date.now(),
        label,
        undo: () => [...items].reverse().forEach((i) => i.undo()),
        redo: () => items.forEach((i) => i.redo()),
      });
      if (past.length > HISTORY_MAX) past.shift();
      future.length = 0;
    }
  }
}

function replay(fn: () => void) {
  replaying = true;
  try {
    fn();
  } finally {
    replaying = false;
  }
}

// Settings that "Copy settings" carries from one speaker to another: processing and delay, not level, mute or pan.
export const COPY_KEYS: Array<keyof ChannelParams> = [
  'gainDb', 'lowDb', 'midDb', 'highDb', 'bassDb', 'presenceDb', 'loudness', 'width',
  'compOn', 'compThreshDb', 'compRatio', 'compMakeupDb', 'gateOn', 'gateThreshDb',
  'eqEnabled', 'eqPreset', 'eqGains', 'delayMs', 'mode', 'phaseInvert',
];

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

interface Mix {
  masterDb: number;
  masterMute: boolean;
  speakers: Record<string, { params: ChannelParams; active: boolean }>;
  rooms: Record<string, { volumeDb: number; mute: boolean; solo: boolean }>;
}

function takeMix(s: FullState): Mix {
  return {
    masterDb: s.masterDb,
    masterMute: s.masterMute,
    speakers: Object.fromEntries(s.speakers.map((sp) => [sp.id, { params: structuredClone(sp.params), active: sp.active }])),
    rooms: Object.fromEntries(s.rooms.map((r) => [r.id, { volumeDb: r.volumeDb, mute: r.mute, solo: r.solo }])),
  };
}

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  msg: string;
  action?: ToastAction;
  id: number;
}

export interface Copied {
  from: string;
  name: string;
  params: Partial<ChannelParams>;
}

interface Store {
  s: FullState | null;
  view: ViewId;
  toast: Toast | null;
  scanning: boolean;
  copied: Copied | null;
  canUndo: boolean;
  init: () => Promise<void>;
  initLite: () => Promise<void>;
  apply: (next: FullState) => void;
  setView: (v: ViewId) => void;
  say: (msg: string, action?: ToastAction) => void;

  setParam: <K extends keyof ChannelParams>(id: string, key: K, value: ChannelParams[K]) => void;
  setRoomParam: <K extends 'volumeDb' | 'mute' | 'solo'>(id: string, key: K, value: Room[K]) => void;
  setMaster: (db: number) => void;
  setMasterMute: (on: boolean) => void;
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
  deleteRooms: (ids: string[]) => Promise<void>;
  setPinned: (on: boolean) => void;
  identify: (id: string) => Promise<void>;
  reconnect: (id: string) => Promise<void>;
  copySettings: (id: string) => void;
  pasteSettings: (ids: string[]) => void;
  restoreMix: (mix: Mix) => void;
  reorderSpeakers: (ids: string[]) => void;
  reorderRooms: (ids: string[]) => void;
  setPlanPosition: (id: string, pos: { x: number; y: number } | null) => void;
  setSpeakerStyle: (id: string, style: { color?: string; icon?: string }) => void;
  setZoom: (zoom: number) => Promise<void>;
  undo: () => void;
  redo: () => void;
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

let toastSeq = 0;

export const useStore = create<Store>((set, get) => {
  const upd = (fn: (s: FullState) => FullState) => set((st) => (st.s ? { s: fn(st.s) } : {}));
  const spName = (id: string) => get().s?.speakers.find((x) => x.id === id)?.name ?? '';
  const afterRecord = () => set({ canUndo: past.length > 0 });
  const refresh = async () => get().apply(await api.getState());

  return {
  s: null,
  view: savedView(),
  toast: null,
  scanning: false,
  copied: null,
  canUndo: false,

  async init() {
    get().apply(await api.getState());
    loadMidiMap(get().s?.midiMap ?? []);
    await on<FullState>('state', (st) => get().apply(st));
    await on<Meters>('meters', pushMeters);
    await on<null>('open-settings', () => window.dispatchEvent(new CustomEvent('rezo-open-settings')));
    // Tabs behave like routes of a single-page app: the address follows the tab and Back/Forward switch tabs.
    if (location.hash.replace('#', '') !== get().view) history.replaceState(null, '', `#${get().view}`);
    window.addEventListener('hashchange', () => {
      const h = location.hash.replace('#', '');
      if (isView(h) && h !== get().view) get().setView(h);
    });
  },

  // Desktop widgets: state and meters only, no routing.
  async initLite() {
    get().apply(await api.getState());
    await on<FullState>('state', (st) => get().apply(st));
    await on<Meters>('meters', pushMeters);
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
        if (sp.error && sp.error !== old?.error) {
          get().say(`${sp.name} : ${sp.error}`, { label: t('Reconnecter'), run: () => void get().reconnect(sp.id) });
        }
      }
    }
    set({ s: next });
  },

  setView(v) {
    try { localStorage.setItem('rezo.view', v); } catch {}
    set({ view: v });
    if (location.hash !== `#${v}`) location.hash = v;
  },

  say(msg, action) {
    set({ toast: { msg, action, id: ++toastSeq } });
  },

  setParam(id, key, value) {
    const before = get().s?.speakers.find((x) => x.id === id)?.params[key];
    if (before !== undefined && !same(before, value)) {
      record(`p:${id}:${String(key)}`, spName(id), () => get().setParam(id, key, before), () => get().setParam(id, key, value));
      afterRecord();
    }
    touch();
    upd((s) => ({ ...s, speakers: s.speakers.map((sp: Speaker) => (sp.id === id ? { ...sp, params: { ...sp.params, [key]: value } } : sp)) }));
    enqueue(`p:${id}:${String(key)}`, () => api.setParam(id, String(key), value));
  },

  setRoomParam(id, key, value) {
    const room = get().s?.rooms.find((r) => r.id === id);
    if (room && room[key] !== value) {
      const before = room[key];
      record(`r:${id}:${key}`, roomLabel(room), () => get().setRoomParam(id, key, before), () => get().setRoomParam(id, key, value));
      afterRecord();
    }
    touch();
    upd((s) => ({ ...s, rooms: s.rooms.map((r) => (r.id === id ? { ...r, [key]: value } : r)) }));
    enqueue(`r:${id}:${key}`, () => api.setRoomParam(id, key, value));
  },

  setMaster(db) {
    const before = get().s?.masterDb;
    if (before !== undefined && before !== db) {
      record('master', t('Volume général'), () => get().setMaster(before), () => get().setMaster(db));
      afterRecord();
    }
    touch();
    upd((s) => ({ ...s, masterDb: db }));
    enqueue('master', () => api.setMaster(db));
  },

  setMasterMute(on) {
    record(`mute:${Date.now()}`, t('Couper le son'), () => get().setMasterMute(!on), () => get().setMasterMute(on));
    afterRecord();
    upd((s) => ({ ...s, masterMute: on }));
    void api.setMasterMute(on);
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
    record(`send:${id}:${Date.now()}`, spName(id), () => void get().setSend(id, !on), () => void get().setSend(id, on));
    afterRecord();
    upd((s) => ({ ...s, speakers: s.speakers.map((sp) => (sp.id === id ? { ...sp, active: on } : sp)) }));
    try {
      get().apply(await api.setSend(id, on));
    } catch (e) {
      get().say(String(e));
    }
  },

  async setRoomSend(id, on) {
    const room = get().s?.rooms.find((r) => r.id === id);
    record(`rsend:${id}:${Date.now()}`, room ? roomLabel(room) : id, () => void get().setRoomSend(id, !on), () => void get().setRoomSend(id, on));
    afterRecord();
    try {
      get().apply(await api.setRoomSend(id, on));
    } catch (e) {
      get().say(String(e));
    }
  },

  setDeviceVolume(id, volume) {
    const before = get().s?.speakers.find((x) => x.id === id)?.deviceVolume;
    if (before !== undefined && before !== null && before !== volume) {
      record(`v:${id}`, spName(id), () => get().setDeviceVolume(id, before), () => get().setDeviceVolume(id, volume));
      afterRecord();
    }
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
    const cur = get().s;
    const before = cur ? takeMix(cur) : null;
    const name = cur?.scenes.find((x) => x.id === id)?.name ?? '';
    try {
      get().apply(await api.applyScene(id));
      if (before && !replaying) {
        record(`scene:${Date.now()}`, name, () => get().restoreMix(before), () => void api.applyScene(id).then((st) => get().apply(st)));
        afterRecord();
        get().say(t('Mix « {name} » appliqué.', { name }), { label: t('Annuler'), run: () => get().undo() });
      }
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
    const before = get().s?.speakers.find((x) => x.id === id)?.roomId;
    if (before !== undefined && before !== roomId) {
      record(`move:${id}:${Date.now()}`, spName(id), () => get().moveSpeaker(id, before), () => get().moveSpeaker(id, roomId));
      afterRecord();
    }
    upd((s) => ({ ...s, speakers: s.speakers.map((sp) => (sp.id === id ? { ...sp, roomId } : sp)) }));
    void api.setSpeakerRoom(id, roomId);
  },

  // Deleting rooms can be undone: the room comes back with its name, icon, colour, place and speakers.
  async deleteRooms(ids) {
    const s = get().s;
    if (!s || !ids.length) return;
    const order = s.rooms.map((r) => r.id);
    const gone = s.rooms.filter((r) => ids.includes(r.id)).map((r) => ({ room: r, members: s.speakers.filter((sp) => sp.roomId === r.id).map((sp) => sp.id) }));
    for (const id of ids) await api.deleteRoom(id);
    await refresh();
    const restore = async () => {
      const newOrder = [...order];
      for (const g of gone) {
        const nid = await api.createRoom(g.room.name, g.room.icon, g.room.color);
        newOrder[newOrder.indexOf(g.room.id)] = nid;
        for (const sp of g.members) await api.setSpeakerRoom(sp, nid);
      }
      await api.reorder({ rooms: newOrder });
      await refresh();
    };
    const label = gone.length === 1 ? roomLabel(gone[0].room) : t('{n} pièces', { n: gone.length });
    record(`rooms-del:${Date.now()}`, label, () => void restore(), () => void get().deleteRooms(ids));
    afterRecord();
    if (!replaying) get().say(t('Supprimé : {name}', { name: label }), { label: t('Annuler'), run: () => get().undo() });
  },

  setPinned(on) {
    upd((s) => ({ ...s, pinned: on }));
    void api.setPinned(on);
  },

  async identify(id) {
    get().say(t('Bip envoyé à {name}…', { name: spName(id) }));
    try {
      await api.identifySpeaker(id);
    } catch (e) {
      get().say(String(e));
    }
  },

  async reconnect(id) {
    get().say(t('Reconnexion de {name}…', { name: spName(id) }));
    try {
      get().apply(await api.reconnectSpeaker(id));
    } catch (e) {
      get().say(String(e));
    }
  },

  copySettings(id) {
    const sp = get().s?.speakers.find((x) => x.id === id);
    if (!sp) return;
    const params = Object.fromEntries(COPY_KEYS.map((k) => [k, structuredClone(sp.params[k])])) as Partial<ChannelParams>;
    set({ copied: { from: id, name: sp.name, params } });
    get().say(t('Réglages de {name} copiés : égaliseur, effets, tonalité et retard.', { name: sp.name }));
  },

  pasteSettings(ids) {
    const c = get().copied;
    const s = get().s;
    if (!c || !s) return;
    const targets = s.speakers.filter((sp) => ids.includes(sp.id) && sp.id !== c.from);
    if (!targets.length) return;
    group(t('Coller les réglages'), () => {
      for (const sp of targets) {
        for (const k of COPY_KEYS) {
          const v = c.params[k];
          if (v !== undefined && !same(sp.params[k], v)) get().setParam(sp.id, k, structuredClone(v) as never);
        }
      }
    });
    afterRecord();
    get().say(t('Réglages de {from} collés sur {to}.', { from: c.name, to: targets.map((x) => x.name).join(', ') }), { label: t('Annuler'), run: () => get().undo() });
  },

  restoreMix(mix) {
    const s = get().s;
    if (!s) return;
    if (s.masterDb !== mix.masterDb) get().setMaster(mix.masterDb);
    if (s.masterMute !== mix.masterMute) get().setMasterMute(mix.masterMute);
    for (const sp of s.speakers) {
      const m = mix.speakers[sp.id];
      if (!m) continue;
      for (const k of Object.keys(m.params) as Array<keyof ChannelParams>) {
        if (!same(sp.params[k], m.params[k])) get().setParam(sp.id, k, m.params[k] as never);
      }
      if (sp.active !== m.active) void get().setSend(sp.id, m.active);
    }
    for (const r of s.rooms) {
      const m = mix.rooms[r.id];
      if (!m) continue;
      if (r.volumeDb !== m.volumeDb) get().setRoomParam(r.id, 'volumeDb', m.volumeDb);
      if (r.mute !== m.mute) get().setRoomParam(r.id, 'mute', m.mute);
      if (r.solo !== m.solo) get().setRoomParam(r.id, 'solo', m.solo);
    }
  },

  reorderSpeakers(ids) {
    const before = get().s?.speakers.map((x) => x.id) ?? [];
    record(`order-sp:${Date.now()}`, t('Ordre des enceintes'), () => get().reorderSpeakers(before), () => get().reorderSpeakers(ids));
    afterRecord();
    upd((s) => ({ ...s, speakers: [...s.speakers].sort((a, b) => ids.indexOf(a.id) - ids.indexOf(b.id)) }));
    void api.reorder({ speakers: ids });
  },

  reorderRooms(ids) {
    const before = get().s?.rooms.map((x) => x.id) ?? [];
    record(`order-rooms:${Date.now()}`, t('Ordre des pièces'), () => get().reorderRooms(before), () => get().reorderRooms(ids));
    afterRecord();
    upd((s) => ({ ...s, rooms: [...s.rooms].sort((a, b) => ids.indexOf(a.id) - ids.indexOf(b.id)) }));
    void api.reorder({ rooms: ids });
  },

  setPlanPosition(id, pos) {
    const sp = get().s?.speakers.find((x) => x.id === id);
    if (sp) {
      const before = sp.planX != null && sp.planY != null ? { x: sp.planX, y: sp.planY } : null;
      record(`plan:${id}:${Date.now()}`, sp.name, () => get().setPlanPosition(id, before), () => get().setPlanPosition(id, pos));
      afterRecord();
    }
    upd((s) => ({ ...s, speakers: s.speakers.map((x) => (x.id === id ? { ...x, planX: pos?.x ?? null, planY: pos?.y ?? null } : x)) }));
    void api.setPlanPosition(id, pos?.x ?? null, pos?.y ?? null);
  },

  setSpeakerStyle(id, style) {
    const sp = get().s?.speakers.find((x) => x.id === id);
    if (sp) {
      const before = { ...(style.color !== undefined ? { color: sp.color ?? '' } : {}), ...(style.icon !== undefined ? { icon: sp.icon ?? '' } : {}) };
      record(`style:${id}:${Date.now()}`, sp.name, () => get().setSpeakerStyle(id, before), () => get().setSpeakerStyle(id, style));
      afterRecord();
    }
    upd((s) => ({
      ...s,
      speakers: s.speakers.map((x) => (x.id === id ? {
        ...x,
        ...(style.color !== undefined ? { color: style.color || null } : {}),
        ...(style.icon !== undefined ? { icon: style.icon || null } : {}),
      } : x)),
    }));
    void api.setSpeakerStyle(id, style);
  },

  async setZoom(zoom) {
    const z = await api.setUiZoom(zoom);
    upd((s) => ({ ...s, uiZoom: z }));
  },

  undo() {
    const e = past.pop();
    if (!e) return get().say(t('Rien à annuler.'));
    replay(e.undo);
    future.push(e);
    set({ canUndo: past.length > 0 });
    get().say(t('Annulé : {what}', { what: e.label }), { label: t('Rétablir'), run: () => get().redo() });
  },

  redo() {
    const e = future.pop();
    if (!e) return get().say(t('Rien à rétablir.'));
    replay(e.redo);
    past.push(e);
    set({ canUndo: true });
    get().say(t('Rétabli : {what}', { what: e.label }));
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

// A speaker's own colour, else its room's.
export function colorOf(s: FullState, sp: Speaker): string {
  return sp.color ?? s.rooms.find((r) => r.id === sp.roomId)?.color ?? 'var(--c-ink3)';
}
