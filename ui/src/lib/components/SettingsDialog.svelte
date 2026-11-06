<script lang="ts">
  import { api, type Diagnostics, type HotkeyAction, type PlayMode } from '../api';
  import { app } from '../stores/app.svelte';
  import { formatAccelerator, singleKeyFromEvent } from '../util/keys';
  import HotkeyInput from './HotkeyInput.svelte';
  import Modal from './Modal.svelte';
  import Switch from './Switch.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const s = $derived(app.settings!);
  let capturingPtt = $state(false);
  let diag = $state<Diagnostics | null>(null);

  const GLOBAL: { action: HotkeyAction; label: string }[] = [
    { action: { type: 'stop_all' }, label: 'Ferma tutti i suoni' },
    { action: { type: 'mute_cable' }, label: 'Attiva/disattiva microfono (suoni per gli altri)' },
    { action: { type: 'mute_monitor' }, label: 'Attiva/disattiva cuffie (suoni per te)' },
    { action: { type: 'pause_hotkeys' }, label: 'Metti in pausa le scorciatoie' },
    { action: { type: 'open_window' }, label: 'Apri MicFX' },
  ];

  const renderDevices = $derived(app.devices.filter((d) => !d.is_cable));
  const cableCandidates = $derived(app.devices);

  async function pttKey(e: KeyboardEvent) {
    if (!capturingPtt) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') {
      capturingPtt = false;
      return;
    }
    const k = singleKeyFromEvent(e);
    if (!k) return;
    capturingPtt = false;
    await app.updateSettings({ ptt: { key: k, enabled: true } });
  }

  async function loadDiag() {
    diag = await api.getDiagnostics();
  }
</script>

