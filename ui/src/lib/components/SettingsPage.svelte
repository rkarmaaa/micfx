<script lang="ts">
  // Impostazioni a pagina intera: sezioni nel menu a sinistra (seguono lo scorrimento), ogni riga
  // ha titolo e spiegazione a sinistra e il controllo a destra. Esc o la freccia tornano ai suoni.
  import { onMount, type Component } from 'svelte';

  import { api, type Diagnostics, type HotkeyAction, type PlayMode } from '../api';
  import {
    Activity,
    ArrowLeft,
    BookOpen,
    ExternalLink,
    FolderOpen,
    HardDrive,
    Import,
    Info,
    Keyboard,
    Mic,
    Play,
    Power,
    Speaker,
    TriangleAlert,
    X,
  } from '../icons';
  import { app } from '../stores/app.svelte';
  import { formatAccelerator, singleKeyFromEvent } from '../util/keys';
  import { CABLE_NAME, isMac, kb, OS_NAME } from '../util/platform';
  import HotkeyInput from './HotkeyInput.svelte';
  import Select from './Select.svelte';
  import Switch from './Switch.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const s = $derived(app.settings!);
  const version = $derived(app.info?.version ?? '');

  type Section = { id: string; label: string; icon: Component<{ size?: number }> };
  const SECTIONS: Section[] = [
    { id: 'audio', label: 'Audio', icon: Speaker },
    { id: 'hotkeys', label: 'Scorciatoie', icon: Keyboard },
    { id: 'playback', label: 'Riproduzione', icon: Play },
    { id: 'ptt', label: 'Push-to-talk', icon: Mic },
    { id: 'import', label: 'Importazione', icon: Import },
    { id: 'startup', label: 'Avvio e finestra', icon: Power },
    { id: 'data', label: 'Dati e diagnostica', icon: HardDrive },
    { id: 'about', label: 'Informazioni', icon: Info },
  ];

  const GLOBAL: { action: HotkeyAction; label: string; help: string }[] = [
    { action: { type: 'stop_all' }, label: 'Ferma tutti i suoni', help: 'Interrompe subito ogni suono in riproduzione.' },
    { action: { type: 'mute_cable' }, label: 'Microfono on/off', help: 'Smetti di mandare suoni agli altri, senza chiudere MicFX.' },
    { action: { type: 'mute_monitor' }, label: 'Cuffie on/off', help: 'Smetti di sentire i suoni in cuffia.' },
    { action: { type: 'pause_hotkeys' }, label: 'Pausa scorciatoie', help: 'Sospende tutte le scorciatoie globali, utile mentre scrivi.' },
    { action: { type: 'open_window' }, label: 'Apri MicFX', help: 'Porta in primo piano questa finestra.' },
  ];

  const MODES: { value: PlayMode; label: string }[] = [
    { value: 'overlap', label: 'Sovrapponi' },
    { value: 'restart', label: 'Riavvia' },
    { value: 'toggle', label: 'Play/Stop' },
    { value: 'exclusive', label: 'Esclusivo' },
  ];
  const DURATION_STEPS = [60, 300, 600, 1800, 3600];
  /** Durate proposte; un valore scritto a mano nel config.json resta visibile come voce in più. */
  const DURATIONS = $derived.by(() => {
    const cur = app.settings?.library.max_duration_s;
    const steps = cur && !DURATION_STEPS.includes(cur) ? [...DURATION_STEPS, cur].sort((x, y) => x - y) : DURATION_STEPS;
    return steps.map((v) => ({ value: v, label: v % 60 ? `${v} secondi` : `${v / 60} ${v === 60 ? 'minuto' : 'minuti'}` }));
  });
  const MODE_HELP: Record<PlayMode, string> = {
    overlap: 'Ogni clic avvia una nuova copia, anche se il suono sta già suonando.',
    restart: 'Se il suono sta già suonando, riparte da capo.',
    toggle: 'Il secondo clic ferma il suono.',
    exclusive: 'Ferma tutti gli altri suoni e parte.',
  };

  const renderDevices = $derived(app.devices.filter((d) => !d.is_cable));
  const cableMissing = $derived(app.status.includes('cable_missing'));

  let active = $state('audio');
  let scroller: HTMLElement;
  let capturingPtt = $state(false);
  let diag = $state<Diagnostics | null>(null);
  let diagLoading = $state(false);

  /** Valori locali dei cursori: si vedono mentre trascini, il core li riceve al rilascio. */
  let tail = $state<number | null>(null);
  let lufs = $state<number | null>(null);
  let keepalive = $state<number | null>(null);

  function fill(v: number, min: number, max: number) {
    return `${((v - min) / (max - min)) * 100}%`;
  }

  function go(id: string, smooth = true) {
    const el = scroller?.querySelector<HTMLElement>(`#set-${id}`);
    if (!el) return;
    active = id;
    scroller.scrollTo({ top: el.offsetTop - 24, behavior: smooth ? 'smooth' : 'instant' });
  }

  onMount(() => {
    if (app.settingsSection) {
      go(app.settingsSection, false);
      app.settingsSection = null;
    }
    scroller.focus({ preventScroll: true });
    // La sezione attiva è l'ultima il cui titolo ha superato il terzo superiore della pagina.
    const onscroll = () => {
      const limit = scroller.scrollTop + scroller.clientHeight / 3;
      let current = SECTIONS[0].id;
      for (const sec of SECTIONS) {
        const el = scroller.querySelector<HTMLElement>(`#set-${sec.id}`);
        if (el && el.offsetTop <= limit) current = sec.id;
      }
      if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 4) current = SECTIONS[SECTIONS.length - 1].id;
      active = current;
    };
    scroller.addEventListener('scroll', onscroll, { passive: true });
    return () => scroller.removeEventListener('scroll', onscroll);
  });

  // Nel menu orizzontale (finestre strette) la sezione attiva resta visibile.
  $effect(() => {
    const id = active;
    document
      .querySelector<HTMLElement>(`.nav [data-sec="${id}"]`)
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.defaultPrevented || app.capturing || capturingPtt) return;
    if (e.key === 'Escape' || ((e.ctrlKey || e.metaKey) && e.key === ',')) {
      e.preventDefault();
      onclose();
    }
  }

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
    diagLoading = true;
    try {
      diag = await api.getDiagnostics();
    } finally {
      diagLoading = false;
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="page">
  <header class="bar">
    <button type="button" class="back" aria-label="Torna ai suoni" onclick={onclose}><ArrowLeft size={18} /></button>
    <h1>Impostazioni</h1>
    <span class="spacer"></span>
    <button type="button" class="close" aria-label="Chiudi le impostazioni" onclick={onclose}><X size={18} /></button>
  </header>

  <div class="body">
    <nav class="nav" aria-label="Sezioni delle impostazioni">
      <ul>
        {#each SECTIONS as sec (sec.id)}
          {@const Icon = sec.icon}
          <li>
            <button type="button" data-sec={sec.id} class:active={active === sec.id} aria-current={active === sec.id ? 'true' : undefined} onclick={() => go(sec.id)}>
              <Icon size={16} />
              <span>{sec.label}</span>
            </button>
          </li>
        {/each}
      </ul>
      <p class="nav-foot">MicFX {version}</p>
    </nav>

    <div class="content" bind:this={scroller} tabindex="-1">
      <div class="inner">
        <!-- ------------------------------------------------------------ Audio -->
        <section id="set-audio" aria-labelledby="h-audio">
          <h2 id="h-audio">Audio</h2>
          <p class="lead">Dove suonano i suoni: nelle tue cuffie e nel microfono virtuale che ascolta Discord.</p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Cuffie</span>
                <span class="d">Quello che senti tu. Di solito il dispositivo predefinito di {OS_NAME}.</span>
              </div>
              <Select
                class="ctl wide"
                label="Cuffie"
                value={s.outputs.monitor.device_id ?? ''}
                options={[
                  { value: '', label: `Predefinito di ${OS_NAME}` },
                  ...renderDevices.map((d) => ({ value: d.id, label: d.name + (d.is_default ? ' (predefinito)' : '') })),
                ]}
                onchange={(v) => app.setOutputDevice('monitor', v || null)}
              />
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Microfono virtuale</span>
                <span class="d"
                  >{isMac
                    ? 'Quello che sentono gli altri: BlackHole, che Discord usa come microfono.'
                    : "Quello che sentono gli altri: l'ingresso di VB-Cable (CABLE Input)."}</span
                >
              </div>
              <Select
                class="ctl wide"
                label="Microfono virtuale"
                value={s.outputs.cable.device_id ?? ''}
                options={[{ value: '', label: `Rileva ${CABLE_NAME} automaticamente` }, ...app.devices.map((d) => ({ value: d.id, label: d.name }))]}
                onchange={(v) => app.setOutputDevice('cable', v || null)}
              />
            </div>
            {#if cableMissing}
              <div class="row callout">
                <TriangleAlert size={16} />
                <span class="grow">{CABLE_NAME} non è installato: i tuoi amici non sentono i suoni.</span>
                <button type="button" class="sm" onclick={() => (app.guideOpen = true)}>Apri la guida</button>
              </div>
            {/if}
            {#if isMac}
              <div class="row">
                <div class="text">
                  <span class="t">La tua voce</span>
                  <span class="d">Il microfono che MicFX manda dentro BlackHole insieme ai suoni, così su Discord ti sentono.</span>
                </div>
                <Select
                  class="ctl wide"
                  label="La tua voce"
                  value={s.mic.device_id ?? ''}
                  options={[
                    { value: '', label: 'Microfono predefinito di macOS' },
                    ...app.mics.map((m) => ({ value: m.id, label: m.name + (m.is_default ? ' (predefinito)' : '') })),
                  ]}
                  onchange={(v) => app.updateSettings({ mic: { device_id: v || null } })}
                />
              </div>
            {/if}
            <div class="row">
              <div class="text">
                <span class="t">Configurazione con Discord</span>
                <span class="d">I passaggi da fare una volta sola, con le verifiche automatiche.</span>
              </div>
              <button type="button" class="ctl" onclick={() => (app.guideOpen = true)}><BookOpen size={15} /> Apri la guida</button>
            </div>
          </div>
        </section>

        <!-- ------------------------------------------------------------ Scorciatoie -->
        <section id="set-hotkeys" aria-labelledby="h-hotkeys">
          <h2 id="h-hotkeys">Scorciatoie globali</h2>
          <p class="lead">
            Funzionano ovunque, anche con MicFX {isMac ? 'nella barra dei menu' : 'nella tray'} e nei giochi a schermo intero. Quelle dei singoli suoni si impostano dal
            suono, con <kbd>{kb('Ctrl')}</kbd> <kbd>{kb('Invio')}</kbd>.
          </p>
          <div class="card">
            {#each GLOBAL as g (g.action.type)}
              <div class="row">
                <div class="text">
                  <span class="t">{g.label}</span>
                  <span class="d">{g.help}</span>
                </div>
                <div class="ctl"><HotkeyInput action={g.action} label={g.label} compact /></div>
              </div>
            {/each}
            <div class="row">
              <div class="text">
                <span class="t">Scorciatoie della finestra</span>
                <span class="d">Tastiera dentro MicFX: frecce, Invio, F2, Canc e le altre.</span>
              </div>
              <button type="button" class="ctl" onclick={() => (app.shortcutsOpen = true)}><Keyboard size={15} /> Mostra elenco</button>
            </div>
          </div>
        </section>

        <!-- ------------------------------------------------------------ Riproduzione -->
        <section id="set-playback" aria-labelledby="h-playback">
          <h2 id="h-playback">Riproduzione</h2>
          <p class="lead">Cosa succede quando premi un suono. Ogni suono può avere una modalità sua.</p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Modalità predefinita</span>
                <span class="d">{MODE_HELP[s.playback.default_mode]}</span>
              </div>
              <Select
                class="ctl"
                label="Modalità predefinita"
                value={s.playback.default_mode}
                options={MODES}
                onchange={(v) => app.updateSettings({ playback: { default_mode: v } })}
              />
            </div>
          </div>
        </section>

        <!-- ------------------------------------------------------------ PTT -->
        <section id="set-ptt" aria-labelledby="h-ptt">
          <h2 id="h-ptt">Push-to-talk di Discord</h2>
          <p class="lead">
            Se in Discord usi il push-to-talk, MicFX tiene premuto il tuo tasto mentre un suono va agli altri e lo rilascia subito
            dopo. Con "Attività vocale" non serve.
          </p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Tasto di Discord</span>
                <span class="d">Lo stesso che hai impostato in Discord → Tasti di scelta rapida.</span>
              </div>
              <div class="ctl inline">
                <button
                  type="button"
                  class="capture"
                  class:active={capturingPtt}
                  onclick={() => (capturingPtt = !capturingPtt)}
                  onkeydown={pttKey}
                  onblur={() => (capturingPtt = false)}
                >
                  {#if capturingPtt}
                    <span class="hint">Premi il tasto…</span>
                  {:else if s.ptt.key}
                    <kbd>{formatAccelerator(s.ptt.key)}</kbd>
                  {:else}
                    <span class="none">Scegli tasto</span>
                  {/if}
                </button>
                {#if s.ptt.key}
                  <button
                    type="button"
                    class="icon"
                    aria-label="Rimuovi tasto"
                    onclick={() => app.updateSettings({ ptt: { key: null, enabled: false } })}><X size={15} /></button
                  >
                {/if}
              </div>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Relay push-to-talk</span>
                <span class="d">{s.ptt.key ? 'Premi il tasto in automatico mentre un suono va agli altri.' : 'Scegli prima il tasto di Discord.'}</span>
              </div>
              <div class="ctl">
                <Switch checked={s.ptt.enabled} label="Relay push-to-talk" disabled={!s.ptt.key} onchange={(v) => app.updateSettings({ ptt: { enabled: v } })} />
              </div>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Rilascio dopo il suono</span>
                <span class="d">Margine prima di lasciare il tasto, perché la coda del suono arrivi intera.</span>
              </div>
              <div class="ctl range">
                <input
                  type="range"
                  min="0"
                  max="1000"
                  step="50"
                  value={tail ?? s.ptt.tail_ms}
                  aria-label="Rilascio dopo il suono"
                  style:--fill={fill(tail ?? s.ptt.tail_ms, 0, 1000)}
                  oninput={(e) => (tail = +e.currentTarget.value)}
                  onchange={async (e) => {
                    await app.updateSettings({ ptt: { tail_ms: +e.currentTarget.value } });
                    tail = null;
                  }}
                />
                <output>{tail ?? s.ptt.tail_ms} ms</output>
              </div>
            </div>
            <p class="foot">
              {isMac
                ? 'Serve il permesso Accessibilità: Impostazioni di Sistema → Privacy e sicurezza → Accessibilità → MicFX.'
                : 'Discord non deve essere avviato come amministratore, altrimenti Windows blocca il tasto simulato.'}
            </p>
          </div>
        </section>

        <!-- ------------------------------------------------------------ Import -->
        <section id="set-import" aria-labelledby="h-import">
          <h2 id="h-import">Importazione</h2>
          <p class="lead">Come vengono preparati i file che aggiungi. Valgono per i prossimi import.</p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Volume di riferimento</span>
                <span class="d">Tutti i suoni vengono livellati a questo volume. Più alto = suoni più forti.</span>
              </div>
              <div class="ctl range">
                <input
                  type="range"
                  min="-30"
                  max="-10"
                  step="1"
                  value={lufs ?? s.loudness.target_lufs}
                  aria-label="Volume di riferimento in LUFS"
                  style:--fill={fill(lufs ?? s.loudness.target_lufs, -30, -10)}
                  oninput={(e) => (lufs = +e.currentTarget.value)}
                  onchange={async (e) => {
                    await app.updateSettings({ loudness: { target_lufs: +e.currentTarget.value } });
                    lufs = null;
                  }}
                />
                <output>{lufs ?? s.loudness.target_lufs} LUFS</output>
              </div>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Taglia il silenzio</span>
                <span class="d">Toglie il silenzio all'inizio e alla fine, così il suono parte subito.</span>
              </div>
              <div class="ctl">
                <Switch
                  checked={s.loudness.trim_silence}
                  label="Taglia il silenzio iniziale e finale"
                  onchange={(v) => app.updateSettings({ loudness: { trim_silence: v } })}
                />
              </div>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Durata massima</span>
                <span class="d">I file più lunghi non vengono importati.</span>
              </div>
              <Select
                class="ctl"
                label="Durata massima"
                value={s.library.max_duration_s}
                options={DURATIONS}
                onchange={(v) => app.updateSettings({ library: { max_duration_s: v } })}
              />
            </div>
          </div>
        </section>

        <!-- ------------------------------------------------------------ Avvio -->
        <section id="set-startup" aria-labelledby="h-startup">
          <h2 id="h-startup">Avvio e finestra</h2>
          <p class="lead">
            Chiudendo la finestra MicFX resta {isMac ? 'nella barra dei menu' : 'nella tray'} e le scorciatoie continuano a funzionare.
          </p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">{isMac ? "Avvia all'accesso" : 'Avvia con Windows'}</span>
                <span class="d">Parte nascosto {isMac ? 'nella barra dei menu' : 'nella tray'}, pronto con le scorciatoie.</span>
              </div>
              <div class="ctl">
                <Switch checked={s.startup.autostart} label={isMac ? "Avvia all'accesso" : 'Avvia con Windows'} onchange={(v) => app.updateSettings({ startup: { autostart: v } })} />
              </div>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Avvisa quando esce una nuova versione</span>
                <span class="d">All'apertura della finestra MicFX controlla su GitHub, al massimo ogni 6 ore.</span>
              </div>
              <div class="ctl">
                <Switch checked={app.prefs.updateNotify} label="Avvisa quando esce una nuova versione" onchange={(v) => app.setPrefs({ updateNotify: v })} />
              </div>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Finestra pronta dopo la chiusura</span>
                <span class="d">Per quanto la finestra resta in memoria per riaprirsi all'istante. 0 = libera subito la memoria.</span>
              </div>
              <div class="ctl range">
                <input
                  type="range"
                  min="0"
                  max="300"
                  step="10"
                  value={keepalive ?? s.ui.keepalive_s}
                  aria-label="Secondi in cui la finestra resta pronta"
                  style:--fill={fill(keepalive ?? s.ui.keepalive_s, 0, 300)}
                  oninput={(e) => (keepalive = +e.currentTarget.value)}
                  onchange={async (e) => {
                    await app.updateSettings({ ui: { keepalive_s: +e.currentTarget.value } });
                    keepalive = null;
                  }}
                />
                <output>{keepalive ?? s.ui.keepalive_s} s</output>
              </div>
            </div>
          </div>
        </section>

        <!-- ------------------------------------------------------------ Dati -->
        <section id="set-data" aria-labelledby="h-data">
          <h2 id="h-data">Dati e diagnostica</h2>
          <p class="lead">Libreria e impostazioni sono salvate su questo PC e restano identiche se aggiorni o reinstalli MicFX.</p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Cartella della libreria</span>
                <span class="d path selectable">{app.info?.library_dir}</span>
              </div>
              <button type="button" class="ctl" onclick={() => api.openDataFolder()}><FolderOpen size={15} /> Apri cartella</button>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Diagnostica audio</span>
                <span class="d">Stato delle uscite, interruzioni dell'audio ed errori. Utile per segnalare un problema.</span>
              </div>
              <button type="button" class="ctl" disabled={diagLoading} onclick={loadDiag}><Activity size={15} /> {diag ? 'Aggiorna' : 'Esegui'}</button>
            </div>
            {#if diag}
              <div class="diag">
                <table>
                  <thead>
                    <tr><th>Uscita</th><th>Stato</th><th>Dispositivo</th><th>Voci</th><th>Underrun</th><th>Errori</th><th>Mix</th></tr>
                  </thead>
                  <tbody>
                    {#each diag.buses as b (b.bus)}
                      <tr>
                        <td>{b.bus === 'cable' ? 'Microfono' : 'Cuffie'}</td>
                        <td><span class="state" class:ok={b.state === 'running'}>{b.state}</span></td>
                        <td class="dev">{b.device || '—'}</td>
                        <td>{b.voices}</td>
                        <td class:bad={b.underruns > 0}>{b.underruns}</td>
                        <td class:bad={b.errors > 0}>{b.errors}</td>
                        <td>{b.mix_rate ? `${b.mix_rate / 1000} kHz` : '—'}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
                <p class="foot">
                  {diag.sounds} suoni · {diag.playing} in riproduzione · push-to-talk {diag.ptt_down ? 'premuto' : 'rilasciato'}
                </p>
              </div>
            {/if}
          </div>
        </section>

        <!-- ------------------------------------------------------------ Informazioni -->
        <section id="set-about" aria-labelledby="h-about">
          <h2 id="h-about">Informazioni</h2>
          <div class="card about">
            <span class="logo" aria-hidden="true"><Mic size={26} strokeWidth={2.2} /></span>
            <div class="grow">
              <span class="name">MicFX</span>
              <span class="d">Soundboard per Discord con microfono virtuale.</span>
            </div>
            <div class="version">
              <span class="d">Versione</span>
              <span class="num">{version}</span>
            </div>
          </div>
          <div class="card links">
            <div class="row">
              <div class="text">
                <span class="t">Codice sorgente e versioni</span>
                <span class="d selectable">github.com/rkarmaaa/micfx</span>
              </div>
              <button type="button" class="ctl" onclick={() => api.openLink('github')}><ExternalLink size={15} /> Apri su GitHub</button>
            </div>
          </div>
          <p class="note">
            Per aggiornare scarica l'ultima versione da GitHub e installala sopra questa: libreria, scorciatoie e impostazioni
            restano al loro posto.
          </p>
        </section>
      </div>
    </div>
  </div>
</div>

<style>
  .page {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }

  /* --- barra --- */
  .bar {
    --focus: rgb(129 140 248 / 0.9);
    display: flex;
    align-items: center;
    gap: 8px;
    height: var(--bar-h);
    padding: 0 10px var(--bar-lift);
    background: var(--bar-bg);
    color: var(--bar-fg);
    border-bottom: 1px solid var(--bar-border);
    flex: none;
    color-scheme: dark;
  }
  h1 {
    font-size: 15px;
    font-weight: 650;
  }
  .spacer {
    flex: 1;
  }
  .back,
  .close {
    width: 34px;
    height: 34px;
    padding: 0;
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--bar-muted);
  }
  .back:hover:not(:disabled),
  .close:hover:not(:disabled) {
    background: var(--bar-hover);
    color: var(--bar-fg);
  }

  /* --- corpo --- */
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 228px minmax(0, 1fr);
  }
  .nav {
    display: flex;
    flex-direction: column;
    padding: 20px 12px 16px;
    border-right: 1px solid var(--border);
    overflow: auto;
  }
  .nav ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .nav button {
    justify-content: flex-start;
    gap: 10px;
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--fg-muted);
    font-size: var(--text-md);
    font-weight: 500;
  }
  .nav button:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--fg);
  }
  .nav button.active,
  .nav button.active:hover:not(:disabled) {
    background: var(--primary-soft);
    color: var(--primary-text);
    font-weight: 600;
  }
  .nav-foot {
    margin-top: auto;
    padding: 12px 10px 0;
    color: var(--fg-faint);
    font-size: var(--text-sm);
  }

  .content {
    position: relative;
    overflow: auto;
    outline: none;
    scroll-padding-top: 24px;
  }
  .inner {
    max-width: 760px;
    margin: 0 auto;
    padding: 28px 32px 64px;
  }
  section + section {
    margin-top: 36px;
  }
  h2 {
    font-size: var(--text-lg);
  }
  .lead {
    max-width: 620px;
    margin: 4px 0 12px;
    color: var(--fg-muted);
    font-size: var(--text-md);
  }
  .lead kbd {
    height: 18px;
  }

  /* --- schede e righe --- */
  .card {
    border: 1px solid var(--border);
    border-radius: var(--r-xl);
    background: var(--surface);
    box-shadow: var(--shadow-xs);
  }
  h2 + .card {
    margin-top: 12px;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 6px 24px;
    padding: 14px 16px;
  }
  .row + .row {
    border-top: 1px solid var(--border);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .t {
    font-size: var(--text-base);
    font-weight: 550;
  }
  .d {
    color: var(--fg-muted);
    font-size: var(--text-md);
  }
  .path {
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ctl {
    justify-self: end;
  }
  .row > :global(.select.ctl) {
    justify-self: end;
    min-width: 180px;
    max-width: 300px;
  }
  .row > :global(.select.ctl.wide) {
    width: 300px;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .range {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .range input {
    width: 200px;
  }
  .range output {
    min-width: 64px;
    color: var(--fg-2);
    font-size: var(--text-md);
    font-weight: 550;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .callout {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--warn-soft);
    color: var(--warn);
    font-size: var(--text-md);
  }
  .callout .grow {
    flex: 1;
    color: var(--fg-2);
  }
  .foot {
    margin: 0;
    padding: 10px 16px 12px;
    border-top: 1px solid var(--border);
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .links {
    margin-top: 10px;
  }
  .note {
    margin-top: 10px;
    color: var(--fg-muted);
    font-size: var(--text-md);
  }

  /* --- tasto PTT --- */
  .capture {
    justify-content: flex-start;
    min-width: 150px;
    padding: 0 8px;
    background: var(--field);
    font-weight: 450;
  }
  .capture.active,
  .capture.active:hover:not(:disabled) {
    border-color: var(--primary);
    box-shadow: 0 0 0 3px var(--primary-soft-2);
  }
  .capture .hint {
    color: var(--primary-text);
  }
  .capture .none {
    color: var(--fg-faint);
  }

  /* --- diagnostica --- */
  .diag {
    border-top: 1px solid var(--border);
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }
  th {
    padding: 8px 12px;
    background: color-mix(in srgb, var(--surface-2) 60%, var(--surface));
    color: var(--fg-muted);
    font-weight: 600;
    text-align: left;
    white-space: nowrap;
  }
  td {
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    white-space: nowrap;
  }
  td.dev {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  td.bad {
    color: var(--danger);
    font-weight: 600;
  }
  .state {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--fg-muted);
  }
  .state.ok {
    background: var(--ok-soft);
    color: var(--ok);
  }

  /* --- informazioni --- */
  .about {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 18px 20px;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: 14px;
    background: var(--primary);
    color: #fff;
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.18),
      0 4px 14px rgb(79 70 229 / 0.35);
    flex: none;
  }
  .about .grow {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-size: var(--text-lg);
    font-weight: 700;
  }
  .version {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }
  .num {
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--primary-soft);
    color: var(--primary-text);
    font-size: var(--text-md);
    font-weight: 650;
  }

  /* --- finestre strette: menu in alto, righe impilate --- */
  @media (max-width: 760px) {
    .body {
      grid-template-columns: 1fr;
      grid-template-rows: auto minmax(0, 1fr);
    }
    .nav {
      padding: 8px 12px;
      border-right: none;
      border-bottom: 1px solid var(--border);
      overflow-x: auto;
      overflow-y: hidden;
      scrollbar-width: none;
    }
    .nav ul {
      flex-direction: row;
    }
    .nav button {
      width: auto;
      white-space: nowrap;
    }
    .nav-foot {
      display: none;
    }
    .inner {
      padding: 20px 16px 48px;
    }
    .row {
      grid-template-columns: 1fr;
    }
    .ctl {
      justify-self: start;
    }
    .row > :global(.select.ctl) {
      justify-self: start;
    }
    .row > :global(.select.ctl.wide) {
      width: 100%;
      max-width: none;
    }
  }
</style>
