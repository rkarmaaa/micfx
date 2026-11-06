<script lang="ts">
  import { app } from '../stores/app.svelte';

  let { x, y, ids, onclose }: { x: number; y: number; ids: string[]; onclose: () => void } = $props();

  let root: HTMLElement;
  let folderOpen = $state(false);
  const one = $derived(ids.length === 1 ? app.sound(ids[0]) : undefined);
  const playing = $derived(ids.some((id) => app.playingBySound.has(id)));

  const pos = $derived({
    left: Math.max(4, Math.min(x, window.innerWidth - 240)),
    top: Math.max(4, Math.min(y, window.innerHeight - 360)),
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
  class="menu"
  role="menu"
  tabindex="-1"
  aria-label={one ? `Azioni per ${one.name}` : `${ids.length} suoni`}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  {onkeydown}
>
  {#if one}
    <button role="menuitem" type="button" onclick={() => run(() => app.play(one.id))}>▶ Riproduci <kbd>Invio</kbd></button>
    <button role="menuitem" type="button" onclick={() => run(() => app.preview(one.id))}>🎧 Ascolta solo tu <kbd>Maiusc+Invio</kbd></button>
    {#if playing}
      <button role="menuitem" type="button" onclick={() => run(() => app.stop(one.id))}>■ Ferma</button>
    {/if}
    <hr />
    <button role="menuitem" type="button" onclick={() => run(() => (app.renamingId = one.id))}>Rinomina <kbd>F2</kbd></button>
    <button
      role="menuitem"
      type="button"
      onclick={() => {
        const p = emojiAt();
        run(() => (app.emojiTarget = { soundId: one.id, ...p }));
      }}>Cambia emoji <kbd>Ctrl+E</kbd></button
    >
    <button role="menuitem" type="button" onclick={() => run(() => (app.editorId = one.id))}>Scorciatoia, volume e modalità… <kbd>Ctrl+Invio</kbd></button>
  {:else}
    <button role="menuitem" type="button" onclick={() => run(() => ids.forEach((id) => app.play(id)))}>▶ Riproduci tutti</button>
    {#if playing}
      <button role="menuitem" type="button" onclick={() => run(() => ids.forEach((id) => app.stop(id)))}>■ Ferma</button>
    {/if}
  {/if}
  <hr />
  <button role="menuitem" type="button" aria-haspopup="true" aria-expanded={folderOpen} onclick={() => (folderOpen = !folderOpen)}
    >📁 Sposta in cartella {folderOpen ? '▾' : '▸'}</button
  >
  {#if folderOpen}
    <div class="sub">
      <button role="menuitem" type="button" onclick={() => run(() => app.moveToFolder(ids, null))}>Nessuna cartella</button>
      {#each app.folders as f (f.id)}
        <button role="menuitem" type="button" onclick={() => run(() => app.moveToFolder(ids, f.id))}>{f.name}</button>
      {/each}
      <input
        class="new-folder"
        placeholder="＋ Nuova cartella…"
        aria-label="Nome della nuova cartella, poi Invio"
        maxlength="40"
        onkeydown={async (e) => {
          e.stopPropagation();
          if (e.key === 'Escape') return onclose();
          if (e.key !== 'Enter') return;
          const name = e.currentTarget.value.trim();
          if (!name) return;
          const targets = [...ids];
          onclose();
          const f = await app.createFolder(name);
          if (f) await app.moveToFolder(targets, f.id);
        }}
      />
    </div>
  {/if}
  <hr />
  <button role="menuitem" type="button" class="danger" onclick={() => run(() => app.deleteSounds(ids))}
    >Elimina{ids.length > 1 ? ` ${ids.length} suoni` : ''} <kbd>Canc</kbd></button
  >
</div>

<style>
  .menu {
    position: fixed;
    z-index: 60;
    min-width: 230px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 4px;
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.25);
    display: flex;
    flex-direction: column;
    outline: none;
  }
  button {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    text-align: left;
    background: none;
    border: none;
    padding: 6px 8px;
    border-radius: 4px;
  }
  button:hover,
  button:focus-visible {
    background: var(--surface-2);
    outline: none;
  }
  kbd {
    font-size: 11px;
    color: var(--fg-muted);
  }
  hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
  .sub {
    display: flex;
    flex-direction: column;
    padding-left: 14px;
    max-height: 180px;
    overflow: auto;
  }
  .danger {
    color: var(--danger);
  }
  .new-folder {
    margin: 4px 0 2px;
  }
</style>
