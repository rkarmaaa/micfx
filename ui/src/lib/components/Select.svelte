<script lang="ts" generics="T extends string | number">
  // Select del design system: sostituisce <select>, il cui menu a tendina lo disegna Windows e non segue il tema.
  // Il focus resta sul pulsante (aria-activedescendant); l'elenco va in fondo a <body>, così né le modali
  // né i contenitori con scorrimento lo tagliano. Stili in global.scss (.select, .listbox, .option).
  import { Check, ChevronDown } from '../icons';

  type Option<V> = { value: V; label: string };

  let {
    value,
    options,
    onchange,
    class: cls = '',
    size = 'md',
    label,
    disabled = false,
  }: {
    value: T;
    options: Option<T>[];
    onchange: (v: T) => void;
    class?: string;
    size?: 'sm' | 'md';
    /** Nome accessibile, se non c'è un'etichetta visibile. */
    label?: string;
    disabled?: boolean;
  } = $props();

  const uid = `sel-${Math.random().toString(36).slice(2, 9)}`;
  let trigger: HTMLButtonElement;
  let list = $state<HTMLElement>();
  let open = $state(false);
  let active = $state(-1);
  let pos = $state({ left: 0, top: 0, width: 0, up: false });

  const current = $derived(options.find((o) => o.value === value));

  /** Sposta l'elenco in fondo a <body>: position: fixed non basta dentro un antenato con transform. */
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function place() {
    const r = trigger.getBoundingClientRect();
    const h = Math.min(list?.scrollHeight ?? 0, 300) + 10;
    const below = window.innerHeight - r.bottom;
    const up = below < h && r.top > below;
    const width = Math.max(r.width, 140);
    pos = {
      left: Math.max(8, Math.min(r.left, window.innerWidth - width - 8)),
      top: up ? r.top - 4 : r.bottom + 4,
      width,
      up,
    };
  }

  function show() {
    if (disabled || open) return;
    active = Math.max(0, options.findIndex((o) => o.value === value));
    open = true;
    place();
  }

  function close(refocus = true) {
    if (!open) return;
    open = false;
    if (refocus) trigger.focus();
  }

  function pick(i: number) {
    const o = options[i];
    close();
    if (o && o.value !== value) onchange(o.value);
  }

  // Dopo l'apertura: misura l'elenco (per decidere sopra/sotto), porta in vista l'attiva, chiude su clic fuori.
  $effect(() => {
    if (!open || !list) return;
    place();
    const el = list;
    const away = (e: Event) => {
      const t = e.target as Node;
      if (!el.contains(t) && !trigger.contains(t)) close(false);
    };
    const scrolled = (e: Event) => !el.contains(e.target as Node) && close(false);
    const lost = () => close(false);
    document.addEventListener('mousedown', away, true);
    document.addEventListener('scroll', scrolled, true);
    window.addEventListener('resize', lost);
    window.addEventListener('blur', lost);
    return () => {
      document.removeEventListener('mousedown', away, true);
      document.removeEventListener('scroll', scrolled, true);
      window.removeEventListener('resize', lost);
      window.removeEventListener('blur', lost);
    };
  });

  $effect(() => {
    if (open && list && active >= 0) list.querySelector(`#${uid}-${active}`)?.scrollIntoView({ block: 'nearest' });
  });

  // Ricerca per iniziali, come nelle select native.
  let typed = '';
  let typedAt = 0;
  function typeahead(ch: string) {
    const now = performance.now();
    typed = now - typedAt > 600 ? ch : typed + ch;
    typedAt = now;
    const q = typed.toLocaleLowerCase();
    const from = typed.length === 1 ? active + 1 : active;
    for (let k = 0; k < options.length; k++) {
      const i = (from + k) % options.length;
      if (options[i].label.toLocaleLowerCase().startsWith(q)) return i;
    }
    return -1;
  }

  function onkeydown(e: KeyboardEvent) {
    const n = options.length;
    if (!n) return;
    const key = e.key;
    if (!open) {
      if (['Enter', ' ', 'ArrowDown', 'ArrowUp', 'F4'].includes(key) || (key === 'ArrowDown' && e.altKey)) {
        e.preventDefault();
        show();
      } else if (key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
        // Come le select native: a menu chiuso, la lettera sceglie direttamente.
        active = Math.max(0, options.findIndex((o) => o.value === value));
        const i = typeahead(key);
        if (i >= 0 && options[i].value !== value) onchange(options[i].value);
      }
      return;
    }
    if (key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (key === 'Tab') {
      close(false);
    } else if (key === 'Enter' || key === ' ') {
      e.preventDefault();
      pick(active);
    } else if (key === 'ArrowDown') {
      e.preventDefault();
      active = Math.min(n - 1, active + 1);
    } else if (key === 'ArrowUp') {
      e.preventDefault();
      if (e.altKey) pick(active);
      else active = Math.max(0, active - 1);
    } else if (key === 'Home' || key === 'PageUp') {
      e.preventDefault();
      active = 0;
    } else if (key === 'End' || key === 'PageDown') {
      e.preventDefault();
      active = n - 1;
    } else if (key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      const i = typeahead(key);
      if (i >= 0) active = i;
    }
  }
</script>

<button
  bind:this={trigger}
  type="button"
  class="select {size} {cls}"
  class:open
  role="combobox"
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-controls={open ? `${uid}-list` : undefined}
  aria-activedescendant={open && active >= 0 ? `${uid}-${active}` : undefined}
  aria-label={label}
  {disabled}
  onclick={() => (open ? close() : show())}
  {onkeydown}
>
  <span class="select-value">{current?.label ?? ''}</span>
  <ChevronDown size={size === 'sm' ? 14 : 16} class="select-chevron" />
</button>

{#if open}
  <div
    use:portal
    bind:this={list}
    id="{uid}-list"
    class="popover listbox"
    class:up={pos.up}
    role="listbox"
    aria-label={label}
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    style:min-width="{pos.width}px"
  >
    {#each options as o, i (o.value)}
      <div
        id="{uid}-{i}"
        class="option"
        class:active={i === active}
        role="option"
        aria-selected={o.value === value}
        tabindex="-1"
        onmousedown={(e) => e.preventDefault()}
        onmousemove={() => (active = i)}
        onclick={() => pick(i)}
        onkeydown={() => {}}
      >
        <span>{o.label}</span>
        {#if o.value === value}<Check size={15} />{/if}
      </div>
    {/each}
  </div>
{/if}
