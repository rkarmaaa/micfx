// Tastiera: KeyboardEvent → nomi del core (posizione fisica, come `KeyboardEvent.code`) e
// viceversa → etichette leggibili secondo il layout attivo (es. "è" su tastiera italiana).
// Su Mac il modificatore "Win" del core è ⌘ (Command) e le etichette usano i simboli di macOS (⌃⌥⇧⌘).

import { isMac } from './platform';

const CODE_TO_NAME: Record<string, string> = {
  Space: 'Space', Enter: 'Enter', NumpadEnter: 'Enter', Tab: 'Tab', Backspace: 'Backspace', Escape: 'Escape',
  Insert: 'Insert', Delete: 'Delete', Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown',
  ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
  Minus: 'Minus', Equal: 'Equal', BracketLeft: 'BracketLeft', BracketRight: 'BracketRight', Backslash: 'Backslash',
  Semicolon: 'Semicolon', Quote: 'Quote', Comma: 'Comma', Period: 'Period', Slash: 'Slash', Backquote: 'Backquote',
  IntlBackslash: 'IntlBackslash', Pause: 'Pause', ScrollLock: 'ScrollLock', PrintScreen: 'PrintScreen', CapsLock: 'CapsLock',
  NumpadAdd: 'NumpadAdd', NumpadSubtract: 'NumpadSubtract', NumpadMultiply: 'NumpadMultiply', NumpadDivide: 'NumpadDivide',
  NumpadDecimal: 'NumpadDecimal',
  MediaPlayPause: 'MediaPlayPause', MediaStop: 'MediaStop', MediaTrackNext: 'MediaTrackNext',
  MediaTrackPrevious: 'MediaTrackPrevious', AudioVolumeMute: 'AudioVolumeMute', AudioVolumeDown: 'AudioVolumeDown',
  AudioVolumeUp: 'AudioVolumeUp',
};

const MODIFIER_CODES = new Set([
  'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight', 'ShiftLeft', 'ShiftRight', 'MetaLeft', 'MetaRight', 'OSLeft', 'OSRight',
]);

export function keyNameFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (/^Numpad[0-9]$/.test(code)) return code;
  return CODE_TO_NAME[code] ?? null;
}

const NAME_TO_CODE: Record<string, string> = (() => {
  const m: Record<string, string> = {};
  for (const [code, name] of Object.entries(CODE_TO_NAME)) if (!(name in m)) m[name] = code;
  for (let c = 65; c <= 90; c++) m[String.fromCharCode(c)] = `Key${String.fromCharCode(c)}`;
  for (let d = 0; d <= 9; d++) m[String(d)] = `Digit${d}`;
  return m;
})();

export type Captured =
  | { kind: 'partial'; display: string }
  | { kind: 'done'; accelerator: string; noModifiers: boolean }
  | { kind: 'cancel' }
  | { kind: 'clear' };

/** Interpreta un keydown durante la cattura di una scorciatoia. */
export function captureFromEvent(e: KeyboardEvent): Captured {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push('Ctrl');
  if (e.altKey) mods.push('Alt');
  if (e.shiftKey) mods.push('Shift');
  if (e.metaKey) mods.push('Win');
  if (MODIFIER_CODES.has(e.code)) {
    return { kind: 'partial', display: mods.length ? formatParts([...mods, '…']) : '…' };
  }
  if (mods.length === 0 && e.code === 'Escape') return { kind: 'cancel' };
  if (mods.length === 0 && (e.code === 'Backspace' || e.code === 'Delete')) return { kind: 'clear' };
  const name = keyNameFromCode(e.code);
  if (!name) return { kind: 'partial', display: 'Tasto non supportato' };
  return { kind: 'done', accelerator: [...mods, name].join('+'), noModifiers: mods.length === 0 };
}

/** Un solo tasto, senza modificatori (per il push-to-talk). */
export function singleKeyFromEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  return keyNameFromCode(e.code);
}

let layout: Map<string, string> | null = null;
let layoutListeners: (() => void)[] = [];

/** Carica la mappa del layout di tastiera (Chromium/WebView2). Facoltativa. */
export async function loadKeyboardLayout(): Promise<void> {
  const kb = (navigator as unknown as { keyboard?: { getLayoutMap?: () => Promise<Map<string, string>> } }).keyboard;
  if (!kb?.getLayoutMap) return;
  try {
    layout = await kb.getLayoutMap();
    layoutListeners.forEach((f) => f());
  } catch {
    /* non disponibile */
  }
}

export function onLayoutLoaded(f: () => void) {
  layoutListeners.push(f);
}

const MOD_LABELS: Record<string, string> = isMac
  ? { Ctrl: '⌃', Alt: '⌥', Shift: '⇧', Win: '⌘' }
  : { Ctrl: 'Ctrl', Alt: 'Alt', Shift: 'Maiusc', Win: 'Win' };

const LABELS: Record<string, string> = {
  ...MOD_LABELS,
  Space: 'Spazio', Enter: 'Invio', Tab: 'Tab', Backspace: '⌫', Escape: 'Esc', Insert: 'Ins', Delete: 'Canc',
  Home: 'Home', End: 'Fine', PageUp: 'Pag ↑', PageDown: 'Pag ↓', Up: '↑', Down: '↓', Left: '←', Right: '→',
  Pause: 'Pausa', ScrollLock: 'Bloc Scorr', PrintScreen: 'Stamp', CapsLock: 'Bloc Maiusc',
  NumpadAdd: 'Num +', NumpadSubtract: 'Num −', NumpadMultiply: 'Num ×', NumpadDivide: 'Num ÷', NumpadDecimal: 'Num ,',
  MediaPlayPause: '⏯', MediaStop: '⏹', MediaTrackNext: '⏭', MediaTrackPrevious: '⏮',
  AudioVolumeMute: '🔇', AudioVolumeDown: '🔉', AudioVolumeUp: '🔊',
};

export function keyLabel(name: string): string {
  if (/^Numpad[0-9]$/.test(name)) return `Num ${name.slice(6)}`;
  if (LABELS[name]) return LABELS[name];
  const code = NAME_TO_CODE[name];
  const ch = code && layout?.get(code);
  if (ch) return ch.length === 1 ? ch.toUpperCase() : ch;
  return name;
}

function formatParts(parts: string[]): string {
  const labels = parts.map((p) => (p === '…' ? '…' : keyLabel(p)));
  // Mac: "⌃⌥⌘1", come nei menu di macOS (l'ordine del core è già quello di Apple).
  if (isMac) return labels.join('');
  return labels.join(' + ');
}

export function formatAccelerator(acc: string | null | undefined): string {
  if (!acc) return '';
  return formatParts(acc.split('+'));
}

/** Dove l'utente sta scrivendo: lì le scorciatoie della finestra non devono scattare. */
export function isTypingTarget(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  if (t.isContentEditable) return true;
  if (t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement) return true;
  if (t instanceof HTMLInputElement) return !['range', 'checkbox', 'radio', 'button'].includes(t.type);
  return false;
}
