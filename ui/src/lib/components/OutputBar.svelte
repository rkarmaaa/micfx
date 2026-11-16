<script lang="ts">
  // Volumi in basso: cuffie (quello che senti tu) e microfono virtuale (quello che sentono gli altri).
  // Il nome del dispositivo apre l'elenco dei dispositivi; sotto quello del microfono, la riduzione del rumore.
  import { CABLE_NAME } from '../util/platform';
  import type { BusKind } from '../api';
  import { Ear, Headphones, Mic, MicOff, TriangleAlert } from '../icons';
  import { app } from '../stores/app.svelte';
  import { cableOptions, monitorOptions } from '../util/devices';
  import { MAX_PERCENT, formatDb, percentToDb } from '../util/volume';
  import Select from './Select.svelte';
  import Switch from './Switch.svelte';

  const DEFAULTS: Record<BusKind, number> = { monitor: 80, cable: 100 };
  /** Gli slider si muovono a passi di 5 %; una tacca per passo (sopra e sotto), più lunghe ogni 25 %. */
  const STEP = 5;
  const TICKS = Array.from({ length: MAX_PERCENT / STEP + 1 }, (_, i) => i * STEP);
  const snap = (p: number) => Math.round(p / STEP) * STEP;

  const rows: { bus: BusKind; label: string; hint: string; device: string }[] = [
    { bus: 'monitor', label: 'Il mio volume', hint: 'Quello che senti tu in cuffia', device: 'Cuffie' },
    { bus: 'cable', label: 'Volume microfono', hint: 'Quello che sentono gli altri su Discord', device: 'Microfono virtuale' },
  ];

  const cableMissing = $derived(app.status.includes('cable_missing'));
  let dock: HTMLElement;

  // Le notifiche si appoggiano sopra la barra, qualunque sia la sua altezza.
  $effect(() => {
    const ro = new ResizeObserver(() => document.documentElement.style.setProperty('--dock-h', `${dock.offsetHeight}px`));
    ro.observe(dock);
    return () => {
      ro.disconnect();
      document.documentElement.style.removeProperty('--dock-h');
    };
  });

  function percent(bus: BusKind) {
    return snap(bus === 'monitor' ? app.monitorPercent : app.cablePercent);
  }

  function deviceName(bus: BusKind): string {
    const s = app.settings;
    if (!s) return '';
    if (bus === 'cable') {
      const d = app.devices.find((x) => x.id === s.outputs.cable.device_id) ?? app.devices.find((x) => x.is_cable);
      return d ? d.name : `${CABLE_NAME} non trovato`;
    }
    const d = s.outputs.monitor.device_id ? app.devices.find((x) => x.id === s.outputs.monitor.device_id) : null;
    const def = app.devices.find((x) => x.is_default);
    return d ? d.name : `Predefinito${def ? ` · ${def.name}` : ''}`;
  }

  function wheel(bus: BusKind, e: WheelEvent) {
    e.preventDefault();
    app.setVolume(bus, Math.max(0, Math.min(MAX_PERCENT, percent(bus) + (e.deltaY < 0 ? STEP : -STEP))));
  }
</script>

