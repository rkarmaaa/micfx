//! Impostazioni voce di Discord, in sola lettura, dal Local Storage del client desktop.
//!
//! Discord salva le impostazioni del motore voce nella chiave `MediaEngineStore` del localStorage di
//! Chromium: un database LevelDB in `%APPDATA%\discord\Local Storage\leveldb`. Il formato non è
//! documentato: qualsiasi errore dà `None` e la UI ripiega sull'elenco da controllare a mano.
//! Si legge solo quando serve (finestra aperta o tornata in primo piano), mai in polling.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Le impostazioni di Discord che decidono se i suoni di MicFX arrivano agli altri.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordVoice {
    /// "Discord", "Discord PTB" o "Discord Canary".
    pub client: &'static str,
    /// `"default"` oppure l'ID dell'endpoint di cattura (stesso formato di Windows).
    pub input_device_id: String,
    pub push_to_talk: bool,
    /// Soppressione del rumore Krisp (`noiseCancellation`): toglie tutto ciò che non è voce.
    pub krisp: bool,
    /// Soppressione del rumore "Standard".
    pub noise_suppression: bool,
    /// Attività vocale avanzata (`vadUseKrisp`): il microfono si apre solo quando qualcuno parla.
    pub advanced_vad: bool,
    pub echo_cancellation: bool,
    pub automatic_gain_control: bool,
    /// Sensibilità di input automatica.
    pub auto_sensitivity: bool,
}

const CLIENTS: [(&str, &str); 3] =
    [("discord", "Discord"), ("discordptb", "Discord PTB"), ("discordcanary", "Discord Canary")];

/// Chiave localStorage: `_https://discord.com\0\x01MediaEngineStore` (anche il vecchio discordapp.com).
const KEY_SUFFIX: &[u8] = b"\x00\x01MediaEngineStore";

fn is_media_engine_key(k: &[u8]) -> bool {
    k.starts_with(b"_https://") && k.ends_with(KEY_SUFFIX)
}

/// Legge le impostazioni del client Discord usato più di recente. `None` se Discord non c'è, non è
/// mai entrato in un canale vocale o il formato non è riconosciuto.
pub fn read_voice_settings() -> Option<DiscordVoice> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from)?;
    let mut best: Option<(SystemTime, DiscordVoice)> = None;
    for (dir, client) in CLIENTS {
        let db = appdata.join(dir).join("Local Storage").join("leveldb");
        let Some(found) = read_from(&db, client) else { continue };
        if best.as_ref().is_none_or(|(t, _)| found.0 > *t) {
            best = Some(found);
        }
    }
    best.map(|(_, v)| v)
}

/// Impostazioni da un database LevelDB, con la data dell'ultima modifica del database.
pub fn read_from(db: &Path, client: &'static str) -> Option<(SystemTime, DiscordVoice)> {
    let (candidates, modified) = leveldb::values(db, is_media_engine_key)?;
    // Dal più recente: un record scritto a metà (Discord sta scrivendo) non è JSON valido e si
    // passa al precedente. Una cancellazione invece è l'ultima parola.
    for v in &candidates {
        let bytes = v.as_deref()?;
        if let Some(p) = decode_value(bytes).and_then(|s| parse_media_engine(&s, client)) {
            return Some((modified, p));
        }
    }
    None
}

/// Valore del localStorage di Chromium: primo byte 0 = UTF-16LE, 1 = Latin-1.
fn decode_value(v: &[u8]) -> Option<String> {
    let (&kind, rest) = v.split_first()?;
    match kind {
        0 => {
            let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
            String::from_utf16(&units).ok()
        }
        1 => Some(rest.iter().map(|&b| b as char).collect()),
        _ => None,
    }
}

/// Interpreta il JSON di `MediaEngineStore`. I campi mancanti valgono quanto i default di Discord.
pub fn parse_media_engine(json: &str, client: &'static str) -> Option<DiscordVoice> {
    let root: serde_json::Value = serde_json::from_str(json).ok()?;
    let d = root.get("default")?;
    let flag = |v: &serde_json::Value, k: &str, default: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(default);
    let opts = d.get("modeOptions").cloned().unwrap_or(serde_json::Value::Null);
    Some(DiscordVoice {
        client,
        input_device_id: d.get("inputDeviceId").and_then(|x| x.as_str()).unwrap_or("default").to_string(),
        push_to_talk: d.get("mode").and_then(|x| x.as_str()) == Some("PUSH_TO_TALK"),
        krisp: flag(d, "noiseCancellation", true),
        noise_suppression: flag(d, "noiseSuppression", false),
        advanced_vad: flag(&opts, "vadUseKrisp", true),
        echo_cancellation: flag(d, "echoCancellation", true),
        automatic_gain_control: flag(d, "automaticGainControl", true),
        auto_sensitivity: flag(&opts, "autoThreshold", true),
    })
}

