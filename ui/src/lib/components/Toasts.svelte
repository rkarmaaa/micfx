<script lang="ts">
  import { toasts } from '../stores/toasts.svelte';
</script>

<div class="toasts" role="region" aria-label="Notifiche" aria-live="polite">
  {#each toasts.list as t (t.id)}
    <div
      class="toast {t.kind}"
      role={t.kind === 'error' ? 'alert' : 'status'}
      onmouseenter={() => toasts.hold(t.id)}
      onmouseleave={() => toasts.release(t.id)}
    >
      <div class="row">
        <div class="text">
          <strong>{t.title}</strong>
          {#if t.detail}<span class="detail">{t.detail}</span>{/if}
        </div>
        {#each t.actions ?? [] as a}
          <button type="button" class="action" onclick={a.run}>{a.label}</button>
        {/each}
        {#if t.kind !== 'progress'}
          <button type="button" class="icon" aria-label="Chiudi notifica" onclick={() => toasts.dismiss(t.id)}>✕</button>
        {/if}
      </div>
      {#if t.lines?.length}
        <ul>
          {#each t.lines as l}<li>{l}</li>{/each}
        </ul>
      {/if}
      {#if t.progress !== undefined}
        <div class="bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(t.progress * 100)}>
          <span style:width="{Math.max(3, t.progress * 100)}%"></span>
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 12px;
    bottom: 96px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 80;
    width: min(380px, calc(100vw - 24px));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-left: 4px solid var(--fg-muted);
    border-radius: 6px;
    padding: 8px 10px;
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  .toast.success {
    border-left-color: var(--ok);
  }
  .toast.progress {
    border-left-color: var(--accent);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .detail {
    color: var(--fg-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .action {
    font-weight: 600;
  }
  ul {
    margin: 6px 0 0;
    padding-left: 18px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .bar {
    margin-top: 6px;
    height: 4px;
    background: var(--surface-2);
    border-radius: 2px;
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
</style>
