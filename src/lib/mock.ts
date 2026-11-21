import type { AudioApp, ChannelParams, FullState, Meters, MixerState, ReceiverInfo, Speaker } from './types';

const base = (over: Partial<ChannelParams> = {}): ChannelParams => ({
  gainDb: 0, volumeDb: 0, mute: false, solo: false, lowDb: 0, midDb: 0, highDb: 0,
  pan: 0, mode: 'stereo', phaseInvert: false, delayMs: 0,
  bassDb: 0, presenceDb: 0, loudness: 0, width: 1, compOn: false, compThreshDb: -18, compRatio: 3, compMakeupDb: 0,
  gateOn: false, gateThreshDb: -55, ...over,
});

const sp = (s: Partial<Speaker> & Pick<Speaker, 'id' | 'name' | 'roomId' | 'protocol'>): Speaker => ({
  address: '192.168.1.42', port: 1400, model: null, uuid: null, active: false, params: base(),
  running: false, online: true, streaming: false, error: null, group: null, latencyMs: null, ...s,
});

let state: FullState = {
  masterDb: 0,
  source: { kind: 'system' },
  localPc: false,
  speakers: [
    sp({ id: 'RINCON_A1', name: 'Living room', roomId: 'salon', protocol: 'sonos', model: 'Sonos Five', lineIn: true, active: true, running: true, streaming: true, latencyMs: 300, deviceVolume: 35, params: base({ volumeDb: -6 }) }),
    sp({ id: 'RINCON_A2', name: 'Living room (rear)', roomId: 'salon', protocol: 'sonos', model: 'Sonos One', active: true, running: true, streaming: true, latencyMs: 300, deviceVolume: 28, params: base({ volumeDb: -9, lowDb: 3 }) }),
    sp({ id: 'RINCON_B1', name: 'Kitchen', roomId: 'cuisine', protocol: 'sonos', model: 'Sonos Play:1', latencyMs: 300, deviceVolume: 20, params: base({ volumeDb: -12 }) }),
    sp({ id: 'native-pc2', name: 'Office PC', roomId: 'bureau', protocol: 'native', address: '192.168.1.62', port: 5004, model: 'Rezo receiver', active: true, running: true, streaming: true, latencyMs: 42, deviceVolume: 100 }),
    sp({ id: 'web-ph1', name: "Lea's iPhone", roomId: 'chambre', protocol: 'web', model: 'Browser', active: true, running: true, streaming: true, latencyMs: 31, deviceVolume: 80, params: base({ pan: -0.2 }) }),
    sp({ id: 'RINCON_C1', name: 'TV soundbar', roomId: '', protocol: 'upnp', model: 'DLNA', online: false }),
  ],
  rooms: [
    { id: 'salon', name: 'Living room', icon: 'Sofa', color: '#d9480f', volumeDb: 0, mute: false, solo: false },
    { id: 'cuisine', name: 'Kitchen', icon: 'CookingPot', color: '#5c940d', volumeDb: 0, mute: false, solo: false },
    { id: 'bureau', name: 'Office', icon: 'Monitor', color: '#1c7ed6', volumeDb: 0, mute: false, solo: false },
    { id: 'chambre', name: 'Bedroom', icon: 'BedDouble', color: '#9c36b5', volumeDb: 0, mute: false, solo: false },
    { id: 'terrasse', name: 'Terrace', icon: 'TreePalm', color: '#c2255c', volumeDb: 0, mute: false, solo: false },
  ],
  scenes: [{ id: 'salon', name: 'Movie night' }, { id: 'soiree', name: 'Party' }, { id: 'nuit', name: 'Night mode' }],
  midiMap: [],
  settings: { udpFrameMs: 5, bitrateKbps: 160, redundancy: true, smartOutput: true, pinEnginePeriod: false, keepUiAlive: false, httpPort: 49494, language: '', sonosLowLatency: true, checkUpdates: true },
  httpPort: 49494,
  capture: { running: true, error: null },
  pcMuted: true,
};

const apps: AudioApp[] = [
  { pid: 4120, name: 'Spotify', exe: 'spotify.exe', icon: null, active: true },
  { pid: 8800, name: 'Google Chrome', exe: 'chrome.exe', icon: null, active: true },
  { pid: 2210, name: 'VLC', exe: 'vlc.exe', icon: null, active: false },
];

