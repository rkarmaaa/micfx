//! Pipeline di import (SPEC §7.1). Gira in un thread worker, mai a runtime.
//!
//! decodifica → canali → resampling 48 kHz → trim silenzio → loudness EBU R128 → i16 TPDF → `.sbpcm`

use std::fs::File;
use std::io::Read;
use std::path::Path;

use ebur128::{EbuR128, Mode};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::packet::Packet;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::SAMPLE_RATE;
use crate::sbpcm;

#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub target_lufs: f64,
    pub ceiling_dbtp: f64,
    pub trim_silence: bool,
    pub max_duration_s: u32,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self { target_lufs: -18.0, ceiling_dbtp: -1.0, trim_silence: true, max_duration_s: 600 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedAudio {
    pub channels: u16,
    pub frames: u64,
    pub loudness_gain_db: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("formato non supportato")]
    Unsupported,
    #[error("decodifica fallita: {0}")]
    DecodeFailed(String),
    #[error("file troppo lungo (oltre {max_s} s)")]
    TooLong { max_s: u32 },
    #[error("file vuoto o senza audio")]
    Empty,
    #[error("disco pieno")]
    DiskFull,
    #[error("file non trovato")]
    NotFound,
    #[error("errore di I/O: {0}")]
    Io(std::io::Error),
}

impl From<std::io::Error> for ImportError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::StorageFull => ImportError::DiskFull,
            std::io::ErrorKind::NotFound => ImportError::NotFound,
            _ => ImportError::Io(e),
        }
    }
}

impl From<sbpcm::SbPcmError> for ImportError {
    fn from(e: sbpcm::SbPcmError) -> Self {
        match e {
            sbpcm::SbPcmError::Io(io) => io.into(),
            other => ImportError::DecodeFailed(other.to_string()),
        }
    }
}

/// Nome leggibile dal nome del file: `airhorn_loud-2.mp3` → `airhorn loud-2`.
pub fn name_from_path(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let cleaned: String = stem.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    let mut name: String = cleaned.chars().take(80).collect();
    if name.is_empty() {
        name = "Suono".to_string();
    }
    name
}

/// Hash xxh3 a 64 bit del file sorgente, per riconoscere i duplicati.
pub fn hash_file(path: &Path) -> Result<String, ImportError> {
    let mut f = File::open(path)?;
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:016x}", h.digest()))
}

/// Importa `src` e scrive il risultato in `dst` (`.sbpcm`).
pub fn import_to(src: &Path, dst: &Path, opts: &ImportOptions) -> Result<ImportedAudio, ImportError> {
    let decoded = decode(src, opts.max_duration_s)?;
    let pcm = process(decoded, opts)?;
    sbpcm::write_file(dst, pcm.channels, SAMPLE_RATE, &pcm.samples)?;
    Ok(ImportedAudio {
        channels: pcm.channels,
        frames: (pcm.samples.len() / pcm.channels as usize) as u64,
        loudness_gain_db: pcm.gain_db,
    })
}

pub(crate) struct Decoded {
    pub samples: Vec<f32>,
    pub channels: usize,
    pub rate: u32,
}

pub(crate) struct Processed {
    pub samples: Vec<i16>,
    pub channels: u16,
    pub gain_db: f32,
}

