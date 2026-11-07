// Emoji: dati precaricati quando la finestra è inattiva (non all'avvio), ricerca in italiano e
// inglese, varianti di carnagione, suggerimento dal nome del suono e verifica (in cache) di cosa il
// font di Windows sa disegnare davvero.

export type EmojiEntry = {
  /** Carattere (con selettore di variante dove serve). */
  e: string;
  /** Nome in italiano. */
  n: string;
  /** Parole chiave normalizzate, separate da `|` (italiano, inglese, shortcode). */
  k: string;
  /** Nome inglese normalizzato. */
  l?: string;
  /** Categoria. */
  g: number;
  /** Versione Emoji in cui è stata introdotta. */
  v: number;
  /** Varianti con tono della pelle 1–5. */
  s?: string[];
};

export type EmojiGroup = { id: number; key: string; label: string };

export type EmojiData = {
  groups: EmojiGroup[];
  emojis: EmojiEntry[];
  byChar: Map<string, EmojiEntry>;
  byGroup: Map<number, EmojiEntry[]>;
  /** Emoji nascoste perché troppo recenti per il font di Windows installato. */
  hidden: number;
};

export const SKIN_TONES = ['', '🏻', '🏼', '🏽', '🏾', '🏿'];
export const SKIN_TONE_LABELS = ['Predefinito', 'Chiara', 'Abbastanza chiara', 'Olivastra', 'Abbastanza scura', 'Scura'];

/** Emoji di riserva per i suoni senza un suggerimento dal nome: scelte in modo stabile dall'ID. */
export const FALLBACK_EMOJIS = [
  '🔊', '🎵', '💥', '🎺', '📯', '🥁', '🎸', '🔔', '📢', '😂', '😎', '🔥', '⚡', '💣', '🎉', '👏',
  '🐐', '🦆', '🐸', '🚨', '🎬', '🎮', '👻', '💀', '🤡', '🍕', '🚀', '🦄', '🌶️', '🧨', '🎤', '🛎️',
];

export function fallbackEmoji(id: string): string {
  let h = 2166136261;
  for (let i = 0; i < id.length; i++) h = Math.imul(h ^ id.charCodeAt(i), 16777619);
  return FALLBACK_EMOJIS[(h >>> 0) % FALLBACK_EMOJIS.length];
}

