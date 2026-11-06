<script lang="ts">
  import type { Hotkey, PlayMode, Playing, Sound } from '../api';
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
    <div class="progress" style:transform="scaleX({progress})" aria-hidden="true"></div>
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
      <button type="button" class="icon" data-action="stop" title="Ferma" aria-label="Ferma {sound.name}">■</button>
    {/if}
    <button type="button" class="icon" data-action="preview" title="Ascolta solo tu (Maiusc+Invio)" aria-label="Anteprima solo in cuffia">🎧</button>
    <button type="button" class="icon" data-action="edit" title="Modifica (Ctrl+Invio)" aria-label="Modifica {sound.name}">✎</button>
    <button type="button" class="icon" data-action="menu" title="Altre azioni" aria-label="Altre azioni">⋯</button>
  </div>
  {#if sound.mode && sound.mode !== defaultMode && view === 'grid'}
    <span class="mode-dot" title="Modalità: {MODE_LABEL[sound.mode]}">{sound.mode === 'toggle' ? '⏯' : sound.mode === 'restart' ? '↺' : sound.mode === 'exclusive' ? '①' : '≋'}</span>
  {/if}
</div>

<style>
  .item {
    position: relative;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    cursor: pointer;
    user-select: none;
    outline: none;
  }
  .item:hover {
    border-color: var(--border-strong);
  }
  .item.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .item.focused:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .item.playing {
    border-color: var(--ok);
  }
  .item.preview {
    border-style: dashed;
  }
  .item.fresh {
    animation: fresh 1.2s ease-out 2;
  }
  @keyframes fresh {
    0% {
      box-shadow: 0 0 0 3px var(--accent);
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
    top: 4px;
    bottom: 4px;
    width: 3px;
    background: var(--accent);
    z-index: 2;
  }
  .grid.drop-before::before {
    left: 0;
  }
  .grid.drop-after::after {
    right: 0;
  }
  .list.drop-before {
    box-shadow: inset 0 3px 0 var(--accent);
  }
  .list.drop-after {
    box-shadow: inset 0 -3px 0 var(--accent);
  }
  .progress {
    position: absolute;
    inset: 0;
    background: var(--ok-soft);
    transform-origin: left;
    pointer-events: none;
  }

  /* --- griglia --- */
  .grid {
    width: var(--tile);
    height: var(--tile);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 8px 6px;
    text-align: center;
  }
  .grid .emoji {
    font-size: calc(var(--tile) * 0.36);
    line-height: 1.1;
    position: relative;
  }
  .grid .name {
    position: relative;
    font-size: 13px;
    line-height: 1.2;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-word;
  }
  .grid .hk {
    position: absolute;
    top: 4px;
    left: 4px;
    font-size: 10px;
    max-width: calc(100% - 34px);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .grid .actions {
    position: absolute;
    top: 2px;
    right: 2px;
    display: none;
    flex-direction: column;
  }
  .grid:hover .actions,
  .grid.focused:focus-within .actions {
    display: flex;
  }
  .grid.playing .actions {
    display: flex;
  }
  .grid.playing .actions [data-action]:not([data-action='stop']) {
    display: none;
  }
  .grid:hover.playing .actions [data-action] {
    display: inline-flex;
  }
  .mode-dot {
    position: absolute;
    bottom: 3px;
    right: 5px;
    font-size: 11px;
    color: var(--fg-muted);
  }

  /* --- elenco --- */
  .list {
    display: grid;
    grid-template-columns: 32px 1fr 64px 84px auto auto;
    align-items: center;
    gap: 10px;
    padding: 4px 10px;
    min-height: 40px;
  }
  .list .emoji {
    font-size: 22px;
    position: relative;
    text-align: center;
  }
  .list .name {
    position: relative;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .list .meta {
    position: relative;
    font-size: 12px;
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }
  .list .hk {
    position: relative;
  }
  .list .actions {
    position: relative;
    display: flex;
    gap: 2px;
    opacity: 0;
  }
  .list:hover .actions,
  .list.focused:focus-within .actions,
  .list.playing .actions {
    opacity: 1;
  }
  .list:not(:has(.hk)) .actions {
    grid-column: 6;
  }

  .hk {
    font-size: 11px;
  }
  .hk.conflict {
    border-color: var(--danger);
    color: var(--danger);
  }
  .rename {
    position: relative;
    width: 100%;
    font: inherit;
    font-size: 13px;
  }
  .actions .icon {
    width: 24px;
    height: 24px;
    font-size: 13px;
  }
</style>