const mixer: MixerState = {
  defaultLocked: false,
  devices: [
    { id: 'spk', name: 'Speakers (Realtek Audio)', kind: 'wired', volume: 0.6, mute: false, default: true, peak: 0 },
    { id: 'bt', name: 'Sony WH-1000XM4 headphones', kind: 'bluetooth', volume: 0.45, mute: false, default: false, peak: 0 },
    { id: 'hdmi', name: 'Dell monitor (HDMI)', kind: 'hdmi', volume: 0.3, mute: true, default: false, peak: 0 },
  ],
  apps: [
    { key: 'C:\spotify.exe', name: 'Spotify', icon: null, deviceId: 'spk', deviceName: 'Speakers (Realtek Audio)', volume: 0.8, mute: false, active: true, peak: 0 },
    { key: 'C:\chrome.exe', name: 'Google Chrome', icon: null, deviceId: 'spk', deviceName: 'Speakers (Realtek Audio)', volume: 1, mute: false, active: false, peak: 0 },
    { key: 'C:\discord.exe', name: 'Discord', icon: null, deviceId: 'bt', deviceName: 'Sony WH-1000XM4 headphones', volume: 0.7, mute: false, active: true, peak: 0 },
  ],
};
const mixerSnapshot = (): MixerState => {
  const m = clone(mixer);
  const k = Math.abs(Math.sin(Date.now() / 700));
  m.devices.forEach((d) => { d.peak = d.default && !d.mute ? k * d.volume : 0; });
  m.apps.forEach((r) => { r.peak = r.active && !r.mute ? k * r.volume * 0.8 : 0; });
  return m;
};

const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v));
const snap = () => clone(state);
const spk = (id: string) => state.speakers.find((s) => s.id === id);

