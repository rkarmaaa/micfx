<script lang="ts">
  // Configurazione una tantum (SPEC §14). I passi che il core sa verificare si spuntano da soli,
  // in tempo reale, mentre l'utente cambia le impostazioni di Windows.
  import { api } from '../api';
  import { Check, CircleHelp, ExternalLink, TriangleAlert } from '../icons';
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
      <span class="check" aria-label={cableOk ? 'Fatto' : 'Da fare'}>{#if cableOk}<Check size={15} strokeWidth={3} />{:else}1{/if}</span>
      <div>
        <strong>Installa VB-Cable</strong>
        {#if cableOk}
          <p>Trovato: {app.devices.find((d) => d.is_cable)?.name ?? 'CABLE Input'}.</p>
        {:else}
          <p>Scarica il pacchetto, estrailo, avvia <code>VBCABLE_Setup_x64.exe</code> come amministratore e riavvia il PC.</p>
          <button type="button" class="primary sm" onclick={() => api.openLink('vb-cable')}><ExternalLink size={14} /> Apri vb-audio.com</button>
        {/if}
      </div>
    </li>

    <li class:ok={micOk}>
      <span class="check" aria-label={micOk ? 'Fatto' : 'Da fare'}>{#if micOk}<Check size={15} strokeWidth={3} />{:else}2{/if}</span>
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
          <button type="button" class="primary sm" disabled={!cableOk} onclick={() => api.openSoundSettings(1)}>Apri scheda Registrazione</button>
        {/if}
      </div>
    </li>

    <li class:ok={defaultOk}>
      <span class="check" aria-label={defaultOk ? 'Fatto' : 'Da fare'}>{#if defaultOk}<Check size={15} strokeWidth={3} />{:else}3{/if}</span>
      <div>
        <strong>Le cuffie restano il dispositivo predefinito</strong>
        {#if defaultOk}
          <p>Ok: CABLE Input non è il predefinito.</p>
        {:else}
          <p>Scheda <em>Riproduzione</em>: imposta le cuffie come predefinite, mai CABLE Input.</p>
          <button type="button" class="sm" onclick={() => api.openSoundSettings(0)}>Apri scheda Riproduzione</button>
        {/if}
      </div>
    </li>

    <li>
      <span class="check">4</span>
      <div>
        <strong>In Discord</strong>
        <p>Impostazioni → Voce e video:</p>
        <ul>
          <li>Dispositivo di input: <em>CABLE Output (VB-Audio Virtual Cable)</em></li>
          <li>
            Soppressione del rumore: <em>Nessuna</em> (Krisp taglia effetti e musica)
            {#if app.discord}{@render state(app.discord.krisp || app.discord.noise_suppression, 'ora attiva')}{/if}
          </li>
          <li>
            Controllo automatico del guadagno: <em>spento</em> (abbassa i suoni)
            {#if app.discord}{@render state(app.discord.automatic_gain_control, 'ora acceso')}{/if}
          </li>
          <li>
            Cancellazione dell'eco: <em>spenta</em>
            {#if app.discord}{@render state(app.discord.echo_cancellation, 'ora accesa')}{/if}
          </li>
          <li>
            Attività vocale avanzata: <em>spenta</em> (apre il microfono solo sulla voce)
            {#if app.discord?.advanced_vad}{@render state(true, 'ora accesa')}{/if}
          </li>
          <li>Modalità di input: <em>Attività vocale</em>, sensibilità manuale intorno a −45 dB<br />
            oppure push-to-talk con il relay nelle impostazioni di MicFX</li>
        </ul>
      </div>
    </li>

    <li class:ok={rateOk}>
      <span class="check" aria-label={rateOk ? 'Fatto' : 'Facoltativo'}>{#if rateOk}<Check size={15} strokeWidth={3} />{:else}5{/if}</span>
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
    <span class="foot">Puoi riaprire questa guida dal pulsante <CircleHelp size={13} /> in alto.</span>
    <button type="button" class="primary" onclick={close}>{cableOk && micOk ? 'Tutto pronto' : 'Chiudi'}</button>
  {/snippet}
</Modal>

{#snippet state(bad: boolean, text: string)}
  {#if bad}
    <span class="st bad"><TriangleAlert size={12} /> {text}</span>
  {:else}
    <span class="st ok"><Check size={12} strokeWidth={3} /> ok</span>
  {/if}
{/snippet}

<style>
  .intro {
    margin-bottom: 16px;
    color: var(--fg-muted);
    font-size: var(--text-md);
  }
  .steps {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .steps > li {
    display: flex;
    gap: 14px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--r-xl);
    background: var(--bg);
  }
  .steps > li.ok {
    border-color: color-mix(in srgb, var(--ok) 35%, var(--border));
    background: color-mix(in srgb, var(--ok) 5%, var(--bg));
  }
  .check {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--surface-3);
    color: var(--fg-2);
    font-size: var(--text-sm);
    font-weight: 700;
    flex: none;
  }
  .ok .check {
    background: var(--ok);
    color: #052e16;
  }
  strong {
    font-weight: 600;
  }
  p {
    margin: 4px 0 10px;
    color: var(--fg-2);
    font-size: var(--text-md);
  }
  p:last-child {
    margin-bottom: 0;
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding-left: 18px;
    color: var(--fg-2);
    font-size: var(--text-md);
  }
  em {
    color: var(--fg);
    font-style: normal;
    font-weight: 600;
  }
  .st {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 4px;
    padding: 0 7px;
    border-radius: 999px;
    font-size: var(--text-xs);
    font-weight: 600;
    line-height: 18px;
    vertical-align: 1px;
  }
  .st.ok {
    background: var(--ok-soft);
    color: var(--ok);
  }
  .st.bad {
    background: var(--warn-soft);
    color: var(--warn);
  }
  .foot {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
</style>
