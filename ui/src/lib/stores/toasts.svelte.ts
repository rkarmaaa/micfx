// Notifiche non bloccanti: errori, conferme con "Annulla", avanzamento import.

export type ToastAction = { label: string; run: () => void };

export type Toast = {
  id: number;
  kind: 'info' | 'success' | 'error' | 'progress';
  title: string;
  detail?: string;
  lines?: string[];
  actions?: ToastAction[];
  progress?: number;
  /** ms; 0 = finché l'utente non la chiude. */
  timeout: number;
  key?: string;
  /** Solo per la X: chiusura voluta dall'utente (non per timeout, azioni o finestra chiusa). */
  onclose?: () => void;
};

class Toasts {
  list = $state<Toast[]>([]);
  private seq = 1;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  show(t: Omit<Toast, 'id' | 'timeout'> & { timeout?: number }): number {
    if (t.key) {
      const existing = this.list.find((x) => x.key === t.key);
      if (existing) {
        this.update(existing.id, t);
        return existing.id;
      }
    }
    const id = this.seq++;
    const timeout = t.timeout ?? (t.kind === 'error' ? 7000 : t.kind === 'progress' ? 0 : 4000);
    this.list = [...this.list.slice(-4), { ...t, id, timeout }];
    this.arm(id, timeout);
    return id;
  }

  update(id: number, patch: Partial<Omit<Toast, 'id'>>) {
    this.list = this.list.map((t) => (t.id === id ? { ...t, ...patch } : t));
    if (patch.timeout !== undefined) this.arm(id, patch.timeout);
  }

  dismiss(id: number) {
    clearTimeout(this.timers.get(id));
    this.timers.delete(id);
    this.list = this.list.filter((t) => t.id !== id);
  }

  /** Sospende la chiusura automatica mentre il mouse è sopra (tempo per cliccare "Annulla"). */
  hold(id: number) {
    clearTimeout(this.timers.get(id));
  }

  release(id: number) {
    const t = this.list.find((x) => x.id === id);
    if (t && t.timeout > 0) this.arm(id, 2500);
  }

  private arm(id: number, timeout: number) {
    clearTimeout(this.timers.get(id));
    if (timeout > 0) this.timers.set(id, setTimeout(() => this.dismiss(id), timeout));
  }

  error(title: string, detail?: string) {
    return this.show({ kind: 'error', title, detail });
  }

  info(title: string, detail?: string) {
    return this.show({ kind: 'info', title, detail });
  }
}

export const toasts = new Toasts();
