<script lang="ts">
  // Impostazioni a pagina intera, in ordine d'uso: dove suonano i suoni, la tua voce, Discord,
  // scorciatoie, suoni, generale, informazioni. Sezioni nel menu a sinistra (seguono lo scorrimento);
  // ogni riga ha titolo e spiegazione a sinistra e il controllo a destra. Esc o la freccia tornano ai suoni.
  import { onMount, type Component } from 'svelte';
  import Keys from './Keys.svelte';

  import { api, type Diagnostics, type HotkeyAction, type PlayMode } from '../api';
  import {
    Activity,
    ArrowLeft,
    BookOpen,
    Ear,
    ExternalLink,
    FolderOpen,
    Info,
    Keyboard,
    MessageCircle,
    Mic,
    MicVocal,
    Play,
    RefreshCw,
    Settings2,
    Speaker,
    Square,
    TriangleAlert,
    X,
  } from '../icons';
  import { app } from '../stores/app.svelte';
  import { cableOptions, micOptions, monitorOptions } from '../util/devices';
  import { singleKeyFromEvent } from '../util/keys';
  import { CABLE_NAME, isMac } from '../util/platform';
  import HotkeyInput from './HotkeyInput.svelte';
  import Select from './Select.svelte';
  import Switch from './Switch.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const s = $derived(app.settings!);
  const version = $derived(app.info?.version ?? '');
  const TRAY = isMac ? 'nella barra dei menu' : 'nella tray';

  // Controllo aggiornamenti manuale, sotto il numero di versione. Se c'è una versione nuova compare
  // il solito toast con "Aggiorna ora".
  let updateState = $state<'idle' | 'checking' | 'newer' | 'current' | 'error'>('idle');
  const updateLabel = $derived(
    {
      idle: 'Controlla aggiornamenti',
      checking: 'Controllo…',
      newer: 'Nuova versione disponibile',
      current: 'Sei aggiornato',
      error: 'Controllo non riuscito, riprova',
    }[updateState],
  );
  async function checkUpdate() {
    updateState = 'checking';
    updateState = (await app.checkUpdate(true)) ?? 'error';
  }

  type Section = { id: string; label: string; icon: Component<{ size?: number }> };
  const SECTIONS: Section[] = [
    { id: 'audio', label: 'Audio', icon: Speaker },
    { id: 'voice', label: 'Voce', icon: MicVocal },
    { id: 'discord', label: 'Discord', icon: MessageCircle },
    { id: 'hotkeys', label: 'Scorciatoie', icon: Keyboard },
    { id: 'sounds', label: 'Suoni', icon: Play },
    { id: 'general', label: 'Generale', icon: Settings2 },
    { id: 'about', label: 'Informazioni', icon: Info },
  ];

  const GLOBAL: { action: HotkeyAction; label: string; help: string }[] = [
    { action: { type: 'stop_all' }, label: 'Ferma tutti i suoni', help: 'Interrompe subito ogni suono.' },
    { action: { type: 'mute_cable' }, label: 'Microfono on/off', help: 'Smetti di mandare suoni agli altri.' },
    { action: { type: 'mute_monitor' }, label: 'Cuffie on/off', help: 'Smetti di sentire i suoni in cuffia.' },
    { action: { type: 'pause_hotkeys' }, label: 'Pausa scorciatoie', help: 'Sospende tutte le scorciatoie, utile mentre scrivi.' },
    { action: { type: 'open_window' }, label: 'Apri MicFX', help: 'Porta in primo piano questa finestra.' },
  ];

  const MODES: { value: PlayMode; label: string }[] = [
    { value: 'overlap', label: 'Sovrapponi' },
    { value: 'restart', label: 'Riavvia' },
    { value: 'toggle', label: 'Play/Stop' },
    { value: 'exclusive', label: 'Esclusivo' },
  ];
  const MODE_HELP: Record<PlayMode, string> = {
    overlap: 'Ogni clic avvia una nuova copia, anche se il suono sta già suonando.',
    restart: 'Se il suono sta già suonando, riparte da capo.',
    toggle: 'Il secondo clic ferma il suono.',
    exclusive: 'Ferma tutti gli altri suoni e parte.',
  };
  const DURATION_STEPS = [60, 300, 600, 1800, 3600];
  /** Durate proposte; un valore scritto a mano nel config.json resta visibile come voce in più. */
  const DURATIONS = $derived.by(() => {
    const cur = app.settings?.library.max_duration_s;
    const steps = cur && !DURATION_STEPS.includes(cur) ? [...DURATION_STEPS, cur].sort((x, y) => x - y) : DURATION_STEPS;
    return steps.map((v) => ({ value: v, label: v % 60 ? `${v} secondi` : `${v / 60} ${v === 60 ? 'minuto' : 'minuti'}` }));
  });

  const cableMissing = $derived(app.status.includes('cable_missing'));
  const listenOn = $derived(app.status.includes('mic_listen_on'));
  /** La voce passa da MicFX: sempre su Mac, su Windows con la riduzione del rumore. */
  const bridged = $derived(isMac || s.mic.noise_suppression);

  let active = $state('audio');
  let scroller: HTMLElement;
  let capturingPtt = $state(false);
  let diag = $state<Diagnostics | null>(null);
  let diagLoading = $state(false);

  /** Valori locali dei cursori: si vedono mentre trascini, il core li riceve al rilascio. */
  let tail = $state<number | null>(null);
  let lufs = $state<number | null>(null);
  let keepalive = $state<number | null>(null);
  /** L'intensità del filtro invece arriva al core mentre trascini: durante la prova la senti cambiare. */
  let strength = $state<number | null>(null);
  const strengthNow = $derived(strength ?? s.mic.noise_strength);
  const STRENGTH_HELP = $derived(
    strengthNow < 35
      ? 'Leggera: voce più naturale, resta un po’ di ambiente.'
      : strengthNow < 70
        ? 'Media: toglie i rumori senza cambiare la voce. Consigliata.'
        : 'Forte: toglie tutto quello che può, la voce può sembrare più asciutta.',
  );

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
    return () => {
      scroller.removeEventListener('scroll', onscroll);
      // Uscendo dalle impostazioni la prova del microfono si ferma.
      void app.setMicTest(false);
    };
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

