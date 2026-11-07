# MicFX

Soundboard per **Windows** e **macOS** (Apple Silicon). Clicchi un suono e lo sentono tutti su Discord, insieme
alla tua voce, mentre tu lo senti in cuffia. Resta nella tray (o nella barra dei menu del Mac) pronto con le
scorciatoie.

| | Windows | macOS |
|---|---|---|
| Sistema | Windows 10 o 11 (x64) | macOS 12 o successivo, chip Apple (M1 e successivi) |
| Microfono virtuale | [VB-Cable](https://vb-audio.com/Cable/) | [BlackHole 2ch](https://existential.audio/blackhole/) |
| File da scaricare | `MicFX_x.y.z_x64-setup.exe` | `MicFX_x.y.z_aarch64.dmg` |
| La tua voce su Discord | "Ascolta questo dispositivo" di Windows | la manda MicFX insieme ai suoni |
| Scorciatoie della finestra | `Ctrl` | `⌘` |

## Cosa fa

- **Griglia o elenco**: clic per suonare, `Maiusc+Invio` (`⇧↩` su Mac) o 🎧 per ascoltarlo solo tu.
- **Il mio volume** (cuffie) e **Volume microfono** (quello che sentono gli altri), ognuno con il suo interruttore.
- **Emoji** per ogni suono: tutte, ricercabili in italiano e inglese, con tono della pelle. Al primo import
  MicFX ne suggerisce una dal nome del file.
- **Rinomina** (F2), **scorciatoie globali facoltative** per ogni suono (funzionano anche nei giochi), cartelle,
  ordine personalizzato col trascinamento, ricerca, eliminazione con Annulla.
- **Import** con trascinamento o selettore: MP3, WAV, FLAC, OGG, M4A. Volumi livellati in automatico (EBU R128).
- Guida di configurazione con Discord che si spunta da sola, relay del push-to-talk, avvio automatico.

I dati restano sul computer e sopravvivono ad aggiornamenti e reinstallazioni, perché sono separati dal programma:
su Windows in `%APPDATA%\MicFX` (impostazioni) e `%LOCALAPPDATA%\MicFX Data` (suoni), su Mac in
`~/Library/Application Support/MicFX` e `~/Library/Application Support/MicFX Data`.

## Installazione

I file per entrambi i sistemi sono nell'[ultima release](https://github.com/rkarmaaa/micfx/releases/latest).

### Windows

Scarica `MicFX_x.y.z_x64-setup.exe` e avvialo: si installa per il solo utente, senza permessi di amministratore. Per aggiornare installa la versione
nuova sopra quella vecchia: suoni, scorciatoie e impostazioni restano.

L'installer non è ancora firmato digitalmente: Windows SmartScreen può mostrare "Windows ha protetto il PC".
Clicca **Ulteriori informazioni** → **Esegui comunque**.

Serve [VB-Cable](https://vb-audio.com/Cable/).

### macOS

Scarica `MicFX_x.y.z_aarch64.dmg`, aprilo e trascina MicFX in **Applicazioni**. Serve
[BlackHole 2ch](https://existential.audio/blackhole/) (gratuito).

- L'app non è notarizzata da Apple: al primo avvio macOS la blocca. Apri **Impostazioni di Sistema** →
  **Privacy e sicurezza** e clicca **Apri comunque**.
- Al primo avvio consenti l'accesso al **microfono**: MicFX manda la tua voce in BlackHole insieme ai suoni.
- Il relay del push-to-talk chiede il permesso **Accessibilità**.
- In Discord scegli **BlackHole 2ch** come dispositivo di input.

## Requisiti per sviluppare

- Rust stable e Node.js 22+.
- Windows: Visual Studio Build Tools con C++ (toolchain MSVC).
- macOS: Xcode Command Line Tools (`xcode-select --install`).

## Sviluppo

```bash
npm install
npm run dev
```

```bash
cargo test --workspace
```

```bash
npm run build
```

Su Windows l'installer per utente (senza amministratore) finisce in `target/release/bundle/nsis/`. Su Mac il
`.dmg` si costruisce con `bash tools/release/build-macos.sh`. Le release ufficiali le costruisce GitHub Actions da un tag `vX.Y.Z`
(`.github/workflows/release.yml`), con i file per entrambi i sistemi.

Documentazione: [docs/SPEC.md](docs/SPEC.md) (specifica), [docs/DECISIONS.md](docs/DECISIONS.md) (scelte),
[CLAUDE.md](CLAUDE.md) (regole per chi lavora sul codice).

## Licenza

Codice pubblicato per consultazione, tutti i diritti riservati: vedi [LICENSE](LICENSE). Gli installer delle
release si possono usare liberamente per uso personale.
