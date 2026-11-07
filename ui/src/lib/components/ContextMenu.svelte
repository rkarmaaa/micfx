<script lang="ts">
  import { kb } from '../util/platform';
  import {
    Check,
    ChevronDown,
    ChevronRight,
    FolderInput,
    FolderPlus,
    Headphones,
    Pencil,
    Play,
    SlidersHorizontal,
    Smile,
    Square,
    Trash,
  } from '../icons';
  import { app } from '../stores/app.svelte';

  let { x, y, ids, onclose }: { x: number; y: number; ids: string[]; onclose: () => void } = $props();

  let root: HTMLElement;
  let folderOpen = $state(false);
  const one = $derived(ids.length === 1 ? app.sound(ids[0]) : undefined);
  const playing = $derived(ids.some((id) => app.playingBySound.has(id)));
  /** Cartella in cui sono già tutti i suoni del menu (undefined se sono sparsi). */
  const sameFolder = $derived.by(() => {
    const set = new Set(ids.map((id) => app.sound(id)?.folder_id ?? null));
    return set.size === 1 ? [...set][0] : undefined;
  });

  // Posizione misurata sul menu vero (cresce aprendo "Sposta in cartella"): come i menu di Windows, se non
  // c'è spazio a destra o sotto si apre a sinistra o sopra il cursore, e resta sempre dentro la finestra.
  const EDGE = 8;
  let size = $state({ w: 0, h: 0 });
  const pos = $derived.by(() => {
    const vw = window.innerWidth, vh = window.innerHeight;
    const left = x + size.w > vw - EDGE ? x - size.w : x;
    const top = y + size.h > vh - EDGE ? y - size.h : y;
    return {
      left: Math.max(EDGE, Math.min(left, vw - size.w - EDGE)),
      top: Math.max(EDGE, Math.min(top, vh - size.h - EDGE)),
    };
  });

  const measure = () => (size = { w: root.offsetWidth, h: root.offsetHeight });

  // Subito, prima del primo disegno; poi a ogni apertura del sottomenu e a ogni cambio (nuove cartelle).
  $effect(() => {
    void folderOpen;
    measure();
  });
  $effect(() => {
    const ro = new ResizeObserver(measure);
    ro.observe(root);
    return () => ro.disconnect();
  });

  $effect(() => {
    root.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const away = (e: MouseEvent) => !root.contains(e.target as Node) && onclose();
    const t = setTimeout(() => {
      document.addEventListener('mousedown', away);
      window.addEventListener('blur', onclose);
      window.addEventListener('resize', onclose);
    }, 0);
    return () => {
      clearTimeout(t);
      document.removeEventListener('mousedown', away);
      window.removeEventListener('blur', onclose);
      window.removeEventListener('resize', onclose);
    };
  });

  // Prima l'azione, poi la chiusura: chiudendo, `ids` e `one` non esistono più.
  function run(f: () => void) {
    f();
    onclose();
  }

  function onkeydown(e: KeyboardEvent) {
    const items = [...root.querySelectorAll<HTMLElement>('[role="menuitem"]')];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      items[(i + 1) % items.length]?.focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      items[(i - 1 + items.length) % items.length]?.focus();
    } else if (e.key === 'Tab') {
      e.preventDefault();
    }
  }

  function emojiAt() {
    const r = root.getBoundingClientRect();
    return { x: r.left, y: r.top };
  }
</script>

<div
  bind:this={root}
  class="popover menu"
  role="menu"
  tabindex="-1"
  aria-label={one ? `Azioni per ${one.name}` : `${ids.length} suoni`}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  {onkeydown}
