// Nuove versioni: la UI chiede al guscio (`check_update`, tauri-plugin-updater) se c'è una release più
// recente quando la finestra si apre (mai a riposo: la finestra è effimera, il core non fa rete).
// Al massimo un controllo ogni CHECK_EVERY_MS, salvo quello manuale.

export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

/** `a` è più recente di `b` (versioni X.Y.Z). */
export function isNewer(a: string, b: string): boolean {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) > (pb[i] ?? 0);
  }
  return false;
}
