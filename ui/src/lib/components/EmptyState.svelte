<script lang="ts">
  import { app } from '../stores/app.svelte';

  const folder = $derived(app.prefs.folder === 'all' ? null : app.folders.find((f) => f.id === app.prefs.folder));
</script>

<div class="empty">
  {#if app.sounds.length === 0}
    <div class="big" aria-hidden="true">🎛️</div>
    <h2>Nessun suono, per ora</h2>
    <p>Trascina qui file audio o cartelle, oppure scegli i file.</p>
    <button type="button" class="primary" onclick={() => app.pickAndImport()}>＋ Aggiungi suoni</button>
    <p class="small">MP3, WAV, FLAC, OGG, M4A · i volumi vengono livellati in automatico</p>
    {#if app.status.includes('cable_missing') || app.status.includes('mic_not_routed')}
      <p class="small">Prima volta? <button type="button" class="link" onclick={() => (app.guideOpen = true)}>Configura Discord in 3 passi</button></p>
    {/if}
  {:else if app.search.trim()}
    <div class="big" aria-hidden="true">🔍</div>
    <h2>Nessun suono per «{app.search.trim()}»</h2>
    {#if folder}
      <p>Stai cercando solo in «{folder.name}».</p>
      <button type="button" onclick={() => app.setPrefs({ folder: 'all' })}>Cerca in tutti i suoni</button>
    {/if}
    <button type="button" class="link" onclick={() => (app.search = '')}>Cancella ricerca</button>
  {:else if folder}
    <div class="big" aria-hidden="true">📁</div>
    <h2>«{folder.name}» è vuota</h2>
    <p>Trascina qui file audio, oppure trascina dei suoni sulla scheda della cartella.</p>
    <button type="button" class="primary" onclick={() => app.pickAndImport()}>＋ Aggiungi suoni qui</button>
  {/if}
</div>

<style>
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px;
    text-align: center;
  }
  .big {
    font-size: 48px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  p {
    margin: 0;
    color: var(--fg-muted);
  }
  .small {
    font-size: 12px;
  }
</style>
