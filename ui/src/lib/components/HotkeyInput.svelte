<script lang="ts">
  // Cattura di una scorciatoia: clic → premi la combinazione. Esc annulla, Canc/⌫ rimuove.
  // Durante la cattura le hotkey globali sono in pausa, altrimenti il sistema le "ruberebbe".
  // Su Mac ⌘ è registrato come modificatore "Win" del core (Command).
  import { isMac } from '../util/platform';
  import type { HotkeyAction } from '../api';
  import { TriangleAlert, X } from '../icons';
  import { app } from '../stores/app.svelte';
  import { captureFromEvent, formatAccelerator } from '../util/keys';

  let {
    action,
    label = 'Scorciatoia',
    compact = false,
  }: { action: HotkeyAction; label?: string; compact?: boolean } = $props();

  let capturing = $state(false);
  let partial = $state('');
  let pending = $state<{ accelerator: string; owner: string } | null>(null);
  let warn = $state('');
  let button: HTMLButtonElement;

  const current = $derived(app.hotkeyFor(action));

  function ownerName(a: HotkeyAction): string {
    switch (a.type) {
      case 'play':
        return `"${app.sound(a.sound_id)?.name ?? 'un suono'}"`;
      case 'preview':
        return `l'anteprima di "${app.sound(a.sound_id)?.name ?? 'un suono'}"`;
      case 'stop_all':
        return '"Ferma tutti i suoni"';
      case 'pause_hotkeys':
        return '"Pausa scorciatoie"';
      case 'mute_monitor':
        return '"Attiva/disattiva cuffie"';
      case 'mute_cable':
        return '"Attiva/disattiva microfono"';
      case 'open_window':
        return '"Apri MicFX"';
    }
  }

  async function start() {
    pending = null;
    warn = '';
    partial = '';
    capturing = true;
    await app.beginCapture();
  }

  async function stop() {
    capturing = false;
    partial = '';
    await app.endCapture();
  }

  async function commit(accelerator: string | null) {
    await stop();
    const h = await app.setHotkey(action, accelerator);
    if (h === undefined) return;
    button?.focus();
  }

  async function onkeydown(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    e.stopPropagation();
    const c = captureFromEvent(e);
    if (c.kind === 'partial') {
      partial = c.display;
      return;
    }
    if (c.kind === 'cancel') return stop();
    if (c.kind === 'clear') return commit(null);
    warn = c.noModifiers
      ? isMac
        ? 'Senza ⌘, ⌥, ⌃ o ⇧ questo tasto smette di funzionare nelle altre app finché MicFX è aperto.'
        : 'Senza Ctrl, Alt o Maiusc questo tasto smette di funzionare nelle altre app finché MicFX è aperto.'
      : '';
    const owner = app.hotkeyOwner(c.accelerator, action);
    if (owner) {
      await stop();
      pending = { accelerator: c.accelerator, owner: ownerName(owner.action) };
      return;
    }
    await commit(c.accelerator);
  }
</script>

<div class="hotkey" class:compact>
  <button
    bind:this={button}
    type="button"
    class="capture"
    class:active={capturing}
    class:conflict={current?.conflict}
    aria-label="{label}: {current ? formatAccelerator(current.accelerator) : 'nessuna'}. Premi per cambiarla"
    onclick={() => (capturing ? stop() : start())}
    onkeydown={onkeydown}
    onblur={() => capturing && stop()}
  >
    {#if capturing}
      <span class="hint">{partial || 'Premi la combinazione…'}</span>
    {:else if current}
      <kbd>{formatAccelerator(current.accelerator)}</kbd>
      {#if current.conflict}<span class="badge" title="Usata da un'altra app: non funziona"><TriangleAlert size={14} /></span>{/if}
    {:else}
      <span class="none">Nessuna</span>
    {/if}
  </button>
  {#if current && !capturing}
    <button type="button" class="icon" title="Rimuovi scorciatoia" aria-label="Rimuovi scorciatoia" onclick={() => commit(null)}><X size={15} /></button>
  {:else}
    <!-- Stesso ingombro con e senza scorciatoia: i campi restano allineati in colonna. -->
    <span class="x-space" aria-hidden="true"></span>
  {/if}
</div>
{#if capturing && !compact}
  <p class="help">Esc per annullare · Canc per rimuovere</p>
{/if}
{#if pending}
  <div class="confirm" role="alertdialog" aria-label="Scorciatoia già in uso">
    <span><kbd>{formatAccelerator(pending.accelerator)}</kbd> è già usata da {pending.owner}. Spostarla qui?</span>
    <button type="button" class="primary sm" onclick={() => commit(pending!.accelerator)}>Sposta</button>
    <button type="button" class="sm" onclick={() => (pending = null)}>Annulla</button>
  </div>
{/if}
{#if current?.conflict && !capturing}
  <p class="warn">Un'altra app usa già questa combinazione: scegline un'altra.</p>
{:else if warn && !capturing}
  <p class="warn">{warn}</p>
{/if}

<style>
  .hotkey {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .x-space {
    width: var(--h-md);
    flex: none;
  }
  .capture {
    justify-content: flex-start;
    min-width: 180px;
    padding: 0 8px;
    background: var(--field);
    font-weight: 450;
  }
  .compact .capture {
    min-width: 150px;
  }
  .capture.active,
  .capture.active:hover:not(:disabled) {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px var(--primary-soft-2);
  }
  .capture.conflict {
    border-color: var(--danger);
  }
  .hint,
  .none {
    color: var(--fg-faint);
  }
  .hint {
    color: var(--primary-text);
  }
  .badge {
    display: inline-flex;
    color: var(--danger);
  }
  .help,
  .warn {
    margin: 6px 0 0;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .warn {
    color: var(--warn);
  }
  .confirm {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--surface-2);
    font-size: var(--text-md);
  }
  .confirm span {
    flex: 1 1 220px;
  }
</style>
