export type Protocol = 'native' | 'sonos' | 'upnp' | 'web';
export type ChannelMode = 'stereo' | 'mono' | 'left' | 'right';

export interface ChannelParams {
  gainDb: number;
  volumeDb: number;
  mute: boolean;
  solo: boolean;
  lowDb: number;
  midDb: number;
  highDb: number;
  pan: number;
  mode: ChannelMode;
  phaseInvert: boolean;
  delayMs: number;
  bassDb: number;
  presenceDb: number;
  loudness: number;
  width: number;
  compOn: boolean;
  compThreshDb: number;
  compRatio: number;
  compMakeupDb: number;
  gateOn: boolean;
  gateThreshDb: number;
}

export interface Speaker {
  id: string;
  name: string;
  roomId: string;
  protocol: Protocol;
  address: string;
  port: number;
  model?: string | null;
  uuid?: string | null;
  lineIn?: boolean;
  useLineIn?: boolean;
  receiverVolume?: number;
  active: boolean;
  params: ChannelParams;
  running: boolean;
  online: boolean;
  streaming: boolean;
  error?: string | null;
  group?: string | null;
  latencyMs?: number | null;
  deviceVolume?: number | null;
  syncedTo?: string | null;
}

export interface SyncReport {
  grouped: string[];
  delayed: [string, number][];
  ungrouped: string[];
}

export interface Room {
  id: string;
  name: string;
  icon: string;
  color: string;
  volumeDb: number;
  mute: boolean;
  solo: boolean;
}

export type Source = { kind: 'system' } | { kind: 'app'; pid: number; name: string };

export interface Settings {
  udpFrameMs: number;
  bitrateKbps: number;
  redundancy: boolean;
  smartOutput: boolean;
  pinEnginePeriod: boolean;
  keepUiAlive: boolean;
  httpPort: number;
  language: string;
  sonosLowLatency: boolean;
}

export interface MidiBinding {
  control: string;
  status: number;
  data1: number;
}

export interface SceneView {
  id: string;
  name: string;
}

export interface FullState {
  masterDb: number;
  source: Source;
  localPc: boolean;
  speakers: Speaker[];
  rooms: Room[];
  scenes: SceneView[];
  midiMap: MidiBinding[];
  settings: Settings;
  httpPort: number;
  capture: { running: boolean; error?: string | null };
  pcMuted: boolean;
}

export interface AudioApp {
  pid: number;
  name: string;
  exe: string;
  icon?: string | null;
  active: boolean;
}

export interface Meters {
  m: [number, number];
  c: Record<string, [number, number]>;
}

export interface ReceiverInfo {
  url: string;
  qrSvg: string;
  ips: string[];
}

export type ViewId = 'rooms' | 'plan' | 'console';

export type DeviceKind = 'bluetooth' | 'wired' | 'usb' | 'hdmi' | 'network' | 'other';

export interface MixerDevice {
  id: string;
  name: string;
  kind: DeviceKind;
  volume: number;
  mute: boolean;
  default: boolean;
  peak: number;
}

export interface MixerApp {
  key: string;
  name: string;
  icon?: string | null;
  deviceId: string;
  deviceName: string;
  volume: number;
  mute: boolean;
  active: boolean;
  peak: number;
}

export interface MixerState {
  devices: MixerDevice[];
  apps: MixerApp[];
  defaultLocked: boolean;
}
