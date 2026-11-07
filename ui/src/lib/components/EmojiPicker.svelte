<script lang="ts">
  // Tutte le emoji, ricercabili in italiano e inglese, con categorie, recenti, suggerite dal nome
  // del suono e tono della pelle. Lista virtualizzata: si disegnano solo le righe visibili, perché
  // impaginare ~1900 glifi a colori tutti insieme costa più di un secondo a freddo.
  // Tastiera: frecce, PagSu/PagGiù, Home/Fine, Invio, Esc; si può scrivere in ogni momento.
  import { tick, type Component } from 'svelte';

  import {
    SKIN_TONE_LABELS,
    emojiData,
    loadEmoji,
    searchEmoji,
    suggestForName,
    withSkin,
    type EmojiData,
    type EmojiEntry,
  } from '../emoji/emoji';
  import { Clock, Flag, Hand, Heart, Lightbulb, PawPrint, Pizza, Plane, Search, Smile, Sparkles, Volleyball, X } from '../icons';
  import { app } from '../stores/app.svelte';

  let {
    x,
    y,
    current,
    name,
    onpick,
    onclose,
  }: { x: number; y: number; current: string; name: string; onpick: (e: string) => void; onclose: () => void } = $props();

  const COLS = 9;
  const CELL = 38;
  const HEAD = 30;
  const NOTE = 64;
  const OVERSCAN = 6 * CELL;
  const W = COLS * CELL + 2 * 10 + 12; // celle + margini + barra di scorrimento
  const H = 452;
  const TONES = ['✋', '✋🏻', '✋🏼', '✋🏽', '✋🏾', '✋🏿'];

  const GROUP_ICON: Record<string, Component<{ size?: number }>> = {
    'smileys-emotion': Smile,
    'people-body': Hand,
    'animals-nature': PawPrint,
    'food-drink': Pizza,
    'travel-places': Plane,
    activities: Volleyball,
    objects: Lightbulb,
    symbols: Heart,
    flags: Flag,
  };

  // Di solito i dati sono già pronti (precaricati all'apertura della finestra): niente attesa.
  const ready = emojiData();
  let data = $state<EmojiData | null>(ready);
  let failed = $state(false);
  if (!ready) {
    loadEmoji().then(
      (d) => (data = d),
      () => (failed = true),
    );
  }

  let query = $state('');
  let hovered = $state<EmojiEntry | null>(null);
  let toneOpen = $state(false);
  /** Cella col focus (indice piatto); -1 = il cursore è nella ricerca. */
  let active = $state(-1);
  let scrollTop = $state(0);
  let viewH = $state(0);
  let root: HTMLElement;
  let input: HTMLInputElement;
  let scroller = $state<HTMLElement>();

  const tone = $derived(app.prefs.skinTone);

  type Section = { key: string; label: string; items: EmojiEntry[]; icon?: Component<{ size?: number }> };

  /** Le recenti possono avere un tono della pelle: si ritrova l'emoji base per nome e varianti. */
  function recentEntry(d: EmojiData, e: string): EmojiEntry | undefined {
    const base = d.byChar.get(e);
    if (base) return base;
    const v = d.emojis.find((x) => x.s?.includes(e));
    return v ? { ...v, e, s: undefined } : undefined;
  }

  const sections = $derived.by<Section[]>(() => {
    if (!data) return [];
    const q = query.trim();
    if (q) {
      const r = searchEmoji(data, q);
      const label = r.length >= 400 ? 'Più di 400 risultati' : r.length === 1 ? '1 risultato' : `${r.length} risultati`;
      return r.length ? [{ key: 'results', label, items: r }] : [];
    }
    const out: Section[] = [];
    const sug = suggestForName(data, name, COLS);
    if (sug.length) out.push({ key: 'suggested', label: `Suggerite per «${name}»`, items: sug, icon: Sparkles });
    const rec = app.prefs.recentEmojis
      .map((e) => recentEntry(data!, e))
      .filter((e): e is EmojiEntry => !!e)
      .slice(0, 2 * COLS);
    if (rec.length) out.push({ key: 'recent', label: 'Usate di recente', items: rec, icon: Clock });
    for (const g of data.groups) {
      out.push({ key: `g${g.id}`, label: g.label, items: data.byGroup.get(g.id) ?? [], icon: GROUP_ICON[g.key] });
    }
    return out;
  });

  type Row =
    | { kind: 'head'; key: string; top: number; h: number; label: string; section: string }
    | { kind: 'cells'; key: string; top: number; h: number; start: number; items: EmojiEntry[] }
    | { kind: 'note'; key: string; top: number; h: number };

  /** Righe a altezza fissa con la loro posizione: titolo di sezione, righe da 9 emoji, nota finale. */
  const layout = $derived.by(() => {
    const rows: Row[] = [];
    const cells: EmojiEntry[] = [];
    const cellRow: number[] = [];
    const heads = new Map<string, number>();
    let top = 4;
    for (const s of sections) {
      heads.set(s.key, top);
      rows.push({ kind: 'head', key: `h-${s.key}`, top, h: HEAD, label: s.label, section: s.key });
      top += HEAD;
      for (let i = 0; i < s.items.length; i += COLS) {
        const items = s.items.slice(i, i + COLS);
        const r = rows.length;
        rows.push({ kind: 'cells', key: `c-${s.key}-${i}`, top, h: CELL, start: cells.length, items });
        for (let j = 0; j < items.length; j++) {
          cells.push(items[j]);
          cellRow.push(r);
        }
        top += CELL;
      }
    }
    if (data?.hidden && !query.trim() && rows.length) {
      rows.push({ kind: 'note', key: 'note', top, h: NOTE });
      top += NOTE;
    }
    return { rows, cells, cellRow, heads, height: top + 6 };
  });

  const visible = $derived(layout.rows.filter((r) => r.top + r.h > scrollTop - OVERSCAN && r.top < scrollTop + viewH + OVERSCAN));

  /** Categoria in vista: l'ultima il cui titolo è passato sopra il bordo della lista. */
  const inView = $derived.by(() => {
    let key = '';
    for (const [k, top] of layout.heads) if (top <= scrollTop + 8) key = k;
    return key;
  });

  const pos = $derived({
    left: Math.max(8, Math.min(x, window.innerWidth - W - 8)),
    top: Math.max(8, Math.min(y, window.innerHeight - H - 8)),
  });

  $effect(() => {
    input?.focus();
    const away = (e: MouseEvent) => {
      if (root && !root.contains(e.target as Node)) onclose();
    };
    // Dopo il clic che ha aperto il selettore.
    const t = setTimeout(() => document.addEventListener('mousedown', away), 0);
    return () => {
      clearTimeout(t);
      document.removeEventListener('mousedown', away);
    };
  });

  // Nuova ricerca: si riparte dall'alto.
  $effect(() => {
    query;
    active = -1;
    if (scroller) scroller.scrollTop = 0;
    scrollTop = 0;
  });

  function pick(entry: EmojiEntry) {
    onpick(withSkin(entry, tone));
  }

  function cellFrom(e: Event): number | null {
    const b = (e.target as HTMLElement).closest<HTMLElement>('[data-i]');
    return b ? Number(b.dataset.i) : null;
  }

  async function focusCell(i: number) {
    const n = layout.cells.length;
    if (!n || !scroller) return;
    i = Math.max(0, Math.min(n - 1, i));
    active = i;
    const row = layout.rows[layout.cellRow[i]];
    if (row.top - HEAD < scroller.scrollTop) scroller.scrollTop = Math.max(0, row.top - HEAD);
    else if (row.top + row.h > scroller.scrollTop + viewH) scroller.scrollTop = row.top + row.h - viewH + 4;
    scrollTop = scroller.scrollTop;
    await tick();
    root.querySelector<HTMLElement>(`[data-i="${i}"]`)?.focus({ preventScroll: true });
  }

  /** Stessa colonna nella riga di emoji sopra o sotto (le righe incomplete accorciano). */
  function vertical(i: number, dir: 1 | -1, steps = 1): number {
    let r = layout.cellRow[i];
    const col = i - (layout.rows[r] as { start: number }).start;
    let target = i;
    for (let k = r + dir; k >= 0 && k < layout.rows.length && steps > 0; k += dir) {
      const row = layout.rows[k];
      if (row.kind !== 'cells') continue;
      target = row.start + Math.min(col, row.items.length - 1);
      r = k;
      steps--;
    }
    return target === i && dir < 0 ? -1 : target;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      if (toneOpen) toneOpen = false;
      else onclose();
      return;
    }
    const target = e.target as HTMLElement;
    if (target === input) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        void focusCell(0);
      } else if (e.key === 'Enter' && query.trim() && layout.cells.length) {
        e.preventDefault();
        pick(layout.cells[0]);
      }
      return;
    }
    if (!target.dataset.i) return;
    const i = Number(target.dataset.i);
    const rowsInView = Math.max(1, Math.floor(viewH / CELL) - 1);
    const go = (j: number) => {
      e.preventDefault();
      if (j < 0) {
        active = -1;
        input.focus();
      } else void focusCell(j);
    };
    switch (e.key) {
      case 'ArrowRight':
        return go(i + 1);
      case 'ArrowLeft':
        return go(Math.max(0, i - 1));
      case 'ArrowDown':
        return go(vertical(i, 1));
      case 'ArrowUp':
        return go(vertical(i, -1));
      case 'PageDown':
        return go(vertical(i, 1, rowsInView));
      case 'PageUp':
        return go(Math.max(0, vertical(i, -1, rowsInView)));
      case 'Home':
        return go(0);
      case 'End':
        return go(layout.cells.length - 1);
    }
    if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey && e.key !== ' ') {
      // Si può sempre scrivere per cercare.
      input.focus();
    }
  }

  async function jump(key: string) {
    if (query) {
      query = '';
      await tick();
    }
    const top = layout.heads.get(key);
    if (top !== undefined && scroller) {
      scroller.scrollTop = top - 4;
      scrollTop = scroller.scrollTop;
    }
  }
