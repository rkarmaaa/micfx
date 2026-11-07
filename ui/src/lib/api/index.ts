// Wrapper tipizzati del contratto UI ↔ Core (docs/SPEC.md §12).
// Nessuna stringa di comando sparsa nei componenti: tutto passa da qui.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';

import type { ApiError } from './types/ApiError';
import type { BusKind } from './types/BusKind';
import type { Device } from './types/Device';
import type { Diagnostics } from './types/Diagnostics';
import type { DiscordInfo } from './types/DiscordInfo';
import type { ErrorCode } from './types/ErrorCode';
import type { Folder } from './types/Folder';
import type { Hotkey } from './types/Hotkey';
import type { HotkeyAction } from './types/HotkeyAction';
import type { ImportProgress } from './types/ImportProgress';
import type { LibraryChanged } from './types/LibraryChanged';
import type { Mic } from './types/Mic';
import type { Notice } from './types/Notice';
import type { PlaybackEnded } from './types/PlaybackEnded';
import type { Playing } from './types/Playing';
import type { Settings } from './types/Settings';
import type { Snapshot } from './types/Snapshot';
import type { Sound } from './types/Sound';
import type { SoundPatch } from './types/SoundPatch';

export type * from './types/ApiError';
export type * from './types/AppInfo';
export type * from './types/BusKind';
export type * from './types/Device';
export type * from './types/Diagnostics';
export type * from './types/DiscordInfo';
export type * from './types/ErrorCode';
export type * from './types/Folder';
export type * from './types/Hotkey';
export type * from './types/HotkeyAction';
export type * from './types/ImportProgress';
export type * from './types/ImportFileError';
export type * from './types/LibraryChanged';
export type * from './types/Mic';
export type * from './types/Notice';
export type * from './types/PlayMode';
export type * from './types/Playing';
export type * from './types/Settings';
export type * from './types/Snapshot';
export type * from './types/Sound';
export type * from './types/SoundPatch';

export type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> | null : T[K] };

/** Patch di un'uscita: `device_id: null` = automatico / segui il predefinito. */
export type OutputPatch = { device_id?: string | null; volume_db?: number; enabled?: boolean };

export class CoreError extends Error {
  readonly code: ErrorCode;
  constructor(code: ErrorCode, message: string) {
    super(message);
    this.code = code;
  }
}

function toCoreError(e: unknown): CoreError {
  if (e && typeof e === 'object' && 'code' in e && 'message' in e) {
    const a = e as ApiError;
    return new CoreError(a.code, a.message);
  }
  return new CoreError('internal', typeof e === 'string' ? e : String(e));
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toCoreError(e);
  }
}

export const api = {
  getSnapshot: () => call<Snapshot>('get_snapshot'),
  recheckSetup: () => call<void>('recheck_setup'),
  playSound: (sound_id: string) => call<number | null>('play_sound', { sound_id }),
  previewSound: (sound_id: string) => call<number | null>('preview_sound', { sound_id }),
  stopSound: (sound_id: string) => call<void>('stop_sound', { sound_id }),
  stopAll: () => call<void>('stop_all'),
  importFiles: (paths: string[], folder_id: string | null) => call<number>('import_files', { paths, folder_id }),
  /** Selettore file nativo (aperto dal core) + import. `null` se l'utente annulla. */
  pickAndImport: (folder_id: string | null) => call<number | null>('pick_and_import', { folder_id }),
  updateSound: (sound_id: string, patch: SoundPatch) => call<Sound>('update_sound', { sound_id, patch }),
  deleteSounds: (sound_ids: string[]) => call<void>('delete_sounds', { sound_ids }),
  restoreSounds: (sound_ids: string[]) => call<Sound[]>('restore_sounds', { sound_ids }),
  upsertFolder: (id: string | null, name: string, order: number | null) =>
    call<Folder>('upsert_folder', { id, name, order }),
  deleteFolder: (folder_id: string) => call<void>('delete_folder', { folder_id }),
  setHotkey: (action: HotkeyAction, accelerator: string | null) =>
    call<Hotkey | null>('set_hotkey', { action, accelerator }),
  pauseHotkeys: (paused: boolean) => call<void>('pause_hotkeys', { paused }),
  setOutput: (bus: BusKind, patch: OutputPatch) => call<ErrorCode[]>('set_output', { bus, patch }),
  updateSettings: (patch: DeepPartial<Settings>) => call<Settings>('update_settings', { patch }),
  getDiagnostics: () => call<Diagnostics>('get_diagnostics'),
  /** Pannello audio di Windows: 0 Riproduzione, 1 Registrazione, 3 Comunicazioni. */
  openSoundSettings: (tab: 0 | 1 | 3) => call<void>('open_sound_settings', { tab }),
  openDataFolder: () => call<void>('open_data_folder'),
  openLink: (which: 'vb-cable' | 'discord-voice' | 'discord-app' | 'github') => call<void>('open_link', { which }),
};

export type EventMap = {
  'playback:started': Playing;
  'playback:ended': PlaybackEnded;
  'library:changed': LibraryChanged;
  'import:progress': ImportProgress;
  'devices:changed': { devices: Device[]; mics: Mic[] };
  'status:changed': { status: ErrorCode[] };
  'hotkeys:changed': { hotkeys: Hotkey[]; paused: boolean };
  'settings:changed': Settings;
  'discord:changed': { discord: DiscordInfo | null };
  notice: Notice;
};

export function on<K extends keyof EventMap>(name: K, cb: (payload: EventMap[K]) => void): Promise<UnlistenFn> {
  return listen<EventMap[K]>(name, (e) => cb(e.payload));
}

export type DropEvent =
  | { type: 'enter'; paths: string[]; x: number; y: number }
  | { type: 'over'; x: number; y: number }
  | { type: 'drop'; paths: string[]; x: number; y: number }
  | { type: 'leave' };

/** File trascinati sulla finestra: i percorsi arrivano dall'evento di drop di Tauri (SPEC §12). */
export function onFileDrop(cb: (e: DropEvent) => void): Promise<UnlistenFn> {
  return getCurrentWebview().onDragDropEvent((ev) => {
    const p = ev.payload;
    const scale = window.devicePixelRatio || 1;
    switch (p.type) {
      case 'enter':
        cb({ type: 'enter', paths: p.paths, x: p.position.x / scale, y: p.position.y / scale });
        break;
      case 'over':
        cb({ type: 'over', x: p.position.x / scale, y: p.position.y / scale });
        break;
      case 'drop':
        cb({ type: 'drop', paths: p.paths, x: p.position.x / scale, y: p.position.y / scale });
        break;
      default:
        cb({ type: 'leave' });
    }
  });
}
