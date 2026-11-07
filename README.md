# MicFX

Soundboard per Windows. Clicchi un suono e lo sentono tutti su Discord, insieme alla tua voce, mentre tu lo
senti in cuffia. Resta nella tray e a riposo non consuma CPU.

## Cosa fa

- **Griglia o elenco**: clic per suonare, `Maiusc+Invio` o 🎧 per ascoltarlo solo tu.
- **Il mio volume** (cuffie) e **Volume microfono** (quello che sentono gli altri), ognuno con il suo interruttore.
- **Emoji** per ogni suono: tutte, ricercabili in italiano e inglese, con tono della pelle. Al primo import
  MicFX ne suggerisce una dal nome del file.
- **Rinomina** (F2), **scorciatoie globali facoltative** per ogni suono (funzionano anche nei giochi), cartelle,
  ordine personalizzato col trascinamento, ricerca, eliminazione con Annulla.
- **Import** con trascinamento o selettore: MP3, WAV, FLAC, OGG, M4A. Volumi livellati in automatico (EBU R128).
- Guida di configurazione con Discord che si spunta da sola, relay del push-to-talk, avvio con Windows.

I dati restano sul PC (`%APPDATA%\MicFX` per le impostazioni, `%LOCALAPPDATA%\MicFX Data` per i suoni) e
sopravvivono ad aggiornamenti e reinstallazioni: sono separati dalla cartella del programma.

## Installazione

Scarica `MicFX_x.y.z_x64-setup.exe` dall'[ultima release](https://github.com/rkarmaaa/micfx/releases/latest)
e avvialo: si installa per il solo utente, senza permessi di amministratore. Per aggiornare installa la versione
nuova sopra quella vecchia: suoni, scorciatoie e impostazioni restano.

L'installer non è ancora firmato digitalmente: Windows SmartScreen può mostrare "Windows ha protetto il PC".
Clicca **Ulteriori informazioni** → **Esegui comunque**.

## Requisiti

- Windows 10 o 11, [VB-Cable](https://vb-audio.com/Cable/) installato.
- Per sviluppare: Rust stable (MSVC), Visual Studio Build Tools con C++, Node.js 22+.

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

L'installer per utente (senza amministratore) finisce in `target/release/bundle/nsis/`.

Documentazione: [docs/SPEC.md](docs/SPEC.md) (specifica), [docs/DECISIONS.md](docs/DECISIONS.md) (scelte),
[CLAUDE.md](CLAUDE.md) (regole per chi lavora sul codice).

## Licenza

Codice pubblicato per consultazione, tutti i diritti riservati: vedi [LICENSE](LICENSE). Gli installer delle
release si possono usare liberamente per uso personale.
