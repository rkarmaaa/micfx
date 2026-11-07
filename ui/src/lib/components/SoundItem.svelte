<script lang="ts">
  import type { Hotkey, PlayMode, Playing, Sound } from '../api';
  import { CircleDot, Ellipsis, Headphones, Layers, Pencil, RotateCcw, Square, ToggleRight } from '../icons';
  import { soundEmoji } from '../stores/app.svelte';
  import { formatAccelerator } from '../util/keys';
  import { formatDuration } from '../util/volume';

  let {
    sound,
    view,
    selected,
    focused,
    fresh,
    hotkey,
    playing,
    now,
    renaming,
    drop,
    dragging,
    defaultMode,
    onrename,
    oncancelrename,
  }: {
    sound: Sound;
    view: 'grid' | 'list';
    selected: boolean;
    focused: boolean;
    fresh: boolean;
    hotkey: Hotkey | undefined;
    playing: Playing | undefined;
    now: number;
    renaming: boolean;
    drop: 'before' | 'after' | null;
    dragging: boolean;
    defaultMode: PlayMode;
    onrename: (name: string) => void;
    oncancelrename: () => void;
  } = $props();

  const MODE_ICON = { overlap: Layers, restart: RotateCcw, toggle: ToggleRight, exclusive: CircleDot };
  const MODE_LABEL: Record<PlayMode, string> = {
    overlap: 'Sovrapponi',
    restart: 'Riavvia',
    toggle: 'Play/Stop',
    exclusive: 'Esclusivo',
  };

  const durationMs = $derived((sound.frames / 48000) * 1000);
  const progress = $derived(playing ? Math.min(1, Math.max(0, (now - playing.started_at_ms) / Math.max(1, playing.duration_ms))) : 0);
  const remaining = $derived(playing ? Math.max(0, playing.duration_ms - (now - playing.started_at_ms)) : 0);

  let input = $state<HTMLInputElement>();
  let done = false;

  $effect(() => {
    if (renaming && input) {
      done = false;
      input.focus();
      input.select();
    }
  });

  function finish(commit: boolean) {
    if (done) return;
    done = true;
    if (commit && input) onrename(input.value);
    else oncancelrename();
  }

  function onkey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Enter') {
      e.preventDefault();
      finish(true);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      finish(false);
    }
  }
</script>

<div
  class="item {view}"
  class:selected
  class:focused
  class:fresh
  class:playing={!!playing}
  class:preview={playing?.preview}
  class:drop-before={drop === 'before'}
  class:drop-after={drop === 'after'}
  class:dragging
  data-sound-id={sound.id}
  role="option"
  aria-selected={selected}
  aria-label="{sound.name}{hotkey ? `, scorciatoia ${formatAccelerator(hotkey.accelerator)}` : ''}{playing ? ', in riproduzione' : ''}"
  tabindex={focused ? 0 : -1}
  title={renaming ? undefined : `${sound.name}\nClic: riproduci · Clic destro: altre azioni`}
