<script lang="ts">
  import { onMount } from 'svelte';

  import { onFileDrop } from './lib/api';
  import ContextMenu from './lib/components/ContextMenu.svelte';
  import DropOverlay from './lib/components/DropOverlay.svelte';
  import EmojiPicker from './lib/components/EmojiPicker.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import FolderTabs from './lib/components/FolderTabs.svelte';
  import OutputBar from './lib/components/OutputBar.svelte';
  import SettingsDialog from './lib/components/SettingsDialog.svelte';
  import SetupGuide from './lib/components/SetupGuide.svelte';
  import ShortcutsHelp from './lib/components/ShortcutsHelp.svelte';
  import SoundCollection from './lib/components/SoundCollection.svelte';
  import SoundEditor from './lib/components/SoundEditor.svelte';
  import StatusBanner from './lib/components/StatusBanner.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import { app, soundEmoji } from './lib/stores/app.svelte';
  import { isTypingTarget, loadKeyboardLayout } from './lib/util/keys';

  let searchInput = $state<HTMLInputElement>();
  let fileDrag = $state<{ count: number; folder: string | null } | null>(null);

  const overlayOpen = $derived(
    !!(app.menu || app.emojiTarget || app.editorId || app.settingsOpen || app.guideOpen || app.shortcutsOpen),
  );
  const emojiSound = $derived(app.emojiTarget ? app.sound(app.emojiTarget.soundId) : undefined);

  onMount(() => {
    void app.init();
    void loadKeyboardLayout();
    const un = onFileDrop((e) => {
      if (e.type === 'leave') {
        fileDrag = null;
        return;
      }
      const el = document.elementFromPoint(e.x, e.y) as HTMLElement | null;
      const f = el?.closest<HTMLElement>('[data-folder-drop]')?.dataset.folderDrop;
      const folder = f === undefined ? app.currentFolder() : f === 'all' ? null : f;
      if (e.type === 'enter') fileDrag = { count: e.paths.length, folder };
      else if (e.type === 'over') fileDrag = fileDrag ? { ...fileDrag, folder } : { count: 1, folder };
      else {
        fileDrag = null;
        void app.importPaths(e.paths, folder);
      }
    });
    return () => void un.then((f) => f());
  });

  function onkeydown(e: KeyboardEvent) {
    if (!app.ready || app.capturing) return;
    const ctrl = e.ctrlKey || e.metaKey;
    const typing = isTypingTarget(e.target);
    if (ctrl && (e.key === 'f' || e.key === 'F')) {
      e.preventDefault();
      searchInput?.focus();
      searchInput?.select();
      return;
    }
    if (overlayOpen) return;
    if (ctrl && (e.key === 'o' || e.key === 'O' || e.key === 'i' || e.key === 'I')) {
      e.preventDefault();
      app.pickAndImport();
    } else if (ctrl && e.key === ',') {
      e.preventDefault();
      app.settingsOpen = true;
    } else if (ctrl && e.key === '1') {
      e.preventDefault();
      app.setPrefs({ view: 'grid' });
    } else if (ctrl && e.key === '2') {
      e.preventDefault();
      app.setPrefs({ view: 'list' });
    } else if (ctrl && (e.key === 'm' || e.key === 'M')) {
      e.preventDefault();
      const bus = e.shiftKey ? 'monitor' : 'cable';
      app.setOutputEnabled(bus, !app.settings?.outputs[bus].enabled);
    } else if (e.key === 'F1' || (!typing && e.key === '?')) {
      e.preventDefault();
      app.shortcutsOpen = true;
    } else if (e.key === 'Escape' && !typing) {
      if (app.playing.length) {
        e.preventDefault();
        app.stopAll();
      } else if (app.selection.size) {
        app.selection.clear();
      }
    } else if (!typing && !ctrl && !e.altKey && e.key.length === 1 && e.key !== ' ' && e.key !== '?') {
      // Basta scrivere per cercare.
      searchInput?.focus();
    }
  }

  const dropFolderName = $derived(
    fileDrag?.folder ? (app.folders.find((f) => f.id === fileDrag!.folder)?.name ?? null) : null,
  );
</script>

<svelte:window {onkeydown} />

{#if app.fatal}
  <main class="fatal">
    <h1>MicFX non è riuscito a partire</h1>
    <p>{app.fatal}</p>
    <button type="button" onclick={() => location.reload()}>Riprova</button>
  </main>
{:else if !app.ready}
  <main class="loading" aria-busy="true">Caricamento…</main>
{:else}
  <main class="app">
    <TopBar bind:searchInput />
    <StatusBanner />
    <FolderTabs dropTarget={fileDrag ? (fileDrag.folder ?? 'all') : null} />
    {#if app.visible.length}
      <SoundCollection />
    {:else}
      <EmptyState />
    {/if}
    <OutputBar />
  </main>

  <!-- Props con ?. : in Svelte 5 un figlio può rileggere le props mentre il blocco {#if} si chiude. -->
  {#if app.menu}
    <ContextMenu
      x={app.menu?.x ?? 0}
      y={app.menu?.y ?? 0}
      ids={app.menu?.ids ?? []}
      onclose={() => (app.menu = null)}
    />
  {/if}
  {#if app.emojiTarget && emojiSound}
    <EmojiPicker
      x={app.emojiTarget?.x ?? 0}
      y={app.emojiTarget?.y ?? 0}
      current={emojiSound ? soundEmoji(emojiSound) : ''}
      name={emojiSound?.name ?? ''}
      onpick={(e) => {
        const id = emojiSound?.id;
        app.emojiTarget = null;
        if (id) app.setEmoji(id, e);
      }}
      onclose={() => (app.emojiTarget = null)}
    />
  {/if}
  {#if app.editorId}
    <SoundEditor id={app.editorId} onclose={() => (app.editorId = null)} />
  {/if}
  {#if app.settingsOpen}
    <SettingsDialog onclose={() => (app.settingsOpen = false)} />
  {/if}
  {#if app.guideOpen}
    <SetupGuide onclose={() => (app.guideOpen = false)} />
  {/if}
  {#if app.shortcutsOpen}
    <ShortcutsHelp onclose={() => (app.shortcutsOpen = false)} />
  {/if}
  {#if fileDrag}
    <DropOverlay count={fileDrag.count} folderName={dropFolderName} />
  {/if}
{/if}
<Toasts />

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .loading,
  .fatal {
    height: 100vh;
    display: grid;
    place-content: center;
    gap: 8px;
    text-align: center;
    color: var(--fg-muted);
  }
</style>
