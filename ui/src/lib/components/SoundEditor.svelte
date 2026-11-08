<script lang="ts">
  import { kb } from '../util/platform';
  import type { PlayMode } from '../api';
  import { Headphones, Play, Square, Trash } from '../icons';
  import { app, soundEmoji } from '../stores/app.svelte';
  import { formatDuration } from '../util/volume';
  import HotkeyInput from './HotkeyInput.svelte';
  import Modal from './Modal.svelte';
  import Select from './Select.svelte';
  import Switch from './Switch.svelte';

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
      <button type="button" class="emoji-btn emoji-font" aria-label="Cambia emoji" onclick={openEmoji}>
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
      <button type="button" class="primary" onclick={() => app.play(s.id)}><Play size={15} fill="currentColor" /> Riproduci</button>
      <button type="button" onclick={() => app.preview(s.id)}><Headphones size={15} /> Ascolta solo tu</button>
      {#if app.playingBySound.has(s.id)}
        <button type="button" class="danger" onclick={() => app.stop(s.id)}><Square size={13} fill="currentColor" /> Ferma</button>
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
          style:--fill="{((gain + 30) / 42) * 100}%"
          oninput={(e) => setGain(+e.currentTarget.value)}
          ondblclick={() => setGain(0)}
        />
        <output>{gain > 0 ? '+' : ''}{gain.toFixed(1)} dB</output>
        <button type="button" class="link" disabled={gain === 0} onclick={() => setGain(0)}>Ripristina</button>
      </div>
    </fieldset>

    <fieldset class="toggle">
      <div class="text">
        <span class="t">Disattiva la mia voce durante la riproduzione</span>
        <p class="help">Finché suona, gli altri sentono solo l'effetto, non il tuo microfono.</p>
      </div>
      <Switch checked={s.mute_voice} label="Disattiva la mia voce durante la riproduzione" onchange={(v) => app.setMuteVoice(s.id, v)} />
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
      <Select
        class="full"
        label="Cartella"
        value={s.folder_id ?? ''}
        options={[{ value: '', label: 'Nessuna' }, ...app.folders.map((f) => ({ value: f.id, label: f.name }))]}
        onchange={(v) => app.moveToFolder([s.id], v || null)}
      />
    </fieldset>

    <dl class="info">
      <dt>Durata</dt>
      <dd>{formatDuration((s.frames / 48000) * 1000)} · {s.channels === 2 ? 'stereo' : 'mono'}</dd>
      <dt>Livellamento</dt>
      <dd>{s.loudness_gain_db > 0 ? '+' : ''}{s.loudness_gain_db.toFixed(1)} dB automatici</dd>
      {#if s.source.path}
        <dt>File originale</dt>
        <dd class="path selectable">{s.source.path}</dd>
      {/if}
      <dt>Aggiunto</dt>
      <dd>{new Date(s.created_at).toLocaleString('it-IT', { dateStyle: 'medium', timeStyle: 'short' })}</dd>
      <dt>Cartella</dt>
      <dd>{folderName(s.folder_id)}</dd>
    </dl>

    {#snippet footer()}
      <button type="button" class="danger ghost" onclick={() => (onclose(), app.deleteSounds([s.id]))}><Trash size={15} /> Elimina</button>
      <span style="flex:1"></span>
      <button type="button" class="primary" onclick={onclose}>Fatto</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .head {
    display: flex;
    gap: 14px;
    align-items: flex-end;
    margin-bottom: 14px;
  }
  .emoji-btn {
    width: 68px;
    height: 68px;
    padding: 0;
    border-radius: var(--r-xl);
    background: var(--surface-2);
    font-size: 40px;
    flex: none;
  }
  .emoji-btn:hover:not(:disabled) {
    background: var(--surface-3);
  }
  .name {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .name span {
    color: var(--fg-muted);
    font-size: var(--text-sm);
    font-weight: 550;
  }
  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 6px;
  }
  fieldset {
    margin: 0;
    padding: 16px 0 0;
    border: none;
    border-top: 1px solid var(--border);
  }
  fieldset + fieldset,
  .row + fieldset {
    margin-top: 16px;
  }
  legend {
    float: left;
    width: 100%;
    margin-bottom: 4px;
    padding: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }
  .help {
    clear: both;
    margin: 0 0 10px;
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  legend + :not(.help) {
    clear: both;
  }
  .toggle {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }
  .toggle .text {
    flex: 1;
    min-width: 0;
  }
  .toggle .t {
    display: block;
    margin-bottom: 4px;
    font-size: var(--text-md);
    font-weight: 600;
  }
  .toggle .help {
    margin: 0;
  }
  .toggle :global(.switch) {
    margin-top: 1px;
  }
  .gain {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .gain input {
    flex: 1;
  }
  .gain output {
    width: 64px;
    color: var(--fg-2);
    font-weight: 550;
    text-align: right;
  }
  .radio {
    display: grid;
    grid-template-columns: 16px 1fr;
    gap: 10px;
    align-items: start;
    padding: 8px 10px;
    margin: 0 -10px;
    border-radius: var(--r-md);
    cursor: default;
  }
  .radio:hover {
    background: var(--surface-2);
  }
  .radio input {
    margin-top: 2px;
  }
  .radio span {
    display: flex;
    flex-direction: column;
    font-weight: 550;
  }
  .radio small {
    color: var(--fg-muted);
    font-size: var(--text-sm);
    font-weight: 400;
  }
  fieldset :global(.select.full) {
    width: 100%;
  }
  .info {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 16px;
    margin: 20px 0 0;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    background: var(--bg);
    color: var(--fg-muted);
    font-size: var(--text-sm);
  }
  .info dt {
    color: var(--fg-faint);
  }
  .info dd {
    margin: 0;
    min-width: 0;
    color: var(--fg-2);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
