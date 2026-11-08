// Stato della UI: derivato da snapshot + eventi del core (SPEC §12). La UI non ha stato proprio
// da salvare: le preferenze di vista vivono in `settings.ui.prefs`, i dati dei suoni in `sound.ui`.
import { SvelteSet } from 'svelte/reactivity';

import {
  api,
  CoreError,
  on,
  type AppInfo,
  type BusKind,
  type Device,
  type DiscordInfo,
  type ErrorCode,
  type Folder,
  type Hotkey,
  type HotkeyAction,
  type ImportProgress,
  type Mic,
  type Notice,
  type PlayMode,
  type Playing,
  type Settings,
  type Snapshot,
  type Sound,
} from '../api';
import { fallbackEmoji, loadEmoji, preloadEmoji, suggestForName } from '../emoji/emoji';
import { errorText } from '../util/messages';
import { dbToPercent, percentToDb } from '../util/volume';
import { toasts } from './toasts.svelte';

export type ViewMode = 'grid' | 'list';
export type SortMode = 'manual' | 'name' | 'recent';
export type TileSize = 's' | 'm' | 'l';

export type Prefs = {
  view: ViewMode;
  sort: SortMode;
  size: TileSize;
  /** 'all' oppure l'ID di una cartella. */
  folder: string;
  recentEmojis: string[];
  skinTone: number;
  guideDone: boolean;
};

const DEFAULT_PREFS: Prefs = {
  view: 'grid',
  sort: 'manual',
  size: 'm',
  folder: 'all',
  recentEmojis: [],
  skinTone: 0,
  guideDone: false,
};

export const SUPPORTED_EXT = ['mp3', 'wav', 'flac', 'ogg', 'oga', 'm4a', 'aac', 'mp4'];

export function soundEmoji(s: Sound): string {
  const e = s.ui?.emoji;
  return typeof e === 'string' && e ? e : fallbackEmoji(s.id);
}

/** Nome di cartella come lo salva il core: spazi compattati, al massimo 40 caratteri (non unità UTF-16:
 *  un'emoji tagliata a metà sarebbe un JSON che il core rifiuta). */
export function cleanFolderName(name: string): string {
  return Array.from(name.replace(/\s+/g, ' ').trim()).slice(0, 40).join('');
}

function soundOrder(s: Sound): number | null {
  const o = s.ui?.order;
  return typeof o === 'number' && Number.isFinite(o) ? o : null;
}

/** Confronto per campi: gli eventi del core arrivano con le chiavi JSON in ordine alfabetico. */
export function sameAction(a: HotkeyAction, b: HotkeyAction): boolean {
  if (a.type !== b.type) return false;
  return !('sound_id' in a) || a.sound_id === (b as { sound_id?: string }).sound_id;
}

function fail(e: unknown, title?: string) {
  if (e instanceof CoreError) {
    toasts.error(title ?? errorText(e.code, e.message), title ? e.message : undefined);
  } else {
    toasts.error(title ?? 'Si è verificato un errore', String(e));
  }
}

class AppStore {
  ready = $state(false);
  fatal = $state<string | null>(null);

  settings = $state<Settings | null>(null);
  devices = $state<Device[]>([]);
  mics = $state<Mic[]>([]);
  discord = $state<DiscordInfo | null>(null);
  folders = $state<Folder[]>([]);
  sounds = $state<Sound[]>([]);
  hotkeys = $state<Hotkey[]>([]);
  hotkeysPaused = $state(false);
  status = $state<ErrorCode[]>([]);
  playing = $state<Playing[]>([]);
  info = $state<AppInfo | null>(null);
  prefs = $state<Prefs>({ ...DEFAULT_PREFS });

  // --- stato solo della finestra
  search = $state('');
  selection = new SvelteSet<string>();
  /** Elemento con il "cursore" da tastiera. */
  focusId = $state<string | null>(null);
  /** Ancora per la selezione con Maiusc. */
  anchorId = $state<string | null>(null);
  renamingId = $state<string | null>(null);
  editorId = $state<string | null>(null);
  emojiTarget = $state<{ soundId: string; x: number; y: number } | null>(null);
  menu = $state<{ x: number; y: number; ids: string[] } | null>(null);
  settingsOpen = $state(false);
  /** Sezione da mostrare all'apertura delle impostazioni (es. 'ptt' dall'avviso di Discord). */
  settingsSection = $state<string | null>(null);
  guideOpen = $state(false);
  shortcutsOpen = $state(false);
  /** ID appena importati: evidenziati per qualche secondo. */
  fresh = new SvelteSet<string>();
  now = $state(Date.now());
  /** Cattura scorciatoia in corso: le hotkey globali sono in pausa. */
  capturing = $state(false);
  /** Scheda sotto il puntatore mentre si trascinano suoni: 'all', l'ID di una cartella o null. */
  soundDropFolder = $state<string | null>(null);

