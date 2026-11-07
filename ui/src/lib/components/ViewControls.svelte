<script lang="ts">
  // Vista della libreria: griglia/elenco, dimensione dei riquadri, ordinamento.
  import { kb } from '../util/platform';
  import { LayoutGrid, List } from '../icons';
  import { app, type SortMode, type TileSize, type ViewMode } from '../stores/app.svelte';
  import Select from './Select.svelte';

  const SIZES: { value: TileSize; label: string }[] = [
    { value: 's', label: 'Piccoli' },
    { value: 'm', label: 'Medi' },
    { value: 'l', label: 'Grandi' },
  ];
  const SORTS: { value: SortMode; label: string }[] = [
    { value: 'manual', label: 'Ordine personale' },
    { value: 'name', label: 'Nome A–Z' },
    { value: 'recent', label: 'Più recenti' },
  ];

  const VIEWS: { v: ViewMode; label: string; icon: typeof LayoutGrid }[] = [
    { v: 'grid', label: kb('Griglia (Ctrl+1)'), icon: LayoutGrid },
    { v: 'list', label: kb('Elenco (Ctrl+2)'), icon: List },
  ];
</script>

<div class="controls">
  {#if app.prefs.view === 'grid'}
    <Select size="sm" class="size" label="Dimensione riquadri" title="Dimensione riquadri" value={app.prefs.size} options={SIZES} onchange={(v) => app.setPrefs({ size: v })} />
  {/if}
  <Select size="sm" label="Ordina per" title="Ordina per" value={app.prefs.sort} options={SORTS} onchange={(v) => app.setPrefs({ sort: v })} />
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
    .controls :global(.size) {
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
