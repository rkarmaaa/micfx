<script lang="ts">
  // Configurazione una tantum (SPEC §14). I passi che il core sa verificare si spuntano da soli,
  // in tempo reale, mentre l'utente cambia le impostazioni di Windows.
  import { api } from '../api';
  import { app } from '../stores/app.svelte';
  import Modal from './Modal.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const cableOk = $derived(!app.status.includes('cable_missing'));
  const micOk = $derived(cableOk && !app.status.includes('mic_not_routed'));
  const defaultOk = $derived(cableOk && !app.status.includes('default_is_cable'));
  const rateOk = $derived(!app.status.includes('rate_mismatch'));
  const routedMic = $derived(app.mics.find((m) => m.routed_to_cable));
  const defaultMic = $derived(app.mics.find((m) => m.is_default));

  function close() {
    if (!app.prefs.guideDone) app.setPrefs({ guideDone: true });
    onclose();
  }
</script>

<Modal title="Configura MicFX con Discord" onclose={close} width={620}>
  <p class="intro">
    MicFX suona i file dentro un microfono virtuale (VB-Cable). Discord ascolta quel microfono, dove arrivano insieme la tua voce
    e i suoni. Si fa una volta sola: i passi verificabili si spuntano da soli.
  </p>

  <ol class="steps">
    <li class:ok={cableOk}>
      <span class="check" aria-label={cableOk ? 'Fatto' : 'Da fare'}>{cableOk ? '✅' : '①'}</span>
      <div>
        <strong>Installa VB-Cable</strong>
        {#if cableOk}
          <p>Trovato: {app.devices.find((d) => d.is_cable)?.name ?? 'CABLE Input'}.</p>
        {:else}
          <p>Scarica il pacchetto, estrailo, avvia <code>VBCABLE_Setup_x64.exe</code> come amministratore e riavvia il PC.</p>
          <button type="button" class="primary" onclick={() => api.openLink('vb-cable')}>Apri vb-audio.com</button>
        {/if}
      </div>
    </li>

    <li class:ok={micOk}>
      <span class="check">{micOk ? '✅' : '②'}</span>
      <div>
        <strong>Manda la tua voce dentro VB-Cable</strong>
        {#if micOk}
          <p>Il microfono «{routedMic?.name}» arriva a CABLE Input.</p>
        {:else}
          <p>
            Pannello audio → scheda <em>Registrazione</em> → doppio clic su
            {defaultMic ? `«${defaultMic.name}»` : 'il tuo microfono'} → scheda <em>Ascolta</em> → spunta
            <em>Ascolta questo dispositivo</em> → <em>Riproduci tramite: CABLE Input</em> → OK.
          </p>
          <button type="button" class="primary" disabled={!cableOk} onclick={() => api.openSoundSettings(1)}>Apri scheda Registrazione</button>
        {/if}
      </div>
    </li>

    <li class:ok={defaultOk}>
      <span class="check">{defaultOk ? '✅' : '③'}</span>
      <div>
        <strong>Le cuffie restano il dispositivo predefinito</strong>
        {#if defaultOk}
          <p>Ok: CABLE Input non è il predefinito.</p>
        {:else}
          <p>Scheda <em>Riproduzione</em>: imposta le cuffie come predefinite, mai CABLE Input.</p>
          <button type="button" onclick={() => api.openSoundSettings(0)}>Apri scheda Riproduzione</button>
        {/if}
      </div>
    </li>

    <li>
      <span class="check">④</span>
      <div>
        <strong>In Discord</strong>
        <p>Impostazioni → Voce e video:</p>
        <ul>
          <li>Dispositivo di input: <em>CABLE Output (VB-Audio Virtual Cable)</em></li>
          <li>Soppressione del rumore: <em>Nessuna</em> (Krisp taglia effetti e musica)</li>
          <li>Controllo automatico del guadagno: <em>spento</em></li>
          <li>Modalità di input: <em>Attività vocale</em>, sensibilità manuale intorno a −45 dB<br />
            oppure push-to-talk con il relay nelle impostazioni di MicFX</li>
        </ul>
      </div>
    </li>

    <li class:ok={rateOk}>
      <span class="check">{rateOk ? '✅' : '⑤'}</span>
      <div>
        <strong>Facoltativo: tutto a 48 kHz</strong>
        <p>
          {rateOk
            ? 'Cuffie e VB-Cable sono già a 48000 Hz.'
            : 'Proprietà → Avanzate → 48000 Hz su cuffie, CABLE Input e CABLE Output. Funziona anche senza, con una conversione in più.'}
        </p>
      </div>
    </li>
  </ol>

  {#snippet footer()}
    <span class="foot">Puoi riaprire questa guida dal pulsante ❔ in alto.</span>
    <button type="button" class="primary" onclick={close}>{cableOk && micOk ? 'Tutto pronto' : 'Chiudi'}</button>
  {/snippet}
</Modal>

<style>
  .intro {
    margin-top: 0;
    color: var(--fg-muted);
  }
  .steps {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .steps > li {
    display: flex;
    gap: 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px;
  }
  .steps > li.ok {
    border-color: var(--ok);
  }
  .check {
    font-size: 18px;
    width: 24px;
    flex: none;
  }
  p {
    margin: 4px 0 8px;
  }
  ul {
    margin: 0;
    padding-left: 18px;
  }
  .foot {
    flex: 1;
    font-size: 12px;
    color: var(--fg-muted);
    align-self: center;
  }
</style>