export async function mockInvoke(cmd: string, a: Record<string, any> = {}): Promise<any> {
  await new Promise((r) => setTimeout(r, 15));
  switch (cmd) {
    case 'get_state': return snap();
    case 'set_param': { const s = spk(a.id); if (s) (s.params as any)[a.key] = a.value; return null; }
    case 'set_room_param': { const r = state.rooms.find((x) => x.id === a.id); if (r) (r as any)[a.key] = a.value; return null; }
    case 'set_master': state.masterDb = a.db; return null;
    case 'set_local_pc': state.localPc = a.on; return null;
    case 'set_source': state.source = a.source; return null;
    case 'list_apps': return clone(apps);
    case 'set_send': { const s = spk(a.id); if (s) { s.active = a.on; s.running = a.on && s.online; s.streaming = s.running; } return snap(); }
    case 'set_room_send': state.speakers.filter((s) => s.roomId === a.id && s.online).forEach((s) => { s.active = a.on; s.running = a.on; s.streaming = a.on; }); return snap();
    case 'set_device_volume': { const s = spk(a.id); if (s) { s.deviceVolume = a.volume; s.receiverVolume = a.volume; } return null; }
    case 'set_use_line_in': { const s = spk(a.id); if (s) s.useLineIn = a.on; return snap(); }
    case 'sync_speakers': {
      const grouped = state.speakers.filter((s) => s.syncedTo);
      if (grouped.length) { grouped.forEach((s) => { s.syncedTo = null; }); return { report: { grouped: [], delayed: [], ungrouped: grouped.map((s) => s.name) }, state: snap() }; }
      const sonos = state.speakers.filter((s) => s.protocol === 'sonos' && s.running);
      sonos.slice(1).forEach((s) => { s.syncedTo = sonos[0].name; });
      const pc = spk('native-pc2'); if (pc) pc.params.delayMs = 258;
      return { report: { grouped: sonos.length > 1 ? sonos.map((s) => s.name) : [], delayed: [['Office PC', 258]], ungrouped: [] }, state: snap() };
    }
    case 'check_update': return { current: '0.2.0', latest: '0.3.0', available: true, page: 'https://github.com/theappstore-cell/Rezo/releases/latest', asset: 'Rezo-0.3.0-windows-x64-portable.exe', size: 8_400_000, canInstall: true, folder: 'C:\\Rezo' };
    case 'install_update': {
      let received = 0;
      const total = 8_400_000;
      await new Promise<void>((res) => {
        const h = setInterval(() => {
          received = Math.min(total, received + 420_000);
          listeners['update-progress']?.forEach((cb) => cb({ received, total, stage: received < total ? 'download' : 'verify' }));
          if (received >= total) { clearInterval(h); res(); }
        }, 120);
      });
      listeners['update-progress']?.forEach((cb) => cb({ received: total, total, stage: 'restart' }));
      return null;
    }
    case 'open_release_page': return null;
    case 'scan_devices': return snap();
    case 'add_manual_device': return snap();
    case 'rename_speaker': { const s = spk(a.id); if (s) s.name = a.name; return null; }
    case 'set_speaker_room': { const s = spk(a.id); if (s) s.roomId = a.roomId; return null; }
    case 'remove_speaker': state.speakers = state.speakers.filter((s) => s.id !== a.id); return snap();
    case 'get_receiver_info': return { url: 'http://192.168.1.20:49494/r', ips: ['192.168.1.20'], qrSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8" fill="#fff"/><path d="M1 1h2v2H1zM5 1h2v2H5zM1 5h2v2H1zM4 4h1v1H4zM6 5h1v2H6z" fill="#0b0d10"/></svg>' } satisfies ReceiverInfo;
    case 'create_room': { const id = `room-${state.rooms.length + 1}`; state.rooms.push({ id, name: a.name, icon: a.icon, color: a.color, volumeDb: 0, mute: false, solo: false }); return id; }
    case 'update_room': { const r = state.rooms.find((x) => x.id === a.id); if (r) { if (a.name) r.name = a.name; if (a.icon) r.icon = a.icon; if (a.color) r.color = a.color; } return null; }
    case 'delete_room': state.rooms = state.rooms.filter((r) => r.id !== a.id); state.speakers.forEach((s) => { if (s.roomId === a.id) s.roomId = ''; }); return null;
    case 'save_scene': { const id = `scene-${state.scenes.length + 1}`; state.scenes.push({ id, name: a.name }); return id; }
    case 'apply_scene': return snap();
    case 'delete_scene': state.scenes = state.scenes.filter((s) => s.id !== a.id); return null;
    case 'rename_scene': { const s = state.scenes.find((x) => x.id === a.id); if (s) s.name = a.name; return null; }
    case 'set_midi_map': state.midiMap = a.map; return null;
    case 'set_settings': state.settings = a.settings; return snap();
    case 'get_autostart': return false;
    case 'set_autostart': return null;
    case 'mixer_state': return mixerSnapshot();
    case 'mixer_set_device_volume': { const d = mixer.devices.find((x) => x.id === a.id); if (d) d.volume = a.volume; return null; }
    case 'mixer_set_device_mute': { const d = mixer.devices.find((x) => x.id === a.id); if (d) d.mute = a.mute; return null; }
    case 'mixer_set_app_volume': { const r = mixer.apps.find((x) => x.key === a.key && x.deviceId === a.deviceId); if (r) r.volume = a.volume; return null; }
    case 'mixer_set_app_mute': { const r = mixer.apps.find((x) => x.key === a.key && x.deviceId === a.deviceId); if (r) r.mute = a.mute; return null; }
    case 'mixer_set_default': mixer.devices.forEach((d) => { d.default = d.id === a.id; }); return null;
    default: return null;
  }
}

type Listener = (payload: any) => void;
const listeners: Record<string, Set<Listener>> = {};

export function mockListen(event: string, cb: Listener): () => void {
  (listeners[event] ??= new Set()).add(cb);
  return () => listeners[event]?.delete(cb);
}

let t = 0;
let timer: number | undefined;
export function startMockMeters() {
  if (timer) return;
  timer = window.setInterval(() => {
    t += 1 / 60;
    const c: Meters['c'] = {};
    for (const s of state.speakers) {
      if (!s.running) continue;
      const g = Math.pow(10, (s.params.volumeDb + (s.params.mute ? -90 : 0)) / 20);
      const env = 0.45 + 0.4 * Math.sin(t * 2.1 + s.id.length) * Math.sin(t * 0.7);
      const l = Math.max(0, env * g * (0.8 + 0.2 * Math.sin(t * 9)));
      const r = Math.max(0, env * g * (0.8 + 0.2 * Math.cos(t * 11)));
      c[s.id] = [l, r];
    }
    const m = Math.pow(10, state.masterDb / 20);
    const env = 0.55 + 0.35 * Math.sin(t * 2.1) * Math.sin(t * 0.7);
    listeners['meters']?.forEach((cb) => cb({ m: [env * m, env * m * 0.93], c } satisfies Meters));
  }, 16);
}