fn decode(path: &Path, max_duration_s: u32) -> Result<Decoded, ImportError> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| match e {
            SymError::IoError(io) if io.kind() != std::io::ErrorKind::UnexpectedEof => ImportError::from(io),
            _ => ImportError::Unsupported,
        })?;

    let (track_id, mut decoder) = {
        let track = format.default_track(TrackType::Audio).ok_or(ImportError::Unsupported)?;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or(ImportError::Unsupported)?;
        if params.codec == CODEC_ID_OPUS {
            // Ogg Opus (.ogg del soundboard di Discord, .opus): Symphonia separa i pacchetti, li decodifica
            // opus-decoder.
            let head = params.extra_data.as_deref().and_then(OpusHead::parse).ok_or(ImportError::Unsupported)?;
            let id = track.id;
            return decode_opus(&mut *format, id, &head, path, max_duration_s);
        }
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(|_| ImportError::Unsupported)?;
        (track.id, decoder)
    };

    let mut samples: Vec<f32> = Vec::new();
    let mut tmp: Vec<f32> = Vec::new();
    let mut channels = 0usize;
    let mut rate = 0u32;
    let mut decode_errors = 0u32;

    while let Some(packet) = next_packet(&mut *format, path, !samples.is_empty())? {
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = buf.spec();
                let ch = spec.channels().count();
                if channels == 0 {
                    channels = ch;
                    rate = spec.rate();
                    if channels == 0 || rate == 0 {
                        return Err(ImportError::Unsupported);
                    }
                } else if ch != channels || spec.rate() != rate {
                    // Cambio di formato a metà stream: raro, si ignorano i pacchetti incoerenti.
                    continue;
                }
                tmp.resize(buf.samples_interleaved(), 0.0);
                buf.copy_to_slice_interleaved::<f32, _>(&mut tmp[..]);
                samples.extend_from_slice(&tmp);
                let frames = samples.len() / channels;
                if frames as u64 > max_duration_s as u64 * rate as u64 {
                    return Err(ImportError::TooLong { max_s: max_duration_s });
                }
            }
            Err(SymError::DecodeError(msg)) => {
                decode_errors += 1;
                if decode_errors > 64 && samples.is_empty() {
                    return Err(ImportError::DecodeFailed(msg.to_string()));
                }
            }
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                if samples.is_empty() {
                    return Err(ImportError::DecodeFailed(e.to_string()));
                }
                break;
            }
        }
    }
    if channels == 0 || samples.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(Decoded { samples, channels, rate })
}

/// Prossimo pacchetto del file; `None` a fine file o se la lettura si interrompe dopo che qualcosa è già
/// stato decodificato (si tiene quello che c'è).
fn next_packet(format: &mut dyn FormatReader, path: &Path, have_audio: bool) -> Result<Option<Packet>, ImportError> {
    match format.next_packet() {
        Ok(p) => Ok(p),
        Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(SymError::ResetRequired) => Ok(None),
        Err(e) if !have_audio => Err(ImportError::DecodeFailed(e.to_string())),
        Err(e) => {
            tracing::warn!("{}: lettura interrotta: {e}", path.display());
            Ok(None)
        }
    }
}

/// Intestazione `OpusHead` di Ogg Opus (RFC 7845 §5.1).
struct OpusHead {
    channels: usize,
    pre_skip: usize,
    gain_db: f32,
    /// Famiglia di mappatura ≠ 0 (più di due canali).
    multistream: Option<opus_rs::multistream::ChannelMappingTable>,
}

impl OpusHead {
    fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 19 || &b[..8] != b"OpusHead" {
            return None;
        }
        let channels = b[9] as usize;
        let pre_skip = u16::from_le_bytes([b[10], b[11]]) as usize;
        let gain_db = i16::from_le_bytes([b[16], b[17]]) as f32 / 256.0;
        let multistream = match b[18] {
            0 if (1..=2).contains(&channels) => None,
            0 => return None,
            family => Some(opus_rs::multistream::ChannelMappingTable::parse(family, b[9], &b[19..])?),
        };
        (channels > 0).then_some(Self { channels, pre_skip, gain_db, multistream })
    }
}

enum OpusDec {
    Single(Box<opus_rs::OpusDecoder>),
    Multi(opus_rs::multistream::MultistreamDecoder),
}

impl OpusDec {
    /// `frame_size`: capacità di `out` per canale. Restituisce i campioni decodificati per canale.
    fn decode(&mut self, packet: &[u8], frame_size: usize, out: &mut [f32]) -> Result<usize, &'static str> {
        match self {
            OpusDec::Single(d) => d.decode(packet, frame_size, out),
            OpusDec::Multi(d) => d.decode(packet, frame_size, out),
        }
    }
}

