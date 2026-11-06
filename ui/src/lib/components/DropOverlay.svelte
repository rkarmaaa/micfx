<script lang="ts">
  let { count, folderName }: { count: number; folderName: string | null } = $props();
</script>

<div class="overlay" aria-live="polite">
  <div class="box">
    <div class="big" aria-hidden="true">📥</div>
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
    pointer-events: none;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.15);
    border: 3px dashed var(--accent);
  }
  .box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    padding: 18px 24px;
  }
  .big {
    font-size: 36px;
  }
  small {
    color: var(--fg-muted);
  }
</style>
