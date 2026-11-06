<script lang="ts">
  import type { Snippet } from 'svelte';

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
      <button class="close icon" type="button" aria-label="Chiudi" title="Chiudi (Esc)" onclick={onclose}>✕</button>
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
    background: rgb(0 0 0 / 0.4);
    display: grid;
    place-items: center;
    z-index: 50;
    padding: 16px;
  }
  .backdrop.side {
    place-items: stretch end;
    padding: 0;
  }
  .panel {
    width: min(var(--w), 100%);
    max-height: calc(100vh - 32px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    outline: none;
  }
  .side .panel {
    max-height: 100vh;
    height: 100vh;
    border-radius: 0;
    border-width: 0 0 0 1px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .body {
    padding: 16px;
    overflow: auto;
    flex: 1;
  }
  footer {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
</style>
