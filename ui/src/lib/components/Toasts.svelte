<script lang="ts">
  import { CircleCheck, CircleX, Info, LoaderCircle, X } from '../icons';
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
        <span class="kind" aria-hidden="true">
          {#if t.kind === 'error'}<CircleX size={18} />{:else if t.kind === 'success'}<CircleCheck size={18} />{:else if t.kind === 'progress'}<LoaderCircle
              size={18}
              class="spin"
            />{:else}<Info size={18} />{/if}
        </span>
        <div class="text">
          <strong>{t.title}</strong>
          {#if t.detail}<span class="detail">{t.detail}</span>{/if}
        </div>
        {#each t.actions ?? [] as a}
          <!-- Un'azione vale una volta sola: un secondo clic su "Annulla" non deve ripetere il ripristino. -->
          <button
            type="button"
            class="action"
            onclick={() => {
              toasts.dismiss(t.id);
              a.run();
            }}>{a.label}</button
          >
        {/each}
        {#if t.kind !== 'progress'}
          <button type="button" class="icon" aria-label="Chiudi notifica" onclick={() => {
              toasts.dismiss(t.id);
              t.onclose?.();
            }}><X size={15} /></button
          >
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
    right: 16px;
    bottom: calc(var(--dock-h, 0px) + 12px);
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 80;
    width: min(380px, calc(100vw - 32px));
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    padding: 10px 10px 10px 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-xl);
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    animation: toast-in var(--t-med) var(--ease);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .kind {
    display: inline-flex;
    align-self: flex-start;
    margin-top: 1px;
    color: var(--fg-muted);
  }
  .error .kind {
    color: var(--danger);
  }
  .success .kind {
    color: var(--ok);
  }
  .progress .kind {
    color: var(--primary-text);
  }
  .kind :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  strong {
    font-size: var(--text-md);
    font-weight: 600;
  }
  .detail {
    overflow: hidden;
    color: var(--fg-muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .action {
    height: var(--h-sm);
    padding: 0 10px;
    border-color: transparent;
    background: var(--primary-soft);
    box-shadow: none;
    color: var(--primary-text);
    font-size: var(--text-sm);
    font-weight: 600;
  }
  .action:hover:not(:disabled) {
    border-color: transparent;
    background: var(--primary-soft-2);
  }
  /* Centrata sulla riga, alta come il pulsante d'azione: X e "Scarica"/"Annulla" restano allineati. */
  .row :global(.icon) {
    width: var(--h-sm);
    min-width: var(--h-sm);
    height: var(--h-sm);
    color: var(--fg-faint);
  }
  ul {
    margin: 6px 0 0 28px;
    padding-left: 14px;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .bar {
    height: 4px;
    margin: 8px 0 2px 28px;
    border-radius: 999px;
    background: var(--track);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--primary);
    transition: width 0.2s var(--ease);
  }
</style>