/// Lettore LevelDB minimo e in sola lettura: tabelle `.ldb` (blocchi Snappy) e log `.log`.
/// Tutti gli accessi sono controllati: un file troncato o in scrittura non fa mai panico.
mod leveldb {
    use std::borrow::Cow;
    use std::path::Path;
    use std::time::SystemTime;

    const TABLE_MAGIC: u64 = 0xdb47_7524_8b80_fb57;
    const LOG_BLOCK: usize = 32 * 1024;
    const MAX_FILE: u64 = 64 * 1024 * 1024;

    /// Valori della chiave cercata, dal più recente (`None` = cancellata), e ultima modifica del database.
    pub fn values(dir: &Path, matches: impl Fn(&[u8]) -> bool) -> Option<(Vec<Option<Vec<u8>>>, SystemTime)> {
        let mut found: Vec<(u64, Option<Vec<u8>>)> = Vec::new();
        let mut modified = SystemTime::UNIX_EPOCH;
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
            if !matches!(ext.as_str(), "ldb" | "sst" | "log") {
                continue;
            }
            // Le tabelle possono sparire durante una compattazione: si salta il file.
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() > MAX_FILE {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if let Ok(t) = meta.modified() {
                modified = modified.max(t);
            }
            let mut visit = |key: &[u8], seq: u64, value: Option<&[u8]>| {
                if matches(key) {
                    found.push((seq, value.map(<[u8]>::to_vec)));
                }
            };
            if ext == "log" {
                log(&bytes, &mut visit);
            } else {
                let _ = table(&bytes, &mut visit);
            }
        }
        if found.is_empty() {
            return None;
        }
        found.sort_by_key(|a| std::cmp::Reverse(a.0));
        Some((found.into_iter().map(|(_, v)| v).collect(), modified))
    }

    type Visit<'a> = dyn FnMut(&[u8], u64, Option<&[u8]>) + 'a;