export function normalize(s: string): string {
  return s
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[_:]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

let loading: Promise<EmojiData> | null = null;
let loaded: EmojiData | null = null;

export function emojiData(): EmojiData | null {
  return loaded;
}

export function loadEmoji(): Promise<EmojiData> {
  loading ??= (async () => {
    const [mod] = await Promise.all([import('./data.json'), ensureFlagFont()]);
    const raw = mod.default as { groups: EmojiGroup[]; emojis: EmojiEntry[] };
    const hidden = await unsupportedEmojis(raw.emojis);
    const emojis = hidden.size ? raw.emojis.filter((e) => !hidden.has(e.e)) : raw.emojis;
    const byGroup = new Map<number, EmojiEntry[]>();
    for (const e of emojis) {
      let l = byGroup.get(e.g);
      if (!l) byGroup.set(e.g, (l = []));
      l.push(e);
    }
    const data: EmojiData = {
      groups: raw.groups.filter((g) => byGroup.has(g.id)),
      emojis,
      byChar: new Map(emojis.map((e) => [e.e, e])),
      byGroup,
      hidden: raw.emojis.length - emojis.length,
    };
    loaded = data;
    return data;
  })();
  loading.catch(() => (loading = null));
  return loading;
}

/** Carica le emoji quando il browser è inattivo, così il selettore si apre già pronto. */
export function preloadEmoji() {
  if (loading) return;
  const go = () => void loadEmoji().catch(() => undefined);
  if ('requestIdleCallback' in window) requestIdleCallback(go, { timeout: 4000 });
  else setTimeout(go, 1500);
}

// Il test su canvas costa ~1 s a freddo: il risultato resta in memoria tra un'apertura e l'altra
// della finestra. Si rifà se cambia il motore (WebView2 aggiornata) o dopo 30 giorni (font di
// Windows aggiornato con emoji nuove).
const SUPPORT_KEY = 'micfx.emoji.unsupported.v1';
const SUPPORT_TTL = 30 * 24 * 3600 * 1000;

/** Cede il thread alla UI senza il ritardo minimo di setTimeout (4 ms, molto di più se nascosta). */
function yieldToUi(): Promise<void> {
  const s = (globalThis as { scheduler?: { yield?: () => Promise<void> } }).scheduler;
  if (s?.yield) return s.yield();
  return new Promise((r) => {
    const ch = new MessageChannel();
    ch.port1.onmessage = () => r();
    ch.port2.postMessage(null);
  });
}

async function unsupportedEmojis(all: EmojiEntry[]): Promise<Set<string>> {
  try {
    const c = JSON.parse(localStorage.getItem(SUPPORT_KEY) ?? 'null') as { ua: string; n: number; at: number; hidden: string[] } | null;
    if (c && c.ua === navigator.userAgent && c.n === all.length && Date.now() - c.at < SUPPORT_TTL) return new Set(c.hidden);
  } catch {
    /* cache illeggibile: si rifà il test */
  }
  // Solo le emoji recenti (Emoji 12+) possono mancare nel font di Windows 10/11.
  const ok = makeSupportTester();
  const hidden: string[] = [];
  let slice = performance.now();
  for (const e of all) {
    if (e.v < 12) continue;
    if (!ok(e.e)) hidden.push(e.e);
    // A pezzi da ~12 ms: la finestra resta reattiva mentre il test gira.
    if (performance.now() - slice > 12) {
      await yieldToUi();
      slice = performance.now();
    }
  }
  try {
    localStorage.setItem(SUPPORT_KEY, JSON.stringify({ ua: navigator.userAgent, n: all.length, at: Date.now(), hidden }));
  } catch {
    /* senza cache si rifà al prossimo avvio */
  }
  return new Set(hidden);
}

async function ensureFlagFont() {
  try {
    await document.fonts.load('24px "Twemoji Country Flags"', '🇮🇹');
  } catch {
    /* il font è facoltativo */
  }
}

/**
 * Prova a disegnare l'emoji su canvas: un glifo a colori ignora il colore di riempimento, un
 * rettangolo vuoto o un carattere di ripiego no. Le sequenze ZWJ non supportate escono larghe.
 */
function makeSupportTester(): (e: string) => boolean {
  const canvas = document.createElement('canvas');
  canvas.width = 64;
  canvas.height = 32;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) return () => true;
  ctx.font = '24px "Twemoji Country Flags", "Segoe UI Emoji", "Segoe UI Symbol", sans-serif';
  ctx.textBaseline = 'top';
  const ref = ctx.measureText('😀').width || 24;
  return (emoji: string) => {
    if (ctx.measureText(emoji).width > ref * 1.35) return false;
    ctx.clearRect(0, 0, 64, 32);
    ctx.fillStyle = '#f00';
    ctx.fillText(emoji, 0, 2);
    ctx.fillStyle = '#00f';
    ctx.fillText(emoji, 32, 2);
    const a = ctx.getImageData(0, 0, 32, 32).data;
    const b = ctx.getImageData(32, 0, 32, 32).data;
    let ink = 0;
    for (let i = 0; i < a.length; i += 4) {
      if (a[i + 3] > 0) ink++;
      if (a[i] !== b[i] || a[i + 1] !== b[i + 1] || a[i + 2] !== b[i + 2] || a[i + 3] !== b[i + 3]) return false;
    }
    return ink > 20;
  };
}

export function withSkin(entry: EmojiEntry, tone: number): string {
  return tone > 0 && entry.s ? entry.s[tone - 1] : entry.e;
}

const wordCache = new WeakMap<EmojiEntry, string[]>();
function words(e: EmojiEntry): string[] {
  let w = wordCache.get(e);
  if (!w) {
    w = e.k.split(/[| ]/).filter(Boolean);
    wordCache.set(e, w);
  }
  return w;
}

