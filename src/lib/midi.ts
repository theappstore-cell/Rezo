import { create } from 'zustand';
import { api } from './api';
import type { MidiBinding } from './types';

interface MidiState {
  supported: boolean;
  ready: boolean;
  inputs: string[];
  learn: boolean;
  armed: string | null;
  map: Record<string, { status: number; data1: number }>;
  error: string | null;
  lastActivity: number;
}

export const useMidi = create<MidiState>(() => ({
  supported: typeof navigator !== 'undefined' && 'requestMIDIAccess' in navigator,
  ready: false,
  inputs: [],
  learn: false,
  armed: null,
  map: {},
  error: null,
  lastActivity: 0,
}));

type Handler = (value01: number) => void;
const handlers = new Map<string, Handler>();
let reverse = new Map<string, string>();
let persistTimer: number | undefined;

export function registerMidiHandler(controlId: string, h: Handler): () => void {
  handlers.set(controlId, h);
  return () => {
    if (handlers.get(controlId) === h) handlers.delete(controlId);
  };
}

const keyOf = (status: number, data1: number) => `${status}:${data1}`;

function rebuildReverse() {
  reverse = new Map();
  for (const [id, b] of Object.entries(useMidi.getState().map)) reverse.set(keyOf(b.status, b.data1), id);
}

function persist() {
  window.clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => {
    const map: MidiBinding[] = Object.entries(useMidi.getState().map).map(([control, b]) => ({ control, ...b }));
    api.setMidiMap(map).catch(() => {});
  }, 400);
}

export function loadMidiMap(bindings: MidiBinding[]) {
  const map: MidiState['map'] = {};
  for (const b of bindings) map[b.control] = { status: b.status, data1: b.data1 };
  useMidi.setState({ map });
  rebuildReverse();
}

export function setLearn(on: boolean) {
  useMidi.setState({ learn: on, armed: null });
}

export function armControl(id: string | null) {
  useMidi.setState({ armed: id });
}

export function clearBinding(id: string) {
  const map = { ...useMidi.getState().map };
  delete map[id];
  useMidi.setState({ map });
  rebuildReverse();
  persist();
}

export function clearAllBindings() {
  useMidi.setState({ map: {}, armed: null });
  rebuildReverse();
  persist();
}

export function handleMidiMessage(data: ArrayLike<number> | null) {
  if (!data || data.length < 2) return;
  const raw = data[0];
  if (raw >= 0xf0) return;
  const type = raw & 0xf0;
  const channel = raw & 0x0f;
  const d1 = data[1];
  const d2 = data.length > 2 ? data[2] : 0;

  let status: number;
  let value01: number;
  if (type === 0xb0) {
    status = raw;
    value01 = d2 / 127;
  } else if (type === 0x90 || type === 0x80) {
    status = 0x90 | channel;
    value01 = type === 0x80 || d2 === 0 ? 0 : Math.max(d2, 1) / 127;
  } else {
    return;
  }

  useMidi.setState({ lastActivity: performance.now() });
  const st = useMidi.getState();

  if (st.learn && st.armed) {
    if (status === (0x90 | channel) && value01 === 0) return;
    const map = { ...st.map };
    for (const [id, b] of Object.entries(map)) if (b.status === status && b.data1 === d1) delete map[id];
    map[st.armed] = { status, data1: d1 };
    useMidi.setState({ map, armed: null });
    rebuildReverse();
    persist();
    return;
  }
  if (st.learn) return;

  const id = reverse.get(keyOf(status, d1));
  if (id) handlers.get(id)?.(value01);
}

let initStarted = false;
let midiAccess: any = null;

// Windows MIDI ports belong to a single process: keep them open only while bindings or MIDI Learn need them.
export function releaseMidi() {
  const access = midiAccess;
  midiAccess = null;
  initStarted = false;
  if (!access) return;
  access.onstatechange = null;
  access.inputs.forEach((input: any) => {
    input.onmidimessage = null;
    try {
      void input.close?.();
    } catch {
    }
  });
  useMidi.setState({ ready: false, inputs: [] });
}

export async function initMidi() {
  const st = useMidi.getState();
  if (!st.supported || initStarted) return;
  initStarted = true;
  try {
    const access = await (navigator as any).requestMIDIAccess({ sysex: false });
    if (!initStarted) return;
    midiAccess = access;
    const refresh = () => {
      const names: string[] = [];
      access.inputs.forEach((input: any) => {
        names.push(input.name ?? 'MIDI');
        input.onmidimessage = (e: any) => handleMidiMessage(e.data);
      });
      useMidi.setState({ inputs: names, ready: true, error: null });
    };
    access.onstatechange = refresh;
    refresh();
  } catch (e) {
    initStarted = false;
    useMidi.setState({ error: 'Accès MIDI refusé ou indisponible', ready: false });
  }
}