</script>

<div
  bind:this={root}
  class="picker"
  role="dialog"
  aria-label="Scegli un'emoji"
  tabindex="-1"
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  style:width="{W}px"
  style:height="{H}px"
  {onkeydown}
>
  <div class="top">
    <div class="search">
      <Search size={15} class="lead" />
      <input
        bind:this={input}
        bind:value={query}
        type="search"
        placeholder="Cerca: tromba, risata, fire…"
        aria-label="Cerca emoji"
        spellcheck="false"
      />
      {#if query}
        <button type="button" class="clear" aria-label="Cancella ricerca" onclick={() => ((query = ''), input.focus())}><X size={14} /></button>
      {/if}
    </div>
    <div class="tone-wrap">
      <button
        type="button"
        class="tone-btn emoji-font"
        aria-haspopup="true"
        aria-expanded={toneOpen}
        title="Tono della pelle: {SKIN_TONE_LABELS[tone]}"
        aria-label="Tono della pelle: {SKIN_TONE_LABELS[tone]}"
        onclick={() => (toneOpen = !toneOpen)}>{TONES[tone]}</button
      >
      {#if toneOpen}
        <div class="tones" role="radiogroup" aria-label="Tono della pelle">
          {#each TONES as t, i (i)}
            <button
              type="button"
              role="radio"
              class="emoji-font"
              aria-checked={tone === i}
              title={SKIN_TONE_LABELS[i]}
              aria-label={SKIN_TONE_LABELS[i]}
              onclick={() => {
                app.setPrefs({ skinTone: i });
                toneOpen = false;
              }}>{t}</button
            >
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <nav class="cats" aria-label="Categorie">
    {#if data}
      {#each sections.filter((s) => s.icon) as s (s.key)}
        {@const Icon = s.icon!}
        <button type="button" class:on={!query && inView === s.key} title={s.label} aria-label={s.label} onclick={() => jump(s.key)}>
          <Icon size={17} />
        </button>
      {/each}
    {:else}
      {#each Array(9) as _, i (i)}<span class="sk-dot"></span>{/each}
    {/if}
  </nav>

  <div class="scroll" bind:this={scroller} bind:clientHeight={viewH} onscroll={() => (scrollTop = scroller!.scrollTop)}>
    {#if failed}
      <p class="empty">Impossibile caricare le emoji.</p>
    {:else if !data}
      <div class="skeleton" aria-busy="true" aria-label="Caricamento delle emoji">
        <span class="sk-title"></span>
        <div class="sk-grid">
          {#each Array(COLS * 8) as _, i (i)}<span style:animation-delay="{(i % COLS) * 40 + Math.floor(i / COLS) * 40}ms"></span>{/each}
        </div>
      </div>
    {:else if !layout.cells.length}
      <div class="empty">
        <Search size={22} />
        <p>Nessuna emoji per «{query.trim()}»</p>
        <span>Prova in italiano o in inglese: «cuore», «heart».</span>
      </div>
    {:else}
      <!-- Eventi delegati. Da tastiera l'anteprima segue il focus dei pulsanti (onfocus). -->
      <!-- svelte-ignore a11y_mouse_events_have_key_events -->
      <div
        class="space"
        role="presentation"
        style:height="{layout.height}px"
        onclick={(e) => {
          const i = cellFrom(e);
          if (i !== null) pick(layout.cells[i]);
        }}
        onmouseover={(e) => {
          const i = cellFrom(e);
          if (i !== null) hovered = layout.cells[i];
        }}
      >
        {#each visible as r (r.key)}
          {#if r.kind === 'head'}
            <h3 style:transform="translateY({r.top}px)">{r.label}</h3>
          {:else if r.kind === 'cells'}
            <div class="row" style:transform="translateY({r.top}px)">
              {#each r.items as entry, j (j)}
                {@const i = r.start + j}
                {@const ch = withSkin(entry, tone)}
                <button
                  type="button"
                  class="emoji emoji-font"
                  class:current={ch === current}
                  data-i={i}
                  tabindex={i === active || (active === -1 && i === 0) ? 0 : -1}
                  aria-label={entry.n}
                  onfocus={() => {
                    active = i;
                    hovered = entry;
                  }}>{ch}</button
                >
              {/each}
            </div>
          {:else}
            <p class="note" style:transform="translateY({r.top}px)">
              {data.hidden} emoji molto recenti non sono mostrate: il font di sistema di questo computer non sa ancora disegnarle.
            </p>
          {/if}
        {/each}
      </div>
    {/if}
  </div>

  <footer>
    {#if hovered}
      <span class="big emoji-font">{withSkin(hovered, tone)}</span>
      <span class="name">{hovered.n}</span>
    {:else}
      <span class="big emoji-font">{current}</span>
      <span class="name muted">Emoji attuale</span>
    {/if}
  </footer>
</div>

<style>
  .picker {
    position: fixed;
    z-index: 70;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-xl);
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    outline: none;
    overflow: hidden;
    animation: pop-in var(--t-med) var(--ease);
  }

  /* --- ricerca e tono --- */
  .top {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 10px 10px 6px;
  }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 0;
  }
  .search :global(.lead) {
    position: absolute;
    left: 10px;
    color: var(--fg-faint);
    pointer-events: none;
  }
  .search input {
    width: 100%;
    padding-left: 32px;
    padding-right: 30px;
  }
  .clear {
    position: absolute;
    right: 4px;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    background: none;
    box-shadow: none;
    color: var(--fg-muted);
  }
  .clear:hover:not(:disabled) {
    border: none;
    background: var(--surface-2);
  }
  .tone-wrap {
    position: relative;
  }
  .tone-btn {
    width: var(--h-md);
    padding: 0;
    font-size: 17px;
    background: var(--field);
  }
  .tones {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 2;
    display: flex;
    gap: 2px;
    padding: 4px;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-lg);
    background: var(--surface);
    box-shadow: var(--shadow-md);
    animation: pop-in var(--t-fast) var(--ease);
  }
  .tones button {
    width: 32px;
    height: 32px;
    padding: 0;
    border: 1px solid transparent;
    background: none;
    box-shadow: none;
    font-size: 18px;
  }
  .tones button:hover:not(:disabled) {
    border-color: transparent;
    background: var(--surface-2);
  }
  .tones [aria-checked='true'],
  .tones [aria-checked='true']:hover:not(:disabled) {
    border-color: var(--primary);
    background: var(--primary-soft);
  }

  /* --- categorie --- */
  .cats {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 38px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
  }
  .cats button {
    position: relative;
    width: 32px;
    height: 32px;
    padding: 0;
    border: none;
    background: none;
    box-shadow: none;
    color: var(--fg-faint);
  }
  .cats button:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--fg);
  }
  .cats button.on {
    color: var(--primary-text);
  }
  .cats button.on::after {
    content: '';
    position: absolute;
    left: 7px;
    right: 7px;
    bottom: -4px;
    height: 2px;
    border-radius: 2px;
    background: var(--primary);
  }

  /* --- lista virtuale --- */
  .scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    position: relative;
    overscroll-behavior: contain;
  }
  .space {
    position: relative;
  }
  h3,
  .row,
  .note {
    position: absolute;
    top: 0;
    left: 10px;
    right: 10px;
    will-change: transform;
  }
  h3 {
    height: 30px;
    margin: 0;
    padding: 10px 4px 0;
    overflow: hidden;
    color: var(--fg-muted);
    font-size: var(--text-xs);
    font-weight: 650;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row {
    display: grid;
    grid-template-columns: repeat(9, 38px);
    height: 38px;
  }
  .emoji {
    width: 38px;
    height: 38px;
    padding: 0;
    border: none;
    border-radius: var(--r-md);
    background: none;
    box-shadow: none;
    font-size: 24px;
    line-height: 1;
    transition: background var(--t-fast) var(--ease);
  }
  .emoji:hover:not(:disabled) {
    border: none;
    background: var(--surface-2);
  }
  .emoji:focus-visible {
    outline: none;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 2px var(--focus);
  }
  .emoji.current {
    background: var(--primary-soft);
    box-shadow: inset 0 0 0 1.5px var(--primary);
  }
  .note {
    margin: 0;
    padding: 12px 4px;
    color: var(--fg-faint);
    font-size: var(--text-sm);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 48px 24px;
    color: var(--fg-faint);
    text-align: center;
  }
  .empty p {
    color: var(--fg-2);
    font-weight: 550;
  }
  .empty span {
    font-size: var(--text-sm);
  }

  /* --- skeleton --- */
  .skeleton {
    padding: 14px 14px 0;
  }
  .sk-title {
    display: block;
    width: 120px;
    height: 10px;
    margin: 2px 0 12px;
    border-radius: 4px;
    background: var(--surface-2);
  }
  .sk-grid {
    display: grid;
    grid-template-columns: repeat(9, 1fr);
    gap: 8px;
  }
  .sk-grid span,
  .sk-dot {
    display: block;
    aspect-ratio: 1;
    border-radius: 50%;
    background: var(--surface-2);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .sk-dot {
    width: 18px;
    height: 18px;
    margin: 0 7px;
    border-radius: 6px;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0.55;
    }
    50% {
      opacity: 1;
    }
  }

  /* --- anteprima --- */
  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 50px;
    padding: 6px 12px;
    border-top: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface-2) 40%, var(--surface));
  }
  .big {
    font-size: 28px;
    line-height: 1;
  }
  .name {
    overflow: hidden;
    font-size: var(--text-md);
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name.muted {
    color: var(--fg-muted);
    font-weight: 400;
  }
</style>