const phraseCache = new WeakMap<EmojiEntry, string[]>();
/** Parole chiave intere (nome italiano, tag, nome inglese...): "fire" è il nome di 🔥, non di ❤️‍🔥. */
function phrases(e: EmojiEntry): string[] {
  let p = phraseCache.get(e);
  if (!p) {
    p = e.k.split('|');
    phraseCache.set(e, p);
  }
  return p;
}

/** Parole dei nomi dei file che non descrivono il suono. */
const STOPWORDS = new Set([
  'the', 'and', 'with', 'made', 'from', 'for', 'feat', 'sound', 'sounds', 'effect', 'effects', 'sfx', 'audio',
  'voicemod', 'mp3', 'wav', 'ogg', 'flac', 'm4a', 'meme', 'free', 'download', 'clip', 'version', 'remix',
  'original', 'official', 'edit', 'hd', 'hq', 'lo', 'la', 'gli', 'le', 'un', 'una', 'uno', 'di', 'da', 'del',
  'della', 'dello', 'dei', 'degli', 'delle', 'con', 'per', 'tra', 'fra', 'che', 'suono', 'suoni', 'effetto',
  'effetti', 'versione',
]);

/** Ricerca: nome che inizia per la query > parola che inizia per la query > contiene. */
export function searchEmoji(data: EmojiData, query: string, limit = 400): EmojiEntry[] {
  const q = normalize(query);
  if (!q) return data.emojis;
  const exact = data.byChar.get(query.trim());
  const terms = q.split(' ');
  const scored: { e: EmojiEntry; s: number; i: number }[] = [];
  data.emojis.forEach((e, i) => {
    if (e === exact) {
      scored.push({ e, s: 1000, i });
      return;
    }
    const label = e.k.slice(0, e.k.indexOf('|') >>> 0);
    let total = 0;
    for (const t of terms) {
      let s = 0;
      if (label === t) s = 36;
      else if (e.l === t) s = 34;
      else if (label.startsWith(t)) s = 33;
      else if (e.l && e.l.split(' ').includes(t)) s = 31;
      else if (phrases(e).includes(t)) s = 30;
      else if (words(e).some((w) => w === t)) s = 24;
      else if (label.includes(` ${t}`)) s = 18;
      else if (words(e).some((w) => w.startsWith(t))) s = 12;
      else if (t.length >= 3 && e.k.includes(t)) s = 4;
      if (s === 0) return;
      total += s;
    }
    scored.push({ e, s: total, i });
  });
  // A parità di punteggio vince il nome più corto: "cuore rosso" prima di "cuore con freccia".
  scored.sort((a, b) => b.s - a.s || a.e.n.length - b.e.n.length || a.i - b.i);
  return scored.slice(0, limit).map((x) => x.e);
}

/** Emoji più adatte al nome di un suono (es. "air horn" → 📯). Vuoto se nessuna somiglia. */
export function suggestForName(data: EmojiData, name: string, limit = 8): EmojiEntry[] {
  const tokens = normalize(name)
    .replace(/[^a-z0-9 ]/g, ' ')
    .split(' ')
    .filter((t) => t.length >= 3 && !/^\d+$/.test(t) && !STOPWORDS.has(t));
  if (!tokens.length) return [];
  const scored: { e: EmojiEntry; s: number; i: number }[] = [];
  data.emojis.forEach((e, i) => {
    if (e.g === 9) return; // le bandiere raramente descrivono un suono
    const ws = words(e);
    const ps = phrases(e);
    // Ogni parola del nome che trova riscontro somma punti: "air horn" premia chi ha sia air sia horn.
    let total = 0;
    for (const t of tokens) {
      let best = ps.includes(t) ? 4 : 0;
      for (const w of ws) {
        if (best >= 3) break;
        let s = 0;
        if (w === t) s = 3;
        else if (w.length >= 3 && w.startsWith(t)) s = 2;
        else if (t.length >= 5 && w.length >= 5 && t.slice(0, 5) === w.slice(0, 5)) s = 1;
        if (s > best) best = s;
      }
      total += best;
    }
    if (total > 0) scored.push({ e, s: total, i });
  });
  scored.sort((a, b) => b.s - a.s || a.i - b.i);
  return scored.slice(0, limit).map((x) => x.e);
}