fn decode_opus(
    format: &mut dyn FormatReader,
    track_id: u32,
    head: &OpusHead,
    path: &Path,
    max_duration_s: u32,
) -> Result<Decoded, ImportError> {
    // Opus decodifica sempre a 48 kHz: niente resampling dopo.
    const RATE: u32 = 48_000;
    let ch = head.channels;
    let mut dec = match &head.multistream {
        None => opus_rs::OpusDecoder::new(RATE as i32, ch).map(|d| OpusDec::Single(Box::new(d))),
        Some(t) => opus_rs::multistream::MultistreamDecoder::new(RATE as i32, t.clone()).map(OpusDec::Multi),
    }
    .map_err(|_| ImportError::Unsupported)?;

    // 120 ms, il pacchetto Opus più lungo.
    const MAX_FRAME: usize = 5760;
    let mut buf = vec![0f32; MAX_FRAME * ch];
    let mut samples: Vec<f32> = Vec::new();
    let mut skip = head.pre_skip;
    let mut decode_errors = 0u32;
    while let Some(packet) = next_packet(format, path, !samples.is_empty())? {
        if packet.track_id != track_id {
            continue;
        }
        let n = match dec.decode(&packet.data, MAX_FRAME, &mut buf) {
            Ok(n) => n.min(buf.len() / ch),
            Err(e) => {
                decode_errors += 1;
                if decode_errors > 64 && samples.is_empty() {
                    return Err(ImportError::DecodeFailed(e.to_string()));
                }
                continue;
            }
        };
        // Pre-skip dell'encoder all'inizio, padding dell'ultima pagina (granule) alla fine.
        let start = skip.min(n);
        skip -= start;
        let end = n.saturating_sub(packet.trim_end.get() as usize).max(start);
        samples.extend_from_slice(&buf[start * ch..end * ch]);
        if (samples.len() / ch) as u64 > max_duration_s as u64 * RATE as u64 {
            return Err(ImportError::TooLong { max_s: max_duration_s });
        }
    }
    if samples.is_empty() {
        return Err(ImportError::Empty);
    }
    if head.gain_db != 0.0 {
        let k = db_to_lin(head.gain_db);
        samples.iter_mut().for_each(|s| *s *= k);
    }
    Ok(Decoded { samples, channels: ch, rate: RATE })
}

pub(crate) fn process(d: Decoded, opts: &ImportOptions) -> Result<Processed, ImportError> {
    // 1. Canali: mono resta mono, stereo resta stereo, oltre 2 → downmix stereo.
    let (mut samples, channels) = match d.channels {
        1 | 2 => (d.samples, d.channels),
        n => (downmix_to_stereo(&d.samples, n), 2),
    };

    // 2. Resampling a 48 kHz solo se serve.
    if d.rate != SAMPLE_RATE {
        samples = resample(&samples, channels, d.rate, SAMPLE_RATE)?;
    }

    // 3. Taglio del silenzio iniziale e finale sotto −50 dBFS, lasciando 5 ms.
    if opts.trim_silence {
        samples = trim_silence(samples, channels, db_to_lin(-50.0), (SAMPLE_RATE as usize * 5) / 1000);
    }
    if samples.is_empty() {
        return Err(ImportError::Empty);
    }

    // 4. Il file salvato non deve clippare: se il decoder supera 0 dBFS si scala prima di misurare.
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 1.0 {
        let k = 0.999 / peak;
        samples.iter_mut().for_each(|s| *s *= k);
    }

    // 5. Loudness EBU R128 → gain come metadato (il file non viene riscritto se cambia).
    let gain_db = loudness_gain_db(&samples, channels, opts);

    // 6. i16 con dither TPDF, seed fisso per test riproducibili.
    let pcm = to_i16_tpdf(&samples, 0x9E37_79B9);
    Ok(Processed { samples: pcm, channels: channels as u16, gain_db })
}

fn downmix_to_stereo(src: &[f32], n: usize) -> Vec<f32> {
    let frames = src.len() / n;
    let mut out = Vec::with_capacity(frames * 2);
    // Canali pari a sinistra, dispari a destra; i canali oltre i primi due a −6 dB.
    for f in src.chunks_exact(n) {
        let (mut l, mut r) = (f[0], f[1]);
        for (i, s) in f.iter().enumerate().skip(2) {
            if i % 2 == 0 {
                l += 0.5 * s;
            } else {
                r += 0.5 * s;
            }
        }
        out.push(l);
        out.push(r);
    }
    out
}

