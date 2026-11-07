<script lang="ts">
  import { AudioLines, FolderOpen, Plus, Search } from '../icons';
  import { app } from '../stores/app.svelte';

  const folder = $derived(app.prefs.folder === 'all' ? null : app.folders.find((f) => f.id === app.prefs.folder));
</script>

<div class="empty">
  {#if app.sounds.length === 0}
    <div class="big" aria-hidden="true"><AudioLines size={26} /></div>
    <h2>Nessun suono, per ora</h2>
    <p>Trascina qui file audio o cartelle, oppure scegli i file.</p>
    <button type="button" class="primary" onclick={() => app.pickAndImport()}><Plus size={16} /> Aggiungi suoni</button>
    <p class="small">MP3, WAV, FLAC, OGG, M4A · i volumi vengono livellati in automatico</p>
    {#if app.status.includes('cable_missing') || app.status.includes('mic_not_routed')}
      <p class="small">Prima volta? <button type="button" class="link" onclick={() => (app.guideOpen = true)}>Configura Discord in 3 passi</button></p>
    {/if}
  {:else if app.search.trim()}
    <div class="big" aria-hidden="true"><Search size={24} /></div>
    <h2>Nessun suono per «{app.search.trim()}»</h2>
    {#if folder}
      <p>Stai cercando solo in «{folder.name}».</p>
      <button type="button" onclick={() => app.setPrefs({ folder: 'all' })}>Cerca in tutti i suoni</button>
    {/if}
    <button type="button" class="link" onclick={() => (app.search = '')}>Cancella ricerca</button>
  {:else if folder}
    <div class="big" aria-hidden="true"><FolderOpen size={24} /></div>
    <h2>«{folder.name}» è vuota</h2>
    <p>Trascina qui file audio, oppure trascina dei suoni sulla scheda della cartella.</p>
    <button type="button" class="primary" onclick={() => app.pickAndImport()}><Plus size={16} /> Aggiungi suoni qui</button>
  {/if}
</div>

<style>
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 32px 24px;
    text-align: center;
  }
  .big {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    margin-bottom: 6px;
    border: 1px solid var(--border);
    border-radius: var(--r-2xl);
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    color: var(--primary-text);
  }
  h2 {
    font-size: var(--text-lg);
  }
  p {
    max-width: 420px;
    color: var(--fg-muted);
    font-size: var(--text-md);
  }
  p + button,
  h2 + button {
    margin-top: 6px;
  }
  .small {
    font-size: var(--text-sm);
    color: var(--fg-faint);
  }
</style>
