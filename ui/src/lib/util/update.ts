// Nuove versioni: la UI chiede a GitHub l'ultima release pubblicata quando la finestra si apre (mai a
// riposo: la finestra è effimera, il core non fa rete). Al massimo una richiesta ogni CHECK_EVERY_MS.

const LATEST_URL = 'https://api.github.com/repos/rkarmaaa/micfx/releases/latest';
export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

/** Versione dell'ultima release pubblicata ("1.2.0"), `null` se GitHub non risponde. */
export async function latestVersion(): Promise<string | null> {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), 8000);
  try {
    const r = await fetch(LATEST_URL, { headers: { Accept: 'application/vnd.github+json' }, signal: ctl.signal });
    if (!r.ok) return null;
    const j = (await r.json()) as { tag_name?: unknown; draft?: unknown; prerelease?: unknown };
    if (typeof j.tag_name !== 'string' || j.draft || j.prerelease) return null;
    const v = j.tag_name.replace(/^v/i, '');
    return /^\d+\.\d+\.\d+$/.test(v) ? v : null;
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

/** `a` è più recente di `b` (versioni X.Y.Z). */
export function isNewer(a: string, b: string): boolean {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) > (pb[i] ?? 0);
  }
  return false;
}
