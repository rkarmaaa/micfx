<script lang="ts">
  // Tutte le emoji, ricercabili in italiano e inglese, con categorie, recenti, suggerite dal nome
  // del suono e tono della pelle. Tastiera: frecce, Invio, Esc; si può scrivere in ogni momento.
  import {
    GROUP_ICONS,
    SKIN_TONE_LABELS,
    loadEmoji,
    searchEmoji,
    suggestForName,
    withSkin,
    type EmojiData,
    type EmojiEntry,
  } from '../emoji/emoji';
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
  const W = 9 * 38 + 36; // celle + padding + barra di scorrimento
  const H = 440;

  let data = $state<EmojiData | null>(null);
  let failed = $state(false);
  let query = $state('');
  let hovered = $state<EmojiEntry | null>(null);
  let root: HTMLElement;
  let input: HTMLInputElement;
  let scroller: HTMLElement;

  const tone = $derived(app.prefs.skinTone);

  loadEmoji().then(
    (d) => (data = d),
    () => (failed = true),
  );

  const results = $derived(data && query.trim() ? searchEmoji(data, query) : null);
  const suggested = $derived(data && !query.trim() ? suggestForName(data, name, 9) : []);
  const recent = $derived(
    data && !query.trim()
      ? app.prefs.recentEmojis.map((e) => data!.byChar.get(e) ?? ({ e, n: e, k: '', g: -1, v: 0 } as EmojiEntry)).slice(0, 18)
      : [],
  );
  const byGroup = $derived.by(() => {
    const m = new Map<number, EmojiEntry[]>();
    if (!data) return m;
    for (const e of data.emojis) {
      let l = m.get(e.g);
      if (!l) m.set(e.g, (l = []));
      l.push(e);
    }
    return m;
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

  function pick(entry: EmojiEntry) {
    onpick(withSkin(entry, tone));
  }

  function cells(): HTMLButtonElement[] {
    return [...root.querySelectorAll<HTMLButtonElement>('button.emoji')];
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      onclose();
      return;
    }
    const target = e.target as HTMLElement;
    if (target === input) {
      if (e.key === 'ArrowDown' || (e.key === 'Enter' && results?.length)) {
        e.preventDefault();
        const first = cells()[0];
        if (e.key === 'Enter' && results?.length) pick(results[0]);
        else first?.focus();
      }
      return;
    }
    if (!target.classList.contains('emoji')) return;
    const list = cells();
    const i = list.indexOf(target as HTMLButtonElement);
    const move = (j: number) => {
      e.preventDefault();
      if (j < 0 && e.key === 'ArrowUp') return input.focus();
      list[Math.max(0, Math.min(list.length - 1, j))]?.focus();
    };
    if (e.key === 'ArrowRight') move(i + 1);
    else if (e.key === 'ArrowLeft') move(i - 1);
    else if (e.key === 'ArrowDown') move(i + COLS);
    else if (e.key === 'ArrowUp') move(i - COLS);
    else if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey && e.key !== ' ') {
      // Si può sempre scrivere per cercare.
      input.focus();
    }
  }

  function jump(groupId: number) {
    const el = root.querySelector(`[data-group="${groupId}"]`);
    if (query) query = '';
    requestAnimationFrame(() => {
      const target = root.querySelector(`[data-group="${groupId}"]`) ?? el;
      if (target && scroller) scroller.scrollTop = (target as HTMLElement).offsetTop - scroller.offsetTop;
    });
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
    <input
      bind:this={input}
      bind:value={query}
      type="search"
      placeholder="Cerca emoji: tromba, risata, fire…"
      aria-label="Cerca emoji"
      spellcheck="false"
    />
    <div class="tones" role="radiogroup" aria-label="Tono della pelle">
      {#each ['✋', '✋🏻', '✋🏼', '✋🏽', '✋🏾', '✋🏿'] as t, i}
        <button
          type="button"
          role="radio"
          class="tone emoji-font"
          aria-checked={tone === i}
          title="Tono: {SKIN_TONE_LABELS[i]}"
          onclick={() => app.setPrefs({ skinTone: i })}>{t}</button
        >
      {/each}
    </div>
  </div>

  {#if data}
    <nav class="groups" aria-label="Categorie">
      {#each data.groups as g}
        <button type="button" class="emoji-font" title={g.label} aria-label={g.label} onclick={() => jump(g.id)}
          >{GROUP_ICONS[g.key] ?? '•'}</button
        >
      {/each}
    </nav>
  {/if}

  <div class="scroll" bind:this={scroller}>
    {#if failed}
      <p class="empty">Impossibile caricare le emoji.</p>
    {:else if !data}
      <p class="empty">Caricamento…</p>
    {:else if results}
      {#if results.length === 0}
        <p class="empty">Nessuna emoji per «{query}».</p>
      {:else}
        <h3>{results.length === 400 ? 'Più di 400' : results.length} risultati</h3>
        {@render grid(results)}
      {/if}
    {:else}
      {#if suggested.length}
        <h3>Suggerite per «{name}»</h3>
        {@render grid(suggested)}
      {/if}
      {#if recent.length}
        <h3>Usate di recente</h3>
        {@render grid(recent)}
      {/if}
      {#each data.groups as g (g.id)}
        <h3 data-group={g.id}>{g.label}</h3>
        {@render grid(byGroup.get(g.id) ?? [])}
      {/each}
      {#if data.hidden}
        <p class="note">
          {data.hidden} emoji molto recenti non sono mostrate perché il font di Windows di questo PC non sa ancora disegnarle.
        </p>
      {/if}
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

{#snippet grid(list: EmojiEntry[])}
  <div class="grid" role="listbox" aria-label="Emoji">
    {#each list as entry (entry.e)}
      <button
        type="button"
        role="option"
        aria-selected={withSkin(entry, tone) === current}
        class="emoji emoji-font"
        class:current={withSkin(entry, tone) === current}
        title={entry.n}
        aria-label={entry.n}
        onclick={() => pick(entry)}
        onmouseenter={() => (hovered = entry)}
        onfocus={() => (hovered = entry)}>{withSkin(entry, tone)}</button
      >
    {/each}
  </div>
{/snippet}

<style>
  .picker {
    position: fixed;
    z-index: 70;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    box-shadow: 0 8px 30px rgb(0 0 0 / 0.3);
    outline: none;
  }
  .top {
    display: flex;
    gap: 6px;
    padding: 8px;
    align-items: center;
  }
  .top input {
    flex: 1;
    min-width: 0;
  }
  .tones {
    display: flex;
  }
  .tone {
    padding: 2px;
    font-size: 16px;
    border: 1px solid transparent;
    background: none;
  }
  .tone[aria-checked='true'] {
    border-color: var(--accent);
    border-radius: 4px;
  }
  .groups {
    display: flex;
    justify-content: space-between;
    padding: 0 8px 4px;
    border-bottom: 1px solid var(--border);
  }
  .groups button {
    background: none;
    border: none;
    font-size: 18px;
    padding: 4px;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 4px 8px 8px;
    position: relative;
  }
  h3 {
    font-size: 12px;
    font-weight: 600;
    color: var(--fg-muted);
    margin: 8px 2px 4px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(9, 38px);
  }
  .emoji {
    width: 38px;
    height: 38px;
    font-size: 24px;
    line-height: 1;
    padding: 0;
    border: none;
    background: none;
    border-radius: 6px;
  }
  .emoji:hover,
  .emoji:focus-visible {
    background: var(--surface-2);
  }
  .emoji.current {
    outline: 2px solid var(--accent);
  }
  .empty,
  .note {
    color: var(--fg-muted);
    padding: 12px 4px;
    margin: 0;
    font-size: 13px;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-top: 1px solid var(--border);
    min-height: 44px;
  }
  .big {
    font-size: 26px;
  }
  .name {
    font-size: 13px;
  }
  .muted {
    color: var(--fg-muted);
  }
</style>
