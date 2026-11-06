<script lang="ts">
  // Cartelle come schede. Clic = filtra, doppio clic = rinomina, clic destro = menu.
  // Ogni scheda accetta suoni trascinati (sposta) e file trascinati da Esplora risorse (importa lì).
  import { app } from '../stores/app.svelte';
  import { toasts } from '../stores/toasts.svelte';

  let { dropTarget }: { dropTarget: string | null } = $props();

  let adding = $state(false);
  let renaming = $state<string | null>(null);
  let menu = $state<{ id: string; x: number; y: number } | null>(null);

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function add(name: string) {
    adding = false;
    if (!name.trim()) return;
    const f = await app.createFolder(name);
    if (f) app.setPrefs({ folder: f.id });
  }

  async function remove(id: string) {
    menu = null;
    const f = app.folders.find((x) => x.id === id);
    if (!f) return;
    const members = app.sounds.filter((s) => s.folder_id === id).map((s) => s.id);
    await app.deleteFolder(id);
    toasts.show({
      kind: 'info',
      title: `Cartella "${f.name}" eliminata`,
      detail: members.length ? `${members.length} suoni restano in "Tutti".` : undefined,
      timeout: 8000,
      actions: [
        {
          label: 'Annulla',
          run: async () => {
            const nf = await app.createFolder(f.name);
            if (nf && members.length) await app.moveToFolder(members, nf.id);
            if (nf) app.setPrefs({ folder: nf.id });
          },
        },
      ],
    });
  }

  function keyNav(e: KeyboardEvent) {
    if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
    const tabs = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="tab"]')];
    const i = tabs.indexOf(document.activeElement as HTMLElement);
    const next = tabs[(i + (e.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length];
    next?.focus();
    next?.click();
  }
</script>

<div class="tabs" role="tablist" aria-label="Cartelle" tabindex="-1" onkeydown={keyNav}>
  <button
    type="button"
    role="tab"
    aria-selected={app.prefs.folder === 'all'}
    tabindex={app.prefs.folder === 'all' ? 0 : -1}
    class:drop={dropTarget === 'all'}
    data-folder-drop="all"
    onclick={() => app.setPrefs({ folder: 'all' })}
  >
    Tutti <span class="count">{app.sounds.length}</span>
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
          if (e.key === 'Enter') e.currentTarget.blur();
          if (e.key === 'Escape') {
            e.stopPropagation();
            renaming = null;
          }
        }}
        onblur={(e) => {
          if (renaming === f.id) app.renameFolder(f.id, e.currentTarget.value);
          renaming = null;
        }}
      />
    {:else}
      <button
        type="button"
        role="tab"
        aria-selected={app.prefs.folder === f.id}
        tabindex={app.prefs.folder === f.id ? 0 : -1}
        class:drop={dropTarget === f.id}
        data-folder-drop={f.id}
        title="Doppio clic per rinominare · Clic destro per altre azioni"
        onclick={() => app.setPrefs({ folder: f.id })}
        ondblclick={() => (renaming = f.id)}
        oncontextmenu={(e) => {
          e.preventDefault();
          menu = { id: f.id, x: e.clientX, y: e.clientY };
        }}
        onkeydown={(e) => {
          if (e.key === 'F2') renaming = f.id;
          if (e.key === 'Delete') remove(f.id);
        }}
      >
        📁 {f.name} <span class="count">{app.folderCounts.get(f.id) ?? 0}</span>
      </button>
    {/if}
  {/each}
  {#if adding}
    <input
      class="rename"
      placeholder="Nome cartella"
      maxlength="40"
      aria-label="Nome della nuova cartella"
      use:focusSelect
      onkeydown={(e) => {
        if (e.key === 'Enter') add(e.currentTarget.value);
        if (e.key === 'Escape') {
          e.stopPropagation();
          adding = false;
        }
      }}
      onblur={(e) => add(e.currentTarget.value)}
    />
  {:else}
    <button type="button" class="add" title="Nuova cartella" aria-label="Nuova cartella" onclick={() => (adding = true)}>＋ Cartella</button>
  {/if}
</div>

{#if menu}
  <div
    class="fmenu"
    role="menu"
    tabindex="-1"
    style:left="{menu.x}px"
    style:top="{menu.y}px"
    onmouseleave={() => (menu = null)}
  >
    <button role="menuitem" type="button" onclick={() => ((renaming = menu!.id), (menu = null))}>Rinomina <kbd>F2</kbd></button>
    <button role="menuitem" type="button" class="danger" onclick={() => remove(menu!.id)}>Elimina cartella <kbd>Canc</kbd></button>
    <p>I suoni non vengono eliminati: tornano in "Tutti".</p>
  </div>
{/if}

<style>
  .tabs {
    display: flex;
    gap: 4px;
    padding: 6px 12px 0;
    overflow-x: auto;
    overflow-y: hidden;
    border-bottom: 1px solid var(--border);
    outline: none;
    flex: none;
  }
  [role='tab'],
  .add {
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    background: none;
    padding: 6px 10px;
    white-space: nowrap;
  }
  [role='tab'][aria-selected='true'] {
    border-color: var(--border);
    background: var(--bg);
    font-weight: 600;
    margin-bottom: -1px;
  }
  .drop {
    outline: 2px dashed var(--accent);
    outline-offset: -2px;
  }
  .count {
    font-size: 11px;
    color: var(--fg-muted);
    margin-left: 2px;
  }
  .add {
    color: var(--fg-muted);
  }
  .rename {
    width: 140px;
    margin-bottom: 2px;
  }
  .fmenu {
    position: fixed;
    z-index: 60;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.25);
    max-width: 240px;
  }
  .fmenu button {
    display: flex;
    justify-content: space-between;
    background: none;
    border: none;
    padding: 6px 8px;
    text-align: left;
  }
  .fmenu button:hover {
    background: var(--surface-2);
  }
  .fmenu p {
    margin: 4px 8px;
    font-size: 11px;
    color: var(--fg-muted);
  }
  .danger {
    color: var(--danger);
  }
</style>
