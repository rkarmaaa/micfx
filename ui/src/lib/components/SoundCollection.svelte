<script lang="ts">
  // Griglia o elenco dei suoni. Clic = suona. Ctrl (⌘ su Mac)/Maiusc+clic = seleziona. Trascina = riordina
  // o sposta in una cartella; trascinando da uno spazio vuoto si seleziona col riquadro (Ctrl per aggiungere).
  // Frecce, Invio, F2, Canc come in Esplora risorse.
  import { isMac, primaryMod } from '../util/platform';
  import { app, soundEmoji } from '../stores/app.svelte';
  import { isTypingTarget } from '../util/keys';
  import SoundItem from './SoundItem.svelte';

  let container: HTMLElement;

  type Drag = {
    id: string;
    ids: string[];
    startX: number;
    startY: number;
    pointerId: number;
    active: boolean;
    x: number;
    y: number;
    target: { id: string; side: 'before' | 'after' } | null;
    folder: string | null | undefined;
  };
  let drag = $state<Drag | null>(null);
  let suppressClick = false;

  // Selezione a rettangolo, come sul desktop di Windows: si trascina partendo da uno spazio vuoto.
  // Coordinate nel contenuto del contenitore (scroll compreso), così il riquadro segue lo scorrimento.
  type Lasso = {
    pointerId: number;
    x0: number;
    y0: number;
    x1: number;
    y1: number;
    clientX: number;
    clientY: number;
    active: boolean;
    /** Selezione di partenza: con Ctrl (⌘) il riquadro aggiunge, senza la sostituisce. */
    base: string[];
    last: string | null;
  };
  let lasso = $state<Lasso | null>(null);
  let scrollRaf = 0;
  const lassoRect = $derived(
    lasso?.active
      ? {
          left: Math.min(lasso.x0, lasso.x1),
          top: Math.min(lasso.y0, lasso.y1),
          width: Math.abs(lasso.x1 - lasso.x0),
          height: Math.abs(lasso.y1 - lasso.y0),
        }
      : null,
  );

  const view = $derived(app.prefs.view);
  const list = $derived(app.visible);

  // Il cursore da tastiera segue la selezione e resta visibile.
  $effect(() => {
    const id = app.focusId;
    if (!id || !container) return;
    const el = container.querySelector<HTMLElement>(`[data-sound-id="${CSS.escape(id)}"]`);
    if (!el) return;
    el.scrollIntoView({ block: 'nearest' });
    if (container.contains(document.activeElement) || document.activeElement === document.body) el.focus({ preventScroll: true });
  });

  function idFrom(e: Event): string | null {
    return (e.target as HTMLElement).closest<HTMLElement>('[data-sound-id]')?.dataset.soundId ?? null;
  }

  function columns(): number {
    const items = [...container.querySelectorAll<HTMLElement>('[data-sound-id]')];
    if (view === 'list' || items.length < 2) return 1;
    const top = items[0].offsetTop;
    const n = items.findIndex((el) => el.offsetTop !== top);
    return n === -1 ? items.length : n;
  }

  function openMenuAt(id: string, x: number, y: number) {
    if (!app.selection.has(id)) app.select(id, 'single');
    app.menu = { x, y, ids: app.targets(id) };
  }

  function onclick(e: MouseEvent) {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    const id = idFrom(e);
    if (!id) {
      if (e.target === container) app.selection.clear();
      return;
    }
    // L'unica azione sul riquadro è Ferma: le altre stanno nel menu del tasto destro.
    if ((e.target as HTMLElement).closest('[data-action="stop"]')) {
      e.stopPropagation();
      app.stop(id);
      return;
    }
    if (app.renamingId === id) return;
    if (primaryMod(e)) return app.select(id, 'toggle');
    if (e.shiftKey) return app.select(id, 'range');
    app.playFromClick(id);
  }

  function oncontextmenu(e: MouseEvent) {
    const id = idFrom(e);
    if (!id) return;
    e.preventDefault();
    openMenuAt(id, e.clientX, e.clientY);
  }

  function onkeydown(e: KeyboardEvent) {
    if (isTypingTarget(e.target)) return;
    const ids = list.map((s) => s.id);
    if (!ids.length) return;
    const cur = app.focusId && ids.includes(app.focusId) ? ids.indexOf(app.focusId) : -1;
    const go = (i: number) => {
      e.preventDefault();
      const j = Math.max(0, Math.min(ids.length - 1, i));
      if (e.shiftKey) app.select(ids[j], 'range');
      else if (primaryMod(e)) app.focusId = ids[j];
      else app.select(ids[j], 'single');
    };
    const cols = columns();
    const rows = Math.max(1, Math.floor(container.clientHeight / ((container.querySelector('[data-sound-id]') as HTMLElement)?.offsetHeight || 40)));
    switch (e.key) {
      case 'ArrowRight':
        if (view === 'list') return;
        return go(cur + 1);
      case 'ArrowLeft':
        if (view === 'list') return;
        return go(Math.max(0, cur - 1));
      case 'ArrowDown':
        return go(cur === -1 ? 0 : cur + cols);
      case 'ArrowUp':
        return go(cur - cols);
      case 'Home':
        return go(0);
      case 'End':
        return go(ids.length - 1);
      case 'PageDown':
        return go(cur + cols * rows);
      case 'PageUp':
        return go(cur - cols * rows);
    }
    const id = cur >= 0 ? ids[cur] : null;
    if (!id) return;
    if (e.key === 'Enter' && primaryMod(e)) {
      e.preventDefault();
      app.editorId = id;
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      // Con più suoni selezionati partono tutti insieme.
      for (const t of app.targets(id)) {
        if (e.shiftKey) app.preview(t);
        else app.play(t);
      }
    } else if (e.key === 'F2') {
      e.preventDefault();
      app.renamingId = id;
    } else if (e.key === 'Delete' || (isMac && e.key === 'Backspace' && e.metaKey)) {
      // Su Mac il tasto "delete" è Backspace: ⌘⌫ come nel Finder.
      e.preventDefault();
      app.deleteSounds(app.targets(id));
    } else if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
      e.preventDefault();
      const r = container.querySelector(`[data-sound-id="${CSS.escape(id)}"]`)!.getBoundingClientRect();
      openMenuAt(id, r.left + 12, r.top + 12);
    } else if ((e.key === 'e' || e.key === 'E') && primaryMod(e)) {
      e.preventDefault();
      const r = container.querySelector(`[data-sound-id="${CSS.escape(id)}"]`)!.getBoundingClientRect();
      app.emojiTarget = { soundId: id, x: r.left, y: r.bottom + 4 };
    } else if ((e.key === 'a' || e.key === 'A') && primaryMod(e)) {
      e.preventDefault();
      app.selection.clear();
      ids.forEach((x) => app.selection.add(x));
    } else if (e.key === 'Escape' && app.selection.size > 1) {
      e.preventDefault();
      e.stopPropagation();
      app.selection.clear();
      app.selection.add(id);
    }
  }

  // ------------------------------------------------------------ trascinamento (pointer, non HTML5:
  // su Windows il drop dei file di Tauri disattiva il drag & drop HTML5 nella WebView)

  function contentPoint(clientX: number, clientY: number) {
    const r = container.getBoundingClientRect();
    return { x: clientX - r.left + container.scrollLeft, y: clientY - r.top + container.scrollTop };
  }

  function updateLasso() {
    if (!lasso) return;
    const p = contentPoint(lasso.clientX, lasso.clientY);
    lasso.x1 = Math.max(0, Math.min(p.x, container.scrollWidth));
    lasso.y1 = Math.max(0, Math.min(p.y, container.scrollHeight));
    const left = Math.min(lasso.x0, lasso.x1), right = Math.max(lasso.x0, lasso.x1);
    const top = Math.min(lasso.y0, lasso.y1), bottom = Math.max(lasso.y0, lasso.y1);
    const hit: string[] = [];
    for (const el of container.querySelectorAll<HTMLElement>('[data-sound-id]')) {
      const l = el.offsetLeft, t = el.offsetTop;
      if (l < right && l + el.offsetWidth > left && t < bottom && t + el.offsetHeight > top) hit.push(el.dataset.soundId!);
    }
    // Il cursore da tastiera si sposta solo a fine trascinamento: spostandolo ora, scrollIntoView
    // litigherebbe con lo scorrimento automatico.
    lasso.last = hit[hit.length - 1] ?? null;
    app.selectMany([...new Set([...lasso.base, ...hit])], null);
  }

  // Vicino ai bordi la lista scorre da sola, come in Esplora risorse.
  function autoScroll() {
    scrollRaf = 0;
    if (!lasso?.active) return;
    const r = container.getBoundingClientRect();
    const EDGE = 28;
    const dy = lasso.clientY < r.top + EDGE ? -(r.top + EDGE - lasso.clientY) : lasso.clientY > r.bottom - EDGE ? lasso.clientY - (r.bottom - EDGE) : 0;
    if (!dy) return;
    const before = container.scrollTop;
    container.scrollTop += Math.max(-24, Math.min(24, dy / 2));
    if (container.scrollTop !== before) updateLasso();
    scrollRaf = requestAnimationFrame(autoScroll);
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0 || app.renamingId) return;
    const id = idFrom(e);
    if (!id) {
      // Barra di scorrimento: niente riquadro.
      if (e.target !== container || e.offsetX >= container.clientWidth || e.offsetY >= container.clientHeight) return;
      const p = contentPoint(e.clientX, e.clientY);
      lasso = {
        pointerId: e.pointerId,
        x0: p.x,
        y0: p.y,
        x1: p.x,
        y1: p.y,
        clientX: e.clientX,
        clientY: e.clientY,
        active: false,
        base: primaryMod(e) || e.shiftKey ? [...app.selection] : [],
        last: null,
      };
      return;
    }
    if ((e.target as HTMLElement).closest('[data-action], input')) return;
    drag = {
      id,
      ids: app.selection.has(id) ? app.targets(id) : [id],
      startX: e.clientX,
      startY: e.clientY,
      pointerId: e.pointerId,
      active: false,
      x: e.clientX,
      y: e.clientY,
      target: null,
      folder: undefined,
    };
  }

  function onpointermove(e: PointerEvent) {
    if (lasso && e.pointerId === lasso.pointerId) {
      lasso.clientX = e.clientX;
      lasso.clientY = e.clientY;
      if (!lasso.active) {
        const p = contentPoint(e.clientX, e.clientY);
        if (Math.hypot(p.x - lasso.x0, p.y - lasso.y0) < 5) return;
        lasso.active = true;
        container.setPointerCapture(e.pointerId);
        container.focus({ preventScroll: true });
      }
      updateLasso();
      if (!scrollRaf) scrollRaf = requestAnimationFrame(autoScroll);
      return;
    }
    if (!drag || e.pointerId !== drag.pointerId) return;
    drag.x = e.clientX;
    drag.y = e.clientY;
    if (!drag.active) {
      if (Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY) < 6) return;
      drag.active = true;
      container.setPointerCapture(e.pointerId);
    }
    const under = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    const folderEl = under?.closest<HTMLElement>('[data-folder-drop]');
    if (folderEl) {
      drag.folder = folderEl.dataset.folderDrop === 'all' ? null : folderEl.dataset.folderDrop!;
      drag.target = null;
      app.soundDropFolder = folderEl.dataset.folderDrop!;
      return;
    }
    drag.folder = undefined;
    app.soundDropFolder = null;
    const item = under?.closest<HTMLElement>('[data-sound-id]');
    if (!item || drag.ids.includes(item.dataset.soundId!) || drag.ids.length > 1) {
      drag.target = null;
      return;
    }
    const r = item.getBoundingClientRect();
    const after = view === 'grid' ? e.clientX > r.left + r.width / 2 : e.clientY > r.top + r.height / 2;
    drag.target = { id: item.dataset.soundId!, side: after ? 'after' : 'before' };
  }

  function endLasso() {
    if (lasso?.active) {
      if (lasso.last && app.selection.has(lasso.last)) {
        app.anchorId = lasso.last;
        app.focusId = lasso.last;
      }
      suppressClick = true;
      setTimeout(() => (suppressClick = false), 0);
    }
    lasso = null;
    if (scrollRaf) cancelAnimationFrame(scrollRaf);
    scrollRaf = 0;
  }

  function onpointerup(e: PointerEvent) {
    if (lasso && e.pointerId === lasso.pointerId) return endLasso();
    if (!drag || e.pointerId !== drag.pointerId) return;
    const d = drag;
    drag = null;
    app.soundDropFolder = null;
    if (!d.active) return;
    suppressClick = true;
    setTimeout(() => (suppressClick = false), 0);
    if (d.folder !== undefined) {
      const moving = d.ids.filter((id) => app.sound(id)?.folder_id !== d.folder);
      if (moving.length) app.moveToFolder(moving, d.folder);
      return;
    }
    if (d.target) {
      const rest = list.map((s) => s.id).filter((x) => x !== d.id);
      const i = rest.indexOf(d.target.id);
      const before = d.target.side === 'before' ? d.target.id : (rest[i + 1] ?? null);
      void app.reorder(d.id, before);
    }
  }

  function cancelDrag(e: KeyboardEvent) {
    if (lasso?.active && e.key === 'Escape') {
      e.stopPropagation();
      app.selectMany(lasso.base, null);
      endLasso();
      return;
    }
    if (drag?.active && e.key === 'Escape') {
      e.stopPropagation();
      drag = null;
      app.soundDropFolder = null;
      suppressClick = true;
    }
  }

  const draggedSound = $derived(drag?.active ? app.sound(drag.id) : undefined);