{#snippet noise()}
  <label class="noise">
    <input
      type="checkbox"
      checked={app.settings?.mic.noise_suppression ?? false}
      onchange={(e) => app.setNoiseSuppression(e.currentTarget.checked)}
    />
    <span class="noise-text">
      <span class="noise-title">Riduzione del rumore</span>
      <span class="noise-hint">
        {#if app.status.includes('mic_listen_on')}
          <TriangleAlert size={12} /> Spegni "Ascolta questo dispositivo"
        {:else}
          Filtra i rumori della tua voce, non i suoni
        {/if}
      </span>
    </span>
  </label>
  <button type="button" class="noise-more" onclick={() => app.openSettings('voice')}>
    <Ear size={14} /> Intensità e prova del microfono
  </button>
{/snippet}

<section class="dock" aria-label="Volumi" bind:this={dock}>
  {#if app.settings && !app.settings.outputs.cable.enabled && !cableMissing}
    <p class="note" role="status"><MicOff size={13} /> Microfono disattivato: i tuoi amici non sentono i suoni.</p>
  {/if}
  <div class="rows">
    {#each rows as r, i (r.bus)}
      {#if i > 0}<span class="sep" aria-hidden="true"></span>{/if}
      {@const enabled = app.settings?.outputs[r.bus].enabled ?? false}
      {@const unavailable = r.bus === 'cable' && cableMissing}
      {@const on = enabled && !unavailable}
      {@const p = percent(r.bus)}
      <div class="row" class:off={!on}>
        <span class="badge" aria-hidden="true">
          {#if r.bus === 'monitor'}<Headphones size={17} />{:else if on}<Mic size={17} />{:else}<MicOff size={17} />{/if}
        </span>
        <div class="labels">
          <span class="label">{r.label}</span>
          {#if unavailable}
            <button type="button" class="device" onclick={() => (app.guideOpen = true)}>
              <TriangleAlert size={12} />
              <span>{CABLE_NAME} non trovato · Configura</span>
            </button>
          {:else}
            <Select
              variant="link"
              label={r.device}
              minWidth={280}
              value={app.settings?.outputs[r.bus].device_id ?? ''}
              text={deviceName(r.bus)}
              options={r.bus === 'monitor' ? monitorOptions(app.devices) : cableOptions(app.devices)}
              onchange={(v) => app.setOutputDevice(r.bus, v || null)}
              footer={r.bus === 'cable' ? noise : undefined}
            />
          {/if}
        </div>
        <Switch
          checked={on}
          disabled={unavailable}
          label={enabled ? `Disattiva: ${r.hint.toLowerCase()}` : `Attiva: ${r.hint.toLowerCase()}`}
          onchange={(v) => app.setOutputEnabled(r.bus, v)}
        />
        <div class="slider">
          <input
            type="range"
            min="0"
            max={MAX_PERCENT}
            step={STEP}
            value={p}
            disabled={unavailable}
            aria-label={r.label}
            aria-valuetext="{p}% ({formatDb(percentToDb(p))})"
            style:--fill="{(p / MAX_PERCENT) * 100}%"
            oninput={(e) => app.setVolume(r.bus, +e.currentTarget.value)}
            onwheel={(e) => wheel(r.bus, e)}
            ondblclick={() => app.setVolume(r.bus, DEFAULTS[r.bus])}
          />
          <span class="ticks" aria-hidden="true">
            {#each TICKS as t (t)}<i class:major={t % 25 === 0}></i>{/each}
          </span>
        </div>
        <output class="value" class:boost={p > 100}>{p}%</output>
      </div>
    {/each}
  </div>
</section>

<style>
  .dock {
    container: dock / inline-size;
    position: relative;
    flex: none;
    /* Lati come la barra superiore. */
    padding: 10px 10px 12px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .rows {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 1px minmax(0, 1fr);
    align-items: center;
    gap: 8px 24px;
  }
  .sep {
    align-self: stretch;
    background: var(--border);
  }
  @container dock (max-width: 820px) {
    .rows {
      grid-template-columns: minmax(0, 1fr);
    }
    .sep {
      height: 1px;
    }
  }
  .row {
    display: grid;
    grid-template-columns: 34px minmax(96px, 150px) auto minmax(80px, 1fr) 4.5ch;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: var(--r-lg);
    background: var(--primary-soft);
    color: var(--primary-text);
    transition:
      background var(--t-fast) var(--ease),
      color var(--t-fast) var(--ease);
  }
  .off .badge {
    background: var(--surface-2);
    color: var(--fg-faint);
  }
  .labels {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .label {
    font-size: var(--text-md);
    font-weight: 600;
  }
  /* Cavo mancante: al posto della select, un collegamento alla guida. */
  .device {
    justify-content: flex-start;
    gap: 3px;
    height: auto;
    max-width: 100%;
    padding: 0;
    border: none;
    background: none;
    box-shadow: none;
    color: var(--warn);
    font-size: var(--text-sm);
    font-weight: 400;
  }
  .device span {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .device:hover:not(:disabled) {
    background: none;
    color: var(--fg);
  }
  /* Riga della riduzione del rumore sotto l'elenco del microfono (lo snippet tiene lo scope anche nel portal). */
  .noise {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 4px 10px;
    border-radius: var(--r-sm);
    cursor: pointer;
  }
  .noise:hover {
    background: var(--surface-2);
  }
  .noise-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .noise-title {
    color: var(--fg);
    font-size: var(--text-md);
    font-weight: 550;
  }
  .noise-hint {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .noise-more {
    justify-content: flex-start;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: none;
    background: none;
    box-shadow: none;
    color: var(--fg-muted);
    font-size: var(--text-sm);
    font-weight: 450;
  }
  .noise-more:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--fg);
  }
  .noise-hint :global(.lucide) {
    flex: none;
    color: var(--warn);
  }
  .off input[type='range'],
  .off .value,
  .off .label {
    opacity: 0.5;
  }
  /* Misure del cursore dei volumi: pomello a pillola alto e stretto, tacche specchiate. */
  .slider {
    --thumb-w: 8px;
    --thumb-h: 20px;
    --track-h: 4px;
    /* Distanza delle tacche dalla traccia e loro lunghezza (minori / ogni 25 %). */
    --tick-gap: 3px;
    --tick-len: 3px;
    --tick-len-major: 5px;
    position: relative;
    min-width: 0;
  }
  /* Il cursore sta sopra le tacche: il pomello le copre quando ci passa. */
  input[type='range'] {
    position: relative;
    z-index: 1;
    display: block;
    width: 100%;
    height: var(--thumb-h);
  }
  input[type='range']::-webkit-slider-runnable-track {
    height: var(--track-h);
  }
  input[type='range']::-webkit-slider-thumb {
    width: var(--thumb-w);
    height: var(--thumb-h);
    margin-top: calc((var(--track-h) - var(--thumb-h)) / 2);
    border-radius: 999px;
  }
  /* Tacche sopra e sotto la traccia, allineate alle posizioni del cursore (metà pomello per lato). */
  .ticks {
    position: absolute;
    top: 50%;
    left: calc(var(--thumb-w) / 2);
    right: calc(var(--thumb-w) / 2);
    display: flex;
    justify-content: space-between;
    align-items: center;
    transform: translateY(-50%);
    pointer-events: none;
  }
  .ticks i {
    --len: var(--tick-len);
    width: 1px;
    height: calc(var(--track-h) + 2 * var(--tick-gap) + 2 * var(--len));
    background: linear-gradient(
      to bottom,
      var(--tick) 0 var(--len),
      transparent var(--len) calc(100% - var(--len)),
      var(--tick) calc(100% - var(--len))
    );
  }
  .ticks i.major {
    --len: var(--tick-len-major);
  }
  .off .ticks {
    opacity: 0.5;
  }
  .value {
    /* Attaccato allo slider: allineato a sinistra, metà del gap della riga. */
    margin-left: -6px;
    color: var(--fg-2);
    font-size: var(--text-md);
    font-weight: 550;
    text-align: left;
    font-variant-numeric: tabular-nums;
  }
  .value.boost {
    color: var(--warn);
  }
  /* Sopra la barra, fuori dal flusso: comparire o sparire non sposta niente. */
  .note {
    position: absolute;
    bottom: calc(100% + 10px);
    left: 10px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: calc(100% - 20px);
    padding: 6px 10px;
    border: 1px solid var(--warn-border);
    border-radius: var(--r-md);
    background: var(--surface);
    box-shadow: var(--shadow-md);
    color: var(--warn);
    font-size: var(--text-sm);
    pointer-events: none;
    animation: note-in var(--t-med) var(--ease);
  }
  @keyframes note-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
</style>
