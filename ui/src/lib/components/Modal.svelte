<script lang="ts">
  import type { Snippet } from 'svelte';

  import { X } from '../icons';

  let {
    title,
    onclose,
    children,
    footer,
    width = 560,
    side = false,
  }: {
    title: string;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
    width?: number;
    /** Pannello laterale invece di finestra centrata. */
    side?: boolean;
  } = $props();

  let panel: HTMLElement;
  let previous: Element | null = null;

  $effect(() => {
    previous = document.activeElement;
    const first = panel.querySelector<HTMLElement>('[autofocus], input, select, textarea, button:not(.close)');
    (first ?? panel).focus();
    return () => {
      if (previous instanceof HTMLElement && document.contains(previous)) previous.focus();
    };
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      e.preventDefault();
      onclose();
    } else if (e.key === 'Tab') {
      // Il focus resta dentro la finestra.
      const items = [...panel.querySelectorAll<HTMLElement>('button, input, select, textarea, [tabindex="0"]')].filter(
        (el) => !el.hasAttribute('disabled') && el.offsetParent !== null,
      );
      if (!items.length) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }
</script>

<div class="backdrop" class:side role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div
    class="panel"
    bind:this={panel}
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    style:--w="{width}px"
    {onkeydown}
  >
    <header>
      <h2>{title}</h2>
      <button class="close icon" type="button" aria-label="Chiudi" title="Chiudi (Esc)" onclick={onclose}><X size={18} /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}
      <footer>{@render footer()}</footer>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--overlay);
    animation: fade var(--t-med) var(--ease);
  }
  .backdrop.side {
    place-items: stretch end;
    padding: 0;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .panel {
    display: flex;
    flex-direction: column;
    width: min(var(--w), 100%);
    max-height: calc(100vh - 48px);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-2xl);
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    outline: none;
    animation: rise var(--t-med) var(--ease);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.985);
    }
  }
  .side .panel {
    height: 100vh;
    max-height: 100vh;
    border-width: 0 0 0 1px;
    border-radius: 0;
    animation: slide 220ms var(--ease);
  }
  @keyframes slide {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 16px 14px 12px 20px;
  }
  h2 {
    font-size: var(--text-lg);
  }
  .body {
    flex: 1;
    overflow: auto;
    padding: 4px 20px 20px;
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface-2) 45%, var(--surface));
    border-radius: 0 0 var(--r-2xl) var(--r-2xl);
  }
  .side footer {
    border-radius: 0;
  }
</style>