>
  {#if one}
    <button role="menuitem" type="button" class="menu-item" onclick={() => run(() => app.play(one.id))}
      ><Play /><span>Riproduci</span><kbd>Invio</kbd></button
    >
    <button role="menuitem" type="button" class="menu-item" onclick={() => run(() => app.preview(one.id))}
      ><Headphones /><span>Ascolta solo tu</span><kbd>{kb('Maiusc+Invio')}</kbd></button
    >
    {#if playing}
      <button role="menuitem" type="button" class="menu-item" onclick={() => run(() => app.stop(one.id))}><Square /><span>Ferma</span></button>
    {/if}
    <hr class="menu-sep" />
    <button role="menuitem" type="button" class="menu-item" onclick={() => run(() => (app.renamingId = one.id))}
      ><Pencil /><span>Rinomina</span><kbd>F2</kbd></button
    >
    <button
      role="menuitem"
      type="button"
      class="menu-item"
      onclick={() => {
        const p = emojiAt();
        run(() => (app.emojiTarget = { soundId: one.id, ...p }));
      }}><Smile /><span>Cambia emoji</span><kbd>{kb('Ctrl+E')}</kbd></button
    >
    <button role="menuitem" type="button" class="menu-item" onclick={() => run(() => (app.editorId = one.id))}
      ><SlidersHorizontal /><span>Scorciatoia, volume e modalità…</span><kbd>{kb('Ctrl+Invio')}</kbd></button
    >
  {:else}
    <button role="menuitem" type="button" class="menu-item" onclick={() => run(() => ids.forEach((id) => app.play(id)))}
      ><Play /><span>Riproduci tutti</span></button
    >
    {#if playing}
      <button role="menuitem" type="button" class="menu-item" onclick={() => run(() => ids.forEach((id) => app.stop(id)))}><Square /><span>Ferma</span></button>
    {/if}
  {/if}
  <hr class="menu-sep" />
  <button role="menuitem" type="button" class="menu-item" aria-haspopup="true" aria-expanded={folderOpen} onclick={() => (folderOpen = !folderOpen)}
    ><FolderInput /><span>Sposta in cartella</span>{#if folderOpen}<ChevronDown />{:else}<ChevronRight />{/if}</button
  >
  {#if folderOpen}
    <div class="sub">
      <button role="menuitem" type="button" class="menu-item" aria-current={sameFolder === null} onclick={() => run(() => app.moveToFolder(ids, null))}
        ><span>Nessuna cartella</span>{#if sameFolder === null}<Check />{/if}</button
      >
      {#each app.folders as f (f.id)}
        <button role="menuitem" type="button" class="menu-item" aria-current={sameFolder === f.id} onclick={() => run(() => app.moveToFolder(ids, f.id))}
          ><span>{f.name}</span>{#if sameFolder === f.id}<Check />{/if}</button
        >
      {/each}
      <label class="new-folder">
        <FolderPlus />
        <input
          placeholder="Nuova cartella…"
          aria-label="Nome della nuova cartella, poi Invio"
          maxlength="40"
          onkeydown={async (e) => {
            e.stopPropagation();
            if (e.key === 'Escape') return onclose();
            if (e.key !== 'Enter') return;
            e.preventDefault();
            const name = e.currentTarget.value;
            if (!name.trim()) return;
            const targets = [...ids];
            onclose();
            // Se esiste già una cartella con questo nome si usa quella.
            const f = await app.createFolder(name);
            if (f) await app.moveToFolder(targets, f.id);
          }}
        />
      </label>
    </div>
  {/if}
  <hr class="menu-sep" />
  <button role="menuitem" type="button" class="menu-item danger" onclick={() => run(() => app.deleteSounds(ids))}
    ><Trash /><span>Elimina{ids.length > 1 ? ` ${ids.length} suoni` : ''}</span><kbd>Canc</kbd></button
  >
</div>

<style>
  .menu {
    min-width: 260px;
    max-width: 320px;
    max-height: calc(100vh - 16px);
    overflow-y: auto;
  }
  .sub {
    display: flex;
    flex-direction: column;
    margin: 2px 0 2px 14px;
    padding-left: 6px;
    border-left: 1px solid var(--border);
    max-height: 200px;
    overflow: auto;
  }
  .sub [aria-current='true'] {
    font-weight: 600;
  }
  .sub [aria-current='true'] :global(.lucide) {
    color: var(--primary-text);
  }
  .new-folder {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 2px;
    padding-left: 8px;
    color: var(--fg-muted);
  }
  .new-folder input {
    flex: 1;
    min-width: 0;
    height: var(--h-sm);
  }
</style>
