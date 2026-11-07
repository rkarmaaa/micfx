<script lang="ts">
  // Vista della libreria: griglia/elenco, dimensione dei riquadri, ordinamento.
  import { LayoutGrid, List } from '../icons';
  import { app, type SortMode, type TileSize, type ViewMode } from '../stores/app.svelte';

  const VIEWS: { v: ViewMode; label: string; icon: typeof LayoutGrid }[] = [
    { v: 'grid', label: 'Griglia (Ctrl+1)', icon: LayoutGrid },
    { v: 'list', label: 'Elenco (Ctrl+2)', icon: List },
  ];
</script>

<div class="controls">
  {#if app.prefs.view === 'grid'}
    <select
      class="sm size"
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
    class="sm"
    aria-label="Ordina per"
    title="Ordina per"
    value={app.prefs.sort}
    onchange={(e) => app.setPrefs({ sort: e.currentTarget.value as SortMode })}
  >
    <option value="manual">Ordine personale</option>
    <option value="name">Nome A–Z</option>
    <option value="recent">Più recenti</option>
  </select>
  <div class="seg" role="radiogroup" aria-label="Vista">
    {#each VIEWS as { v, label, icon: Icon } (v)}
      <button type="button" role="radio" aria-checked={app.prefs.view === v} title={label} aria-label={label} onclick={() => app.setPrefs({ view: v })}>
        <Icon size={16} />
      </button>
    {/each}
  </div>
</div>

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  @media (max-width: 760px) {
    .size {
      display: none;
    }
  }
  .seg {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--surface-2);
  }
  .seg button {
    width: 28px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    box-shadow: none;
    color: var(--fg-muted);
  }
  .seg button:hover:not(:disabled) {
    background: transparent;
    color: var(--fg);
  }
  .seg [aria-checked='true'],
  .seg [aria-checked='true']:hover:not(:disabled) {
    background: var(--surface);
    color: var(--fg);
    box-shadow: var(--shadow-xs), 0 0 0 1px var(--border);
  }
</style>
