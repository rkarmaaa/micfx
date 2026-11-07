<script lang="ts">
  // Barra superiore (#202020 in entrambi i temi): marchio, ricerca, azioni principali.
  import { CircleHelp, Mic, Pause, Plus, Search, Settings, Square, X } from '../icons';
  import { app } from '../stores/app.svelte';

  let { searchInput = $bindable() }: { searchInput?: HTMLInputElement } = $props();

  const playingCount = $derived(app.playing.length);
</script>

<header class="bar">
  <div class="brand">
    <span class="logo" aria-hidden="true"><Mic size={15} strokeWidth={2.4} /></span>
    <span class="word">MicFX</span>
  </div>

  <div class="search">
    <Search size={16} class="lead" />
    <input
      bind:this={searchInput}
      bind:value={app.search}
      type="search"
      placeholder="Cerca suoni"
      aria-label="Cerca suoni (Ctrl+F)"
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
    {#if app.search}
      <button
        type="button"
        class="clear"
        aria-label="Cancella ricerca"
        title="Cancella (Esc)"
        onclick={() => {
          app.search = '';
          searchInput?.focus();
        }}><X size={14} /></button
      >
    {:else}
      <kbd class="hint" aria-hidden="true">Ctrl F</kbd>
    {/if}
  </div>

  <span class="spacer"></span>

  {#if app.hotkeysPaused}
    <button type="button" class="paused" title="Le scorciatoie globali sono in pausa. Clic per riattivarle." onclick={() => app.togglePause()}>
      <Pause size={14} /><span class="label">Scorciatoie in pausa</span>
    </button>
  {/if}

  <button
    type="button"
    class="stop"
    class:live={playingCount > 0}
    disabled={playingCount === 0}
    title="Ferma tutti i suoni (Esc)"
    aria-label="Ferma tutti i suoni"
    onclick={() => app.stopAll()}
  >
    <Square size={13} fill="currentColor" />
    <span class="label">Ferma tutto</span>
    {#if playingCount > 1}<span class="count">{playingCount}</span>{/if}
  </button>
  <button type="button" class="primary add" title="Importa file audio (Ctrl+O)" onclick={() => app.pickAndImport()}>
    <Plus size={16} strokeWidth={2.4} /><span class="label">Aggiungi suoni</span>
  </button>

  <span class="divider" aria-hidden="true"></span>

  <button type="button" class="tool" title="Guida configurazione Discord" aria-label="Guida configurazione" onclick={() => (app.guideOpen = true)}>
    <CircleHelp size={18} />
  </button>
  <button type="button" class="tool" title="Impostazioni (Ctrl+,)" aria-label="Impostazioni" onclick={() => app.openSettings()}>
    <Settings size={18} />
  </button>
</header>

<style>
  .bar {
    --focus: rgb(129 140 248 / 0.9);
    display: flex;
    align-items: center;
    gap: 8px;
    height: var(--bar-h);
    padding: 0 10px 0 14px;
    background: var(--bar-bg);
    color: var(--bar-fg);
    border-bottom: 1px solid var(--bar-border);
    flex: none;
    color-scheme: dark;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-right: 10px;
    user-select: none;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--primary);
    color: #fff;
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.18);
  }
  .word {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 400px;
    min-width: 150px;
  }
  .search :global(.lead) {
    position: absolute;
    left: 11px;
    color: var(--bar-muted);
    pointer-events: none;
  }
  .search input {
    width: 100%;
    height: 34px;
    padding: 0 58px 0 35px;
    border: 1px solid #383838;
    border-radius: var(--r-md);
    background: var(--bar-field);
    color: var(--bar-fg);
    box-shadow: none;
    font-size: var(--text-md);
  }
  .search input::placeholder {
    color: #8b8b93;
  }
  .search input:hover:not(:focus) {
    border-color: #474747;
    background: var(--bar-field-hover);
  }
  .search input:focus {
    border-color: var(--primary);
    background: #262626;
    box-shadow: 0 0 0 3px rgb(99 102 241 / 0.35);
  }
  .hint {
    position: absolute;
    right: 8px;
    height: 20px;
    border-color: #3d3d3d;
    background: #262626;
    box-shadow: none;
    color: #8b8b93;
    pointer-events: none;
  }
  .clear {
    position: absolute;
    right: 5px;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: var(--r-sm);
    background: none;
    box-shadow: none;
    color: var(--bar-muted);
  }
  .clear:hover:not(:disabled) {
    background: var(--bar-hover);
    color: var(--bar-fg);
  }

  .spacer {
    flex: 1;
  }

  .stop,
  .paused {
    height: 34px;
    border-color: #383838;
    background: transparent;
    box-shadow: none;
    color: var(--bar-fg);
  }
  .stop:hover:not(:disabled),
  .paused:hover:not(:disabled) {
    background: var(--bar-hover);
    border-color: #4a4a4a;
  }
  .stop:disabled {
    opacity: 0.38;
  }
  .stop.live {
    border-color: rgb(248 113 113 / 0.45);
    background: rgb(248 113 113 / 0.12);
    color: #fecaca;
  }
  .stop.live:hover:not(:disabled) {
    background: rgb(248 113 113 / 0.2);
    border-color: rgb(248 113 113 / 0.6);
  }
  .count {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 999px;
    background: rgb(248 113 113 / 0.25);
    font-size: var(--text-xs);
    line-height: 18px;
    font-variant-numeric: tabular-nums;
  }
  .paused {
    border-color: rgb(251 191 36 / 0.45);
    color: #fcd34d;
  }
  .add {
    height: 34px;
    padding: 0 14px 0 11px;
  }

  .divider {
    width: 1px;
    height: 20px;
    margin: 0 4px;
    background: #3a3a3a;
  }
  .tool {
    width: 34px;
    height: 34px;
    padding: 0;
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--bar-muted);
  }
  .tool:hover:not(:disabled) {
    background: var(--bar-hover);
    color: var(--bar-fg);
  }
  .tool:active:not(:disabled) {
    background: var(--bar-active);
  }

  @media (max-width: 760px) {
    .word {
      display: none;
    }
    .brand {
      margin-right: 4px;
    }
  }
  @media (max-width: 640px) {
    .stop .label,
    .paused .label,
    .add .label,
    .hint {
      display: none;
    }
    .stop,
    .paused,
    .add {
      width: 34px;
      padding: 0;
    }
    .search input {
      padding-right: 32px;
    }
  }
</style>