{#snippet range(
  value: number,
  min: number,
  max: number,
  step: number,
  label: string,
  out: string,
  oninput: (v: number) => void,
  onchange: (v: number) => void,
)}
  <div class="ctl range">
    <input
      type="range"
      {min}
      {max}
      {step}
      {value}
      aria-label={label}
      style:--fill={fill(value, min, max)}
      oninput={(e) => oninput(+e.currentTarget.value)}
      onchange={(e) => onchange(+e.currentTarget.value)}
    />
    <output>{out}</output>
  </div>
{/snippet}

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
                <span class="d">Quello che senti tu.</span>
              </div>
              <Select
                class="ctl wide"
                label="Cuffie"
                value={s.outputs.monitor.device_id ?? ''}
                options={monitorOptions(app.devices)}
                onchange={(v) => app.setOutputDevice('monitor', v || null)}
              />
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Microfono virtuale</span>
                <span class="d">Quello che sentono gli altri: {isMac ? 'BlackHole' : 'CABLE Input di VB-Cable'}.</span>
              </div>
              <Select
                class="ctl wide"
                label="Microfono virtuale"
                value={s.outputs.cable.device_id ?? ''}
                options={cableOptions(app.devices)}
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
          </div>
        </section>

        <!-- ------------------------------------------------------------ Voce -->
        <section id="set-voice" aria-labelledby="h-voice">
          <h2 id="h-voice">Voce</h2>
          <p class="lead">
            Come arriva la tua voce agli altri. {bridged
              ? `MicFX la manda in ${CABLE_NAME} insieme ai suoni, già pulita e allo stesso volume.`
              : 'Ora la manda Windows con "Ascolta questo dispositivo"; con la riduzione del rumore la manda MicFX.'}
          </p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Riduzione del rumore</span>
                <span class="d">Toglie tastiera, ventole e rumori di fondo dalla tua voce. I suoni restano intatti.</span>
              </div>
              <div class="ctl">
                <Switch checked={s.mic.noise_suppression} label="Riduzione del rumore" onchange={(v) => app.setNoiseSuppression(v)} />
              </div>
            </div>
            {#if listenOn}
              <div class="row callout">
                <TriangleAlert size={16} />
                <span class="grow">"Ascolta questo dispositivo" è ancora acceso: la tua voce arriva due volte, una senza filtro.</span>
                <button type="button" class="sm" onclick={() => api.openSoundSettings(1)}>Apri scheda Registrazione</button>
              </div>
            {/if}
            {#if bridged}
              <div class="row">
                <div class="text">
                  <span class="t">Microfono</span>
                  <span class="d">Quello in cui parli.</span>
                </div>
                <Select
                  class="ctl wide"
                  label="Microfono"
                  value={s.mic.device_id ?? ''}
                  options={micOptions(app.mics)}
                  onchange={(v) => app.updateSettings({ mic: { device_id: v || null } })}
                />
              </div>
            {/if}
            {#if s.mic.noise_suppression}
              <div class="row">
                <div class="text">
                  <span class="t">Intensità</span>
                  <span class="d">{STRENGTH_HELP}</span>
                </div>
                <div class="ctl strength">
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="10"
                    value={strengthNow}
                    aria-label="Intensità della riduzione del rumore"
                    aria-valuetext={STRENGTH_HELP.split(':')[0]}
                    style:--fill={fill(strengthNow, 0, 100)}
                    oninput={(e) => {
                      strength = +e.currentTarget.value;
                      void app.updateSettings({ mic: { noise_strength: strength } });
                    }}
                    onchange={() => (strength = null)}
                  />
                  <span class="ends" aria-hidden="true"><span>Leggera</span><span>Forte</span></span>
                </div>
              </div>
            {/if}
            <div class="row">
              <div class="text">
                <span class="t">Prova il microfono</span>
                <span class="d"
                  >{app.micTest
                    ? 'Parla: ti senti come ti sentono gli altri. Muovi l’intensità per confrontare.'
                    : 'Ti senti in cuffia come ti sentono gli altri. Usa le cuffie, non le casse.'}</span
                >
              </div>
              <button type="button" class="ctl test" class:on={app.micTest} aria-pressed={app.micTest} onclick={() => app.setMicTest(!app.micTest)}>
                {#if app.micTest}<Square size={13} /> Smetti{:else}<Ear size={15} /> Ascoltati{/if}
              </button>
            </div>
          </div>
        </section>

        <!-- ------------------------------------------------------------ Discord -->
        <section id="set-discord" aria-labelledby="h-discord">
          <h2 id="h-discord">Discord</h2>
          <p class="lead">Discord ascolta il microfono virtuale: lì arrivano insieme la tua voce e i suoni.</p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Configurazione guidata</span>
                <span class="d">I passaggi da fare una volta sola, con le verifiche automatiche.</span>
              </div>
              <button type="button" class="ctl" onclick={() => (app.guideOpen = true)}><BookOpen size={15} /> Apri la guida</button>
            </div>
          </div>

          <h3 class="sub">Push-to-talk</h3>
          <p class="lead">Se in Discord usi il push-to-talk, MicFX tiene premuto il tuo tasto mentre un suono va agli altri. Con "Attività vocale" non serve.</p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Tasto di Discord</span>
                <span class="d">Lo stesso di Discord → Tasti di scelta rapida.</span>
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
                    <kbd><Keys acc={s.ptt.key} /></kbd>
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
                <span class="t">Premi il tasto durante i suoni</span>
                <span class="d">{s.ptt.key ? 'Lo rilascia subito dopo.' : 'Scegli prima il tasto di Discord.'}</span>
              </div>
              <div class="ctl">
                <Switch checked={s.ptt.enabled} label="Premi il tasto durante i suoni" disabled={!s.ptt.key} onchange={(v) => app.updateSettings({ ptt: { enabled: v } })} />
              </div>
            </div>
            {#if s.ptt.enabled}
              <div class="row">
                <div class="text">
                  <span class="t">Rilascio dopo il suono</span>
                  <span class="d">Margine perché la coda del suono arrivi intera.</span>
                </div>
                {@render range(
                  tail ?? s.ptt.tail_ms,
                  0,
                  1000,
                  50,
                  'Rilascio dopo il suono',
                  `${tail ?? s.ptt.tail_ms} ms`,
                  (v) => (tail = v),
                  async (v) => {
                    await app.updateSettings({ ptt: { tail_ms: v } });
                    tail = null;
                  },
                )}
              </div>
            {/if}
            <p class="foot">
              {isMac
                ? 'Serve il permesso Accessibilità: Impostazioni di Sistema → Privacy e sicurezza → Accessibilità → MicFX.'
                : 'Discord non deve essere avviato come amministratore, altrimenti Windows blocca il tasto simulato.'}
            </p>
          </div>
        </section>

        <!-- ------------------------------------------------------------ Scorciatoie -->
        <section id="set-hotkeys" aria-labelledby="h-hotkeys">
          <h2 id="h-hotkeys">Scorciatoie</h2>
          <p class="lead">
            Funzionano ovunque, anche con MicFX {TRAY} e nei giochi. Quelle dei singoli suoni si impostano dal suono, con
            <kbd><Keys text="Ctrl" /></kbd> <kbd><Keys text="Invio" /></kbd>.
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
                <span class="t">Tasti della finestra</span>
                <span class="d">Frecce, Invio, F2, Canc e gli altri dentro MicFX.</span>
              </div>
              <button type="button" class="ctl" onclick={() => (app.shortcutsOpen = true)}><Keyboard size={15} /> Mostra elenco</button>
            </div>
          </div>
        </section>

        <!-- ------------------------------------------------------------ Suoni -->
        <section id="set-sounds" aria-labelledby="h-sounds">
          <h2 id="h-sounds">Suoni</h2>
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

          <h3 class="sub">Nuovi suoni</h3>
          <p class="lead">Come vengono preparati i file che aggiungi. Valgono per i prossimi import.</p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">Volume di riferimento</span>
                <span class="d">Tutti i suoni vengono livellati qui. Più alto = più forti.</span>
              </div>
              {@render range(
                lufs ?? s.loudness.target_lufs,
                -30,
                -10,
                1,
                'Volume di riferimento in LUFS',
                `${lufs ?? s.loudness.target_lufs} LUFS`,
                (v) => (lufs = v),
                async (v) => {
                  await app.updateSettings({ loudness: { target_lufs: v } });
                  lufs = null;
                },
              )}
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Taglia il silenzio</span>
                <span class="d">Il suono parte subito, senza silenzio all'inizio e alla fine.</span>
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

        <!-- ------------------------------------------------------------ Generale -->
        <section id="set-general" aria-labelledby="h-general">
          <h2 id="h-general">Generale</h2>
          <p class="lead">Chiudendo la finestra MicFX resta {TRAY} e le scorciatoie continuano a funzionare.</p>
          <div class="card">
            <div class="row">
              <div class="text">
                <span class="t">{isMac ? "Avvia all'accesso" : 'Avvia con Windows'}</span>
                <span class="d">Parte nascosto {TRAY}, pronto con le scorciatoie.</span>
              </div>
              <div class="ctl">
                <Switch checked={s.startup.autostart} label={isMac ? "Avvia all'accesso" : 'Avvia con Windows'} onchange={(v) => app.updateSettings({ startup: { autostart: v } })} />
              </div>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Avvisa delle nuove versioni</span>
                <span class="d">All'apertura della finestra, al massimo ogni 6 ore.</span>
              </div>
              <div class="ctl">
                <Switch checked={app.prefs.updateNotify} label="Avvisa delle nuove versioni" onchange={(v) => app.setPrefs({ updateNotify: v })} />
              </div>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Finestra pronta dopo la chiusura</span>
                <span class="d">Resta in memoria per riaprirsi all'istante. 0 = libera subito la memoria.</span>
              </div>
              {@render range(
                keepalive ?? s.ui.keepalive_s,
                0,
                300,
                10,
                'Secondi in cui la finestra resta pronta',
                `${keepalive ?? s.ui.keepalive_s} s`,
                (v) => (keepalive = v),
                async (v) => {
                  await app.updateSettings({ ui: { keepalive_s: v } });
                  keepalive = null;
                },
              )}
            </div>
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
              <button type="button" class="link check-update" disabled={updateState === 'checking'} onclick={checkUpdate}>
                <RefreshCw size={12} class={updateState === 'checking' ? 'spin' : ''} />
                {updateLabel}
              </button>
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
            <div class="row">
              <div class="text">
                <span class="t">Libreria e impostazioni</span>
                <span class="d path selectable" title={app.info?.library_dir}>{app.info?.library_dir}</span>
              </div>
              <button type="button" class="ctl" onclick={() => api.openDataFolder()}><FolderOpen size={15} /> Apri cartella</button>
            </div>
            <div class="row">
              <div class="text">
                <span class="t">Diagnostica audio</span>
                <span class="d">Stato delle uscite, interruzioni ed errori. Utile per segnalare un problema.</span>
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
          <p class="note">
            Libreria, scorciatoie e impostazioni restano su questo {isMac ? 'Mac' : 'PC'}, identiche se aggiorni o reinstalli MicFX.
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
  /* Sottogruppo dentro una sezione (Push-to-talk, Nuovi suoni). */
  .sub {
    margin-top: 22px;
    font-size: var(--text-base);
    font-weight: 600;
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
  /* Intensità della riduzione del rumore: niente numeri, gli estremi a parole sotto il cursore. */
  .strength {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 220px;
  }
  .strength input {
    width: 100%;
  }
  .ends {
    display: flex;
    justify-content: space-between;
    color: var(--fg-faint);
    font-size: var(--text-xs);
  }
  /* Prova del microfono: acceso, il pulsante resta evidenziato finché non lo fermi. */
  .test {
    min-width: 116px;
  }
  .test.on,
  .test.on:hover:not(:disabled) {
    border-color: var(--primary);
    background: var(--primary-soft);
    color: var(--primary-text);
    animation: listening 1.6s var(--ease) infinite;
  }
  @keyframes listening {
    50% {
      box-shadow: 0 0 0 4px var(--primary-soft-2);
    }
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
  .links .text {
    max-width: 100%;
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
  .check-update {
    gap: 4px;
    margin-top: 4px;
    color: var(--fg-muted);
    font-size: var(--text-xs);
    font-weight: 500;
  }
  .check-update:hover:not(:disabled) {
    color: var(--fg);
  }
  .check-update :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
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
