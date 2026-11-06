# Decisioni

Scelte prese durante lo sviluppo, rispetto a [SPEC.md](SPEC.md). Le più recenti in fondo.

## Nome e percorsi

- Nome definitivo **MicFX** (decisione aperta §17). Crate con prefisso `mfx-`, eseguibile `micfx.exe`,
  identificativo `com.micfx.desktop`.
- Dati in `%APPDATA%\MicFX` e `%LOCALAPPDATA%\MicFX`, **non** nella cartella col nome del bundle: l'uninstaller
  NSIS di Tauri, con "elimina dati" spuntato, cancella solo quella. Requisito: i dati restano identici dopo
  aggiornamenti e reinstallazioni.
- Se `index.json` e il suo `.bak` sono illeggibili ma esistono file `.sbpcm`, l'engine ricostruisce l'indice
  ("Suono recuperato N") invece di perdere la libreria. Gli orfani si cancellano solo con un indice valido.

## Audio

- WASAPI direttamente con `windows` 0.62 invece del crate `wasapi`: un solo binding Windows in tutto il
  workspace, controllo esplicito di `Initialize`/`Start`/`Stop`/`Reset` e dell'invalidazione.
- Buffer richiesto 20 ms (periodo di default del dispositivo, ~10 ms). Prefill dell'intero buffer prima di `Start()`.
- Ogni bus ha 32 voci + 8 slot riservati alle voci rubate mentre sfumano (8 ms), così il furto non taglia.
- Cambio di dispositivo con `BusCmd::SetDevice(DeviceId)`: l'ID ha dimensione fissa (niente allocazioni verso il
  thread audio) e le voci in corso continuano sul nuovo endpoint. Serve anche per "segui il predefinito".
- Un solo evento Win32 sveglia l'engine (comandi, eventi dei bus, notifiche, hotkey): equivalente ai due della
  spec, più semplice.
- Uscita **Microfono** con interruttore `enabled` (non previsto dalla spec per `cable`): richiesta esplicita.
  Spegnere un'uscita ferma subito le voci su quel bus; l'anteprima suona in cuffia anche col monitor spento.
- Volume degli slider: 0–100 % con curva cubica (`dB = 60·log10(p/100)`), 0 % = −60 dB = silenzio, 100 % = 0 dB.

## Hotkey e PTT

- `RegisterHotKey` con `hwnd = NULL` (coda messaggi del thread) invece di una finestra message-only: stesso
  comportamento, nessuna finestra.
- Nomi dei tasti = posizione fisica (come `KeyboardEvent.code`). Per lettere/cifre/punteggiatura il VK si ricava
  dallo scancode col layout attivo: sulla tastiera italiana il tasto "è" resta lo stesso tasto.
- Una combinazione appartiene a una sola azione: assegnarla altrove la sposta (la UI chiede conferma prima).
- Durante la cattura di una scorciatoia la UI mette in pausa le hotkey globali, altrimenti Windows le intercetta.
- Di default nessuna scorciatoia, né per i suoni né per le azioni globali.

## Libreria e UX

- `sound.mode` è opzionale: `null` = segue la modalità predefinita delle impostazioni (default `overlap`).
- Eliminazione con **Annulla**: i suoni restano 20 s in un cestino del core (file e scorciatoie compresi).
- Duplicati riconosciuti dall'hash xxh3 del sorgente: non vengono reimportati, la UI offre "Mostra".
- Trascinando cartelle si importano ricorsivamente i file audio (massimo 500 per volta).
- Emoji del suono in `sound.ui.emoji`; preferenze di vista in `settings.ui.prefs` (vista, ordinamento, dimensione,
  cartella, emoji recenti, tono della pelle). Ordine manuale in `sound.ui.order` (un solo update per spostamento).
- Ogni suono importato riceve subito un'emoji suggerita dal nome (italiano + inglese), o una di riserva stabile.

## Emoji

- Dati da `emojibase-data` 18 (nomi e tag italiani e inglesi + shortcode GitHub), generati in
  `ui/src/lib/emoji/data.json` e caricati solo quando servono.
- Windows non disegna le bandiere: si include il font **Twemoji Country Flags** (78 KB, solo indicatori regionali).
- All'apertura si verifica su canvas cosa sa disegnare il font di sistema: sul PC di sviluppo (Windows 11) sono
  visibili 1913 emoji su 1923; le 10 nascoste sono le più recenti (Emoji 16–18), che il font non ha ancora.
  Il selettore lo dice in fondo alla lista.

## Microfono (modo Sistema)

- Stato aggiuntivo `mic_not_routed`: l'engine legge la proprietà "Ascolta questo dispositivo"
  (`{24dbb0fc-9311-4b3d-9cf0-18ff155639d4}`, pid 0/1) dei microfoni e verifica che punti a CABLE Input. La guida
  di configurazione si spunta da sola mentre l'utente cambia le impostazioni di Windows.
- "Volume microfono" nella UI è il volume dei **suoni** mandati nel microfono virtuale (quello che sentono gli
  altri), non il volume della voce: in modo Sistema l'app non tocca il microfono.

## Ancora da misurare (Fase 0 / Fase 5)

- Processi `msedgewebview2.exe` dopo la distruzione della finestra, working set a riposo, context switch:
  script in `tools/bench/measure.ps1`, misura da fare sulla build release.
- Latenza hotkey → primo campione con trace QPC: non ancora strumentata.
- Relay PTT con Discord reale e bridge del microfono (v1.1) non verificati.
