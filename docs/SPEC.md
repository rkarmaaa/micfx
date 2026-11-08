# Soundboard per Windows — Documento di sviluppo

Nov 6, 2026 · @Marco

## 1. Panoramica

Un'app Windows residente nella system tray che riproduce file audio dentro VB-Cable: li senti tu in cuffia e li sentono i tuoi amici su Discord, insieme alla tua voce. Il requisito che guida ogni scelta è il costo a riposo: **0% CPU, nessun thread che si sveglia, nessun processo WebView, RAM sotto i 20 MB**.

**Dentro lo scope (v1)**

- Libreria di suoni con import, normalizzazione del volume e hotkey globali.
- Uscita simultanea verso CABLE Input (Discord) e verso le cuffie (monitor), con volumi separati.
- Voce del microfono verso Discord tramite la funzione di Windows "Ascolta questo dispositivo", a costo zero per l'app. Bridge interno previsto in v1.1.
- Relay del tasto push-to-talk di Discord, avvio con Windows, istanza singola.

**Fuori scope**

- Design, UI e UX: arrivano dal file Figma collegato a Claude Code. Qui c'è solo il contratto che la UI deve usare (sezione 12).
- Effetti voce, text-to-speech, macOS e Linux, driver audio proprietario, sync cloud.

**Come usarlo con Claude Code**

1. Esporta il documento in Markdown e salvalo nel repo come `docs/SPEC.md`.
2. Crea un `CLAUDE.md` alla radice che rimanda a `docs/SPEC.md` e copia le regole real-time (6.5) e il budget (sezione 3).
3. Procedi per fasi (sezione 16). Una fase si chiude solo con i criteri di accettazione misurati e verdi.
4. Collega il Figma nella Fase 4, quando il core funziona già senza UI.

Nome di lavoro: `soundboard`, crate con prefisso `sb-`. Sostituibile in qualsiasi momento.

## 2. Requisiti funzionali

La v1 consegna 14 requisiti Must e 2 Should. Il bridge interno del microfono (F16) è rinviato alla v1.1. Gli ID servono a Claude Code per riferirsi ai requisiti nei commit e nei test.

| ID | Requisito | Dettaglio | Priorità |
| --- | --- | --- | --- |
| F01 | Riproduzione nel mic virtuale | Ogni suono va su CABLE Input. Discord lo riceve da CABLE Output. | Must |
| F02 | Monitor in cuffia | Stesso suono sul dispositivo di ascolto scelto o sul predefinito, volume indipendente, disattivabile. | Must |
| F03 | Anteprima privata | Riproduce solo in cuffia, senza mandare nulla su Discord. | Should |
| F04 | Import nella libreria | Drag & drop o selettore file: MP3, WAV, FLAC, OGG Vorbis, M4A/AAC. Conversione una tantum nel formato interno (sezione 7). | Must |
| F05 | Normalizzazione volume | Loudness EBU R128 misurata all'import e salvata come gain. Regolazione fine per suono in dB. | Must |
| F06 | Hotkey globali | Una combinazione per suono, più azioni globali: stop tutto, pausa hotkey, mute monitor, apri finestra. Attive anche nei giochi a schermo intero. | Must |
| F07 | Modalità di riproduzione | Per suono: sovrapponi, riavvia, play/stop, esclusivo (ferma gli altri). | Must |
| F08 | Stop senza click | Stop singolo e stop tutto con fade-out di 8 ms. | Must |
| F09 | Limiter d'uscita | Nessun clipping con più suoni insieme. Ceiling −1 dBFS. | Must |
| F10 | Voce verso Discord (modo Sistema) | Il mic arriva su CABLE Input tramite "Ascolta questo dispositivo" di Windows. L'app verifica e guida la configurazione. | Must |
| F11 | Relay push-to-talk | Durante la riproduzione l'app tiene premuto il tasto PTT di Discord e lo rilascia 200 ms dopo la fine. | Should |
| F12 | Residenza in tray | Chiudere la finestra non chiude l'app. Menu tray: apri, stop tutto, pausa hotkey, esci. | Must |
| F13 | Avvio con Windows | Opzionale. Parte direttamente in tray, senza creare la UI. | Must |
| F14 | Istanza singola | Un secondo avvio apre la finestra dell'istanza già attiva. | Must |
| F15 | Gestione dispositivi | Rileva VB-Cable, sample rate, cuffie collegate e scollegate, senza riavviare l'app. | Must |
| F16 | Bridge mic interno | Cattura del mic dentro l'app: guadagno, mute da hotkey, ducking. Attivo solo mentre Discord ascolta. | Could (v1.1) |
| F17 | Persistenza | Impostazioni, libreria e hotkey sopravvivono a riavvii e crash. | Must |

## 3. Performance: principi e budget

A riposo l'app non fa nulla: nessun timer periodico, nessun polling, nessun thread audio, nessuna WebView. La CPU si usa solo in risposta a un evento (hotkey, dispositivo, click) e solo finché serve.

**Principi**

1. **Solo eventi, mai polling.** Hotkey via `RegisterHotKey`, dispositivi via `IMMNotificationClient`, audio via WASAPI event-driven. Nessun `sleep` in loop, nessun timer ricorrente.
2. **Lavoro a tempo.** Gli stream audio partono al primo suono e si fermano 2 s dopo l'ultimo. La UI esiste solo mentre è visibile (sezione 10).
3. **Calcola una volta, all'import.** Decodifica, resampling e loudness avvengono all'import. A runtime restano lettura, gain e somma.
4. **Memoria del sistema, non dell'app.** I suoni sono file PCM mappati in memoria: la cache la gestisce Windows, l'heap resta piccolo.
5. **Thread audio intoccabile.** Zero allocazioni, zero lock, zero I/O (regole in 6.5).
6. **Mai `timeBeginPeriod`.** Alzare la risoluzione del timer di sistema costa energia a tutto il PC.
7. **Misura, non supporre.** Ogni fase si chiude con le misure della tabella.

| Stato | Metrica | Target | Strumento |
| --- | --- | --- | --- |
| Riposo (tray, UI chiusa) | CPU media su 10 min | 0,00% | Process Explorer, WPA "CPU Usage (Precise)" |
| Riposo | Context switch del processo | 0/s a regime, dopo 2 min senza attività | Process Explorer, colonna Context Switch Delta |
| Riposo | Working set | ≤ 20 MB (da confermare in Fase 0) | Task Manager, VMMap |
| Riposo | Processi `msedgewebview2.exe` figli | 0 | Process Explorer, vista ad albero |
| Riposo | Presenza nel report `powercfg /energy` | Assente | Report di 60 s |
| Riproduzione (2 uscite, 4 suoni) | CPU totale | < 0,3% su CPU desktop da 6+ core | WPA |
| Riproduzione | Underrun audio | 0 in 30 min | Contatori interni (6.5) |
| Hotkey → primo campione in cuffia | Latenza p95 | ≤ 40 ms | Trace interno QPC + test in loopback |
| Avvio a freddo → hotkey attive | Tempo | ≤ 200 ms | Trace interno |
| Apertura UI | Tempo al primo frame | ≤ 600 ms a freddo, ≤ 50 ms se solo nascosta | Trace interno |
| Installer | Dimensione | ≤ 10 MB | File di build |