>
  {#if playing}
    <!-- Ritaglio a parte: il riquadro resta overflow visibile, altrimenti la griglia con
         aspect-ratio calcola righe più basse dei riquadri (Chromium) e si sovrappongono. -->
    <div class="fx" aria-hidden="true">
      <div class="progress" style:transform="scaleX({progress})"></div>
      <div class="progress-line" style:transform="scaleX({progress})"></div>
    </div>
  {/if}
  <span class="emoji emoji-font" aria-hidden="true">{soundEmoji(sound)}</span>

  {#if renaming}
    <input
      bind:this={input}
      class="rename"
      value={sound.name}
      maxlength="80"
      aria-label="Nuovo nome"
      onkeydown={onkey}
      onblur={() => finish(true)}
      onclick={(e) => e.stopPropagation()}
      onpointerdown={(e) => e.stopPropagation()}
    />
  {:else}
    <span class="name">{sound.name}</span>
  {/if}

  {#if view === 'list'}
    <span class="meta duration">{playing ? `−${formatDuration(remaining)}` : formatDuration(durationMs)}</span>
    <span class="meta mode" title="Modalità di riproduzione">{sound.mode ? MODE_LABEL[sound.mode] : ''}</span>
  {/if}

  {#if hotkey}
    <kbd class="hk" class:conflict={hotkey.conflict} title={hotkey.conflict ? "Usata da un'altra app: non funziona" : 'Scorciatoia globale'}
      >{formatAccelerator(hotkey.accelerator)}</kbd
    >
  {/if}

  <div class="actions">
    {#if playing}
      <button type="button" class="act" data-action="stop" title="Ferma" aria-label="Ferma {sound.name}"><Square size={12} fill="currentColor" /></button>
    {/if}
    <button type="button" class="act" data-action="preview" title="Ascolta solo tu (Maiusc+Invio)" aria-label="Anteprima solo in cuffia"
      ><Headphones size={14} /></button
    >
    <button type="button" class="act" data-action="edit" title="Modifica (Ctrl+Invio)" aria-label="Modifica {sound.name}"><Pencil size={14} /></button>
    <button type="button" class="act" data-action="menu" title="Altre azioni" aria-label="Altre azioni"><Ellipsis size={14} /></button>
  </div>
  {#if sound.mode && sound.mode !== defaultMode && view === 'grid'}
    {@const ModeIcon = MODE_ICON[sound.mode]}
    <span class="mode-dot" title="Modalità: {MODE_LABEL[sound.mode]}"><ModeIcon size={13} /></span>
  {/if}
  {#if playing?.preview}
    <span class="preview-badge" title="Anteprima: la senti solo tu"><Headphones size={12} /></span>
  {/if}
</div>

<style>
  .item {
    --tone: var(--primary);
    --tone-soft: var(--primary-soft);
    position: relative;
    border: 1px solid var(--border);
    border-radius: var(--r-xl);
    background: var(--surface);
    box-shadow: var(--shadow-xs);
    cursor: default;
    user-select: none;
    outline: none;
    transition:
      border-color var(--t-fast) var(--ease),
      box-shadow var(--t-fast) var(--ease),
      background var(--t-fast) var(--ease),
      transform var(--t-fast) var(--ease);
  }
  .item:hover {
    border-color: var(--border-hover);
    box-shadow: var(--shadow-sm);
  }
  .item:active:not(:has(.actions :active)) {
    transform: scale(0.985);
  }
  .item.selected {
    border-color: var(--primary);
    background: color-mix(in srgb, var(--primary) 6%, var(--surface));
    box-shadow: 0 0 0 1px var(--primary);
  }
  .item.focused:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .item.preview {
    --tone: var(--fg-muted);
    --tone-soft: color-mix(in srgb, var(--fg) 7%, transparent);
  }
  .item.fresh {
    animation: fresh 1.1s var(--ease) 2;
  }
  @keyframes fresh {
    0% {
      box-shadow: 0 0 0 4px var(--primary-soft-2);
    }
    100% {
      box-shadow: 0 0 0 0 transparent;
    }
  }
  .item.dragging {
    opacity: 0.35;
  }
  .grid.drop-before::before,
  .grid.drop-after::after {
    content: '';
    position: absolute;
    top: 8px;
    bottom: 8px;
    width: 3px;
    border-radius: 2px;
    background: var(--primary);
    z-index: 3;
  }
  .grid.drop-before::before {
    left: 2px;
  }
  .grid.drop-after::after {
    right: 2px;
  }
  .list.drop-before {
    box-shadow: inset 0 3px 0 var(--primary);
  }
  .list.drop-after {
    box-shadow: inset 0 -3px 0 var(--primary);
  }

  /* --- riproduzione: velatura che avanza + linea in basso --- */
  .fx {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    overflow: hidden;
    pointer-events: none;
  }
  .progress {
    position: absolute;
    inset: 0;
    background: var(--tone-soft);
    transform-origin: left;
    pointer-events: none;
  }
  .progress-line {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: var(--tone);
    transform-origin: left;
    pointer-events: none;
    z-index: 2;
  }
  .item.playing {
    border-color: color-mix(in srgb, var(--tone) 55%, var(--border));
  }
  .item.playing.selected {
    border-color: var(--primary);
  }

  /* --- griglia --- */
  .grid {
    width: 100%;
    aspect-ratio: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 10px 8px 12px;
    text-align: center;
  }
  .grid .emoji {
    position: relative;
    font-size: calc(var(--tile) * 0.34);
    line-height: 1.1;
    transition: transform var(--t-med) var(--ease);
  }
  .grid:hover .emoji {
    transform: scale(1.06);
  }
  .grid .name {
    position: relative;
    max-width: 100%;
    color: var(--fg);
    font-size: var(--text-md);
    font-weight: 550;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-word;
  }
  .grid .hk {
    position: absolute;
    top: 7px;
    left: 7px;
    max-width: calc(100% - 14px);
    overflow: hidden;
    text-overflow: ellipsis;
    z-index: 1;
  }
  .grid .actions {
    position: absolute;
    top: 5px;
    right: 5px;
    display: flex;
    gap: 1px;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    opacity: 0;
    transform: translateY(-2px);
    pointer-events: none;
    transition:
      opacity var(--t-fast) var(--ease),
      transform var(--t-fast) var(--ease);
    z-index: 2;
  }
  .grid:hover .actions,
  .grid.focused:focus-visible .actions,
  .grid:has(.actions :focus-visible) .actions,
  .grid.playing .actions {
    opacity: 1;
    transform: none;
    pointer-events: auto;
  }
  .grid.playing:not(:hover) .actions [data-action]:not([data-action='stop']) {
    display: none;
  }
  .mode-dot,
  .preview-badge {
    position: absolute;
    bottom: 7px;
    display: inline-flex;
    color: var(--fg-faint);
    z-index: 1;
  }
  .mode-dot {
    right: 8px;
  }
  .preview-badge {
    left: 8px;
    color: var(--fg-muted);
  }

  /* --- elenco --- */
  .list {
    display: grid;
    /* Colonne fisse: ogni riga è una griglia a sé, così restano allineate anche senza scorciatoia. */
    grid-template-columns: 32px minmax(0, 1fr) 56px 84px 124px 110px;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 4px 8px 4px 10px;
    border-color: transparent;
    border-radius: var(--r-lg);
    background: transparent;
    box-shadow: none;
  }
  .list:hover {
    border-color: transparent;
    box-shadow: none;
    background: var(--surface-2);
  }
  .list.selected {
    border-color: var(--primary);
    box-shadow: none;
  }
  .list .emoji {
    position: relative;
    font-size: 22px;
    text-align: center;
  }
  .list .name {
    position: relative;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 550;
  }
  .list .meta {
    position: relative;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .list .duration {
    text-align: right;
  }
  .list .hk {
    position: relative;
    grid-column: 5;
    justify-self: start;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .list .actions {
    position: relative;
    grid-column: 6;
    justify-self: end;
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease);
  }
  .list:hover .actions,
  .list.focused:focus-visible .actions,
  .list:has(.actions :focus-visible) .actions,
  .list.playing .actions {
    opacity: 1;
  }

  .hk {
    height: 18px;
    padding: 0 5px;
    font-size: 10.5px;
  }
  .hk.conflict {
    border-color: var(--danger);
    color: var(--danger);
  }
  .rename {
    position: relative;
    width: 100%;
    height: 28px;
    font-size: var(--text-md);
    text-align: inherit;
  }
  .act {
    width: 26px;
    height: 26px;
    padding: 0;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    box-shadow: none;
    color: var(--fg-muted);
  }
  .act:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--fg);
  }
  .act[data-action='stop'] {
    color: var(--danger);
  }
</style>