fn resample(samples: &[f32], channels: usize, from: u32, to: u32) -> Result<Vec<f32>, ImportError> {
    use rubato::audioadapter_buffers::direct::InterleavedSlice;
    use rubato::{Fft, FixedSync, Resampler};
    let frames = samples.len() / channels;
    let input = InterleavedSlice::new(samples, channels, frames)
        .map_err(|e| ImportError::DecodeFailed(format!("resampling: {e}")))?;
    let mut rs = Fft::<f32>::new(from as usize, to as usize, 1024, channels, FixedSync::Input)
        .map_err(|e| ImportError::DecodeFailed(format!("resampling: {e}")))?;
    let out =
        rs.process_all(&input, frames, None).map_err(|e| ImportError::DecodeFailed(format!("resampling: {e}")))?;
    Ok(out.take_data())
}

fn trim_silence(samples: Vec<f32>, channels: usize, threshold: f32, keep_frames: usize) -> Vec<f32> {
    let frames = samples.len() / channels;
    let loud = |f: usize| samples[f * channels..(f + 1) * channels].iter().any(|s| s.abs() > threshold);
    let Some(first) = (0..frames).find(|&f| loud(f)) else {
        // Tutto sotto soglia: si conserva com'è invece di produrre un file vuoto.
        return samples;
    };
    let last = (0..frames).rev().find(|&f| loud(f)).unwrap_or(first);
    let start = first.saturating_sub(keep_frames);
    let end = (last + 1 + keep_frames).min(frames);
    if start == 0 && end == frames {
        return samples;
    }
    samples[start * channels..end * channels].to_vec()
}

pub(crate) fn loudness_gain_db(samples: &[f32], channels: usize, opts: &ImportOptions) -> f32 {
    let measured = measure_loudness(samples, channels);
    let Some(lufs) = measured.loudness else { return 0.0 };
    let mut gain = opts.target_lufs - lufs;
    if measured.true_peak > 0.0 {
        let tp_db = 20.0 * measured.true_peak.log10();
        gain = gain.min(opts.ceiling_dbtp - tp_db);
    }
    gain.clamp(-40.0, 30.0) as f32
}

pub(crate) struct Measured {
    /// Loudness integrata, o momentanea massima sotto i 400 ms, o RMS come ultima risorsa.
    pub loudness: Option<f64>,
    /// True peak lineare (massimo tra i canali).
    pub true_peak: f64,
}

pub(crate) fn measure_loudness(samples: &[f32], channels: usize) -> Measured {
    let rms_fallback = || {
        let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum / samples.len().max(1) as f64).sqrt();
        (rms > 1e-6).then(|| 20.0 * rms.log10())
    };
    let peak_fallback = || samples.iter().fold(0.0f64, |m, s| m.max(s.abs() as f64));
    let Ok(mut ebu) = EbuR128::new(channels as u32, SAMPLE_RATE, Mode::I | Mode::M | Mode::TRUE_PEAK) else {
        return Measured { loudness: rms_fallback(), true_peak: peak_fallback() };
    };
    let mut max_momentary = f64::NEG_INFINITY;
    for chunk in samples.chunks(4800 * channels) {
        if ebu.add_frames_f32(chunk).is_err() {
            return Measured { loudness: rms_fallback(), true_peak: peak_fallback() };
        }
        if let Ok(m) = ebu.loudness_momentary()
            && m.is_finite()
        {
            max_momentary = max_momentary.max(m);
        }
    }
    let frames = samples.len() / channels;
    let integrated = ebu.loudness_global().ok().filter(|v| v.is_finite());
    let loudness = if frames >= SAMPLE_RATE as usize * 400 / 1000 && integrated.is_some() {
        integrated
    } else if max_momentary.is_finite() {
        Some(max_momentary)
    } else {
        integrated.or_else(rms_fallback)
    };
    let true_peak =
        (0..channels as u32).filter_map(|c| ebu.true_peak(c).ok()).fold(0.0f64, f64::max).max(peak_fallback());
    Measured { loudness, true_peak }
}

