<script lang="ts">
  // Un avviso alla volta, il più importante. Si aggiorna da solo quando il problema sparisce.
  import type { ErrorCode } from '../api';
  import { api } from '../api';
  import { app } from '../stores/app.svelte';
  import { STATUS_TEXT } from '../util/messages';

  const ORDER: ErrorCode[] = ['cable_missing', 'cable_busy', 'mic_not_routed', 'default_is_cable', 'rate_mismatch'];
  let dismissed = $state<ErrorCode[]>([]);

  const code = $derived(ORDER.find((c) => app.status.includes(c) && !dismissed.includes(c)));
  const text = $derived(code ? STATUS_TEXT[code] : undefined);
</script>

{#if code && text}
  <div class="banner" class:minor={code === 'rate_mismatch'} role="status">
    <span class="icon" aria-hidden="true">{code === 'rate_mismatch' ? 'ℹ️' : '⚠️'}</span>
    <div class="text">
      <strong>{text.title}</strong>
      <span>{text.body}</span>
    </div>
    <div class="actions">
      {#if code === 'cable_missing'}
        <button type="button" class="primary" onclick={() => api.openLink('vb-cable')}>Scarica VB-Cable</button>
        <button type="button" onclick={() => (app.guideOpen = true)}>Guida</button>
      {:else if code === 'mic_not_routed'}
        <button type="button" class="primary" onclick={() => api.openSoundSettings(1)}>Apri impostazioni microfono</button>
        <button type="button" onclick={() => (app.guideOpen = true)}>Come fare</button>
      {:else if code === 'default_is_cable'}
        <button type="button" class="primary" onclick={() => api.openSoundSettings(0)}>Apri impostazioni audio</button>
      {:else}
        <button type="button" onclick={() => api.openSoundSettings(code === 'rate_mismatch' ? 0 : 0)}>Apri impostazioni audio</button>
      {/if}
      <button type="button" class="icon" title="Nascondi fino al prossimo avvio" aria-label="Nascondi avviso" onclick={() => (dismissed = [...dismissed, code])}
        >✕</button
      >
    </div>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 8px 12px;
    background: var(--warn-soft);
    border-bottom: 1px solid var(--warn);
  }
  .banner.minor {
    background: var(--surface-2);
    border-bottom-color: var(--border);
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    font-size: 13px;
  }
  .actions {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
</style>
