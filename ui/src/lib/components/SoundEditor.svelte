<script lang="ts">
  import type { PlayMode } from '../api';
  import { app, soundEmoji } from '../stores/app.svelte';
  import { formatDuration } from '../util/volume';
  import HotkeyInput from './HotkeyInput.svelte';
  import Modal from './Modal.svelte';

  let { id, onclose }: { id: string; onclose: () => void } = $props();

  const s = $derived(app.sound(id));
  let name = $state('');
  let gainTimer: ReturnType<typeof setTimeout> | null = null;
  let gain = $state(0);

  $effect(() => {
    // Riallinea quando cambia il suono mostrato (non mentre si scrive).
    id;
    const cur = app.sound(id);
    name = cur?.name ?? '';
    gain = cur?.user_gain_db ?? 0;
  });

  $effect(() => {
    if (!s) onclose();
  });

  const MODES: { value: PlayMode; label: string; help: string }[] = [
    { value: 'overlap', label: 'Sovrapponi', help: 'Ogni clic avvia una nuova copia, anche se sta già suonando.' },
    { value: 'restart', label: 'Riavvia', help: 'Se sta suonando riparte da capo.' },
    { value: 'toggle', label: 'Play/Stop', help: 'Il secondo clic lo ferma.' },
    { value: 'exclusive', label: 'Esclusivo', help: 'Ferma tutti gli altri suoni e parte.' },
  ];

  const defaultMode = $derived(app.settings?.playback.default_mode ?? 'overlap');

  function commitName() {
    if (s && name.trim() && name.trim() !== s.name) app.rename(s.id, name);
    else if (s) name = s.name;
  }

  function setGain(v: number) {
    gain = v;
    if (gainTimer) clearTimeout(gainTimer);
    gainTimer = setTimeout(() => s && app.setGain(s.id, v), 120);
  }

  function openEmoji(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    app.emojiTarget = { soundId: id, x: r.left, y: r.bottom + 4 };
  }

  const folderName = (fid: string | null) => app.folders.find((f) => f.id === fid)?.name ?? 'Nessuna';
</script>

{#if s}
  <Modal title="Modifica suono" {onclose} side width={420}>
    <div class="head">
      <button type="button" class="emoji-btn emoji-font" title="Cambia emoji (Ctrl+E)" aria-label="Cambia emoji" onclick={openEmoji}>
        {soundEmoji(s)}
      </button>
      <label class="name">
        <span>Nome</span>
        <input
          bind:value={name}
          maxlength="80"
          onblur={commitName}
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              commitName();
              (e.currentTarget as HTMLInputElement).blur();
            }
          }}
        />
      </label>
    </div>

    <div class="row">
      <button type="button" onclick={() => app.play(s.id)}>▶ Riproduci</button>
      <button type="button" onclick={() => app.preview(s.id)} title="Solo in cuffia, Discord non lo sente">🎧 Ascolta solo tu</button>
      {#if app.playingBySound.has(s.id)}
        <button type="button" onclick={() => app.stop(s.id)}>■ Ferma</button>
      {/if}
    </div>

    <fieldset>
      <legend>Scorciatoia da tastiera</legend>
      <p class="help">Facoltativa. Funziona ovunque, anche nei giochi a schermo intero.</p>
      <HotkeyInput action={{ type: 'play', sound_id: id }} label="Scorciatoia per riprodurre" />
    </fieldset>

    <fieldset>
      <legend>Volume di questo suono</legend>
      <p class="help">I suoni sono già livellati all'import. Qui correggi solo quelli che restano troppo alti o bassi.</p>
      <div class="gain">
        <input
          type="range"
          min="-30"
          max="12"
          step="0.5"
          value={gain}
          aria-label="Guadagno del suono"
          oninput={(e) => setGain(+e.currentTarget.value)}
          ondblclick={() => setGain(0)}
        />
        <output>{gain > 0 ? '+' : ''}{gain.toFixed(1)} dB</output>
        <button type="button" class="link" disabled={gain === 0} onclick={() => setGain(0)}>Ripristina</button>
      </div>
    </fieldset>

    <fieldset>
      <legend>Modalità di riproduzione</legend>
      <label class="radio">
        <input type="radio" name="mode" checked={s.mode === null} onchange={() => app.setMode(s.id, null)} />
        <span>Predefinita <small>({MODES.find((m) => m.value === defaultMode)?.label}, dalle impostazioni)</small></span>
      </label>
      {#each MODES as m}
        <label class="radio">
          <input type="radio" name="mode" checked={s.mode === m.value} onchange={() => app.setMode(s.id, m.value)} />
          <span>{m.label} <small>{m.help}</small></span>
        </label>
      {/each}
    </fieldset>

    <fieldset>
      <legend>Cartella</legend>
      <select value={s.folder_id ?? ''} onchange={(e) => app.moveToFolder([s.id], e.currentTarget.value || null)}>
        <option value="">Nessuna</option>
        {#each app.folders as f (f.id)}
          <option value={f.id}>{f.name}</option>
        {/each}
      </select>
    </fieldset>

    <dl class="info">
      <dt>Durata</dt>
      <dd>{formatDuration((s.frames / 48000) * 1000)} · {s.channels === 2 ? 'stereo' : 'mono'}</dd>
      <dt>Livellamento</dt>
      <dd>{s.loudness_gain_db > 0 ? '+' : ''}{s.loudness_gain_db.toFixed(1)} dB automatici</dd>
      {#if s.source.path}
        <dt>File originale</dt>
        <dd class="path" title={s.source.path}>{s.source.path}</dd>
      {/if}
      <dt>Aggiunto</dt>
      <dd>{new Date(s.created_at).toLocaleString('it-IT', { dateStyle: 'medium', timeStyle: 'short' })}</dd>
      <dt>Cartella</dt>
      <dd>{folderName(s.folder_id)}</dd>
    </dl>

    {#snippet footer()}
      <button type="button" class="danger" onclick={() => (onclose(), app.deleteSounds([s.id]))}>Elimina</button>
      <span style="flex:1"></span>
      <button type="button" onclick={onclose}>Fatto</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .head {
    display: flex;
    gap: 12px;
    align-items: flex-end;
    margin-bottom: 12px;
  }
  .emoji-btn {
    font-size: 40px;
    width: 64px;
    height: 64px;
    padding: 0;
  }
  .name {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }
  fieldset {
    border: 1px solid var(--border);
    border-radius: 6px;
    margin: 10px 0;
    padding: 8px 10px 10px;
  }
  legend {
    font-weight: 600;
    padding: 0 4px;
  }
  .help {
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .gain {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .gain input {
    flex: 1;
  }
  .gain output {
    width: 64px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .radio {
    display: flex;
    gap: 6px;
    align-items: baseline;
    padding: 3px 0;
  }
  .radio small {
    color: var(--fg-muted);
  }
  .info {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .info dd {
    margin: 0;
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .danger {
    color: var(--danger);
  }
</style>
