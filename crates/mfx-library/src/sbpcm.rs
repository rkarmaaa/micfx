//! Formato `.sbpcm` (SPEC §7.2): header di 64 byte little-endian, poi i16 interleaved.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use memmap2::Mmap;

pub const MAGIC: [u8; 8] = *b"SBPCM01\0";
pub const VERSION: u16 = 1;
pub const HEADER_LEN: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum SbPcmError {
    #[error("header non valido: {0}")]
    InvalidHeader(&'static str),
    #[error("file troncato: attesi {expected} byte, presenti {actual}")]
    Truncated { expected: u64, actual: u64 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbPcmHeader {
    pub channels: u16,
    pub sample_rate: u32,
    pub frames: u64,
}

impl SbPcmHeader {
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        h[0..8].copy_from_slice(&MAGIC);
        h[8..10].copy_from_slice(&VERSION.to_le_bytes());
        h[10..12].copy_from_slice(&self.channels.to_le_bytes());
        h[12..16].copy_from_slice(&self.sample_rate.to_le_bytes());
        h[16..24].copy_from_slice(&self.frames.to_le_bytes());
        // 24..28 flags = 0, 28..64 reserved = 0
        h
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SbPcmError> {
        if bytes.len() < HEADER_LEN {
            return Err(SbPcmError::InvalidHeader("meno di 64 byte"));
        }
        if bytes[0..8] != MAGIC {
            return Err(SbPcmError::InvalidHeader("magic errato"));
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != VERSION {
            return Err(SbPcmError::InvalidHeader("versione non supportata"));
        }
        let channels = u16::from_le_bytes([bytes[10], bytes[11]]);
        if !(1..=2).contains(&channels) {
            return Err(SbPcmError::InvalidHeader("canali non validi"));
        }
        let sample_rate = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        if sample_rate == 0 {
            return Err(SbPcmError::InvalidHeader("sample rate nullo"));
        }
        let frames = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        Ok(Self { channels, sample_rate, frames })
    }

    pub fn data_len_bytes(&self) -> u64 {
        self.frames * self.channels as u64 * 2
    }
}

/// Scrive un `.sbpcm` su file temporaneo e lo rinomina: un crash non lascia file a metà.
pub fn write_file(path: &Path, channels: u16, sample_rate: u32, samples: &[i16]) -> Result<(), SbPcmError> {
    assert!(channels == 1 || channels == 2);
    let frames = (samples.len() / channels as usize) as u64;
    let header = SbPcmHeader { channels, sample_rate, frames };
    let tmp = path.with_extension("sbpcm.tmp");
    {
        let mut f = std::io::BufWriter::with_capacity(1 << 20, File::create(&tmp)?);
        f.write_all(&header.encode())?;
        // Little-endian su disco; su x86/ARM Windows è l'ordine nativo.
        let mut chunk = Vec::with_capacity(64 * 1024);
        for block in samples.chunks(32 * 1024) {
            chunk.clear();
            for s in block {
                chunk.extend_from_slice(&s.to_le_bytes());
            }
            f.write_all(&chunk)?;
        }
        let f = f.into_inner().map_err(|e| e.into_error())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// File `.sbpcm` mappato in memoria. La cache la gestisce Windows (SPEC §7.3).
pub struct SbPcmFile {
    mmap: Mmap,
    header: SbPcmHeader,
}

impl SbPcmFile {
    pub fn open(path: &Path) -> Result<Self, SbPcmError> {
        let file = File::open(path)?;
        // SAFETY: il file lo scrive solo MicFX con rename atomico; nessuno lo tronca mentre è mappato.
        let mmap = unsafe { Mmap::map(&file)? };
        let header = SbPcmHeader::decode(&mmap)?;
        let expected = HEADER_LEN as u64 + header.data_len_bytes();
        if (mmap.len() as u64) < expected {
            return Err(SbPcmError::Truncated { expected, actual: mmap.len() as u64 });
        }
        Ok(Self { mmap, header })
    }

    pub fn header(&self) -> SbPcmHeader {
        self.header
    }

    /// Campioni interleaved. La mappatura parte allineata a pagina e i dati a 64 byte.
    #[cfg(target_endian = "little")]
    pub fn samples(&self) -> &[i16] {
        let data = &self.mmap[HEADER_LEN..HEADER_LEN + self.header.data_len_bytes() as usize];
        debug_assert_eq!(data.as_ptr() as usize % 2, 0);
        // SAFETY: allineamento verificato sopra, lunghezza pari, i16 accetta ogni pattern di bit.
        unsafe { std::slice::from_raw_parts(data.as_ptr() as *const i16, data.len() / 2) }
    }

    /// Chiede a Windows di portare in RAM le prime pagine (SPEC §7.3, opzionale).
    #[cfg(windows)]
    pub fn prefetch_head(&self, bytes: usize) {
        use windows::Win32::System::Memory::{PrefetchVirtualMemory, WIN32_MEMORY_RANGE_ENTRY};
        use windows::Win32::System::Threading::GetCurrentProcess;
        let entry = WIN32_MEMORY_RANGE_ENTRY {
            VirtualAddress: self.mmap.as_ptr() as *mut _,
            NumberOfBytes: bytes.min(self.mmap.len()),
        };
        // SAFETY: intervallo interno alla mappatura viva. È solo un suggerimento al gestore memoria.
        let _ = unsafe { PrefetchVirtualMemory(GetCurrentProcess(), &[entry], 0) };
    }

    #[cfg(not(windows))]
    pub fn prefetch_head(&self, _bytes: usize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mfx-sbpcm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn round_trip() {
        let dir = tmpdir();
        let path = dir.join("a.sbpcm");
        let samples: Vec<i16> = (0..2000).map(|i| (i * 7 - 3000) as i16).collect();
        write_file(&path, 2, 48_000, &samples).unwrap();
        let f = SbPcmFile::open(&path).unwrap();
        assert_eq!(f.header(), SbPcmHeader { channels: 2, sample_rate: 48_000, frames: 1000 });
        assert_eq!(f.samples(), &samples[..]);
        let len = std::fs::metadata(&path).unwrap().len();
        assert_eq!(len, 64 + 4000);
        drop(f);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rejects_bad_headers() {
        let good = SbPcmHeader { channels: 1, sample_rate: 48_000, frames: 10 }.encode();
        assert!(SbPcmHeader::decode(&good).is_ok());
        assert!(SbPcmHeader::decode(&good[..10]).is_err());
        let mut bad = good;
        bad[0] = b'X';
        assert!(SbPcmHeader::decode(&bad).is_err());
        let mut bad = good;
        bad[10] = 3;
        assert!(SbPcmHeader::decode(&bad).is_err());
        let mut bad = good;
        bad[8] = 9;
        assert!(SbPcmHeader::decode(&bad).is_err());
    }

    #[test]
    fn detects_truncation() {
        let dir = tmpdir();
        let path = dir.join("t.sbpcm");
        let mut bytes = SbPcmHeader { channels: 2, sample_rate: 48_000, frames: 100 }.encode().to_vec();
        bytes.extend_from_slice(&[0u8; 10]);
        std::fs::write(&path, bytes).unwrap();
        assert!(matches!(SbPcmFile::open(&path), Err(SbPcmError::Truncated { .. })));
        std::fs::remove_dir_all(dir).ok();
    }
}
