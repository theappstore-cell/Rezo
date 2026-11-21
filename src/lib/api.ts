import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { mockInvoke, mockListen, startMockMeters } from './mock';
import type { AudioApp, FullState, Meters, MidiBinding, MixerState, ReceiverInfo, Settings, Source, SyncReport, UpdateInfo } from './types';

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return isTauri ? invoke<T>(cmd, args) : (mockInvoke(cmd, args as Record<string, any>) as Promise<T>);
}

export async function on<T>(event: string, cb: (p: T) => void): Promise<() => void> {
  if (isTauri) {
    const un = await listen<T>(event, (e) => cb(e.payload));
    return un;
  }
  if (event === 'meters') startMockMeters();
  return mockListen(event, cb);
}

export const api = {
  getState: () => call<FullState>('get_state'),
  setParam: (id: string, key: string, value: unknown) => call<void>('set_param', { id, key, value }),
  setRoomParam: (id: string, key: string, value: unknown) => call<void>('set_room_param', { id, key, value }),
  setMaster: (db: number) => call<void>('set_master', { db }),
  setLocalPc: (on: boolean) => call<void>('set_local_pc', { on }),
  setSource: (source: Source) => call<void>('set_source', { source }),
  listApps: () => call<AudioApp[]>('list_apps'),
  setSend: (id: string, on: boolean) => call<FullState>('set_send', { id, on }),
  setRoomSend: (id: string, on: boolean) => call<FullState>('set_room_send', { id, on }),
  setDeviceVolume: (id: string, volume: number) => call<void>('set_device_volume', { id, volume }),
  setUseLineIn: (id: string, on: boolean) => call<FullState>('set_use_line_in', { id, on }),
  syncSpeakers: () => call<{ report: SyncReport; state: FullState }>('sync_speakers'),
  checkUpdate: () => call<UpdateInfo>('check_update'),
  installUpdate: () => call<void>('install_update'),
  openReleasePage: () => call<void>('open_release_page'),
  scanDevices: () => call<FullState>('scan_devices'),
  addManualDevice: (ip: string, kind: 'upnp' | 'native', port?: number) =>
    call<FullState>('add_manual_device', { ip, kind, port }),
  renameSpeaker: (id: string, name: string) => call<void>('rename_speaker', { id, name }),
  setSpeakerRoom: (id: string, roomId: string) => call<void>('set_speaker_room', { id, roomId }),
  removeSpeaker: (id: string) => call<FullState>('remove_speaker', { id }),
  getReceiverInfo: (ip?: string) => call<ReceiverInfo>('get_receiver_info', { ip }),
  createRoom: (name: string, icon: string, color: string) => call<string>('create_room', { name, icon, color }),
  updateRoom: (id: string, patch: { name?: string; icon?: string; color?: string }) =>
    call<void>('update_room', { id, ...patch }),
  deleteRoom: (id: string) => call<void>('delete_room', { id }),
  saveScene: (name: string) => call<string>('save_scene', { name }),
  applyScene: (id: string) => call<FullState>('apply_scene', { id }),
  deleteScene: (id: string) => call<void>('delete_scene', { id }),
  renameScene: (id: string, name: string) => call<void>('rename_scene', { id, name }),
  setMidiMap: (map: MidiBinding[]) => call<void>('set_midi_map', { map }),
  setSettings: (settings: Settings) => call<FullState>('set_settings', { settings }),
  getAutostart: () => call<boolean>('get_autostart'),
  setAutostart: (on: boolean) => call<void>('set_autostart', { on }),
  setWindowMode: (wide: boolean) => call<void>('set_window_mode', { wide }),
  setPinned: (pinned: boolean) => call<void>('set_pinned', { pinned }),
  hideWindow: () => call<void>('hide_window'),
  quit: () => call<void>('quit_app'),
  openMixer: () => call<void>('open_mixer'),
  setLanguage: (lang: string) => call<void>('set_language', { lang }),
  mixerState: () => call<MixerState>('mixer_state'),
  mixerDeviceVolume: (id: string, volume: number) => call<void>('mixer_set_device_volume', { id, volume }),
  mixerDeviceMute: (id: string, mute: boolean) => call<void>('mixer_set_device_mute', { id, mute }),
  mixerAppVolume: (deviceId: string, key: string, volume: number) => call<void>('mixer_set_app_volume', { deviceId, key, volume }),
  mixerAppMute: (deviceId: string, key: string, mute: boolean) => call<void>('mixer_set_app_mute', { deviceId, key, mute }),
  mixerSetDefault: (id: string) => call<void>('mixer_set_default', { id }),
};

export type { Meters };