Con la finestra aperta l'app può costare quanto una normale WebView: il budget di riposo vale a UI chiusa.

## 4. Architettura

Un solo processo Rust con tre thread sempre presenti ma addormentati; i thread audio e la UI nascono solo quando servono. VB-Cable è il punto d'incontro: l'app ci scrive i suoni, Windows ci inoltra il microfono, Discord legge il risultato.

&#91;embedded content: routing audio e thread · modo Sistema\]

I riquadri tratteggiati non esistono a riposo: per questo il costo con la UI chiusa e niente in riproduzione è zero.

**Thread**

| Thread | Ruolo | Quando è attivo | Si sveglia per |
| --- | --- | --- | --- |
| Principale (Tauri) | Tray, finestra, IPC | Sempre, fermo in attesa di messaggi | Click sulla tray, comandi della UI |
| Hotkey | `RegisterHotKey` e `GetMessageW` | Sempre, fermo in attesa | `WM_HOTKEY` |
| Engine | Stato, config, libreria, dispositivi, modalità, PTT, timer one-shot. COM MTA | Sempre, fermo su WaitForMultipleObjects | Comandi, notifiche, eventi dai bus |
| Render Cable e Monitor | Mixer e WASAPI (6.3), MMCSS "Pro Audio" | Dal primo play a 2 s dopo l'ultimo suono | Evento del buffer a ogni periodo |
| Import worker | Pipeline della sezione 7 | Solo durante un import | — |
| Cattura mic (v1.1) | Bridge interno (6.6) | Bridge acceso e Discord in chiamata | Evento del buffer del mic |

Le callback COM di Windows (dispositivi, sessioni) arrivano su thread di sistema: inoltrano un messaggio all'engine e ritornano subito.

L'engine attende su due eventi Win32, uno per i comandi e uno per i messaggi dei bus. Il timeout è la prossima scadenza dei suoi timer one-shot (debounce della config, coda del PTT, keepalive della UI), altrimenti infinito. Chi gli scrive accoda il messaggio e chiama `SetEvent`.

**Percorso di un play**

1. `WM_HOTKEY` arriva al thread hotkey, che invia `Action::Play(id)` al canale dell'engine.
2. L'engine risolve la modalità, prende o crea l'`Arc<Mmap>` del suono e manda `Play` a ogni bus (push su `rtrb` + `SetEvent`).
3. Se il bus era fermo: prefill e `Start()`. Se era chiuso: prima `Initialize`.
4. A fine voce il bus pubblica `Ended`; l'engine aggiorna lo stato, rilascia il PTT se serve, avvisa la UI se è aperta e libera gli `Arc` restituiti.

## 5. Stack tecnologico

Rust per tutto ciò che gira sempre, Tauri 2 come guscio, una WebView solo per la UI e solo quando è aperta. Il core audio non dipende da Tauri: si compila e si testa da solo. Versioni verificate su crates.io e npm al 6 novembre 2026.

| Livello | Scelta | Versione | Motivo |
| --- | --- | --- | --- |
| Linguaggio core | Rust stable, edition 2024 | ≥ 1.85 | Nessun GC, nessun runtime, controllo totale su thread e memoria. |
| Guscio app | Tauri | 2.12 | Tray, finestre create e distrutte a richiesta, IPC, bundler NSIS. |
| Audio I/O | `wasapi` | 0.26 | Wrapper diretto di WASAPI: shared mode event-driven, Start e Stop espliciti. |
| API Windows extra | `windows` (windows-rs) | 0.62 | `IMMNotificationClient`, `IAudioSessionManager2`, `RegisterHotKey`, `SendInput`, MMCSS. |
| Decodifica, solo import | `symphonia` | 0.6 | Rust puro: MP3, AAC/M4A, FLAC, Vorbis, WAV. Non gira mai a runtime. |
| Resampling, solo import | `rubato` | 5.0 | Sinc di alta qualità, eseguito una volta per file. |
| Loudness, solo import | `ebur128` | 0.1 | Loudness integrata e true peak secondo EBU R128. |
| Memory map | `memmap2` | 0.9 | File PCM mappati, zero copie. |
| Code real-time | `rtrb` | 0.4 | Ring buffer SPSC lock-free, pensato per l'audio. |
| Canale di controllo | `crossbeam-channel` | 0.5 | Coda dei comandi verso l'engine. Il risveglio passa da un evento Win32 (sezione 4). |
| Serializzazione | `serde`, `serde_json` | 1.0 | Config e indice libreria leggibili. |
| Tipi TypeScript | `ts-rs` | 12.0 | Genera i tipi TS dai tipi Rust del contratto IPC. |
| Errori e log | `thiserror`, `tracing`, `tracing-appender` | 2.0, 0.1, 0.2 | Codici d'errore stabili, log su file non bloccante. |
| Plugin Tauri | `tauri-plugin-single-instance`, `tauri-plugin-autostart` | 2.5, 2.7 | Istanza singola e avvio con Windows. |
| Frontend | Svelte 5, TypeScript, Vite, SCSS | 5.57, —, 8.3, — | Nessun virtual DOM, runtime minimo, avvio rapido in una WebView creata a freddo. |
| Test | `criterion`, `assert_no_alloc` | 0.8, 1.1 | Benchmark del mixer, verifica di zero allocazioni nel thread audio. |

**Scartati**

- `cpal`: comodo e multipiattaforma, ma nasconde il ciclo di vita dello stream e l'invalidazione dei dispositivi, che qui servono espliciti.
- `tauri-plugin-global-shortcut`: un modulo proprio su `RegisterHotKey` (sezione 8) dà controllo diretto su `MOD_NOREPEAT`, sui conflitti e sul thread che riceve i messaggi.
- Electron, .NET/WPF: runtime residente di decine di MB, incompatibile con il budget di riposo.
- UI nativa (Slint, egui, Win32): footprint minore con la finestra aperta, ma tradurre il Figma costa molto di più. Con la WebView effimera il costo a riposo è lo stesso.
- File Opus (`.opus`, `.ogg` di Discord): Symphonia non li decodifica; vedi DECISIONS (`opus-rs`).

## 6. Motore audio

Ogni uscita è uno stream WASAPI in shared mode, event-driven, che esiste solo mentre suona qualcosa. Il mixer legge PCM già pronto da memoria mappata: per ogni campione fa una moltiplicazione e una somma.

