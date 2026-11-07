// Sistema operativo della WebView: Windows (WebView2) o macOS (WKWebView).
// Su Mac le scorciatoie della finestra usano ⌘ al posto di Ctrl, e i testi parlano di BlackHole e
// delle Impostazioni di Sistema invece di VB-Cable e del pannello audio di Windows.
// Nel mock (`/mock.html?mac`) si può forzare la variante Mac.

function detectMac(): boolean {
  if (typeof navigator === 'undefined') return false;
  try {
    const q = new URLSearchParams(location.search);
    if (q.has('mac')) return q.get('mac') !== '0';
  } catch {
    /* location non disponibile */
  }
  const uaPlatform = (navigator as unknown as { userAgentData?: { platform?: string } }).userAgentData?.platform;
  return /mac/i.test(uaPlatform ?? navigator.platform ?? '') || /Mac OS X/.test(navigator.userAgent);
}

export const isMac = detectMac();

/** Il tasto principale delle scorciatoie della finestra è premuto: ⌘ su Mac, Ctrl altrove. */
export function primaryMod(e: KeyboardEvent | MouseEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

/** Nome del tasto principale per le etichette. */
export const MOD = isMac ? '⌘' : 'Ctrl';

const MAC_WORDS: [RegExp, string][] = [
  [/\bCtrl\b/g, '⌘'],
  [/\bMaiusc\b/g, '⇧'],
  [/\bAlt\b/g, '⌥'],
  [/\bInvio\b/g, '↩'],
];

/**
 * Testo di una scorciatoia della finestra, scritto per Windows ("Ctrl+F", "Ctrl + Maiusc + M"):
 * su Mac diventa "⌘F", "⌘ + ⇧ + M". Su Windows resta com'è.
 */
export function kb(text: string): string {
  if (!isMac) return text;
  let out = text;
  for (const [re, sym] of MAC_WORDS) out = out.replace(re, sym);
  // "⌘+F" → "⌘F" (stile dei menu di macOS); con gli spazi resta leggibile così com'è.
  return out.replace(/([⌘⇧⌥⌃])\+/g, '$1');
}

/** Nome del sistema nei testi ("Avvia con Windows" / "Avvia all'accesso al Mac"). */
export const OS_NAME = isMac ? 'macOS' : 'Windows';

/** Nome del cavo virtuale consigliato. */
export const CABLE_NAME = isMac ? 'BlackHole' : 'VB-Cable';