fn to_i16_tpdf(samples: &[f32], seed: u32) -> Vec<i16> {
    let mut state = seed.max(1);
    let mut rnd = move || {
        // xorshift32 → [0, 1)
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f32 / 4_294_967_296.0
    };
    samples
        .iter()
        .map(|&s| {
            let tpdf = rnd() - rnd(); // triangolare in (−1, 1) LSB
            let v = (s * 32767.0 + tpdf).round();
            v.clamp(-32768.0, 32767.0) as i16
        })
        .collect()
}

pub(crate) fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, secs: f32, amp: f32, channels: usize) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        let mut v = Vec::with_capacity(n * channels);
        for i in 0..n {
            let s = amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin();
            for _ in 0..channels {
                v.push(s);
            }
        }
        v
    }

    fn opts() -> ImportOptions {
        ImportOptions::default()
    }

    #[test]
    fn loudness_gain_hits_target() {
        // Sinusoide 1 kHz a −20 dBFS di picco, stereo, 3 s: −20 LUFS (i due canali si sommano) → +2 dB verso −18.
        let s = sine(1000.0, 48_000, 3.0, db_to_lin(-20.0), 2);
        let g = loudness_gain_db(&s, 2, &opts());
        assert!((g - 2.0).abs() < 0.3, "gain {g}");
    }

    #[test]
    fn gain_limited_by_true_peak() {
        // Sinusoide quasi a fondo scala: il ceiling −1 dBTP impedisce il guadagno positivo.
        let mut o = opts();
        o.target_lufs = 0.0;
        let s = sine(1000.0, 48_000, 2.0, 0.9, 1);
        let g = loudness_gain_db(&s, 1, &o);
        let tp_db = 20.0 * 0.9f32.log10();
        assert!(g <= -1.0 - tp_db + 0.3, "gain {g}");
    }

    #[test]
    fn short_sounds_use_momentary() {
        let s = sine(1000.0, 48_000, 0.2, db_to_lin(-20.0), 1);
        let m = measure_loudness(&s, 1);
        assert!(m.loudness.is_some());
        let g = loudness_gain_db(&s, 1, &opts());
        assert!(g.is_finite() && g.abs() < 30.0);
    }

    #[test]
    fn silence_gets_zero_gain() {
        let s = vec![0.0f32; 48_000];
        assert_eq!(loudness_gain_db(&s, 1, &opts()), 0.0);
    }

    #[test]
    fn trims_leading_and_trailing_silence() {
        let mut s = vec![0.0f32; 48_000];
        s.extend(sine(440.0, 48_000, 0.5, 0.5, 1));
        s.extend(vec![0.0f32; 24_000]);
        let out = trim_silence(s, 1, db_to_lin(-50.0), 240);
        // 0,5 s di segnale + al massimo 2 × 5 ms di margine
        assert!(out.len() <= 24_000 + 480 && out.len() >= 23_900, "{}", out.len());
    }

    #[test]
    fn resamples_44k1_to_48k() {
        let s = sine(1000.0, 44_100, 1.0, 0.5, 2);
        let out = resample(&s, 2, 44_100, 48_000).unwrap();
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() <= 2, "frames {frames}");
    }

    #[test]
    fn downmix_keeps_two_channels() {
        let s = vec![1.0, 0.0, 0.5, 0.5, 0.0, 0.0]; // 1 frame 6 canali
        assert_eq!(downmix_to_stereo(&s, 6), vec![1.25, 0.25]);
    }

    #[test]
    fn dither_is_deterministic_and_bounded() {
        let s = vec![1.0f32, -1.0, 0.0, 0.25];
        let a = to_i16_tpdf(&s, 7);
        assert_eq!(a, to_i16_tpdf(&s, 7));
        assert!(a[0] >= 32766 && a[1] <= -32766 && a[2].abs() <= 1);
    }

    #[test]
    fn names_from_paths() {
        assert_eq!(name_from_path(Path::new("C:/x/air_horn  loud.mp3")), "air horn loud");
        assert_eq!(name_from_path(Path::new("C:/x/___.wav")), "Suono");
    }

    #[test]
    fn imports_a_wav_end_to_end() {
        let dir = std::env::temp_dir().join(format!("mfx-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tone.wav");
        write_test_wav(&src, &sine(440.0, 44_100, 1.0, 0.3, 2), 2, 44_100);
        let dst = dir.join("out.sbpcm");
        let r = import_to(&src, &dst, &opts()).unwrap();
        assert_eq!(r.channels, 2);
        assert!((r.frames as i64 - 48_000).abs() < 600, "frames {}", r.frames);
        let f = sbpcm::SbPcmFile::open(&dst).unwrap();
        assert_eq!(f.header().frames, r.frames);
        assert!(hash_file(&src).unwrap().len() == 16);
        drop(f);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn imports_ogg_opus() {
        // Come i .ogg del soundboard di Discord: Opus in Ogg. 0,5 s di 440 Hz stereo (ffmpeg, libopus 24k).
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/tone-opus.ogg");
        let d = decode(&src, 600).unwrap();
        assert_eq!((d.channels, d.rate), (2, 48_000));
        // Il pre-skip (312 campioni) è tolto: restano 0,5 s esatti, salvo il padding dell'ultima pagina.
        let frames = d.samples.len() / 2;
        assert!((frames as i64 - 24_000).abs() < 480, "frames {frames}");
        let peak = d.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        // ffmpeg (volumedetect) sullo stesso file: picco −20,7 dBFS.
        let peak_db = 20.0 * peak.log10();
        assert!((peak_db + 20.7).abs() < 0.5, "picco {peak_db} dB");

        let dir = std::env::temp_dir().join(format!("mfx-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = import_to(&src, &dir.join("o.sbpcm"), &opts()).unwrap();
        assert_eq!(r.channels, 2);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn opus_head_parsing() {
        let mut h = b"OpusHead\x01\x02\x38\x01\x80\xbb\x00\x00\x00\x01\x00".to_vec();
        let p = OpusHead::parse(&h).unwrap();
        assert_eq!((p.channels, p.pre_skip, p.gain_db), (2, 312, 1.0));
        assert!(p.multistream.is_none());
        h[9] = 6; // famiglia 0 con 6 canali: non valida
        assert!(OpusHead::parse(&h).is_none());
        h[18] = 1;
        h.extend_from_slice(&[4, 2, 0, 4, 1, 2, 3, 5]);
        let t = OpusHead::parse(&h).unwrap().multistream.unwrap();
        assert_eq!((t.stream_count, t.coupled_count, t.mapping), (4, 2, vec![0, 4, 1, 2, 3, 5]));
        assert!(OpusHead::parse(b"OggS....").is_none());
    }

    #[test]
    fn rejects_non_audio() {
        let dir = std::env::temp_dir().join(format!("mfx-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("fake.mp3");
        std::fs::write(&src, b"non sono un mp3").unwrap();
        let err = import_to(&src, &dir.join("o.sbpcm"), &opts()).unwrap_err();
        assert!(matches!(err, ImportError::Unsupported | ImportError::DecodeFailed(_) | ImportError::Empty), "{err:?}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_too_long() {
        let dir = std::env::temp_dir().join(format!("mfx-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("long.wav");
        write_test_wav(&src, &sine(440.0, 8000, 3.0, 0.3, 1), 1, 8000);
        let mut o = opts();
        o.max_duration_s = 1;
        let err = import_to(&src, &dir.join("o.sbpcm"), &o).unwrap_err();
        assert!(matches!(err, ImportError::TooLong { .. }), "{err:?}");
        std::fs::remove_dir_all(dir).ok();
    }

    fn write_test_wav(path: &Path, samples: &[f32], channels: u16, rate: u32) {
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s * 32767.0) as i16).to_le_bytes()).collect();
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&channels.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        w.extend_from_slice(&(channels * 2).to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(&data);
        std::fs::write(path, w).unwrap();
    }
}