### 6.1 Uscite e formato

- **Bus Cable**: endpoint di riproduzione "CABLE Input (VB-Audio Virtual Cable)". Discord lo legge da CABLE Output.
- **Bus Monitor**: endpoint scelto dall'utente, oppure "segui il predefinito". Mai uguale a CABLE Input.
- **Shared mode obbligatorio.** In exclusive mode l'app bloccherebbe il flusso del microfono che Windows mixa su CABLE Input.
- **Periodo**: quello di default del dispositivo, tipicamente 10 ms. Periodi più corti abbassano la latenza ma moltiplicano i risvegli.
- **Formato**: stream sempre in float 32 bit, 48 kHz, stereo. Se coincide con il mix format dell'endpoint non c'è conversione. Altrimenti si usano `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY` e si segnala `rate_mismatch` alla UI.
- **Un mixer per bus.** Ogni bus ha thread, voci e orologio propri e riceve gli stessi comandi play e stop. I due dispositivi non vanno sincronizzati tra loro.

### 6.2 Ciclo di vita di uno stream

Uno stream passa da chiuso ad attivo solo al primo play, e torna a zero da solo. Dopo 2 s di silenzio va in pausa calda: client inizializzato ma fermo, nessun evento, zero CPU. Un nuovo play riparte in pochi millisecondi senza rifare `Initialize`; dopo 60 s senza play il client viene rilasciato.

&#91;embedded content: stati di un bus audio · 3 stati, 5 transizioni\]

La linea tratteggiata vale da qualunque stato: un dispositivo invalidato chiude il bus e l'engine riceve `BusLost`.

### 6.3 Thread di render

```rust
// Un thread per bus. MMCSS "Pro Audio", COM MTA.
loop {
    let timeout = if state == WarmIdle { WARM_HOLD_60S } else { INFINITE };
    match wait_any(&[buffer_event, cmd_event, quit_event], timeout) {
        Buffer => {
            let free = buffer_frames - client.current_padding()?;
            let out = render.get_buffer(free)?;
            mixer.render(out, free);              // niente alloc, niente lock
            render.release_buffer(free)?;
            if mixer.silent_for() >= IDLE_STOP_2S { client.stop()?; state = WarmIdle; }
        }
        Cmd => {
            while let Ok(cmd) = cmd_rx.pop() { mixer.apply(cmd); }   // rtrb
            if state != Running && mixer.has_voices() {
                ensure_initialized()?; prefill_whole_buffer()?; client.start()?;
                state = Running;
            }
        }
        Timeout => { release_client(); state = Closed; }   // una sola volta
        Quit => break,
    }
}
```

- In pausa calda l'evento del buffer non scatta: il thread si sveglia al massimo una volta, al timeout dei 60 s, poi attende senza limite.
- Il prefill riempie tutto il buffer prima di `Start()`, così il primo periodo non è silenzio.
- Chi scrive nel ring dei comandi chiama `SetEvent(cmd_event)` subito dopo il push.
- `AUDCLNT_E_DEVICE_INVALIDATED` da qualsiasi chiamata: si rilascia il client, stato `Closed`, evento `BusLost` all'engine (sezione 9).

### 6.4 Mixer e voci

- **Pool fisso** di 32 voci per bus, allocato all'apertura dello stream. Oltre 32 si ruba la voce più vecchia, con fade di 8 ms.
- **Voce**: `{ play_id, sound: SoundRef, pos, gain_cur, gain_target, fade, state }`. `SoundRef` = puntatore e lunghezza nel file mappato, più un `Arc` che tiene viva la mappatura.
- **Gli `Arc` non muoiono nel thread audio.** Si creano nell'engine e, a voce finita, tornano all'engine su un ring dedicato. Un drop lì dentro potrebbe smappare il file, cioè una syscall.
- **Render a blocchi**: azzera l'uscita, poi per ogni voce `out[i] += s16[i] * gain`, con rampa lineare del gain sul blocco. Mono duplicato su L e R. Loop semplici che il compilatore vettorizza.
- **Gain di una voce** = gain loudness (metadato) × gain utente del suono × volume del bus × fade.
- **Fade**: 2 ms in ingresso (evita il click se il file non parte da zero), 8 ms in uscita.
- **Modalità di riproduzione** risolte dall'engine prima dei comandi. Riavvia = stop delle voci di quel suono + play. Play/stop = stop se suona, altrimenti play. Esclusivo = stop di tutto + play.
- **Limiter** di picco sul bus, senza lookahead: attack 1 ms, release 100 ms, ceiling −1 dBFS, più clip di sicurezza. Lavora solo con voci attive.
- **Fine voce**: il thread pubblica `Ended(play_id)` su un ring verso l'engine e lo sveglia con un evento. Un play è finito quando tutti i bus che l'hanno ricevuto lo hanno chiuso.

### 6.5 Regole real-time (da copiare in CLAUDE.md)

