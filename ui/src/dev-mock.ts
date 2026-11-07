// Banco di prova visivo (solo sviluppo, http://localhost:5173/mock.html): simula il core in memoria per
// vedere e provare la UI nel browser senza Tauri. Tipizzato sul contratto IPC: se cambia, svelte-check lo segnala.
import { emit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';

import type { Folder, Settings, Snapshot, Sound } from './lib/api';
import { isMac } from './lib/util/platform';

const q = new URLSearchParams(location.search);
const NAMES: [string, string][] = [
  ['Airhorn', '📯'], ['Bruh', '😐'], ['Vine boom', '💥'], ['Applausi', '👏'], ['Risata registrata', '😂'],
  ['Sad trombone', '🎺'], ['Errore di Windows XP', '💻'], ['Rullo di tamburi', '🥁'], ['Sirena', '🚨'],
  ['Wow', '😮'], ['Oof', '😵'], ['Campanello', '🛎️'], ['Gong', '🔔'], ['Cri cri cri', '🦗'], ['Ta-da', '🎉'],
  ['Pew pew', '🔫'], ['Anatra', '🦆'], ['Capra che urla', '🐐'], ['Fanfara', '🎺'], ['Suspense', '🎬'],
  ['Boing', '🦘'], ['Ding', '✨'], ['Nope', '🙅'], ['Mission failed', '💀'], ['Allarme rosso', '🚨'],
  ['Cassa registratore', '💰'], ['Tuono', '⚡'], ['Esplosione nucleare con un nome molto lungo', '☢️'],
  ['Rana', '🐸'], ['Fantasma', '👻'], ['Clown', '🤡'], ['Pizza', '🍕'],
];
const N = Number(q.get('n') ?? NAMES.length);

let folders: Folder[] = [
  { id: 'f1', name: 'Meme', order: 0 },
  { id: 'f2', name: 'Reazioni', order: 1 },
  { id: 'f3', name: 'Musica', order: 2 },
];
let sounds: Sound[] = Array.from({ length: N }, (_, i) => {
  const [name, emoji] = NAMES[i % NAMES.length];
  return {
    id: `s${i}`,
    name: i < NAMES.length ? name : `${name} ${i}`,
    file: `s${i}.sbpcm`,
    channels: 2,
    frames: 48000 * (1 + (i % 7)),
    loudness_gain_db: -3.2,
    user_gain_db: 0,
    mode: i === 3 ? 'toggle' : i === 5 ? 'restart' : null,
    folder_id: i % 4 === 0 ? 'f1' : i % 4 === 1 ? 'f2' : null,
    tags: [],
    source: { path: `C:\\Users\\Marco\\Downloads\\${name}.mp3`, xxh3: `${i}` },
    ui: { emoji },
    created_at: new Date(Date.now() - i * 3600_000).toISOString(),
  };
});

let settings: Settings = {
  schema_version: 1,
  outputs: {
    cable: { device_id: null, volume_db: 0, enabled: true },
    monitor: { device_id: null, volume_db: -6, enabled: true },
  },
  mic: { mode: isMac ? 'bridge' : 'system', device_id: null },
  playback: { max_voices: 32, default_mode: 'overlap', fade_in_ms: 2, fade_out_ms: 8, idle_stop_ms: 2000, warm_hold_ms: 60000 },
  loudness: { target_lufs: -18, ceiling_dbtp: -1, trim_silence: true },
  library: { max_duration_s: 300 },
  ptt: { enabled: false, key: null, scancode: null, tail_ms: 200 },
  hotkeys: [],
  ui: { keepalive_s: 30, window: null, prefs: { guideDone: true, view: q.get('view') ?? 'grid' } },
  startup: { autostart: false },
} as Settings;

const hotkeys = [
  { action: { type: 'play' as const, sound_id: 's0' }, accelerator: 'Ctrl+Alt+1', active: true, conflict: false },
  { action: { type: 'play' as const, sound_id: 's2' }, accelerator: 'Ctrl+Alt+2', active: true, conflict: false },
  { action: { type: 'stop_all' as const }, accelerator: 'Ctrl+Alt+0', active: true, conflict: false },
];
const status = (q.get('status') ?? '').split(',').filter(Boolean) as import('./lib/api').ErrorCode[];

const devices = [
  { id: 'd1', name: 'Cuffie (Realtek High Definition Audio)', is_default: true, is_cable: false, mix_rate: 48000 },
  { id: 'd2', name: 'Altoparlanti (Realtek)', is_default: false, is_cable: false, mix_rate: 48000 },
  { id: 'd3', name: 'CABLE Input (VB-Audio Virtual Cable)', is_default: false, is_cable: true, mix_rate: 48000 },
];

let playSeq = 1;
const libraryChanged = (added: Sound[] = [], updated: Sound[] = [], removed: string[] = []) =>
  void emit('library:changed', { added, updated, removed, folders: [...folders].sort((a, b) => a.order - b.order) });

function merge(t: Record<string, unknown>, p: Record<string, unknown>) {
  for (const [k, v] of Object.entries(p)) {
    if (v && typeof v === 'object' && !Array.isArray(v) && t[k] && typeof t[k] === 'object') merge(t[k] as Record<string, unknown>, v as Record<string, unknown>);
    else t[k] = v;
  }
}

const trash: Sound[] = [];

mockWindows('main');
mockIPC(
  async (cmd, raw) => {
    const a = (raw ?? {}) as Record<string, any>;
    await new Promise((r) => setTimeout(r, 15));
    switch (cmd) {
      case 'get_snapshot':
        return {
          settings: JSON.parse(JSON.stringify(settings)),
          devices,
          mics: [{ id: 'm1', name: 'Microfono (USB)', is_default: true, listen_enabled: true, routed_to_cable: true }],
          library: { folders, sounds },
          hotkeys,
          hotkeys_paused: q.has('paused'),
          status,
          playing: [],
          app: { version: '1.0.0', data_dir: 'C:\\Users\\Marco\\AppData\\Local\\MicFX Data', library_dir: 'C:\\Users\\Marco\\AppData\\Local\\MicFX Data\\library' },
          notices: [],
          discord: null,
        } satisfies Snapshot as Snapshot;
      case 'play_sound':
      case 'preview_sound': {
        const s = sounds.find((x) => x.id === a.sound_id)!;
        const p = { play_id: playSeq++, sound_id: s.id, started_at_ms: Date.now(), duration_ms: (s.frames / 48000) * 1000, preview: cmd === 'preview_sound' };
        void emit('playback:started', p);
        setTimeout(() => void emit('playback:ended', { play_id: p.play_id, sound_id: s.id }), p.duration_ms);
        return p.play_id;
      }
      case 'update_sound': {
        const s = sounds.find((x) => x.id === a.sound_id)!;
        const p = a.patch;
        if ('folder_id' in p) s.folder_id = p.folder_id;
        if (p.name) s.name = p.name;
        if (p.ui) s.ui = { ...s.ui, ...p.ui };
        if ('mode' in p) s.mode = p.mode;
        if (p.user_gain_db !== undefined) s.user_gain_db = p.user_gain_db;
        libraryChanged([], [{ ...s }]);
        return s;
      }
      case 'delete_sounds':
        for (const id of a.sound_ids) {
          const i = sounds.findIndex((s) => s.id === id);
          if (i >= 0) trash.push(...sounds.splice(i, 1));
        }
        libraryChanged([], [], a.sound_ids);
        return null;
      case 'restore_sounds': {
        const back = trash.filter((s) => a.sound_ids.includes(s.id));
        sounds.push(...back);
        libraryChanged(back);
        return back;
      }
      case 'upsert_folder': {
        const name = String(a.name).split(/\s+/).filter(Boolean).join(' ').slice(0, 40);
        if (folders.some((f) => f.id !== a.id && f.name.toLowerCase() === name.toLowerCase()))
          throw { code: 'folder_exists', message: `Esiste già una cartella «${name}».` };
        let f = folders.find((x) => x.id === a.id);
        if (f) f.name = name;
        else {
          f = { id: a.id ?? `f${Math.random().toString(36).slice(2, 8)}`, name, order: a.order ?? Math.max(-1, ...folders.map((x) => x.order)) + 1 };
          folders = [...folders, f];
        }
        libraryChanged();
        return f;
      }
      case 'delete_folder': {
        folders = folders.filter((f) => f.id !== a.folder_id);
        const moved = sounds.filter((s) => s.folder_id === a.folder_id);
        moved.forEach((s) => (s.folder_id = null));
        libraryChanged([], moved);
        return null;
      }
      case 'set_output': {
        const o = settings.outputs[a.bus as 'cable' | 'monitor'];
        Object.assign(o, a.patch);
        void emit('settings:changed', JSON.parse(JSON.stringify(settings)));
        return status;
      }
      case 'update_settings':
        merge(settings as unknown as Record<string, unknown>, JSON.parse(JSON.stringify(a.patch)));
        return JSON.parse(JSON.stringify(settings));
      case 'get_diagnostics':
        return {
          version: '1.0.0',
          buses: [
            { bus: 'monitor', state: 'running', device: devices[0].name, voices: 0, underruns: 0, errors: 0, blocks: 1234, mix_rate: 48000, buffer_frames: 480 },
            { bus: 'cable', state: 'idle', device: devices[2].name, voices: 0, underruns: 0, errors: 0, blocks: 900, mix_rate: 48000, buffer_frames: 480 },
          ],
          playing: 0,
          mapped_sounds: 3,
          sounds: sounds.length,
          config_path: 'C:\\Users\\Marco\\AppData\\Roaming\\MicFX\\config.json',
          library_path: 'C:\\Users\\Marco\\AppData\\Local\\MicFX Data\\library\\index.json',
          ptt_down: false,
        };
      case 'set_hotkey':
        return null;
      case 'pick_and_import':
        return null;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

await import('./main');
