<script lang="ts">
  // Cartelle come schede. Clic = filtra, doppio clic o F2 = rinomina, clic destro o Maiusc+F10 = menu.
  // Ogni scheda accetta suoni trascinati (sposta) e file trascinati da Esplora risorse (importa lì).
  import { tick } from 'svelte';

  import { Folder, Pencil, Plus, Trash } from '../icons';
  import { app } from '../stores/app.svelte';
  import { toasts } from '../stores/toasts.svelte';

  let { dropTarget }: { dropTarget: string | null } = $props();

  const MENU_W = 248;
  const MENU_H = 132;

  let adding = $state(false);
  let renaming = $state<string | null>(null);
  let menu = $state<{ id: string; x: number; y: number } | null>(null);
  let tabs: HTMLElement;
  let fadeL = $state(false);
  let fadeR = $state(false);

  /** Sfumature ai bordi solo quando ci sono schede nascoste da quel lato. */
  function updateFades() {
    if (!tabs) return;
    fadeL = tabs.scrollLeft > 2;
    fadeR = tabs.scrollLeft + tabs.clientWidth < tabs.scrollWidth - 2;
  }

  $effect(() => {
    app.folders.length;
    adding;
    const ro = new ResizeObserver(updateFades);
    ro.observe(tabs);
    updateFades();
    return () => ro.disconnect();
  });

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function focusTab(id: string) {
    await tick();
    tabs?.querySelector<HTMLElement>(`[role="tab"][data-folder-drop="${CSS.escape(id)}"]`)?.focus();
  }

  // La scheda attiva resta visibile anche con tante cartelle.
  $effect(() => {
    const id = app.prefs.folder;
    tabs?.querySelector<HTMLElement>(`[role="tab"][data-folder-drop="${CSS.escape(id)}"]`)?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  });

  /** Chiude il campo "nuova cartella" una volta sola. Invio, Esc e la perdita del focus (che
   *  arriva anche quando il campo sparisce dopo Invio) non possono creare due cartelle. */
  async function finishAdd(value: string | null) {
    if (!adding) return;
    adding = false;
    if (value === null || !value.trim()) return;
    const existing = app.folderByName(value);
    if (existing) {
      app.setPrefs({ folder: existing.id });
      toasts.info(`La cartella «${existing.name}» esiste già`, 'Ora è quella selezionata.');
      return;
    }
    const f = await app.createFolder(value);
    if (f) {
      app.setPrefs({ folder: f.id });
      void focusTab(f.id);
    }
  }

  function finishRename(id: string, value: string | null, refocus: boolean) {
    if (renaming !== id) return;
    renaming = null;
    if (value !== null) void app.renameFolder(id, value);
    if (refocus) void focusTab(id);
  }

  function remove(id: string) {
    menu = null;
    void app.deleteFolder(id);
  }

  function openMenu(id: string, x: number, y: number) {
    menu = {
      id,
      x: Math.max(4, Math.min(x, window.innerWidth - MENU_W - 4)),
      y: Math.max(4, Math.min(y, window.innerHeight - MENU_H - 4)),
    };
  }

  function closeMenu(refocus: boolean) {
    const id = menu?.id;
    menu = null;
    if (refocus && id) void focusTab(id);
  }

  /** Menu: focus sul primo elemento, si chiude cliccando fuori, cambiando finestra o ridimensionando. */
  function menuSetup(node: HTMLElement) {
    node.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const away = (e: MouseEvent) => !node.contains(e.target as Node) && closeMenu(false);
    const close = () => closeMenu(false);
    // Dopo il clic che l'ha aperto.
    const t = setTimeout(() => {
      document.addEventListener('mousedown', away);
      window.addEventListener('blur', close);
      window.addEventListener('resize', close);
    }, 0);
    return {
      destroy() {
        clearTimeout(t);
        document.removeEventListener('mousedown', away);
        window.removeEventListener('blur', close);
        window.removeEventListener('resize', close);
      },
    };
  }

  function menuKey(e: KeyboardEvent) {
    const items = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="menuitem"]')];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      closeMenu(true);
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

  function keyNav(e: KeyboardEvent) {
    // Nei campi di testo le frecce muovono il cursore, non la scheda.
    if (e.target instanceof HTMLInputElement) return;
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft' && e.key !== 'Home' && e.key !== 'End') return;
    e.preventDefault();
    const list = [...tabs.querySelectorAll<HTMLElement>('[role="tab"]')];
    const i = list.indexOf(document.activeElement as HTMLElement);
    const j =
      e.key === 'Home' ? 0 : e.key === 'End' ? list.length - 1 : (i + (e.key === 'ArrowRight' ? 1 : -1) + list.length) % list.length;
    list[j]?.focus();
    list[j]?.click();
  }

  /** La rotellina scorre le schede in orizzontale quando non ci stanno tutte. */
  function wheelScroll(node: HTMLElement) {
    const onwheel = (e: WheelEvent) => {
      if (e.ctrlKey || !e.deltaY || node.scrollWidth <= node.clientWidth) return;
      e.preventDefault();
      node.scrollLeft += e.deltaY;
    };
    node.addEventListener('wheel', onwheel, { passive: false });
    return { destroy: () => node.removeEventListener('wheel', onwheel) };
  }
</script>

<div
  class="tabs"
  class:fade-l={fadeL}
  class:fade-r={fadeR}
  role="tablist"
  aria-label="Cartelle"
  tabindex="-1"
  bind:this={tabs}
  onkeydown={keyNav}
  onscroll={updateFades}
  use:wheelScroll