1. Nel thread audio: nessuna allocazione, nessun `Mutex`, nessun log, nessun I/O, nessun drop che libera memoria.
2. Comunicazione solo via `rtrb` (comandi in entrata, eventi e rifiuti in uscita) e atomici.
3. Thread registrato in MMCSS con `AvSetMmThreadCharacteristicsW("Pro Audio")`.
4. Flush-to-zero e denormals-are-zero attivi sul thread, per non rallentare il release del limiter.
5. Contatori atomici: underrun, errori, blocchi renderizzati. L'engine li legge solo quando la UI diagnostica li chiede.
6. In debug `assert_no_alloc` avvolge `mixer.render`: un'allocazione fa fallire i test.
7. Profilo release: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`.

### 6.6 Microfono verso Discord

**Modo Sistema (v1, predefinito).** Windows inoltra il microfono a CABLE Input con "Ascolta questo dispositivo". È lo stesso schema usato da Resanance ([guida](https://www.itgeared.com/how-to-get-resanance-to-work-on-discord/)). L'app non apre il microfono: costo zero, ma niente gain o mute del mic dall'app.

**Modo Bridge (v1.1, F16).** L'app cattura il microfono e lo somma nel bus Cable.

- Thread di cattura event-driven sul mic, ring `rtrb` verso il thread del bus Cable.
- Mic e VB-Cable hanno orologi diversi e il ring deriva. Un resampler adattivo, rapporto entro ±0,2% regolato da un PI sul riempimento del ring (target 20 ms), lo tiene stabile senza salti udibili.
- Gain del mic, mute da hotkey, ducking del mic mentre suona un effetto.
- **Acceso solo se qualcuno ascolta.** Un thread COM MTA osserva le sessioni di cattura su CABLE Output con `IAudioSessionManager2` e `IAudioSessionNotification`. Bridge attivo solo con una sessione `Active`, cioè Discord in chiamata.
- La documentazione Microsoft descrive l'API per un generico endpoint senza dire se valga per la cattura ([IAudioSessionEnumerator](https://learn.microsoft.com/en-us/windows/win32/api/audiopolicy/nn-audiopolicy-iaudiosessionenumerator)): va verificato nella Fase 0.
- Con il Bridge attivo "Ascolta questo dispositivo" va spento, altrimenti la voce arriva doppia. La UI lo ricorda quando attivi il Bridge.

### 6.7 Latenza

| Tratto | Stima, da misurare in Fase 0 |
| --- | --- |
| `WM_HOTKEY` → comando nel ring | < 1 ms |
| Ripartenza da pausa calda + prefill | 1–3 ms |
| Buffer WASAPI condiviso | 10–20 ms |
| Motore audio di Windows e driver | \~10 ms |
| Da stream chiuso, in più: `Initialize` | 10–50 ms |

Il taglio del silenzio iniziale all'import (sezione 7) toglie altra latenza percepita.

## 7. Libreria suoni

Ogni file importato viene convertito una volta in PCM 16 bit a 48 kHz e salvato nella libreria: a runtime non esiste decodifica. La riproduzione legge direttamente dal file mappato in memoria.

### 7.1 Pipeline di import

Gira in un thread worker creato per il job, che termina a fine lavoro. Il gain calcolato resta un metadato: cambiarlo non riscrive il file.

1. Hash xxh3 del sorgente (`xxhash-rust`) per riconoscere i duplicati.
2. Decodifica con Symphonia, feature `mp3`, `aac`, `isomp4`, `flac`, `ogg`, `vorbis`, `wav`, `pcm`.
3. Canali: mono resta mono, stereo resta stereo, oltre 2 canali downmix a stereo.
4. Resampling a 48 kHz con rubato, solo se serve.
5. Taglio del silenzio iniziale e finale sotto −50 dBFS, lasciando 5 ms. Disattivabile.
6. Loudness EBU R128 con `ebur128`: `gain_db = target_lufs − integrata`, limitato da `true_peak + gain_db ≤ ceiling`. Default: −18 LUFS, ceiling −1 dBTP. Sotto i 400 ms la loudness integrata non è definita: si usa la momentanea massima.
7. Conversione a i16 con dither TPDF, scrittura su file temporaneo, rename atomico.
8. Aggiornamento dell'indice ed evento `library://changed`.

### 7.2 Formato `.sbpcm`

Header di 64 byte little-endian, poi i campioni. I dati partono allineati a 64 byte, comodo per il SIMD.

| Offset | Campo | Tipo | Valore |
| --- | --- | --- | --- |
| 0 | `magic` | `[u8; 8]` | `b"SBPCM01\0"` |
| 8 | `version` | `u16` | 1 |
| 10 | `channels` | `u16` | 1 o 2 |
| 12 | `sample_rate` | `u32` | 48000 |
| 16 | `frames` | `u64` | Numero di frame |
| 24 | `flags` | `u32` | Riservato, 0 |
| 28 | `reserved` | `[u8; 36]` | Zeri |
| 64 | `data` | `i16` interleaved | `frames × channels` campioni |

Perché i16: metà spazio del float, conversione gratuita nel mixer, 16 bit con dither trasparenti per effetti e voce. In stereo occupa 192 KB al secondo, circa 11,5 MB al minuto. Durata massima di default: 10 minuti per file, configurabile.

### 7.3 Runtime

- All'avvio si carica solo `index.json`, nessun file audio.
- Al primo play di un suono l'engine lo mappa con `memmap2` e tiene l'`Arc<Mmap>`. La mappatura occupa spazio di indirizzi, non RAM.
- Windows porta le pagine in RAM al primo accesso e le tiene nella cache di sistema. Opzionale: `PrefetchVirtualMemory` sui primi 256 KB appena arriva l'hotkey.
- Un file mappato non si può cancellare: un suono eliminato sparisce subito dall'indice e il file viene cancellato quando l'ultimo `Arc` torna all'engine.

### 7.4 Posizioni su disco

- Indice: `%LOCALAPPDATA%\Soundboard\library\index.json`
- Audio: `%LOCALAPPDATA%\Soundboard\library\pcm\<uuid>.sbpcm`
- Gli originali non vengono copiati (opzione "conserva originali", default spenta). L'indice salva percorso e hash del sorgente.

## 8. Hotkey globali e push-to-talk

Le hotkey usano `RegisterHotKey` su un thread dedicato: il thread dorme in `GetMessageW` e si sveglia solo quando premi una combinazione registrata. Nessun hook globale della tastiera in v1.

### 8.1 Modulo hotkey (`sb-platform::hotkeys`)

- Thread dedicato con una finestra message-only (`HWND_MESSAGE`) e loop `GetMessageW`. A ogni `WM_HOTKEY` invia l'azione al canale dell'engine e torna a dormire.
- `RegisterHotKey` va chiamato dal thread che possiede la finestra: registrazioni e cancellazioni arrivano a quel thread con `PostThreadMessageW`.
- Sempre `MOD_NOREPEAT`: tenere premuto il tasto non ripete il suono.
- Conflitto con un'altra app (`ERROR_HOTKEY_ALREADY_REGISTERED`): codice `hotkey_conflict` alla UI, binding salvato ma inattivo.
- Pausa hotkey: `UnregisterHotKey` di tutto tranne la hotkey di pausa.
- Formato in config: stringhe come `"Ctrl+Alt+F1"`, `"Numpad5"`, `"F13"`. Il parser è in Rust; la UI invia e riceve solo stringhe.
- Tasti senza modificatori ammessi (F13–F24, tastierino), ma finché sono registrati nessun'altra app li riceve. La UI deve dirlo.

**Perché niente hook.** Un hook `WH_KEYBOARD_LL` intercetta ogni tasto di ogni app: costo e latenza su tutto l'input del PC. Tasti laterali del mouse e "tieni premuto per suonare" lo richiedono, quindi passano alla v2, con l'hook attivo solo se esiste un binding che lo usa.

### 8.2 Relay push-to-talk (F11)

- Config: tasto PTT di Discord, salvato come scancode, e coda di rilascio di 200 ms.
- Primo play sul bus Cable: `SendInput` key-down con `KEYEVENTF_SCANCODE`. Fine dell'ultimo play più 200 ms: key-up, con un timer one-shot dell'engine.
- Rilascio garantito anche in uscita, in pausa hotkey e nel panic hook.
- Limite noto: se rilasci il tasto fisico mentre un suono è in corso, Discord smette di trasmettere.
- `SendInput` non raggiunge processi con privilegi più alti e il blocco non produce un errore riconoscibile ([SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)). La guida di setup dice di non avviare Discord come amministratore.
- Da validare in Fase 3: Discord accetta il PTT da input iniettato.
- Alternativa consigliata: in Discord "Attività vocale" con sensibilità manuale, senza relay.