  private rafOn = false;
  /** Suono selezionato da un clic che l'ha fatto suonare: si deseleziona quando l'audio finisce. */
  private clickPlayed: string | null = null;
  /** ...e il suo audio è partito davvero (un altro suono che finisce subito dopo il clic non conta). */
  private clickStarted = false;
  private creatingFolders = new Map<string, Promise<Folder | null>>();
  private prefsTimer: ReturnType<typeof setTimeout> | null = null;
  /** Volume voluto dall'utente e non ancora confermato dal core. */
  private volumeWant: Partial<Record<BusKind, number>> = {};
  private volumeBusy: Partial<Record<BusKind, boolean>> = {};
  private importToasts = new Map<number, number>();

  // ------------------------------------------------------------------ derivati

  hotkeyBySound = $derived.by(() => {
    const m = new Map<string, Hotkey>();
    for (const h of this.hotkeys) if (h.action.type === 'play') m.set(h.action.sound_id, h);
    return m;
  });

  playingBySound = $derived.by(() => {
    const m = new Map<string, Playing>();
    for (const p of this.playing) {
      const prev = m.get(p.sound_id);
      if (!prev || p.started_at_ms > prev.started_at_ms) m.set(p.sound_id, p);
    }
    return m;
  });

  folderCounts = $derived.by(() => {
    const m = new Map<string, number>();
    for (const s of this.sounds) if (s.folder_id) m.set(s.folder_id, (m.get(s.folder_id) ?? 0) + 1);
    return m;
  });