</script>

<svelte:window onkeydown={cancelDrag} />

<div
  bind:this={container}
  class="collection {view}"
  style:--tile={app.prefs.size === 's' ? '104px' : app.prefs.size === 'l' ? '172px' : '136px'}
  role="listbox"
  aria-label="Suoni"
  aria-multiselectable="true"
  tabindex={app.focusId && list.some((s) => s.id === app.focusId) ? -1 : 0}
  {onclick}
  {oncontextmenu}
  {onkeydown}
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  onpointercancel={() => ((drag = null), (app.soundDropFolder = null), endLasso())}
  onfocus={(e) => {
    if (e.target === container && list.length && !app.focusId) app.focusId = list[0].id;
  }}
>
  {#each list as s (s.id)}
    <SoundItem
      sound={s}
      {view}
      selected={app.selection.has(s.id)}
      focused={app.focusId === s.id}
      fresh={app.fresh.has(s.id)}
      hotkey={app.hotkeyBySound.get(s.id)}
      playing={app.playingBySound.get(s.id)}
      now={app.now}
      renaming={app.renamingId === s.id}
      drop={drag?.target?.id === s.id ? drag.target.side : null}
      dragging={!!drag?.active && drag.ids.includes(s.id)}
      defaultMode={app.settings?.playback.default_mode ?? 'overlap'}
      onrename={(name) => {
        app.renamingId = null;
        app.rename(s.id, name);
        container.querySelector<HTMLElement>(`[data-sound-id="${CSS.escape(s.id)}"]`)?.focus();
      }}
      oncancelrename={() => {
        app.renamingId = null;
        container.querySelector<HTMLElement>(`[data-sound-id="${CSS.escape(s.id)}"]`)?.focus();
      }}
    />
  {/each}
  {#if lassoRect}
    <div
      class="lasso"
      style:left="{lassoRect.left}px"
      style:top="{lassoRect.top}px"
      style:width="{lassoRect.width}px"
      style:height="{lassoRect.height}px"
      aria-hidden="true"
    ></div>
  {/if}
</div>

{#if draggedSound && drag}
  <div class="ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 12}px" aria-hidden="true">
    <span class="emoji-font">{soundEmoji(draggedSound)}</span>
    <span>{drag.ids.length > 1 ? `${drag.ids.length} suoni` : draggedSound.name}</span>
    {#if drag.folder !== undefined}
      <span class="to">→ {drag.folder === null ? 'Tutti' : app.folders.find((f) => f.id === drag!.folder)?.name}</span>
    {/if}
  </div>
{/if}

<style>
  .collection {
    position: relative;
    flex: 1;
    overflow: auto;
    padding: 6px 16px 16px;
    outline: none;
    align-content: start;
  }
  .collection.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--tile), 1fr));
    gap: 12px;
  }
  .collection.list {
    display: flex;
    flex-direction: column;
    gap: 0;
    padding-inline: 12px;
  }
  .lasso {
    position: absolute;
    z-index: 5;
    border: 1px solid var(--primary);
    border-radius: var(--r-xs);
    background: var(--primary-soft);
    pointer-events: none;
  }
  .ghost {
    position: fixed;
    z-index: 90;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px 6px 10px;
    border: 1px solid var(--primary);
    border-radius: var(--r-md);
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    font-size: var(--text-md);
    font-weight: 550;
    white-space: nowrap;
    pointer-events: none;
  }
  .ghost .to {
    color: var(--primary-text);
  }
</style>