## 9. Gestione dispositivi

L'app reagisce ai cambi di dispositivo tramite notifiche di Windows, senza interrogare il sistema a intervalli. Gli endpoint si salvano per ID, con il nome come riserva.

- **Enumerazione** con `IMMDeviceEnumerator::EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)` all'avvio e dopo ogni notifica.
- **Riconoscere VB-Cable**: endpoint di riproduzione il cui nome contiene "CABLE Input" e "VB-Audio". Si salva l'ID; se sparisce, per esempio dopo una reinstallazione del driver, si ricerca per nome e si aggiorna.
- **VB-Cable assente**: stato `cable_missing`, l'app funziona solo in monitor e la UI mostra la guida. In v1 VB-Cable non entra nell'installer: si linka vb-cable.com. La [licenza](https://vb-audio.com/Services/licensing.htm) permette di includerlo se l'utente lo riconosce come prodotto VB-Audio e può donare, ma il setup richiede amministratore e riavvio ([VB-Cable](https://vb-audio.com/Cable/)).
- **Notifiche**: l'engine registra un `IMMNotificationClient`. Le callback non lavorano: inoltrano un messaggio al canale dell'engine e ritornano.
  - `OnDeviceStateChanged`, `OnDeviceAdded`, `OnDeviceRemoved`: nuova enumerazione e aggiornamento dello stato.
  - `OnDefaultDeviceChanged(eRender, eConsole)`: se il monitor segue il predefinito, il bus Monitor si chiude e riapre sul nuovo, subito se sta suonando, altrimenti al prossimo play.
- **Invalidazione**: a un `BusLost` l'engine riapre il bus con backoff di 250 ms, 1 s e 5 s solo se ci sono voci attive. Altrimenti attende il prossimo play.

**Controlli di configurazione.** L'engine li calcola a ogni cambio e li espone in `status`; la UI li mostra.

| Codice | Condizione | Effetto |
| --- | --- | --- |
| `cable_missing` | Nessun CABLE Input attivo | Solo monitor, guida all'installazione |
| `rate_mismatch` | Mix format di un bus diverso da 48 kHz | Funziona con la conversione di Windows, suggerisce 48 kHz |
| `default_is_cable` | Dispositivo di riproduzione predefinito = CABLE Input | Avviso: non sentiresti più l'audio del PC |
| `monitor_is_cable` | Monitor impostato su CABLE Input | Rifiutato, si torna al predefinito |
| `cable_busy` | `AUDCLNT_E_DEVICE_IN_USE` su CABLE Input | Un'altra app lo tiene in exclusive mode |

## 10. Ciclo di vita dell'app

Un solo processo, sempre attivo in tray. La UI nasce quando apri la finestra e muore poco dopo che la chiudi; avvio con Windows e secondo lancio non creano la WebView se non serve.

### 10.1 Avvio: `soundboard.exe [--tray]`

1. Istanza singola (`tauri-plugin-single-instance`): se un'istanza esiste già, riceve gli argomenti e apre la finestra; il nuovo processo esce.
2. Logger su file.
3. Lettura di `config.json` e `index.json`, solo metadati.
4. Thread engine: COM MTA, enumerazione dispositivi, registrazione di `IMMNotificationClient`.
5. Thread hotkey: registrazione delle combinazioni.
6. Icona nella tray.
7. Senza `--tray` crea la finestra; con `--tray` no.

All'avvio non parte nessun thread audio.

### 10.2 Finestra effimera

- `tauri.conf.json` con `"windows": []`: nessuna finestra automatica. Si crea da codice con `WebviewWindowBuilder` quando serve.
- Chiusura con la X: la finestra si nasconde e parte un timer one-shot `ui_keepalive`, default 30 s, regolabile da 0 a 300. Riaprirla entro quel tempo è istantaneo.
- Mentre è nascosta: `IsVisible = false` sul controller e `TrySuspend` sulla WebView, raggiungibili da `with_webview`. `TrySuspend` porta anche il livello di memoria a basso ([MemoryUsageTargetLevel](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2.memoryusagetargetlevel)).
- Allo scadere la finestra viene distrutta. Atteso: zero processi `msedgewebview2.exe` figli entro 10 s. Si verifica in Fase 0; il piano B è nella sezione 17.
- Tauri esce quando si chiude l'ultima finestra. In `RunEvent::ExitRequested` con `code == None` si chiama `api.prevent_exit()`; solo "Esci" dal tray chiama `app.exit(0)` ([esempio ufficiale](https://github.com/tauri-apps/tauri/commit/00e1567584721644797b587205187f9cbe4e5cd1)).
- La UI non ha stato proprio: all'apertura chiede `get_snapshot` (sezione 12).

### 10.3 Uscita

- "Esci" dal tray: stop delle voci con fade, rilascio del PTT, flush della config, `UnregisterHotKey`, chiusura degli stream, `app.exit(0)`.
- Panic hook: rilascia il PTT e scrive il log prima dell'abort.

### 10.4 Avvio con Windows

`tauri-plugin-autostart` con argomento `--tray`, nella chiave Run dell'utente corrente. Spento di default. Nessun servizio, nessun task pianificato, nessun privilegio di amministratore.

### 10.5 Sospensione e ripresa

Nessuna gestione dedicata. I dispositivi che spariscono e ricompaiono arrivano come notifiche, le hotkey restano registrate.

## 11. Persistenza e configurazione

Due file JSON: impostazioni in `%APPDATA%\Soundboard\config.json`, libreria in `%LOCALAPPDATA%\Soundboard\library\index.json`. Li scrive solo l'engine, con debounce e scrittura atomica.

- **Scrittura**: debounce di 500 ms dall'ultima modifica con timer one-shot. File `.tmp`, flush, sostituzione con `ReplaceFileW` che conserva la versione precedente come `.bak`.
- **Lettura**: all'avvio. JSON non valido: si prova il `.bak`, poi i default, con log `config_recovered`.
- **Versioni**: campo `schema_version` e migrazioni `vN → vN+1` in catena, ognuna con test.
- **Un solo scrittore**: l'engine. UI e altri thread mandano comandi.
- **Campo `ui` libero** su ogni suono: un oggetto opaco dove la UI salva i propri dati (colore, icona, posizione nella griglia) senza modifiche al core.

`config.json`

```json
{
  "schema_version": 1,
  "outputs": {
    "cable":   { "device_id": "{0.0.0.00000000}.{...}", "volume_db": 0.0 },
    "monitor": { "device_id": null, "volume_db": -6.0, "enabled": true }
  },
  "mic": { "mode": "system" },
  "playback": {
    "max_voices": 32, "default_mode": "overlap",
    "fade_in_ms": 2, "fade_out_ms": 8,
    "idle_stop_ms": 2000, "warm_hold_ms": 60000
  },
  "loudness": { "target_lufs": -18.0, "ceiling_dbtp": -1.0, "trim_silence": true },
  "ptt": { "enabled": false, "scancode": null, "tail_ms": 200 },
  "hotkeys": [
    { "accelerator": "Ctrl+Alt+F1",  "action": { "type": "play", "sound_id": "8f1c..." } },
    { "accelerator": "Ctrl+Alt+F12", "action": { "type": "stop_all" } }
  ],
  "ui": { "keepalive_s": 30, "window": { "x": 120, "y": 80, "w": 960, "h": 640 } },
  "startup": { "autostart": false }
}
```

`index.json`

```json
{
  "schema_version": 1,
  "folders": [{ "id": "f1", "name": "Meme", "order": 0 }],
  "sounds": [{
    "id": "8f1c...", "name": "Airhorn", "file": "8f1c....sbpcm",
    "channels": 2, "frames": 96000,
    "loudness_gain_db": -3.4, "user_gain_db": 0.0,
    "mode": "restart", "folder_id": "f1", "tags": ["meme"],
    "source": { "path": "C:\\Users\\...\\airhorn.mp3", "xxh3": "..." },
    "ui": {},
    "created_at": "2026-11-06T14:00:00Z"
  }]
}
```

Valori ammessi: `mode` = `overlap` | `restart` | `toggle` | `exclusive`; `mic.mode` = `system` | `bridge`. Azioni hotkey: `play`, `preview`, `stop_all`, `pause_hotkeys`, `mute_monitor`, `open_window`.

## 12. Contratto UI ↔ Core

Il core è l'unica fonte di verità: la UI è una vista senza stato che legge uno snapshot all'apertura e poi ascolta eventi. È l'unica parte del documento che la UI costruita dal Figma deve conoscere.

**Regole**

- All'apertura `get_snapshot()`, poi sottoscrizione agli eventi. Con la finestra distrutta il core non emette nulla.
- La UI non legge file, non tocca l'audio, non fa polling: ogni azione è un comando.
- Tipi TypeScript generati con `ts-rs` in `ui/src/lib/api/types/`, wrapper tipizzati di `invoke` e `listen` in `ui/src/lib/api/`. Nessuna stringa di comando sparsa nei componenti.
- Ogni comando restituisce `Result<T, ApiError>`, con `ApiError { code, message }` e `code` stabile (sezione 15).
- Nessun evento ad alta frequenza. Le barre di avanzamento si animano in locale da `started_at_ms` e `duration_ms`, con `requestAnimationFrame` solo a finestra visibile.
- Meter di livello, solo se il Figma lo prevede: evento `meter` a massimo 20 Hz, emesso solo a finestra visibile. Il thread audio scrive il picco per bus su un atomico.
- Sicurezza: capability Tauri con i soli comandi custom, nessun plugin fs o shell esposto, CSP restrittiva, asset solo locali. I file trascinati arrivano come percorsi dall'evento di drop di Tauri.

**Comandi**

| Comando | Argomenti | Risposta | Note |
| --- | --- | --- | --- |
| `get_snapshot` | — | `Snapshot` | Impostazioni, dispositivi, libreria, hotkey, stato, suoni in corso |
| `play_sound` | `sound_id` | `play_id` | Rispetta la modalità del suono, va su Cable e Monitor |
| `preview_sound` | `sound_id` | `play_id` | Solo Monitor (F03) |
| `stop_sound` | `sound_id` | — | Fade di 8 ms su tutte le voci del suono |
| `stop_all` | — | — |  |
| `import_files` | `paths[]`, `folder_id?` | `job_id` | Avanzamento con l'evento `import:progress` |
| `update_sound` | `sound_id`, `patch` | `Sound` | `name`, `user_gain_db`, `mode`, `folder_id`, `tags`, `ui` |
| `delete_sounds` | `sound_ids[]` | — |  |
| `upsert_folder`, `delete_folder` | `Folder`, `folder_id` | `Folder`, — | Eliminare una cartella sposta i suoni nella radice |
| `set_hotkey` | `action`, `accelerator \| null` | `Hotkey` | `null` rimuove; può rispondere `hotkey_conflict` |
| `pause_hotkeys` | `paused` | — |  |
| `set_output` | `bus`, `device_id \| null`, `volume_db?`, `enabled?` | `Status` | `null` sul monitor = segui il predefinito |
| `update_settings` | `patch` parziale | `Settings` | Validazione nel core |
| `get_diagnostics` | — | `Diagnostics` | Stato dei bus, voci, underrun, formati, versione |

**Eventi**

| Evento | Payload | Quando |
| --- | --- | --- |
| `playback:started` | `play_id`, `sound_id`, `buses`, `started_at_ms`, `duration_ms` | Primo campione accodato |
| `playback:ended` | `play_id`, `sound_id` | Tutti i bus hanno chiuso il play |
| `library:changed` | `added[]`, `updated[]`, `removed[]` | Import, modifica, eliminazione |
| `import:progress` | `job_id`, `done`, `total`, `current`, `errors[]` | Dopo ogni file |
| `devices:changed` | `devices[]` | Notifica di Windows |
| `status:changed` | `status[]` | Cambia un codice della sezione 9 |
| `hotkeys:changed` | `hotkeys[]` | Registrazione, conflitto, pausa |
| `meter` | `cable_peak`, `monitor_peak` (0–1) | Massimo 20 Hz, solo a finestra visibile |

```ts
type Snapshot = {
  settings: Settings;
  devices: Device[];        // { id, name, is_default, is_cable, mix_rate }
  library: { folders: Folder[]; sounds: Sound[] };
  hotkeys: Hotkey[];        // { action, accelerator, active, conflict }
  status: StatusCode[];     // es. ["rate_mismatch"]
  playing: Playing[];       // { play_id, sound_id, started_at_ms, duration_ms }
};
```

`started_at_ms` è in millisecondi Unix: la UI lo confronta con `Date.now()`.

## 13. Struttura del repository

Workspace Cargo con quattro crate di libreria e un guscio Tauri sottile. Audio e libreria si compilano e si testano senza Tauri e senza dispositivi reali.

```text
soundboard/
├─ CLAUDE.md                 # rimanda a docs/SPEC.md, copia regole 6.5 e budget
├─ docs/SPEC.md              # questo documento
├─ Cargo.toml                # workspace e profili
├─ .cargo/config.toml        # crt-static
├─ crates/
│  ├─ sb-audio/              # bus, thread di render, mixer, voci, limiter
│  │  ├─ src/bus.rs          # ciclo di vita WASAPI (6.2, 6.3)
│  │  ├─ src/mixer.rs        # voci, gain, fade, limiter (6.4)
│  │  ├─ src/sink.rs         # trait AudioSink: WASAPI reale e mock per i test
│  │  └─ benches/mixer.rs
│  ├─ sb-library/            # import, formato .sbpcm, indice, loudness (7)
│  ├─ sb-platform/           # hotkey, SendInput, notifiche dispositivi, sessioni, MMCSS
│  └─ sb-core/               # thread engine, stato, config, comandi ed eventi
├─ src-tauri/                # guscio: tray, finestra effimera, IPC
│  ├─ src/main.rs
│  ├─ src/ipc.rs             # #[tauri::command] verso sb-core
│  ├─ src/window.rs          # crea, nasconde, sospende, distrugge (10.2)
│  ├─ capabilities/default.json
│  └─ tauri.conf.json        # "windows": []
├─ ui/                       # Svelte 5 + TS + SCSS, dal Figma
│  ├─ src/lib/api/           # wrapper invoke/listen, types/ generati da ts-rs
│  ├─ src/lib/stores/        # stato derivato da snapshot + eventi
│  └─ src/styles/            # _variables.scss, _mixins.scss
├─ tools/bench/              # script di misura (sezione 15)
└─ .github/workflows/ci.yml  # build, clippy, test, export dei tipi
```

- **Dipendenze a senso unico**: `src-tauri → sb-core → sb-audio, sb-library, sb-platform`. `sb-audio` e `sb-library` non conoscono Tauri né `sb-core`.
- **Profilo release**: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
- **`.cargo/config.toml`**: `rustflags = ["-C", "target-feature=+crt-static"]` per non dipendere dal Visual C++ Redistributable.
- **Target**: `x86_64-pc-windows-msvc`, Windows 10 e 11.

## 14. Setup utente: VB-Cable e Discord

Configurazione una tantum. L'app rileva da sola i problemi dei punti 1, 2 (sulle sue uscite), 3 e 5, con i codici della sezione 9; gli altri dipendono da Windows e Discord e la UI li mostra come guida.

1. **Installa VB-Cable** da vb-cable.com: estrai il pacchetto, avvia il setup come amministratore, riavvia ([VB-Cable](https://vb-audio.com/Cable/)).
2. **Tutto a 48 kHz.** Pannello audio classico (`mmsys.cpl`) → Riproduzione e Registrazione → Proprietà → Avanzate: 48000 Hz per CABLE Input, CABLE Output, microfono e cuffie. Nessuno stream andrà convertito.
3. **Exclusive mode spento su CABLE Input**: nella stessa scheda togli "Consenti alle applicazioni di assumere il controllo esclusivo del dispositivo".
4. **Microfono su VB-Cable** (modo Sistema): Registrazione → microfono → Proprietà → Ascolta → "Ascolta questo dispositivo" → "Riproduci tramite questo dispositivo: CABLE Input" ([guida](https://www.itgeared.com/how-to-get-resanance-to-work-on-discord/)).
5. **Riproduzione predefinita = le tue cuffie**, mai CABLE Input.
6. **Discord → Impostazioni → Voce e video** ([riferimento](https://voxbooster.pages.dev/blog/discord-voice-modifier-real-time-guide)):
   - Dispositivo di input: CABLE Output (VB-Audio Virtual Cable). Output: le tue cuffie.
   - Soppressione del rumore: Nessuna. Krisp è addestrato sul parlato naturale e può trattare come rumore ciò che non lo è, quindi taglia effetti e musica.
   - Controllo automatico del guadagno: spento, altrimenti il volume pompa.
   - Cancellazione dell'eco e Riduzione del rumore (Avanzate): spente se usi le cuffie.
   - Modalità di input: Attività vocale con sensibilità manuale intorno a −40/−50 dB, oppure Push-to-talk con il relay F11.
   - Con il relay PTT, Discord non va avviato come amministratore (sezione 8.2).
7. **Scheda Comunicazioni** del pannello audio: "Non fare nulla", così Windows non abbassa gli altri suoni quando sei in chiamata.

## 15. Errori, log, test e misure

Ogni errore ha un codice stabile e ogni voce del budget ha una misura ripetibile. Il thread audio non scrive log: espone contatori.

### 15.1 Codici d'errore

Un enum Rust con `thiserror`, esportato in TypeScript con `ts-rs`: `cable_missing`, `rate_mismatch`, `default_is_cable`, `monitor_is_cable`, `cable_busy`, `device_lost`, `hotkey_conflict`, `hotkey_invalid`, `import_unsupported`, `import_decode_failed`, `import_too_long`, `disk_full`, `sound_not_found`, `config_recovered`, `ptt_send_failed`. I testi per l'utente li decide la UI a partire dal codice.

### 15.2 Log

- `tracing` con `tracing-appender` non bloccante in `%LOCALAPPDATA%\Soundboard\logs\`, rotazione giornaliera, ultimi 5 file.
- Livello `info` in release: avvio, cambi di stato dei bus, dispositivi, errori, import. Il singolo play solo in `debug`.
- I contatori del thread audio (underrun, errori) li legge e li logga l'engine quando un bus si chiude.

### 15.3 Test

| Livello | Cosa | Dove |
| --- | --- | --- |
| Unit | Mixer: somma, rampe, fade, furto di voci, limiter sotto il ceiling | `sb-audio` |
| Unit | `.sbpcm` andata e ritorno, header non validi | `sb-library` |
| Unit | Gain di loudness, limite sul true peak, suoni sotto 400 ms | `sb-library` |
| Unit | Modalità di riproduzione, parser degli accelerator, migrazioni config | `sb-core`, `sb-platform` |
| Golden | Sequenza di comandi renderizzata offline su `MockSink`, confronto bit a bit (dither con seed fisso) | `sb-audio` |
| Real-time | `assert_no_alloc` attorno a `render` in tutti i test del mixer | `sb-audio` |
| Benchmark | ns per frame con 1, 8 e 32 voci, `criterion` | `sb-audio/benches` |
| Manuale | Scollega le cuffie, cambia il predefinito, reinstalla VB-Cable, sospendi e riprendi | Fase 2 |
| End-to-end | Chiamata Discord reale: l'amico sente voce e suoni, senza tagli | Fase 3 |

CI su GitHub Actions (`windows-latest`): build, `clippy -D warnings`, test, export dei tipi. I runner possono non avere dispositivi audio, quindi i test usano `MockSink`.

### 15.4 Come si misura il budget

- **Riposo**: app in tray per 10 min con la UI distrutta. Process Explorer, scheda Threads: Context Switch Delta a 0. Conferma con `wpr -start CPU` per 60 s, `wpr -stop idle.etl`, poi WPA, tabella CPU Usage (Precise) filtrata sul processo.
- **Timer ed energia**: `powercfg /energy /duration 60` da prompt amministratore. L'app non deve comparire tra le richieste di risoluzione del timer.
- **Memoria**: working set e private bytes dopo 10 min, con Process Explorer o VMMap.
- **WebView**: albero dei processi in Process Explorer. Dopo `ui_keepalive` nessun `msedgewebview2.exe` figlio.
- **Latenza**: trace interno con QPC su `WM_HOTKEY` e sul primo buffer che contiene la voce, più `GetStreamLatency`. Si riporta il p95 su 100 pressioni.
- **Script**: `tools/bench/` raccoglie questi passi in uno script PowerShell che stampa ogni metrica accanto al suo target.

## 16. Roadmap e criteri di accettazione

Sette fasi in sequenza, e ogni gate è una misura, non un'impressione. Il Figma entra solo in Fase 4, quando il core è già misurato; la Fase 0 viene prima di tutto perché può cambiare l'architettura della UI.

&#91;embedded content: roadmap · 7 fasi, 7 gate\]

Un gate rosso blocca la fase successiva: si corregge, si rimisura, poi si prosegue.

**Fase 0 · Spike tecnici** (codice usa e getta in `spikes/`)

- [ ] App Tauri senza finestre all'avvio, con tray. Finestra creata e distrutta 20 volte: conta i `msedgewebview2.exe` e misura il working set a riposo.
- [ ] Render WASAPI su CABLE Input in shared mode event-driven: pausa calda e tempo di ripartenza.
- [ ] `IAudioSessionManager2` su CABLE Output: stato `Active` con Discord in chiamata, `Inactive` fuori.
- [ ] Decisione scritta in `docs/DECISIONS.md`: Tauri confermato o piano B (sezione 17).

**Fase 1 · Core audio headless**

- [ ] `sb-library`: import, `.sbpcm`, loudness, indice, test.
- [ ] `sb-audio`: mixer, voci, fade, limiter, `MockSink`, test golden, benchmark.
- [ ] Bus WASAPI con il ciclo di vita della 6.2.
- [ ] CLI di prova: `sb-cli import <file>` e `sb-cli play <id>` su Cable e Monitor.

**Fase 2 · App residente**

- [ ] Guscio Tauri con `"windows": []`, tray e `prevent_exit`.
- [ ] Modulo hotkey, thread engine, config con scrittura atomica.
- [ ] Notifiche dispositivi, riconoscimento di VB-Cable, codici di stato.
- [ ] Istanza singola e avvio con Windows in `--tray`.
- [ ] Script `tools/bench` e prima misura del budget di riposo.

**Fase 3 · Integrazione Discord**

- [ ] Relay PTT con rilascio garantito.
- [ ] Controlli della sezione 9 esposti in `status`.
- [ ] Prova reale in chiamata con un amico, impostazioni della sezione 14.

**Fase 4 · UI dal Figma**

- [ ] Tipi `ts-rs` e wrapper in `ui/src/lib/api/`.
- [ ] UI Svelte sul contratto della sezione 12, design dal Figma.
- [ ] Finestra effimera: nascondi, sospendi, distruggi dopo `ui_keepalive`.

**Fase 5 · Release v1**

- [ ] Installer NSIS per utente, senza amministratore.
- [ ] Tabella del budget completa e test manuali della 15.3.

**Fase 6 · v1.1 Bridge mic**

- [ ] Cattura del mic, resampler adattivo, ducking, mute da hotkey.
- [ ] Accensione e spegnimento automatici in base alle sessioni su CABLE Output.

## 17. Rischi e decisioni aperte

Il rischio più grosso è una WebView che resta in memoria dopo la distruzione della finestra. Si verifica per primo, in Fase 0, perché cambierebbe l'architettura della UI ma non il core.

| Rischio | Impatto | Mitigazione |
| --- | --- | --- |
| I processi WebView2 sopravvivono alla distruzione della finestra | Decine di MB occupati a riposo | Fase 0 lo misura. Piano B: UI in un eseguibile separato `soundboard-ui.exe`, avviato all'apertura e chiuso alla chiusura, collegato al core da una named pipe. Il core resta identico. |
| Il guscio Tauri supera 20 MB anche senza finestra | Budget di riposo mancato | Fase 0 lo misura. Stesso piano B: core senza Tauri, tray e hotkey in Win32 diretto. |
| "Ascolta questo dispositivo" aggiunge latenza o artefatti alla voce | Voce peggiore su Discord | Bridge interno in v1.1 (6.6). |
| Discord ignora il PTT iniettato, o gira come amministratore | Relay F11 inutile | Validazione in Fase 3, ripiego su Attività vocale. |
| L'API delle sessioni non funziona sugli endpoint di cattura | Il Bridge non si accende e spegne da solo | Validazione in Fase 0, ripiego su toggle manuale con hotkey. |
| Mix format diverso da 48 kHz | Conversione a carico del motore audio di Windows | `AUTOCONVERTPCM`, avviso `rate_mismatch`, guida al punto 14.2. |
| L'ID dell'endpoint cambia dopo una reinstallazione di VB-Cable | Bus Cable perso | Riserva per nome (sezione 9). |
| Un'altra app tiene CABLE Input in exclusive mode | Niente suoni su Discord | Codice `cable_busy`, guida al punto 14.3. |
| Combinazioni già usate da altre app | Hotkey inattive | Codice `hotkey_conflict` mostrato dalla UI. |

**Decisioni aperte**

- [ ] Nome definitivo dell'app: serve per cartelle, installer e mutex dell'istanza singola.
- [ ] Il Figma prevede un meter di livello? Se sì, si attiva l'evento `meter`.
- [ ] Solo per te o anche da distribuire? Nel secondo caso conviene firmare l'eseguibile per evitare gli avvisi di SmartScreen.
- [ ] Durata massima per file: bastano 10 minuti o servono tracce lunghe?

## Fonti

Versioni di crate e pacchetti verificate su crates.io e npm il 6 novembre 2026.

- [VB-Audio — VB-CABLE Virtual Audio Device](https://vb-audio.com/Cable/)
- [VB-Audio — Licensing](https://vb-audio.com/Services/licensing.htm)
- [Microsoft Learn — IAudioSessionEnumerator](https://learn.microsoft.com/en-us/windows/win32/api/audiopolicy/nn-audiopolicy-iaudiosessionenumerator)
- [Microsoft Learn — SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [Microsoft Learn — CoreWebView2.MemoryUsageTargetLevel](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2.memoryusagetargetlevel)
- [Tauri — esempio `prevent_exit` con `code.is_none()`](https://github.com/tauri-apps/tauri/commit/00e1567584721644797b587205187f9cbe4e5cd1)
- [ITGeared — Resanance su Discord](https://www.itgeared.com/how-to-get-resanance-to-work-on-discord/)
- [VoxBooster — impostazioni voce di Discord](https://voxbooster.pages.dev/blog/discord-voice-modifier-real-time-guide)