    pub(super) fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
        let mut r = 0u64;
        for shift in (0..64).step_by(7) {
            let c = *b.get(*i)?;
            *i += 1;
            r |= u64::from(c & 0x7f) << shift;
            if c < 0x80 {
                return Some(r);
            }
        }
        None
    }

    fn block(file: &[u8], off: u64, size: u64) -> Option<Cow<'_, [u8]>> {
        let off = usize::try_from(off).ok()?;
        let end = off.checked_add(usize::try_from(size).ok()?)?;
        let data = file.get(off..end)?;
        match *file.get(end)? {
            0 => Some(Cow::Borrowed(data)),
            1 => snap::raw::Decoder::new().decompress_vec(data).ok().map(Cow::Owned),
            _ => None,
        }
    }

    /// Voci di un blocco (chiavi con prefisso condiviso, array di restart in coda).
    fn entries(b: &[u8], f: &mut dyn FnMut(&[u8], &[u8])) -> Option<()> {
        let n = b.len().checked_sub(4)?;
        let restarts = u32::from_le_bytes(b.get(n..)?.try_into().ok()?) as usize;
        let end = n.checked_sub(restarts.checked_mul(4)?)?;
        let mut key = Vec::new();
        let mut i = 0;
        while i < end {
            let shared = usize::try_from(varint(b, &mut i)?).ok()?;
            let own = usize::try_from(varint(b, &mut i)?).ok()?;
            let vlen = usize::try_from(varint(b, &mut i)?).ok()?;
            if shared > key.len() {
                return None;
            }
            key.truncate(shared);
            let kend = i.checked_add(own)?;
            key.extend_from_slice(b.get(i..kend)?);
            let vend = kend.checked_add(vlen)?;
            f(&key, b.get(kend..vend)?);
            i = vend;
        }
        Some(())
    }

    pub(super) fn table(file: &[u8], visit: &mut Visit<'_>) -> Option<()> {
        let footer = file.get(file.len().checked_sub(48)?..)?;
        if u64::from_le_bytes(footer.get(40..48)?.try_into().ok()?) != TABLE_MAGIC {
            return None;
        }
        let mut i = 0;
        let _meta_off = varint(footer, &mut i)?;
        let _meta_size = varint(footer, &mut i)?;
        let index_off = varint(footer, &mut i)?;
        let index_size = varint(footer, &mut i)?;
        let index = block(file, index_off, index_size)?;
        let mut handles = Vec::new();
        entries(&index, &mut |_, h| handles.push(h.to_vec()))?;
        for h in handles {
            let mut j = 0;
            let (Some(off), Some(size)) = (varint(&h, &mut j), varint(&h, &mut j)) else { continue };
            let Some(data) = block(file, off, size) else { continue };
            let _ = entries(&data, &mut |k, v| {
                // Chiave interna = chiave utente + 8 byte (sequenza << 8 | tipo).
                let Some(split) = k.len().checked_sub(8) else { return };
                let (user, trailer) = k.split_at(split);
                let Ok(t) = <[u8; 8]>::try_from(trailer) else { return };
                let t = u64::from_le_bytes(t);
                visit(user, t >> 8, (t & 0xff == 1).then_some(v));
            });
        }
        Some(())
    }

    /// Log di scrittura: blocchi da 32 KiB, record con intestazione da 7 byte (crc, lunghezza, tipo).
    pub(super) fn log(file: &[u8], visit: &mut Visit<'_>) {
        let mut pending: Vec<u8> = Vec::new();
        let mut pos = 0usize;
        while pos + 7 <= file.len() {
            let left = LOG_BLOCK - pos % LOG_BLOCK;
            if left < 7 {
                pos += left;
                continue;
            }
            let len = u16::from_le_bytes([file[pos + 4], file[pos + 5]]) as usize;
            let kind = file[pos + 6];
            let start = pos + 7;
            if len > left - 7 || start + len > file.len() {
                break; // record troncato: Discord sta scrivendo
            }
            let data = &file[start..start + len];
            pos = start + len;
            match kind {
                1 => {
                    let _ = batch(data, visit);
                }
                2 => {
                    pending.clear();
                    pending.extend_from_slice(data);
                }
                3 => pending.extend_from_slice(data),
                4 => {
                    pending.extend_from_slice(data);
                    let _ = batch(&pending, visit);
                    pending.clear();
                }
                _ => break, // zeri in coda o dati non validi
            }
        }
    }

    fn batch(b: &[u8], visit: &mut Visit<'_>) -> Option<()> {
        let seq = u64::from_le_bytes(b.get(0..8)?.try_into().ok()?);
        let count = u32::from_le_bytes(b.get(8..12)?.try_into().ok()?);
        let mut i = 12;
        for n in 0..u64::from(count) {
            let kind = *b.get(i)?;
            i += 1;
            let klen = usize::try_from(varint(b, &mut i)?).ok()?;
            let kend = i.checked_add(klen)?;
            let key = b.get(i..kend)?;
            i = kend;
            match kind {
                1 => {
                    let vlen = usize::try_from(varint(b, &mut i)?).ok()?;
                    let vend = i.checked_add(vlen)?;
                    visit(key, seq + n, Some(b.get(i..vend)?));
                    i = vend;
                }
                0 => visit(key, seq + n, None),
                _ => return None,
            }
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"_https://discord.com\x00\x01MediaEngineStore";

    fn json(krisp: bool, input: &str) -> String {
        format!(
            r#"{{"default":{{"mode":"VOICE_ACTIVITY","modeOptions":{{"threshold":-60,"autoThreshold":true,"vadUseKrisp":true}},"echoCancellation":true,"noiseSuppression":false,"automaticGainControl":true,"noiseCancellation":{krisp},"inputDeviceId":"{input}"}},"stream":{{}}}}"#
        )
    }

    fn latin1(s: &str) -> Vec<u8> {
        std::iter::once(1).chain(s.bytes()).collect()
    }

    fn utf16(s: &str) -> Vec<u8> {
        std::iter::once(0).chain(s.encode_utf16().flat_map(u16::to_le_bytes)).collect()
    }

    fn put_varint(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    /// Blocco con una voce per chiave, senza prefissi condivisi.
    fn raw_block(items: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
        let mut b = Vec::new();
        for (k, v) in items {
            put_varint(&mut b, 0);
            put_varint(&mut b, k.len() as u64);
            put_varint(&mut b, v.len() as u64);
            b.extend_from_slice(k);
            b.extend_from_slice(v);
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b
    }

    /// Tabella LevelDB minima: un blocco dati compresso con Snappy + indice + footer.
    fn table_bytes(entries: &[(&[u8], u64, &[u8])]) -> Vec<u8> {
        let items: Vec<(Vec<u8>, Vec<u8>)> = entries
            .iter()
            .map(|(k, seq, v)| {
                let mut ik = k.to_vec();
                ik.extend_from_slice(&((seq << 8) | 1).to_le_bytes());
                (ik, v.to_vec())
            })
            .collect();
        let data = snap::raw::Encoder::new().compress_vec(&raw_block(&items)).unwrap();
        let mut file = data.clone();
        file.push(1);
        file.extend_from_slice(&[0; 4]);
        let mut handle = Vec::new();
        put_varint(&mut handle, 0);
        put_varint(&mut handle, data.len() as u64);
        let index = raw_block(&[(b"zzz".to_vec(), handle)]);
        let index_off = file.len() as u64;
        file.extend_from_slice(&index);
        file.push(0);
        file.extend_from_slice(&[0; 4]);
        let mut footer = Vec::new();
        put_varint(&mut footer, 0);
        put_varint(&mut footer, 0);
        put_varint(&mut footer, index_off);
        put_varint(&mut footer, index.len() as u64);
        footer.resize(40, 0);
        footer.extend_from_slice(&0xdb47_7524_8b80_fb57u64.to_le_bytes());
        file.extend_from_slice(&footer);
        file
    }

    fn log_bytes(seq: u64, key: &[u8], value: &[u8]) -> Vec<u8> {
        let mut batch = Vec::new();
        batch.extend_from_slice(&seq.to_le_bytes());
        batch.extend_from_slice(&1u32.to_le_bytes());
        batch.push(1);
        put_varint(&mut batch, key.len() as u64);
        batch.extend_from_slice(key);
        put_varint(&mut batch, value.len() as u64);
        batch.extend_from_slice(value);
        let mut rec = vec![0, 0, 0, 0];
        rec.extend_from_slice(&(batch.len() as u16).to_le_bytes());
        rec.push(1);
        rec.extend_from_slice(&batch);
        rec
    }

    fn temp_db() -> PathBuf {
        let d = std::env::temp_dir().join(format!("mfx-discord-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn rand_suffix() -> u128 {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn parses_real_shape() {
        let v = parse_media_engine(&json(true, "{0.0.1.00000000}.{cable}"), "Discord").unwrap();
        assert!(v.krisp && v.advanced_vad && v.echo_cancellation && v.automatic_gain_control && v.auto_sensitivity);
        assert!(!v.push_to_talk && !v.noise_suppression);
        assert_eq!(v.input_device_id, "{0.0.1.00000000}.{cable}");
        // Campi assenti: default di Discord (Krisp acceso).
        let d = parse_media_engine(r#"{"default":{"mode":"PUSH_TO_TALK"}}"#, "Discord").unwrap();
        assert!(d.push_to_talk && d.krisp);
        assert_eq!(d.input_device_id, "default");
        assert!(parse_media_engine("{non json", "Discord").is_none());
    }

    #[test]
    fn decodes_both_value_encodings() {
        assert_eq!(decode_value(&latin1("abc")).as_deref(), Some("abc"));
        assert_eq!(decode_value(&utf16("àé€")).as_deref(), Some("àé€"));
        assert_eq!(decode_value(&[]), None);
    }

    #[test]
    fn newest_record_wins_across_tables_and_log() {
        let db = temp_db();
        let old = latin1(&json(true, "default"));
        let other = latin1("{}");
        std::fs::write(
            db.join("000005.ldb"),
            table_bytes(&[
                (KEY, 70, old.as_slice()),
                (&b"_https://discord.com\x00\x01Altro"[..], 71, other.as_slice()),
            ]),
        )
        .unwrap();
        let (_, v) = read_from(&db, "Discord").unwrap();
        assert!(v.krisp);
        assert_eq!(v.input_device_id, "default");

        // L'utente spegne Krisp: il nuovo valore è nel log, con sequenza più alta.
        std::fs::write(db.join("000006.log"), log_bytes(900, KEY, &utf16(&json(false, "{cable}")))).unwrap();
        let (_, v) = read_from(&db, "Discord").unwrap();
        assert!(!v.krisp);
        assert_eq!(v.input_device_id, "{cable}");
        std::fs::remove_dir_all(db).ok();
    }

    #[test]
    fn torn_or_garbage_files_never_panic() {
        let db = temp_db();
        let good_value = latin1(&json(false, "default"));
        let good = table_bytes(&[(KEY, 10, good_value.as_slice())]);
        // Record più recente scritto a metà nel log: si usa quello precedente.
        let mut torn = log_bytes(20, KEY, &latin1(&json(true, "default")));
        torn.truncate(torn.len() - 30);
        std::fs::write(db.join("000001.ldb"), &good).unwrap();
        std::fs::write(db.join("000002.log"), &torn).unwrap();
        std::fs::write(db.join("000003.ldb"), &good[..good.len() / 2]).unwrap();
        std::fs::write(db.join("000004.ldb"), [0xffu8; 100]).unwrap();
        let (_, v) = read_from(&db, "Discord").unwrap();
        assert!(!v.krisp);
        for cut in 0..good.len() {
            let mut sink = |_: &[u8], _: u64, _: Option<&[u8]>| {};
            let _ = leveldb::table(&good[..cut], &mut sink);
            leveldb::log(&torn[..cut.min(torn.len())], &mut sink);
        }
        std::fs::remove_dir_all(db).ok();
    }

    /// Sul PC reale: `cargo test -p mfx-platform real_discord -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_discord_settings() {
        println!("{:?}", read_voice_settings());
    }
}
