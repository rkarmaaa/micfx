<script lang="ts">
  // Un avviso alla volta, il più importante. Si aggiorna da solo quando il problema sparisce.
  import { CABLE_NAME, isMac } from '../util/platform';
  import type { ErrorCode } from '../api';
  import { api } from '../api';
  import { Info, TriangleAlert, X } from '../icons';
  import { app } from '../stores/app.svelte';
  import { STATUS_TEXT } from '../util/messages';

  const ORDER: ErrorCode[] = [
    'cable_missing',
    'cable_busy',
    'discord_wrong_input',
    'discord_filters_sounds',
    'mic_not_routed',
    'default_is_cable',
    'discord_push_to_talk',
    'rate_mismatch',
  ];
  let dismissed = $state<ErrorCode[]>([]);

  const code = $derived(ORDER.find((c) => app.status.includes(c) && !dismissed.includes(c)));
  const text = $derived(code ? STATUS_TEXT[code] : undefined);
</script>

{#if code && text}
  <div class="banner" class:minor={code === 'rate_mismatch'} role="status">
    <span class="sign" aria-hidden="true">{#if code === 'rate_mismatch'}<Info size={18} />{:else}<TriangleAlert size={18} />{/if}</span>
    <div class="text">
      <strong>{text.title}</strong>
      <span>{text.body}</span>
    </div>
    <div class="actions">
      {#if code === 'cable_missing'}
        <button type="button" class="primary sm" onclick={() => api.openLink(isMac ? 'blackhole' : 'vb-cable')}>Scarica {CABLE_NAME}</button>
        <button type="button" class="sm" onclick={() => (app.guideOpen = true)}>Guida</button>
      {:else if code === 'mic_not_routed'}
        <button type="button" class="primary sm" onclick={() => api.openSoundSettings(1)}>Apri impostazioni microfono</button>
        <button type="button" class="sm" onclick={() => (app.guideOpen = true)}>Come fare</button>
      {:else if code === 'discord_wrong_input' || code === 'discord_filters_sounds'}
        <button type="button" class="primary sm" onclick={() => api.openLink('discord-app')}>Apri Discord</button>
      {:else if code === 'discord_push_to_talk'}
        <button type="button" class="primary sm" onclick={() => app.openSettings('ptt')}>Apri impostazioni</button>
      {:else if code === 'default_is_cable'}
        <button type="button" class="primary sm" onclick={() => api.openSoundSettings(0)}>Apri impostazioni audio</button>
      {:else}
        <button type="button" class="sm" onclick={() => api.openSoundSettings(0)}>Apri impostazioni audio</button>
      {/if}
      <button type="button" class="icon sm" title="Nascondi fino al prossimo avvio" aria-label="Nascondi avviso" onclick={() => (dismissed = [...dismissed, code])}
        ><X size={15} /></button
      >
    </div>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 12px 16px 0;
    padding: 10px 10px 10px 14px;
    border: 1px solid var(--warn-border);
    border-radius: var(--r-xl);
    background: var(--warn-soft);
    flex: none;
  }
  .banner.minor {
    border-color: var(--border);
    background: var(--surface);
  }
  .sign {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--r-md);
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    color: var(--warn);
    flex: none;
  }
  .minor .sign {
    background: var(--primary-soft);
    color: var(--primary-text);
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    font-size: var(--text-md);
  }
  .text strong {
    font-weight: 600;
  }
  .text span {
    color: var(--fg-2);
    font-size: var(--text-sm);
  }
  .actions {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  /* Finestre piccole: resta il titolo con le azioni, la spiegazione è nella guida. */
  @media (max-width: 760px), (max-height: 560px) {
    .text span {
      display: none;
    }
    .banner {
      padding-block: 7px;
    }
    .sign {
      width: 28px;
      height: 28px;
    }
  }
</style>
