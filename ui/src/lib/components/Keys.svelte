<script lang="ts">
  // Contenuto di un <kbd>: i tasti che hanno un simbolo universale (Maiusc ⇧, Invio ↵, frecce, Tab,
  // Backspace, tasti multimediali; su Mac anche ⌘ ⌥ ⌃) diventano icone Lucide, gli altri restano testo.
  // Due ingressi: `acc` è un accelerator del core ("Ctrl+Shift+A"), `text` un testo scritto per
  // Windows ("Ctrl + Maiusc + M", "Clic destro / Maiusc + F10").
  import type { Component } from 'svelte';
  import { KEY_ICONS } from '../icons';
  import { keyLabel } from '../util/keys';
  import { isMac } from '../util/platform';

  let { acc = null, text = null }: { acc?: string | null; text?: string | null } = $props();

  type Part = { icon: Component[]; label: string } | { t: string };

  const MODS = new Set(['Ctrl', 'Alt', 'Shift', 'Win']);
  const MAC_MODS: Record<string, string> = { Ctrl: '⌃', Alt: '⌥', Shift: '⇧', Win: '⌘' };

  // Parole dei testi per Windows → nome del core. Le più lunghe prima ("Bloc Maiusc" prima di "Maiusc").
  const WORDS: [string, string][] = [
    ['Bloc Maiusc', 'CapsLock'],
    ['Pag ↑', 'PageUp'],
    ['Pag ↓', 'PageDown'],
    ['Maiusc', 'Shift'],
    ['Invio', 'Enter'],
    ['Spazio', 'Space'],
    // Nei testi della finestra Ctrl è il tasto principale: ⌘ su Mac (come `kb`).
    ['Ctrl', isMac ? 'Win' : 'Ctrl'],
    ['Alt', 'Alt'],
    ['Win', 'Win'],
    ['Tab', 'Tab'],
    ['⌫', 'Backspace'],
    ['⌘', 'Win'],
    ['⇧', 'Shift'],
    ['⌥', 'Alt'],
    ['⌃', 'Ctrl'],
    ['↑', 'Up'],
    ['↓', 'Down'],
    ['←', 'Left'],
    ['→', 'Right'],
  ];
  const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const WORD_RE = new RegExp(
    WORDS.map(([w]) => (/^\w/.test(w) ? `\\b${esc(w)}\\b` : esc(w))).join('|'),
    'g',
  );
  const WORD_TO_NAME = new Map(WORDS);

  function icons(name: string): Component[] | null {
    if (isMac && MODS.has(name)) return KEY_ICONS[`Mac${name}`] ?? null;
    return KEY_ICONS[name] ?? null;
  }

  /** Un tasto: icona se ne ha una, altrimenti la sua etichetta (testo). */
  function key(name: string): Part {
    const label = name === 'Shift' ? 'Maiusc' : keyLabel(name);
    const icon = icons(name);
    if (icon) return { icon, label: isMac && MAC_MODS[name] ? MAC_MODS[name] : label };
    return { t: isMac && MAC_MODS[name] ? MAC_MODS[name] : label };
  }

  function fromAcc(a: string): Part[] {
    const out: Part[] = [];
    a.split('+').forEach((p, i) => {
      // Mac: "⌃⌥⌘1", come nei menu di macOS.
      if (i > 0 && !isMac) out.push({ t: ' + ' });
      out.push(p === '…' ? { t: '…' } : key(p));
    });
    return out;
  }

  function fromText(s: string): Part[] {
    const out: Part[] = [];
    let last = 0;
    let afterMod = false;
    for (const m of s.matchAll(WORD_RE)) {
      let before = s.slice(last, m.index);
      // Mac: "⌘+F" → "⌘F"; con gli spazi ("⌘ + ⇧ + M") resta com'è.
      if (isMac && afterMod && before === '+') before = '';
      if (before) out.push({ t: before });
      const name = WORD_TO_NAME.get(m[0])!;
      out.push(key(name));
      afterMod = MODS.has(name);
      last = m.index + m[0].length;
    }
    let rest = s.slice(last);
    if (isMac && afterMod && rest.startsWith('+')) rest = rest.slice(1);
    if (rest) out.push({ t: rest });
    return out;
  }

  const parts = $derived(acc ? fromAcc(acc) : text ? fromText(text) : []);
</script>

{#each parts as p, i (i)}{#if 't' in p}{p.t}{:else}<span class="k" role="img" aria-label={p.label} title={p.label}
      >{#each p.icon as Icon, j (j)}<Icon />{/each}</span
    >{/if}{/each}

<style>
  .k {
    display: inline-flex;
    align-items: center;
    vertical-align: middle;
  }
  .k :global(svg) {
    width: 1.25em;
    height: 1.25em;
    stroke-width: 2.25;
  }
</style>