  /** Suoni mostrati: cartella, ricerca e ordinamento. */
  visible = $derived.by(() => {
    const folder = this.prefs.folder;
    const q = this.search
      .toLowerCase()
      .normalize('NFD')
      .replace(/[̀-ͯ]/g, '')
      .trim();
    let list = this.sounds.filter((s) => folder === 'all' || s.folder_id === folder);
    if (q) {
      list = list.filter((s) => {
        const name = s.name.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '');
        return name.includes(q) || soundEmoji(s) === this.search.trim() || s.tags.some((t) => t.toLowerCase().includes(q));
      });
    }
    const idx = new Map(this.sounds.map((s, i) => [s.id, i]));
    const sorted = [...list];
    switch (this.prefs.sort) {
      case 'name':
        sorted.sort((a, b) => a.name.localeCompare(b.name, 'it', { sensitivity: 'base', numeric: true }));
        break;
      case 'recent':
        sorted.sort((a, b) => b.created_at.localeCompare(a.created_at) || idx.get(b.id)! - idx.get(a.id)!);
        break;
      default:
        sorted.sort((a, b) => {
          const oa = soundOrder(a) ?? idx.get(a.id)! * 1024;
          const ob = soundOrder(b) ?? idx.get(b.id)! * 1024;
          return oa - ob || idx.get(a.id)! - idx.get(b.id)!;
        });
    }
    return sorted;
  });

  monitorPercent = $derived(this.settings ? dbToPercent(this.settings.outputs.monitor.volume_db) : 0);
  cablePercent = $derived(this.settings ? dbToPercent(this.settings.outputs.cable.volume_db) : 0);

  // ------------------------------------------------------------------ avvio

  async init() {
    try {
      this.applySnapshot(await api.getSnapshot());
    } catch (e) {
      this.fatal = e instanceof Error ? e.message : String(e);
      return;
    }
    this.subscribe();
    this.ready = true;
    // Riapertura dopo "chiudi in tray": la WebView era sospesa, si riallinea lo stato.
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') this.resync();
      this.tick();
    });
    if (!this.prefs.guideDone && (this.sounds.length === 0 || this.status.includes('cable_missing'))) {
      this.guideOpen = this.status.includes('cable_missing') || this.status.includes('mic_not_routed');
    }
    void this.backfillEmoji(this.sounds);
    preloadEmoji();
  }

  async resync() {
    try {
      this.applySnapshot(await api.getSnapshot());
    } catch {
      /* il prossimo evento riallinea */
    }
  }

  private applySnapshot(s: Snapshot) {
    this.settings = s.settings;
    this.devices = s.devices;
    this.mics = s.mics;
    this.discord = s.discord;
    this.folders = s.library.folders;
    this.sounds = s.library.sounds;
    this.hotkeys = s.hotkeys;
    this.hotkeysPaused = s.hotkeys_paused;
    this.status = s.status;
    this.playing = s.playing;
    this.info = s.app;
    this.prefs = { ...DEFAULT_PREFS, ...(s.settings.ui.prefs as Partial<Prefs>) };
    if (this.prefs.folder !== 'all' && !this.folders.some((f) => f.id === this.prefs.folder)) this.prefs.folder = 'all';
    for (const n of s.notices) this.showNotice(n);
    this.tick();
  }

  private subscribe() {
    on('playback:started', (p) => {
      this.playing = [...this.playing.filter((x) => x.play_id !== p.play_id), p];
      if (p.sound_id === this.clickPlayed) this.clickStarted = true;
      this.tick();
    });
    on('playback:ended', (p) => {
      this.playing = this.playing.filter((x) => x.play_id !== p.play_id);
      // Finito tutto l'audio, il riquadro cliccato torna normale (niente bordo di selezione).
      const id = this.clickPlayed;
      if (!this.playing.length && id && this.clickStarted) {
        this.clickPlayed = null;
        if (this.selection.size === 1 && this.selection.has(id)) {
          this.selection.clear();
          this.anchorId = null;
        }
      }
    });
    on('library:changed', (c) => {
      const removed = new Set(c.removed);
      const updated = new Map(c.updated.map((s) => [s.id, s]));
      let list = this.sounds.filter((s) => !removed.has(s.id)).map((s) => updated.get(s.id) ?? s);
      const have = new Set(list.map((s) => s.id));
      list = [...list, ...c.added.filter((s) => !have.has(s.id))];
      // Gli "Annulla" rimettono i suoni nella posizione originale: si riordina come il core.
      this.sounds = list;
      this.folders = c.folders;
      for (const id of c.removed) {
        this.selection.delete(id);
        if (this.focusId === id) this.focusId = null;
        if (this.editorId === id) this.editorId = null;
      }
      if (this.prefs.folder !== 'all' && !this.folders.some((f) => f.id === this.prefs.folder)) this.setPrefs({ folder: 'all' });
      if (c.added.length) void this.backfillEmoji(c.added);
    });
    on('import:progress', (p) => this.onImportProgress(p));
    on('devices:changed', (d) => {
      this.devices = d.devices;
      this.mics = d.mics;
    });
    on('status:changed', (s) => (this.status = s.status));
    on('hotkeys:changed', (h) => {
      this.hotkeys = h.hotkeys;
      this.hotkeysPaused = h.paused;
    });
    on('settings:changed', (s) => {
      // L'eco del core può essere più vecchio di quello che l'utente sta trascinando: i volumi
      // ancora da confermare restano quelli locali, altrimenti lo slider salta indietro.
      for (const bus of ['monitor', 'cable'] as BusKind[]) {
        const want = this.volumeWant[bus];
        if (want !== undefined) s.outputs[bus].volume_db = want;
      }
      this.settings = s;
    });
    on('discord:changed', (d) => (this.discord = d.discord));
    // Tornando su MicFX dopo aver cambiato Discord, il core ricontrolla da solo.
    window.addEventListener('focus', () => api.recheckSetup().catch(() => {}));
    on('notice', (n) => this.showNotice(n));
  }

  private showNotice(n: Notice) {
    const kind = n.code === 'config_recovered' || n.code === 'library_recovered' ? 'info' : 'error';
    toasts.show({ kind, title: n.message, timeout: kind === 'info' ? 10000 : 7000 });
  }

  /** Animazione locale delle barre di avanzamento: rAF solo se qualcosa suona e la finestra è visibile. */
  tick() {
    if (this.rafOn) return;
    const loop = () => {
      if (this.playing.length === 0 || document.visibilityState !== 'visible') {
        this.rafOn = false;
        return;
      }
      this.now = Date.now();
      requestAnimationFrame(loop);
    };
    if (this.playing.length && document.visibilityState === 'visible') {
      this.rafOn = true;
      requestAnimationFrame(loop);
    }
  }

  openSettings(section: string | null = null) {
    this.menu = null;
    this.emojiTarget = null;
    this.editorId = null;
    this.settingsSection = section;
    this.settingsOpen = true;
  }

  // ------------------------------------------------------------------ preferenze

  setPrefs(patch: Partial<Prefs>) {
    // Cambiando cartella la selezione riparte da zero, come in Esplora risorse: niente azioni
    // (Canc, sposta) su suoni selezionati altrove e ora invisibili.
    if (patch.folder !== undefined && patch.folder !== this.prefs.folder) {
      this.selection.clear();
      this.anchorId = null;
    }
    this.prefs = { ...this.prefs, ...patch };
    if (this.prefsTimer) clearTimeout(this.prefsTimer);
    this.prefsTimer = setTimeout(() => {
      api.updateSettings({ ui: { prefs: { ...this.prefs } } }).catch((e) => fail(e));
    }, 400);
  }

  rememberEmoji(e: string) {
    const recent = [e, ...this.prefs.recentEmojis.filter((x) => x !== e)].slice(0, 24);
    this.setPrefs({ recentEmojis: recent });
  }

  // ------------------------------------------------------------------ riproduzione

  async play(id: string) {
    try {
      await api.playSound(id);
    } catch (e) {
      fail(e);
    }
  }

  async preview(id: string) {
    try {
      await api.previewSound(id);
    } catch (e) {
      fail(e);
    }
  }

  async stop(id: string) {
    await api.stopSound(id).catch((e) => fail(e));
  }

  async stopAll() {
    await api.stopAll().catch((e) => fail(e));
  }

  // ------------------------------------------------------------------ uscite

  /** Slider: lo stato locale si aggiorna subito, il core al massimo ogni 40 ms. */
  setVolume(bus: BusKind, percent: number) {
    if (!this.settings) return;
    const db = percentToDb(percent);
    this.settings.outputs[bus].volume_db = db;
    this.volumeWant[bus] = db;
    void this.flushVolume(bus);
  }

  /** Una sola richiesta per bus alla volta, sempre con l'ultimo valore: l'ordine non si inverte
   *  e il valore finale arriva sempre al core. */
  private async flushVolume(bus: BusKind) {
    if (this.volumeBusy[bus]) return;
    this.volumeBusy[bus] = true;
    try {
      let sent: number | undefined;
      while (this.volumeWant[bus] !== undefined && this.volumeWant[bus] !== sent) {
        sent = this.volumeWant[bus]!;
        await api.setOutput(bus, { volume_db: sent }).catch((e) => fail(e));
        await new Promise((r) => setTimeout(r, 30));
      }
      if (this.volumeWant[bus] === sent) this.volumeWant[bus] = undefined;
    } finally {
      this.volumeBusy[bus] = false;
    }
  }

  async setOutputEnabled(bus: BusKind, enabled: boolean) {
    if (!this.settings) return;
    this.settings.outputs[bus].enabled = enabled;
    try {
      this.status = await api.setOutput(bus, { enabled });
    } catch (e) {
      fail(e);
    }
  }

  async setOutputDevice(bus: BusKind, deviceId: string | null) {
    try {
      this.status = await api.setOutput(bus, { device_id: deviceId });
      if (this.settings) this.settings.outputs[bus].device_id = deviceId;
    } catch (e) {
      fail(e);
    }
  }

  async updateSettings(patch: Parameters<typeof api.updateSettings>[0]) {
    try {
      this.settings = await api.updateSettings(patch);
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  // ------------------------------------------------------------------ libreria

  sound(id: string | null | undefined): Sound | undefined {
    return id ? this.sounds.find((s) => s.id === id) : undefined;
  }

  private patchLocal(id: string, f: (s: Sound) => Sound) {
    this.sounds = this.sounds.map((s) => (s.id === id ? f(s) : s));
  }

  async rename(id: string, name: string) {
    const clean = name.replace(/\s+/g, ' ').trim();
    const s = this.sound(id);
    if (!s || !clean || clean === s.name) return;
    this.patchLocal(id, (x) => ({ ...x, name: clean }));
    try {
      await api.updateSound(id, { name: clean });
    } catch (e) {
      this.patchLocal(id, (x) => ({ ...x, name: s.name }));
      fail(e, 'Impossibile rinominare');
    }
  }

  async setEmoji(id: string, emoji: string) {
    this.patchLocal(id, (x) => ({ ...x, ui: { ...x.ui, emoji } }));
    this.rememberEmoji(emoji);
    await api.updateSound(id, { ui: { emoji } }).catch((e) => fail(e));
  }

  async setGain(id: string, db: number) {
    this.patchLocal(id, (x) => ({ ...x, user_gain_db: db }));
    await api.updateSound(id, { user_gain_db: db }).catch((e) => fail(e));
  }

  async setMuteVoice(id: string, mute_voice: boolean) {
    this.patchLocal(id, (x) => ({ ...x, mute_voice }));
    await api.updateSound(id, { mute_voice }).catch((e) => fail(e));
  }

  async setMode(id: string, mode: PlayMode | null) {
    this.patchLocal(id, (x) => ({ ...x, mode }));
    await api.updateSound(id, { mode }).catch((e) => fail(e));
  }

  /** Sposta i suoni (subito in vista, poi nel core). Chi non arriva al core torna dov'era. */
  async moveToFolder(ids: string[], folderId: string | null, opts: { quiet?: boolean } = {}) {
    const moving = ids.filter((id) => {
      const s = this.sound(id);
      return s !== undefined && s.folder_id !== folderId;
    });
    if (!moving.length) return;
    const before = new Map(moving.map((id) => [id, this.sound(id)!.folder_id]));
    const moved = new Set(moving);
    this.sounds = this.sounds.map((s) => (moved.has(s.id) ? { ...s, folder_id: folderId } : s));
    const results = await Promise.allSettled(moving.map((id) => api.updateSound(id, { folder_id: folderId })));
    const failed = moving.filter((_, i) => results[i].status === 'rejected');
    if (failed.length) {
      const back = new Set(failed);
      this.sounds = this.sounds.map((s) => (back.has(s.id) ? { ...s, folder_id: before.get(s.id) ?? null } : s));
      const reason = (results.find((r) => r.status === 'rejected') as PromiseRejectedResult).reason;
      fail(reason, failed.length === moving.length ? 'Spostamento non riuscito' : `${failed.length} suoni non spostati`);
    }
    const ok = moving.length - failed.length;
    if (!ok || opts.quiet) return;
    const name = folderId ? `«${this.folders.find((f) => f.id === folderId)?.name ?? 'cartella'}»` : '«Tutti» (nessuna cartella)';
    toasts.show({ kind: 'success', title: ok === 1 ? `Spostato in ${name}` : `${ok} suoni spostati in ${name}` });
  }

  /** Elimina subito dalla vista; per 20 s il core tiene i file e "Annulla" li rimette al loro posto. */
  async deleteSounds(ids: string[]) {
    if (!ids.length) return;
    const names = ids.map((id) => this.sound(id)?.name).filter(Boolean) as string[];
    const backup = this.sounds;
    this.sounds = this.sounds.filter((s) => !ids.includes(s.id));
    for (const id of ids) this.selection.delete(id);
    try {
      await api.deleteSounds(ids);
    } catch (e) {
      this.sounds = backup;
      fail(e, 'Eliminazione non riuscita');
      return;
    }
    const tid = toasts.show({
      kind: 'info',
      title: ids.length === 1 ? `"${names[0]}" eliminato` : `${ids.length} suoni eliminati`,
      timeout: 8000,
      actions: [
        {
          label: 'Annulla',
          run: async () => {
            toasts.dismiss(tid);
            try {
              const restored = await api.restoreSounds(ids);
              this.selection.clear();
              for (const s of restored) this.selection.add(s.id);
            } catch (e) {
              fail(e, 'Impossibile ripristinare');
            }
          },
        },
      ],
    });
  }

  /** Riordino manuale: il suono va prima di `beforeId` (o in fondo se null). Un solo update. */
  async reorder(id: string, beforeId: string | null) {
    if (id === beforeId) return;
    const list = this.visible;
    // Al primo riordino ogni suono riceve un ordine esplicito, poi basta spostarne uno.
    if (list.some((s) => soundOrder(s) === null)) {
      const all = [...this.visible];
      await Promise.all(
        all.map((s, i) => {
          const order = (i + 1) * 1024;
          this.patchLocal(s.id, (x) => ({ ...x, ui: { ...x.ui, order } }));
          return api.updateSound(s.id, { ui: { order } }).catch(() => undefined);
        }),
      );
    }
    const ordered = this.visible.filter((s) => s.id !== id);
    const at = beforeId ? ordered.findIndex((s) => s.id === beforeId) : ordered.length;
    const prev = at > 0 ? soundOrder(ordered[at - 1])! : null;
    const next = at < ordered.length ? soundOrder(ordered[at])! : null;
    const order = prev === null && next === null ? 1024 : prev === null ? next! - 1024 : next === null ? prev + 1024 : (prev + next) / 2;
    this.patchLocal(id, (x) => ({ ...x, ui: { ...x.ui, order } }));
    if (this.prefs.sort !== 'manual') this.setPrefs({ sort: 'manual' });
    await api.updateSound(id, { ui: { order } }).catch((e) => fail(e));
  }

  // ------------------------------------------------------------------ cartelle

  /** Cartella con questo nome, senza badare a maiuscole e spazi (come il core). */
  folderByName(name: string): Folder | undefined {
    const key = cleanFolderName(name).toLowerCase();
    return key ? this.folders.find((f) => f.name.toLowerCase() === key) : undefined;
  }

  /** Crea una cartella. Se ne esiste già una con lo stesso nome, o la sta già creando un'altra
   *  richiesta (doppio Invio, Invio + perdita del focus), restituisce quella: mai due cartelle uguali. */
  createFolder(name: string): Promise<Folder | null> {
    const clean = cleanFolderName(name);
    if (!clean) return Promise.resolve(null);
    const existing = this.folderByName(clean);
    if (existing) return Promise.resolve(existing);
    const key = clean.toLowerCase();
    const inflight = this.creatingFolders.get(key);
    if (inflight) return inflight;
    const p = (async () => {
      try {
        const f = await api.upsertFolder(null, clean, null);
        this.addFolderLocal(f);
        return f;
      } catch (e) {
        fail(e, 'Impossibile creare la cartella');
        return null;
      } finally {
        this.creatingFolders.delete(key);
      }
    })();
    this.creatingFolders.set(key, p);
    return p;
  }

  private addFolderLocal(f: Folder) {
    if (this.folders.some((x) => x.id === f.id)) return;
    this.folders = [...this.folders, f].sort((a, b) => a.order - b.order);
  }

  async renameFolder(id: string, name: string) {
    const f = this.folders.find((x) => x.id === id);
    const clean = cleanFolderName(name);
    if (!f || !clean || clean === f.name) return;
    const other = this.folderByName(clean);
    if (other && other.id !== id) {
      toasts.error(`Esiste già una cartella «${other.name}»`, 'Scegli un altro nome.');
      return;
    }
    const setName = (n: string) => (this.folders = this.folders.map((x) => (x.id === id ? { ...x, name: n } : x)));
    setName(clean);
    try {
      await api.upsertFolder(id, clean, f.order);
    } catch (e) {
      setName(f.name);
      fail(e, 'Impossibile rinominare la cartella');
    }
  }

  /** Elimina la cartella (i suoni tornano in "Tutti") con "Annulla" per qualche secondo. */
  async deleteFolder(id: string) {
    const f = this.folders.find((x) => x.id === id);
    if (!f) return;
    const members = this.sounds.filter((s) => s.folder_id === id).map((s) => s.id);
    if (this.prefs.folder === id) this.setPrefs({ folder: 'all' });
    try {
      await api.deleteFolder(id);
    } catch (e) {
      fail(e, 'Impossibile eliminare la cartella');
      return;
    }
    toasts.show({
      kind: 'info',
      title: `Cartella «${f.name}» eliminata`,
      detail: members.length ? (members.length === 1 ? 'Il suono resta in «Tutti».' : `${members.length} suoni restano in «Tutti».`) : undefined,
      timeout: 8000,
      actions: [{ label: 'Annulla', run: () => void this.restoreFolder(f, members) }],
    });
  }

  /** Ricrea la cartella con lo stesso ID e la stessa posizione, e ci rimette i suoni. */
  private async restoreFolder(f: Folder, members: string[]) {
    let back: Folder;
    try {
      back = await api.upsertFolder(f.id, f.name, f.order);
    } catch (e) {
      fail(e, 'Impossibile ripristinare la cartella');
      return;
    }
    this.addFolderLocal(back);
    // Tornano solo i suoni che nel frattempo non sono stati spostati altrove.
    const still = members.filter((id) => this.sound(id)?.folder_id === null);
    await this.moveToFolder(still, back.id, { quiet: true });
    this.setPrefs({ folder: back.id });
  }

  // ------------------------------------------------------------------ scorciatoie

  hotkeyFor(action: HotkeyAction): Hotkey | undefined {
    return this.hotkeys.find((h) => sameAction(h.action, action));
  }

  /** Chi usa già questa combinazione dentro MicFX (per chiedere conferma prima di spostarla). */
  hotkeyOwner(accelerator: string, except: HotkeyAction): Hotkey | undefined {
    const norm = (a: string) => a.toLowerCase().split('+').sort().join('+');
    return this.hotkeys.find((h) => norm(h.accelerator) === norm(accelerator) && !sameAction(h.action, except));
  }

  async setHotkey(action: HotkeyAction, accelerator: string | null): Promise<Hotkey | null | undefined> {
    try {
      const h = await api.setHotkey(action, accelerator);
      if (h?.conflict) {
        toasts.show({
          kind: 'error',
          title: 'Combinazione già usata da un\'altra app',
          detail: 'È salvata ma non funziona finché l\'altra app la tiene. Scegline un\'altra.',
        });
      }
      return h;
    } catch (e) {
      fail(e);
      return undefined;
    }
  }

  /** Le hotkey sono state messe in pausa da noi per la cattura (non dall'utente). */
  private pausedForCapture = false;

  async beginCapture() {
    this.capturing = true;
    if (!this.hotkeysPaused && !this.pausedForCapture) {
      this.pausedForCapture = true;
      await api.pauseHotkeys(true).catch(() => undefined);
    }
  }

  async endCapture() {
    this.capturing = false;
    if (this.pausedForCapture) {
      this.pausedForCapture = false;
      await api.pauseHotkeys(false).catch(() => undefined);
    }
  }

  async togglePause() {
    await api.pauseHotkeys(!this.hotkeysPaused).catch((e) => fail(e));
  }

  // ------------------------------------------------------------------ import

  async importPaths(paths: string[], folderId: string | null = this.currentFolder()) {
    const ok = paths.filter((p) => !/\.[a-z0-9]+$/i.test(p) || SUPPORTED_EXT.includes(p.split('.').pop()!.toLowerCase()));
    const rejected = paths.length - ok.length;
    if (!ok.length) {
      toasts.error('Nessun file audio', 'Puoi importare MP3, WAV, FLAC, OGG e M4A.');
      return;
    }
    try {
      const job = await api.importFiles(ok, folderId);
      if (rejected) toasts.info(`${rejected} file ignorati`, 'Formato non supportato.');
      this.startImportToast(job);
    } catch (e) {
      fail(e, 'Importazione non riuscita');
    }
  }

  async pickAndImport() {
    try {
      const job = await api.pickAndImport(this.currentFolder());
      if (job !== null) this.startImportToast(job);
    } catch (e) {
      fail(e, 'Importazione non riuscita');
    }
  }

  currentFolder(): string | null {
    return this.prefs.folder === 'all' ? null : this.prefs.folder;
  }

  private startImportToast(job: number) {
    if (this.importToasts.has(job)) return;
    const id = toasts.show({ kind: 'progress', title: 'Importazione…', progress: 0, timeout: 0 });
    this.importToasts.set(job, id);
  }

  private onImportProgress(p: ImportProgress) {
    if (!this.importToasts.has(p.job_id)) this.startImportToast(p.job_id);
    const tid = this.importToasts.get(p.job_id)!;
    if (!p.finished) {
      toasts.update(tid, {
        title: p.total > 1 ? `Importazione ${Math.min(p.done + 1, p.total)} di ${p.total}…` : 'Importazione…',
        detail: p.current || undefined,
        progress: p.total ? p.done / p.total : 0,
      });
      return;
    }
    this.importToasts.delete(p.job_id);
    const added = p.added.length;
    const lines = p.errors.map((e) => `${e.path.split(/[\\/]/).pop()}: ${e.message}`);
    const dup = p.errors.find((e) => e.existing_sound_id);
    if (added) {
      this.selection.clear();
      for (const id of p.added) {
        this.selection.add(id);
        this.fresh.add(id);
      }
      this.focusId = p.added[p.added.length - 1];
      setTimeout(() => p.added.forEach((id) => this.fresh.delete(id)), 2500);
    }
    toasts.update(tid, {
      kind: p.errors.length && !added ? 'error' : p.errors.length ? 'info' : 'success',
      title: added
        ? added === 1
          ? `"${this.sound(p.added[0])?.name ?? 'Suono'}" aggiunto`
          : `${added} suoni aggiunti`
        : p.errors.length === 1
          ? 'File non importato'
          : 'Nessun file importato',
      detail: added && p.errors.length ? `${p.errors.length} non importati:` : undefined,
      lines: lines.slice(0, 6).concat(lines.length > 6 ? [`…e altri ${lines.length - 6}`] : []),
      progress: undefined,
      actions: dup?.existing_sound_id
        ? [{ label: 'Mostra', run: () => this.reveal(dup.existing_sound_id!) }]
        : undefined,
      timeout: p.errors.length ? 9000 : 3500,
    });
  }

  /** Porta in vista un suono: pulisce filtri che lo nasconderebbero, lo seleziona e lo evidenzia. */
  reveal(id: string) {
    const s = this.sound(id);
    if (!s) return;
    if (this.prefs.folder !== 'all' && s.folder_id !== this.prefs.folder) this.setPrefs({ folder: 'all' });
    this.search = '';
    this.selection.clear();
    this.selection.add(id);
    this.focusId = id;
    this.fresh.add(id);
    setTimeout(() => this.fresh.delete(id), 2000);
  }

  /** Ogni suono nuovo riceve un'emoji: quella che somiglia di più al nome, o una di riserva. */
  private async backfillEmoji(list: Sound[]) {
    const missing = list.filter((s) => typeof s.ui?.emoji !== 'string');
    if (!missing.length) return;
    let data;
    try {
      data = await loadEmoji();
    } catch {
      return;
    }
    for (const s of missing) {
      const current = this.sound(s.id);
      if (!current || typeof current.ui?.emoji === 'string') continue;
      const emoji = suggestForName(data, s.name, 1)[0]?.e ?? fallbackEmoji(s.id);
      this.patchLocal(s.id, (x) => ({ ...x, ui: { ...x.ui, emoji } }));
      await api.updateSound(s.id, { ui: { emoji } }).catch(() => undefined);
    }
  }

  // ------------------------------------------------------------------ selezione

  /** Clic su un riquadro: lo seleziona e lo suona; a fine audio la selezione sparisce da sola. */
  playFromClick(id: string) {
    this.select(id, 'single');
    this.clickPlayed = id;
    this.clickStarted = false;
    void this.play(id);
  }

  select(id: string, mode: 'single' | 'toggle' | 'range') {
    this.clickPlayed = null;
    if (mode === 'single') {
      this.selection.clear();
      this.selection.add(id);
      this.anchorId = id;
    } else if (mode === 'toggle') {
      if (this.selection.has(id)) this.selection.delete(id);
      else this.selection.add(id);
      this.anchorId = id;
    } else {
      const list = this.visible;
      const a = list.findIndex((s) => s.id === (this.anchorId ?? id));
      const b = list.findIndex((s) => s.id === id);
      const [from, to] = a < b ? [a, b] : [b, a];
      this.selection.clear();
      for (const s of list.slice(Math.max(0, from), to + 1)) this.selection.add(s.id);
    }
    this.focusId = id;
  }

  /** Bersaglio delle azioni: la selezione visibile se contiene `id`, altrimenti solo `id`.
   *  I suoni selezionati ma nascosti (spostati in un'altra cartella, filtrati) restano fuori. */
  targets(id: string): string[] {
    if (!this.selection.has(id) || this.selection.size < 2) return [id];
    return this.visible.filter((s) => this.selection.has(s.id)).map((s) => s.id);
  }
}

export const app = new AppStore();
