<script lang="ts">
  import { app, type SortMode, type TileSize, type ViewMode } from '../stores/app.svelte';

  let { searchInput = $bindable() }: { searchInput?: HTMLInputElement } = $props();

  const playingCount = $derived(app.playing.length);
</script>

<header class="bar">
  <div class="search">
    <span aria-hidden="true">🔍</span>
    <input
      bind:this={searchInput}
      bind:value={app.search}
      type="search"
      placeholder="Cerca suoni…  (Ctrl+F)"
      aria-label="Cerca suoni"
      spellcheck="false"
      onkeydown={(e) => {
        if (e.key === 'Escape' && app.search) {
          e.stopPropagation();
          app.search = '';
        } else if (e.key === 'Enter' && app.visible.length) {
          // Invio nella ricerca suona il primo risultato.
          e.preventDefault();
          const first = app.visible[0];
          app.select(first.id, 'single');
          if (e.shiftKey) app.preview(first.id);
          else app.play(first.id);
        } else if (e.key === 'ArrowDown' && app.visible.length) {
          e.preventDefault();
          app.select(app.visible[0].id, 'single');
          (document.querySelector(`[data-sound-id="${CSS.escape(app.visible[0].id)}"]`) as HTMLElement | null)?.focus();
        }
      }}
    />
  </div>

  <div class="seg" role="radiogroup" aria-label="Vista">
    {#each [['grid', '▦', 'Griglia (Ctrl+1)'], ['list', '☰', 'Elenco (Ctrl+2)']] as [v, icon, label]}
      <button
        type="button"
        role="radio"
        aria-checked={app.prefs.view === v}
        title={label}
        aria-label={label}
        onclick={() => app.setPrefs({ view: v as ViewMode })}>{icon}</button
      >
    {/each}
  </div>

  {#if app.prefs.view === 'grid'}
    <select
      aria-label="Dimensione riquadri"
      title="Dimensione riquadri"
      value={app.prefs.size}
      onchange={(e) => app.setPrefs({ size: e.currentTarget.value as TileSize })}
    >
      <option value="s">Piccoli</option>
      <option value="m">Medi</option>
      <option value="l">Grandi</option>
    </select>
  {/if}

  <select
    aria-label="Ordina per"
    title="Ordina per"
    value={app.prefs.sort}
    onchange={(e) => app.setPrefs({ sort: e.currentTarget.value as SortMode })}
  >
    <option value="manual">Ordine personale</option>
    <option value="name">Nome A–Z</option>
    <option value="recent">Aggiunti di recente</option>
  </select>

  <span class="spacer"></span>

  {#if app.hotkeysPaused}
    <button type="button" class="paused" title="Le scorciatoie globali sono in pausa. Clic per riattivarle." onclick={() => app.togglePause()}
      >⏸ Scorciatoie in pausa</button
    >
  {/if}

  <button
    type="button"
    class="stop"
    disabled={playingCount === 0}
    title="Ferma tutti i suoni (Esc)"
    onclick={() => app.stopAll()}
  >
    ■ Ferma tutto{playingCount > 1 ? ` (${playingCount})` : ''}
  </button>
  <button type="button" class="primary" title="Importa file audio (Ctrl+O)" onclick={() => app.pickAndImport()}>＋ Aggiungi suoni</button>
  <button type="button" class="icon" title="Guida configurazione Discord" aria-label="Guida configurazione" onclick={() => (app.guideOpen = true)}
    >❔</button
  >
  <button type="button" class="icon" title="Impostazioni (Ctrl+,)" aria-label="Impostazioni" onclick={() => (app.settingsOpen = true)}>⚙</button>
</header>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex-wrap: wrap;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1 1 220px;
    max-width: 380px;
  }
  .search input {
    flex: 1;
  }
  .seg {
    display: inline-flex;
  }
  .seg button {
    border-radius: 0;
    width: 34px;
  }
  .seg button:first-child {
    border-radius: 4px 0 0 4px;
  }
  .seg button:last-child {
    border-radius: 0 4px 4px 0;
    margin-left: -1px;
  }
  .seg [aria-checked='true'] {
    background: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }
  .spacer {
    flex: 1;
  }
  .paused {
    color: var(--warn);
    border-color: var(--warn);
  }
  .stop:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
