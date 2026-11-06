<script lang="ts">
  // Cattura di una scorciatoia: clic → premi la combinazione. Esc annulla, Canc/⌫ rimuove.
  // Durante la cattura le hotkey globali sono in pausa, altrimenti Windows le "ruberebbe".
  import type { HotkeyAction } from '../api';
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
      ? 'Senza Ctrl, Alt o Maiusc questo tasto smette di funzionare nelle altre app finché MicFX è aperto.'
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
      {#if current.conflict}<span class="badge" title="Usata da un'altra app: non funziona">⚠</span>{/if}
    {:else}
      <span class="none">Nessuna</span>
    {/if}
  </button>
  {#if current && !capturing}
    <button type="button" class="icon" title="Rimuovi scorciatoia" aria-label="Rimuovi scorciatoia" onclick={() => commit(null)}>✕</button>
  {/if}
</div>
{#if capturing && !compact}
  <p class="help">Esc per annullare · Canc per rimuovere</p>
{/if}
{#if pending}
  <div class="confirm" role="alertdialog" aria-label="Scorciatoia già in uso">
    <span><kbd>{formatAccelerator(pending.accelerator)}</kbd> è già usata da {pending.owner}. Spostarla qui?</span>
    <button type="button" onclick={() => commit(pending!.accelerator)}>Sposta</button>
    <button type="button" onclick={() => (pending = null)}>Annulla</button>
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
  .capture {
    min-width: 160px;
    text-align: left;
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .compact .capture {
    min-width: 120px;
  }
  .capture.active {
    outline: 2px solid var(--accent);
  }
  .capture.conflict {
    border-color: var(--danger);
  }
  .hint,
  .none {
    color: var(--fg-muted);
  }
  .badge {
    color: var(--danger);
  }
  .help,
  .warn {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .warn {
    color: var(--warn);
  }
  .confirm {
    margin-top: 6px;
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    font-size: 13px;
  }
</style>
