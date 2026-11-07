<script lang="ts">
  // Volumi in basso: cuffie (quello che senti tu) e microfono virtuale (quello che sentono gli altri).
  import type { BusKind } from '../api';
  import { ChevronDown, Headphones, Mic, MicOff, TriangleAlert } from '../icons';
  import { app } from '../stores/app.svelte';
  import { MAX_PERCENT, formatDb, percentToDb } from '../util/volume';
  import Switch from './Switch.svelte';

  const DEFAULTS: Record<BusKind, number> = { monitor: 79, cable: 100 };

  const rows: { bus: BusKind; label: string; hint: string }[] = [
    { bus: 'monitor', label: 'Il mio volume', hint: 'Quello che senti tu in cuffia' },
    { bus: 'cable', label: 'Volume microfono', hint: 'Quello che sentono gli altri su Discord' },
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
    return bus === 'monitor' ? app.monitorPercent : app.cablePercent;
  }

  function deviceName(bus: BusKind): string {
    const s = app.settings;
    if (!s) return '';
    if (bus === 'cable') {
      const d = app.devices.find((x) => x.id === s.outputs.cable.device_id) ?? app.devices.find((x) => x.is_cable);
      return d ? d.name : 'VB-Cable non trovato';
    }
    const d = s.outputs.monitor.device_id ? app.devices.find((x) => x.id === s.outputs.monitor.device_id) : null;
    const def = app.devices.find((x) => x.is_default);
    return d ? d.name : `Predefinito${def ? ` · ${def.name}` : ''}`;
  }

  function wheel(bus: BusKind, e: WheelEvent) {
    e.preventDefault();
    const step = e.shiftKey ? 1 : 5;
    app.setVolume(bus, Math.max(0, Math.min(MAX_PERCENT, percent(bus) + (e.deltaY < 0 ? step : -step))));
  }
</script>

<section class="dock" aria-label="Volumi" bind:this={dock}>
  <div class="rows">
    {#each rows as r (r.bus)}
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
          <button
            type="button"
            class="device"
            class:warn={unavailable}
            title="{r.hint}. Clic per scegliere il dispositivo."
            onclick={() => (unavailable ? (app.guideOpen = true) : app.openSettings('audio'))}
          >
            {#if unavailable}<TriangleAlert size={12} />{/if}
            <span>{unavailable ? 'VB-Cable non trovato · Configura' : deviceName(r.bus)}</span>
            {#if !unavailable}<ChevronDown size={12} />{/if}
          </button>
        </div>
        <Switch
          checked={on}
          disabled={unavailable}
          label={enabled ? `Disattiva: ${r.hint.toLowerCase()}` : `Attiva: ${r.hint.toLowerCase()}`}
          onchange={(v) => app.setOutputEnabled(r.bus, v)}
        />
        <input
          type="range"
          min="0"
          max={MAX_PERCENT}
          step="1"
          value={p}
          disabled={unavailable}
          aria-label={r.label}
          aria-valuetext="{p}% ({formatDb(percentToDb(p))})"
          title="{formatDb(percentToDb(p))} · oltre 100% amplifica · rotellina per regolare, doppio clic per {DEFAULTS[r.bus]}%"
          style:--fill="{(p / MAX_PERCENT) * 100}%"
          oninput={(e) => app.setVolume(r.bus, +e.currentTarget.value)}
          onwheel={(e) => wheel(r.bus, e)}
          ondblclick={() => app.setVolume(r.bus, DEFAULTS[r.bus])}
        />
        <output class="value" class:boost={p > 100}>{p}%</output>
      </div>
    {/each}
  </div>
  {#if app.settings && !app.settings.outputs.cable.enabled && !cableMissing}
    <p class="note"><MicOff size={13} /> Microfono disattivato: i tuoi amici non sentono i suoni.</p>
  {/if}
</section>

<style>
  .dock {
    flex: none;
    padding: 10px 16px 12px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .rows {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(380px, 1fr));
    gap: 8px 28px;
  }
  .row {
    display: grid;
    grid-template-columns: 34px minmax(110px, 190px) auto minmax(80px, 1fr) 44px;
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
  .device {
    justify-content: flex-start;
    gap: 3px;
    height: auto;
    max-width: 100%;
    padding: 0;
    border: none;
    background: none;
    box-shadow: none;
    color: var(--fg-muted);
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
  .device.warn {
    color: var(--warn);
  }
  .off input[type='range'],
  .off .value,
  .off .label {
    opacity: 0.5;
  }
  input[type='range'] {
    width: 100%;
  }
  .value {
    color: var(--fg-2);
    font-size: var(--text-md);
    font-weight: 550;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .value.boost {
    color: var(--warn);
  }
  .note {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    color: var(--warn);
    font-size: var(--text-sm);
  }
</style>
