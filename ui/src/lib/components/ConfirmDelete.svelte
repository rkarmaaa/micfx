<script lang="ts">
  // Conferma prima di eliminare più suoni insieme (riquadro di selezione, Ctrl+A, menu di gruppo).
  import { Trash } from '../icons';
  import { app } from '../stores/app.svelte';
  import Modal from './Modal.svelte';

  let { ids, onclose }: { ids: string[]; onclose: () => void } = $props();

  const names = $derived(ids.map((id) => app.sound(id)?.name).filter(Boolean) as string[]);
  const SHOWN = 6;

  function confirm() {
    const targets = [...ids];
    onclose();
    void app.deleteSounds(targets, true);
  }
</script>

<Modal title="Eliminare {ids.length} suoni?" {onclose} width={420}>
  <p class="lead">Spariranno dalla libreria. Subito dopo puoi ancora recuperarli con "Annulla".</p>
  <ul>
    {#each names.slice(0, SHOWN) as n}
      <li>{n}</li>
    {/each}
    {#if names.length > SHOWN}
      <li class="more">e altri {names.length - SHOWN}</li>
    {/if}
  </ul>
  {#snippet footer()}
    <span style="flex:1"></span>
    <button type="button" onclick={onclose}>Annulla</button>
    <!-- Il focus va su "Annulla" (primo pulsante): Invio per sbaglio non elimina niente. -->
    <button type="button" class="danger" onclick={confirm}><Trash size={15} /> Elimina {ids.length} suoni</button>
  {/snippet}
</Modal>

<style>
  .lead {
    margin: 0 0 10px;
    color: var(--fg-muted);
  }
  ul {
    margin: 0;
    padding-left: 18px;
    font-size: var(--text-md);
  }
  li {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more {
    color: var(--fg-muted);
  }
</style>
