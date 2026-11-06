<script lang="ts">
  import type { BusKind } from '../api';
  import { app } from '../stores/app.svelte';
  import { formatDb, percentToDb } from '../util/volume';
  import Switch from './Switch.svelte';

  const DEFAULTS: Record<BusKind, number> = { monitor: 79, cable: 100 };

  type Row = {
    bus: BusKind;
    icon: string;
    label: string;
    hint: string;
  };

  const rows: Row[] = [
    { bus: 'monitor', icon: '🎧', label: 'Il mio volume', hint: 'Quello che senti tu in cuffia' },
    { bus: 'cable', icon: '🎙️', label: 'Volume microfono', hint: 'Quello che sentono gli altri su Discord' },
  ];

  const cableMissing = $derived(app.status.includes('cable_missing'));

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
    app.setVolume(bus, Math.max(0, Math.min(100, percent(bus) + (e.deltaY < 0 ? step : -step))));
  }
</script>

<section class="outputs" aria-label="Volumi">
  {#each rows as r (r.bus)}
    {@const enabled = app.settings?.outputs[r.bus].enabled ?? false}
    {@const unavailable = r.bus === 'cable' && cableMissing}
    {@const p = percent(r.bus)}
    <div class="row" class:off={!enabled || unavailable}>
      <span class="icon" aria-hidden="true">{r.icon}</span>
      <div class="labels">
        <span class="label">{r.label}</span>
        <button
          type="button"
          class="device link"
          title="{r.hint}. Clicca per scegliere il dispositivo."
          onclick={() => (unavailable ? (app.guideOpen = true) : (app.settingsOpen = true))}
        >
          {unavailable ? 'VB-Cable non trovato · Configura' : deviceName(r.bus)}
        </button>
      </div>
      <Switch
        checked={enabled && !unavailable}
        disabled={unavailable}
        label={enabled ? `Disattiva: ${r.hint.toLowerCase()}` : `Attiva: ${r.hint.toLowerCase()}`}
        onchange={(v) => app.setOutputEnabled(r.bus, v)}
      />
      <input
        type="range"
        min="0"
        max="100"
        step="1"
        value={p}
        disabled={unavailable}
        aria-label={r.label}
        aria-valuetext="{p}% ({formatDb(percentToDb(p))})"
        title="{formatDb(percentToDb(p))} · rotellina per regolare, doppio clic per {DEFAULTS[r.bus]}%"
        style:--fill="{p}%"
        oninput={(e) => app.setVolume(r.bus, +e.currentTarget.value)}
        onwheel={(e) => wheel(r.bus, e)}
        ondblclick={() => app.setVolume(r.bus, DEFAULTS[r.bus])}
      />
      <output class="value">{p}%</output>
    </div>
  {/each}
  {#if app.settings && !app.settings.outputs.cable.enabled && !cableMissing}
    <p class="note">🔇 Microfono disattivato: i tuoi amici non sentono i suoni.</p>
  {/if}
</section>

<style>
  .outputs {
    display: grid;
    gap: 6px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .row {
    display: grid;
    grid-template-columns: 24px minmax(120px, 220px) auto 1fr 44px;
    align-items: center;
    gap: 10px;
  }
  .row.off .labels,
  .row.off input,
  .row.off .value {
    opacity: 0.45;
  }
  .icon {
    font-size: 18px;
    text-align: center;
  }
  .labels {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    font-weight: 600;
  }
  .device {
    font-size: 12px;
    color: var(--fg-muted);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0;
  }
  input[type='range'] {
    width: 100%;
  }
  .value {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--warn);
  }
</style>
