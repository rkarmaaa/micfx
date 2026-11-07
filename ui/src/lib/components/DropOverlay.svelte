<script lang="ts">
  import { Import } from '../icons';

  let { count, folderName }: { count: number; folderName: string | null } = $props();
</script>

<div class="overlay" aria-live="polite">
  <div class="box">
    <div class="big" aria-hidden="true"><Import size={26} /></div>
    <strong>Rilascia per aggiungere {count === 1 ? '1 elemento' : `${count} elementi`}</strong>
    <span>{folderName ? `nella cartella «${folderName}»` : 'alla libreria'}</span>
    <small>Trascina su una scheda in alto per scegliere la cartella</small>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 55%, transparent);
    pointer-events: none;
    animation: fade var(--t-fast) var(--ease);
  }
  .overlay::before {
    content: '';
    position: absolute;
    inset: 10px;
    border: 2px dashed var(--primary);
    border-radius: var(--r-2xl);
    background: var(--primary-soft);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .box {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 22px 28px;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-2xl);
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    text-align: center;
  }
  .big {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    margin-bottom: 8px;
    border-radius: var(--r-xl);
    background: var(--primary);
    color: var(--primary-fg);
  }
  strong {
    font-size: var(--text-lg);
    font-weight: 650;
  }
  span {
    color: var(--fg-2);
  }
  small {
    margin-top: 6px;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
</style>
