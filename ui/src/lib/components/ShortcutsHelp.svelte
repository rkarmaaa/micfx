<script lang="ts">
  import Modal from './Modal.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const rows: [string, string][] = [
    ['Clic', 'Riproduci (per tutti)'],
    ['Invio / Spazio', 'Riproduci il suono selezionato'],
    ['Maiusc + Invio', 'Ascolta solo tu (anteprima in cuffia)'],
    ['Esc', 'Ferma tutti i suoni'],
    ['Frecce, Home, Fine', 'Muoviti tra i suoni'],
    ['Ctrl / Maiusc + clic', 'Seleziona più suoni'],
    ['Ctrl + A', 'Seleziona tutti'],
    ['F2', 'Rinomina'],
    ['Ctrl + E', 'Cambia emoji'],
    ['Ctrl + Invio', 'Modifica: scorciatoia, volume, modalità'],
    ['Canc', 'Elimina (con Annulla)'],
    ['Clic destro / Maiusc + F10', 'Altre azioni'],
    ['Trascina', 'Riordina o sposta in una cartella'],
    ['Ctrl + F o scrivi', 'Cerca'],
    ['Invio nella ricerca', 'Riproduci il primo risultato'],
    ['Ctrl + O', 'Aggiungi suoni'],
    ['Ctrl + 1 / Ctrl + 2', 'Griglia / elenco'],
    ['Ctrl + M', 'Attiva/disattiva microfono'],
    ['Ctrl + Maiusc + M', 'Attiva/disattiva cuffie'],
    ['Ctrl + ,', 'Impostazioni'],
    ['F1 o ?', 'Questo elenco'],
  ];
</script>

<Modal title="Scorciatoie della finestra" {onclose} width={480}>
  <table>
    <tbody>
      {#each rows as [k, v]}
        <tr><td><kbd>{k}</kbd></td><td>{v}</td></tr>
      {/each}
    </tbody>
  </table>
  <p class="help">Le scorciatoie globali (che funzionano anche fuori da MicFX) si impostano per ogni suono e nelle Impostazioni.</p>
</Modal>

<style>
  table {
    width: 100%;
    border-collapse: collapse;
  }
  td {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
    color: var(--fg-2);
    font-size: var(--text-md);
  }
  tr:last-child td {
    border-bottom: none;
  }
  td:first-child {
    width: 1%;
    padding-right: 20px;
    white-space: nowrap;
  }
  .help {
    margin-top: 12px;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
</style>