<Modal title="Impostazioni" {onclose} width={640}>
  <section>
    <h3>Dispositivi</h3>
    <label class="field">
      <span>🎧 Cuffie (quello che senti tu)</span>
      <select value={s.outputs.monitor.device_id ?? ''} onchange={(e) => app.setOutputDevice('monitor', e.currentTarget.value || null)}>
        <option value="">Segui il dispositivo predefinito di Windows</option>
        {#each renderDevices as d (d.id)}
          <option value={d.id}>{d.name}{d.is_default ? ' (predefinito)' : ''}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span>🎙️ Microfono virtuale (quello che sentono gli altri)</span>
      <select value={s.outputs.cable.device_id ?? ''} onchange={(e) => app.setOutputDevice('cable', e.currentTarget.value || null)}>
        <option value="">Rileva VB-Cable automaticamente</option>
        {#each cableCandidates as d (d.id)}
          <option value={d.id}>{d.name}</option>
        {/each}
      </select>
    </label>
    {#if app.status.includes('cable_missing')}
      <p class="warn">VB-Cable non trovato. <button type="button" class="link" onclick={() => (app.guideOpen = true)}>Apri la guida</button></p>
    {/if}
  </section>

  <section>
    <h3>Scorciatoie globali</h3>
    <p class="help">Funzionano anche con MicFX chiuso nella tray e nei giochi. Le scorciatoie dei singoli suoni si impostano dal suono (✎).</p>
    {#each GLOBAL as g (g.action.type)}
      <div class="hk-row">
        <span>{g.label}</span>
        <HotkeyInput action={g.action} label={g.label} compact />
      </div>
    {/each}
  </section>

  <section>
    <h3>Riproduzione</h3>
    <label class="field">
      <span>Modalità predefinita al clic</span>
      <select
        value={s.playback.default_mode}
        onchange={(e) => app.updateSettings({ playback: { default_mode: e.currentTarget.value as PlayMode } })}
      >
        <option value="overlap">Sovrapponi: ogni clic avvia una nuova copia</option>
        <option value="restart">Riavvia: riparte da capo</option>
        <option value="toggle">Play/Stop: il secondo clic ferma</option>
        <option value="exclusive">Esclusivo: ferma gli altri suoni</option>
      </select>
    </label>
  </section>

  <section>
    <h3>Push-to-talk di Discord</h3>
    <p class="help">
      Se in Discord usi il push-to-talk, MicFX tiene premuto il tuo tasto mentre un suono va agli altri e lo rilascia poco dopo.
      Con "Attività vocale" non serve.
    </p>
    <div class="inline">
      <Switch checked={s.ptt.enabled} label="Relay push-to-talk" disabled={!s.ptt.key} onchange={(v) => app.updateSettings({ ptt: { enabled: v } })} />
      <span>Tasto di Discord:</span>
      <button type="button" class:active={capturingPtt} onclick={() => (capturingPtt = !capturingPtt)} onkeydown={pttKey} onblur={() => (capturingPtt = false)}>
        {capturingPtt ? 'Premi il tasto…' : s.ptt.key ? formatAccelerator(s.ptt.key) : 'Scegli tasto'}
      </button>
      {#if s.ptt.key}
        <button type="button" class="icon" aria-label="Rimuovi tasto" title="Rimuovi tasto" onclick={() => app.updateSettings({ ptt: { key: null, enabled: false } })}>✕</button>
      {/if}
    </div>
    <label class="field small">
      <span>Rilascio dopo la fine del suono: {s.ptt.tail_ms} ms</span>
      <input
        type="range"
        min="0"
        max="1000"
        step="50"
        value={s.ptt.tail_ms}
        onchange={(e) => app.updateSettings({ ptt: { tail_ms: +e.currentTarget.value } })}
      />
    </label>
    <p class="help">Discord non deve essere avviato come amministratore, altrimenti Windows blocca il tasto simulato.</p>
  </section>

  <section>
    <h3>Import</h3>
    <label class="field small">
      <span>Livello di riferimento: {s.loudness.target_lufs} LUFS <small>(più alto = suoni più forti; vale per i prossimi import)</small></span>
      <input
        type="range"
        min="-30"
        max="-10"
        step="1"
        value={s.loudness.target_lufs}
        onchange={(e) => app.updateSettings({ loudness: { target_lufs: +e.currentTarget.value } })}
      />
    </label>
    <div class="inline">
      <Switch
        checked={s.loudness.trim_silence}
        label="Taglia il silenzio iniziale e finale"
        onchange={(v) => app.updateSettings({ loudness: { trim_silence: v } })}
      />
      <span>Taglia il silenzio all'inizio e alla fine (il suono parte subito)</span>
    </div>
    <label class="field small">
      <span>Durata massima per file</span>
      <select value={s.library.max_duration_s} onchange={(e) => app.updateSettings({ library: { max_duration_s: +e.currentTarget.value } })}>
        <option value={60}>1 minuto</option>
        <option value={300}>5 minuti</option>
        <option value={600}>10 minuti</option>
        <option value={1800}>30 minuti</option>
        <option value={3600}>60 minuti</option>
      </select>
    </label>
  </section>

  <section>
    <h3>Avvio e finestra</h3>
    <div class="inline">
      <Switch checked={s.startup.autostart} label="Avvia con Windows" onchange={(v) => app.updateSettings({ startup: { autostart: v } })} />
      <span>Avvia con Windows (parte nascosto nella tray)</span>
    </div>
    <label class="field small">
      <span>Chiudendo la finestra MicFX resta nella tray. Tieni la finestra pronta per: {s.ui.keepalive_s} s</span>
      <input
        type="range"
        min="0"
        max="300"
        step="10"
        value={s.ui.keepalive_s}
        onchange={(e) => app.updateSettings({ ui: { keepalive_s: +e.currentTarget.value } })}
      />
    </label>
  </section>

  <section>
    <h3>Dati</h3>
    <p class="help">
      Libreria e impostazioni sono salvate sul PC e restano identiche se aggiorni o reinstalli MicFX.
    </p>
    <p class="path">{app.info?.library_dir}</p>
    <div class="inline">
      <button type="button" onclick={() => api.openDataFolder()}>Apri cartella dati</button>
      <button type="button" onclick={loadDiag}>Diagnostica</button>
    </div>
    {#if diag}
      <table class="diag">
        <thead><tr><th>Uscita</th><th>Stato</th><th>Dispositivo</th><th>Voci</th><th>Underrun</th><th>Errori</th><th>Mix</th></tr></thead>
        <tbody>
          {#each diag.buses as b}
            <tr>
              <td>{b.bus === 'cable' ? 'Microfono' : 'Cuffie'}</td>
              <td>{b.state}</td>
              <td>{b.device || '—'}</td>
              <td>{b.voices}</td>
              <td>{b.underruns}</td>
              <td>{b.errors}</td>
              <td>{b.mix_rate ? `${b.mix_rate} Hz` : '—'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="help">MicFX {diag.version} · {diag.sounds} suoni · {diag.playing} in riproduzione · PTT {diag.ptt_down ? 'premuto' : 'rilasciato'}</p>
    {/if}
  </section>
</Modal>

<style>
  section {
    padding-bottom: 14px;
    margin-bottom: 14px;
    border-bottom: 1px solid var(--border);
  }
  section:last-child {
    border-bottom: none;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 14px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }
  .field.small span {
    font-size: 13px;
  }
  .help {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .warn {
    color: var(--warn);
    font-size: 13px;
  }
  .hk-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
    align-items: start;
    padding: 4px 0;
  }
  .inline {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 8px;
    flex-wrap: wrap;
  }
  .active {
    outline: 2px solid var(--accent);
  }
  .path {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
    word-break: break-all;
  }
  .diag {
    width: 100%;
    font-size: 12px;
    border-collapse: collapse;
    margin-top: 8px;
  }
  .diag th,
  .diag td {
    text-align: left;
    padding: 3px 6px;
    border-bottom: 1px solid var(--border);
  }
</style>