>
  <button
    type="button"
    role="tab"
    aria-selected={app.prefs.folder === 'all'}
    tabindex={app.prefs.folder === 'all' ? 0 : -1}
    class:drop={dropTarget === 'all'}
    data-folder-drop="all"
    onclick={() => app.setPrefs({ folder: 'all' })}
  >
    <span class="label">Tutti</span> <span class="count">{app.sounds.length}</span>
  </button>
  {#each app.folders as f (f.id)}
    {#if renaming === f.id}
      <input
        class="rename"
        value={f.name}
        maxlength="40"
        aria-label="Nuovo nome della cartella"
        use:focusSelect
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            finishRename(f.id, e.currentTarget.value, true);
          } else if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            finishRename(f.id, null, true);
          }
        }}
        onblur={(e) => finishRename(f.id, e.currentTarget.value, false)}
      />
    {:else}
      <button
        type="button"
        role="tab"
        aria-selected={app.prefs.folder === f.id}
        tabindex={app.prefs.folder === f.id ? 0 : -1}
        class:drop={dropTarget === f.id}
        data-folder-drop={f.id}
        title="{f.name} · doppio clic per rinominare · clic destro per altre azioni"
        onclick={() => app.setPrefs({ folder: f.id })}
        ondblclick={() => (renaming = f.id)}
        oncontextmenu={(e) => {
          e.preventDefault();
          openMenu(f.id, e.clientX, e.clientY);
        }}
        onkeydown={(e) => {
          if (e.key === 'F2') {
            e.preventDefault();
            renaming = f.id;
          } else if (e.key === 'Delete') {
            e.preventDefault();
            remove(f.id);
          } else if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
            e.preventDefault();
            const r = e.currentTarget.getBoundingClientRect();
            openMenu(f.id, r.left, r.bottom + 2);
          }
        }}
      >
        <Folder size={14} />
        <span class="label">{f.name}</span>
        <span class="count">{app.folderCounts.get(f.id) ?? 0}</span>
      </button>
    {/if}
  {/each}
  {#if adding}
    <input
      class="rename"
      placeholder="Nome cartella"
      maxlength="40"
      aria-label="Nome della nuova cartella, poi Invio"
      use:focusSelect
      onkeydown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          void finishAdd(e.currentTarget.value);
        } else if (e.key === 'Escape') {
          e.preventDefault();
          e.stopPropagation();
          void finishAdd(null);
        }
      }}
      onblur={(e) => void finishAdd(e.currentTarget.value)}
    />
  {:else}
    <button type="button" class="add" title="Nuova cartella" onclick={() => (adding = true)}><Plus size={15} /> Nuova cartella</button>
  {/if}
</div>

{#if menu}
  <div
    class="popover fmenu"
    role="menu"
    tabindex="-1"
    aria-label="Azioni della cartella"
    style:left="{menu.x}px"
    style:top="{menu.y}px"
    style:width="{MENU_W}px"
    use:menuSetup
    onkeydown={menuKey}
  >
    <button
      role="menuitem"
      type="button"
      class="menu-item"
      onclick={() => {
        renaming = menu?.id ?? null;
        menu = null;
      }}><Pencil /><span>Rinomina</span><kbd>F2</kbd></button
    >
    <hr class="menu-sep" />
    <button role="menuitem" type="button" class="menu-item danger" onclick={() => menu && remove(menu.id)}
      ><Trash /><span>Elimina cartella</span><kbd>Canc</kbd></button
    >
    <p>I suoni restano nella libreria, in «Tutti».</p>
  </div>
{/if}

<style>
  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    outline: none;
    padding: 2px;
    margin: -2px;
  }
  .tabs::-webkit-scrollbar {
    display: none;
  }
  .tabs.fade-l {
    mask-image: linear-gradient(to right, transparent, #000 28px);
  }
  .tabs.fade-r {
    mask-image: linear-gradient(to left, transparent, #000 28px);
  }
  .tabs.fade-l.fade-r {
    mask-image: linear-gradient(to right, transparent, #000 28px, #000 calc(100% - 28px), transparent);
  }

  [role='tab'],
  .add {
    flex: none;
    height: 30px;
    gap: 7px;
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: var(--r-md);
    background: transparent;
    box-shadow: none;
    color: var(--fg-muted);
    font-weight: 500;
  }
  [role='tab']:hover:not(:disabled),
  .add:hover:not(:disabled) {
    background: var(--surface-2);
    border-color: transparent;
    color: var(--fg);
  }
  [role='tab'][aria-selected='true'],
  [role='tab'][aria-selected='true']:hover:not(:disabled) {
    background: var(--surface);
    border-color: var(--border-strong);
    box-shadow: var(--shadow-xs);
    color: var(--fg);
    font-weight: 600;
  }
  [role='tab'] :global(.lucide) {
    color: var(--fg-faint);
  }
  [role='tab'][aria-selected='true'] :global(.lucide) {
    color: var(--primary-text);
  }
  .label {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--fg-muted);
    font-size: var(--text-xs);
    font-weight: 550;
    line-height: 17px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }
  [role='tab'][aria-selected='true'] .count {
    background: var(--primary-soft);
    color: var(--primary-text);
  }
  .drop,
  .drop:hover:not(:disabled) {
    background: var(--primary-soft);
    border: 1px dashed var(--primary);
    color: var(--fg);
  }
  .add {
    margin-left: 4px;
    color: var(--fg-faint);
  }
  .rename {
    flex: none;
    width: 170px;
    height: 30px;
  }
  .fmenu {
    padding: 4px;
  }
  .fmenu p {
    margin: 4px 8px 4px;
    color: var(--fg-muted);
    font-size: var(--text-xs);
  }
</style>
