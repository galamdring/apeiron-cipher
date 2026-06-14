//! Write-ahead log (WAL) — append-only bincode journal for world mutations.
//!
//! # On-disk format
//!
//! The WAL is stored as a sequence of *segment files* named
//! `wal_NNNN.bin` (zero-padded four-digit index: `wal_0001.bin`,
//! `wal_0002.bin`, …).  Each segment is a linear sequence of fixed-header
//! entry frames:
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  Magic cookie   │0x57414C31 ("WAL1") — 4 bytes LE               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Sequence number               │ u64 LE (8 bytes)               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Timestamp (Unix µs)           │ u64 LE (8 bytes)               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Payload length                │ u32 LE (4 bytes)               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  CRC-32 (magic+seq+ts+len+payload)│ u32 LE (4 bytes)            │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  Payload bytes (bincode-encoded WorldMutation)                  │
//! │  … `payload_len` bytes …                                        │
//! └─────────────────────────────────────────────────────────────────┘
//!
//! Total fixed header: 4 + 8 + 8 + 4 + 4 = 28 bytes.
//! ```
//!
//! ## Torn-entry safety
//!
//! A `kill -9` or power loss can truncate any byte inside a frame.
//! Recovery reads entries in order.  The first frame that fails any of the
//! following checks is declared **torn** and discarded; all prior entries are
//! durable:
//!
//! 1. Fewer than `FRAME_HEADER_LEN` bytes remain → [`PersistenceError::TornEntry`].
//! 2. Magic bytes are wrong → [`PersistenceError::BadMagic`].
//! 3. Fewer than `payload_len` bytes follow the header → [`PersistenceError::TornEntry`].
//! 4. CRC-32 mismatch → [`PersistenceError::ChecksumMismatch`].
//!
//! ## fsync strategy
//!
//! The writer accumulates written bytes in a pending counter.  An
//! `fdatasync` is issued when *either* condition is met:
//!
//! - At least 1 KiB has been written since the last sync, **or**
//! - At least 1 ms has elapsed since the last sync.
//!
//! The timer is checked lazily on each [`WalWriter::append`] call; no
//! background thread is required.  Callers that need a guaranteed sync can
//! call [`WalWriter::flush`] explicitly (e.g. before suspending to a
//! snapshot).
//!
//! ## Segment rotation
//!
//! When the current segment reaches [`SEGMENT_ROTATE_BYTES`] (64 MiB) the
//! writer opens a new segment with an incremented index.  The old segment is
//! `fdatasync`-ed and closed before the new one opens.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::persistence::error::PersistenceError;
use crate::persistence::mutation::WorldMutation;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Magic cookie written at the start of every entry frame: ASCII "WAL1".
///
/// Four bytes in big-endian order so the file looks human-readable under
/// `xxd`: `57 41 4C 31`.
pub const FRAME_MAGIC: [u8; 4] = *b"WAL1";

/// Total size of the fixed entry frame header in bytes.
///
/// Layout: magic(4) + seq(8) + timestamp(8) + payload_len(4) + crc32(4) = 28.
pub const FRAME_HEADER_LEN: usize = 4 + 8 + 8 + 4 + 4;

/// Rotate to a new segment file after writing this many bytes.
///
/// 64 MiB keeps individual segment files small enough for fast sequential
/// reads at recovery time while still providing plenty of headroom between
/// rotations.
pub const SEGMENT_ROTATE_BYTES: u64 = 64 * 1024 * 1024;

/// Issue `fdatasync` after accumulating this many bytes since the last sync.
///
/// 1 KiB is conservative — most `fdatasync` implementations on NVMe flush in
/// well under 1 ms, so batching to 1 KiB keeps throughput high without
/// sacrificing durability guarantees.
pub const FSYNC_BYTE_THRESHOLD: u64 = 1024;

/// Issue `fdatasync` after this much wall-clock time since the last sync.
pub const FSYNC_TIME_THRESHOLD: Duration = Duration::from_millis(1);

// ── WalWriter ─────────────────────────────────────────────────────────────────

/// Append-only WAL writer.
///
/// Maintains one open segment file at a time and appends serialised
/// [`WorldMutation`] entries to it.  When the segment reaches
/// [`SEGMENT_ROTATE_BYTES`] the writer transparently opens a new segment.
///
/// # Creating a writer
///
/// ```rust,ignore
/// let writer = WalWriter::open("/var/game/saves/wal")?;
/// ```
///
/// If the directory does not exist it is created.  If matching `wal_NNNN.bin`
/// files are already present, the writer resumes writing to the highest-indexed
/// segment (appending after its current end-of-file).
///
/// # Error handling
///
/// All I/O errors are wrapped in [`PersistenceError`].  A
/// [`PersistenceError::WalWrite`] from [`WalWriter::append`] means the
/// mutation was NOT durably committed.  The caller is responsible for deciding
/// whether to retry, surface a warning, or terminate the session.
pub struct WalWriter {
    /// Directory that holds the segment files.
    dir: PathBuf,
    /// Handle to the currently active segment file (write mode, append-only).
    file: File,
    /// Index of the currently active segment (`0001`, `0002`, …).
    segment_index: u32,
    /// Bytes written to the current segment since it was opened.
    segment_bytes: u64,
    /// Monotonically increasing sequence counter.  Starts at the last known
    /// sequence from a previously-written segment, or 0 for a fresh WAL.
    next_seq: u64,
    /// Bytes written since the last `fdatasync`.
    unflushed_bytes: u64,
    /// Wall-clock time of the last successful `fdatasync` (or writer open).
    last_sync: Instant,
}

impl WalWriter {
    /// Opens (or creates) a WAL in `dir`.
    ///
    /// If the directory is empty a fresh WAL starting at `wal_0001.bin` / seq 0
    /// is created.  If existing segments are present the writer appends to the
    /// highest-indexed one and picks up the sequence counter from the last
    /// successfully written entry in that segment.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::WalOpen`] if the directory cannot be created
    /// or the segment file cannot be opened.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(PersistenceError::WalOpen)?;

        // Find the highest-indexed segment already present.
        let (segment_index, existing_seq) = Self::find_latest_segment(&dir)?;

        let path = segment_path(&dir, segment_index);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(PersistenceError::WalOpen)?;

        let segment_bytes = file.metadata().map_err(PersistenceError::WalOpen)?.len();

        Ok(Self {
            dir,
            file,
            segment_index,
            segment_bytes,
            next_seq: existing_seq,
            unflushed_bytes: 0,
            last_sync: Instant::now(),
        })
    }

    /// Appends a serialised `WorldMutation` to the WAL and returns its
    /// assigned sequence number.
    ///
    /// The entry is written and the writer decides whether to issue an
    /// `fdatasync` based on the configured byte/time thresholds.  The entry
    /// is guaranteed to be *written* to the OS buffer; it is *durable* only
    /// after the next `fdatasync` (triggered automatically or via
    /// [`WalWriter::flush`]).
    ///
    /// # Errors
    ///
    /// - [`PersistenceError::Encode`] if bincode serialisation fails.
    /// - [`PersistenceError::WalWrite`] if an OS write or sync call fails.
    /// - [`PersistenceError::WalOpen`] if segment rotation fails to open the
    ///   new file.
    pub fn append(&mut self, mutation: &WorldMutation) -> Result<u64, PersistenceError> {
        // 1. Encode the payload.
        let payload = bincode::serde::encode_to_vec(mutation, bincode::config::standard())
            .map_err(PersistenceError::Encode)?;

        // 2. Assign sequence number and timestamp.
        let seq = self.next_seq;
        let ts_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_micros() as u64;

        // 3. Build the frame in a local buffer to issue a single write syscall.
        //    Layout: magic(4) + seq(8) + ts(8) + payload_len(4) + crc(4) + payload(N)
        let payload_len = payload.len() as u32;
        let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
        frame.extend_from_slice(&FRAME_MAGIC);
        frame.extend_from_slice(&seq.to_le_bytes());
        frame.extend_from_slice(&ts_us.to_le_bytes());
        frame.extend_from_slice(&payload_len.to_le_bytes());
        // CRC covers everything written so far (header bytes before the crc field)
        // plus the payload.
        let crc = {
            let mut h = crc32fast::Hasher::new();
            h.update(&frame); // magic + seq + ts + payload_len
            h.update(&payload);
            h.finalize()
        };
        frame.extend_from_slice(&crc.to_le_bytes());
        frame.extend_from_slice(&payload);

        // 4. Write the frame.
        self.file
            .write_all(&frame)
            .map_err(PersistenceError::WalWrite)?;

        let frame_len = frame.len() as u64;
        self.segment_bytes += frame_len;
        self.unflushed_bytes += frame_len;
        self.next_seq += 1;

        // 5. Conditionally fdatasync.
        let elapsed = self.last_sync.elapsed();
        if self.unflushed_bytes >= FSYNC_BYTE_THRESHOLD || elapsed >= FSYNC_TIME_THRESHOLD {
            self.fdatasync()?;
        }

        // 6. Rotate segment if over the size threshold (after sync, so the
        //    just-written entry is durable before we close the old segment).
        if self.segment_bytes >= SEGMENT_ROTATE_BYTES {
            self.rotate_segment()?;
        }

        Ok(seq)
    }

    /// Forces an `fdatasync` on the current segment file.
    ///
    /// Call this before suspending to a snapshot or when a guaranteed
    /// durability point is required.
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::WalWrite`] if the OS sync call fails.
    pub fn flush(&mut self) -> Result<(), PersistenceError> {
        self.fdatasync()
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Issues `fdatasync` (data-only; no metadata sync) and resets the
    /// unflushed-byte counter and sync timer.
    fn fdatasync(&mut self) -> Result<(), PersistenceError> {
        self.file.sync_data().map_err(PersistenceError::WalWrite)?;
        self.unflushed_bytes = 0;
        self.last_sync = Instant::now();
        Ok(())
    }

    /// Rotates to the next segment: syncs and closes the current file, then
    /// opens a new one.
    fn rotate_segment(&mut self) -> Result<(), PersistenceError> {
        // Ensure all bytes are durable before closing.
        self.fdatasync()?;

        self.segment_index += 1;
        let path = segment_path(&self.dir, self.segment_index);
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(PersistenceError::WalOpen)?;
        self.segment_bytes = 0;
        Ok(())
    }

    /// Scans `dir` for `wal_NNNN.bin` files and returns `(highest_index,
    /// next_seq)`.  `next_seq` is the sequence number *after* the last valid
    /// entry in the highest-indexed segment.  If no segments exist, returns
    /// `(1, 0)` to start fresh.
    fn find_latest_segment(dir: &Path) -> Result<(u32, u64), PersistenceError> {
        let mut max_index = 0u32;

        let entries = std::fs::read_dir(dir).map_err(PersistenceError::WalOpen)?;
        for entry in entries {
            let entry = entry.map_err(PersistenceError::WalOpen)?;
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if let Some(idx) = parse_segment_name(&s) {
                max_index = max_index.max(idx);
            }
        }

        if max_index == 0 {
            // No existing segments — start fresh.
            return Ok((1, 0));
        }

        // Scan the latest segment to find the last valid sequence.
        let path = segment_path(dir, max_index);
        let next_seq = scan_last_seq(&path)?;
        Ok((max_index, next_seq))
    }
}

// ── WalReader ─────────────────────────────────────────────────────────────────

/// Sequential reader over one or more WAL segment files.
///
/// Used during recovery to replay all durable mutations in the order they
/// were written.  Torn entries (the last, partially-written entry from a
/// `kill -9`) are silently discarded; all earlier entries are returned.
///
/// # Example
///
/// ```rust,ignore
/// let reader = WalReader::open("/var/game/saves/wal")?;
/// for result in reader {
///     match result {
///         Ok((_seq, mutation)) => apply(mutation),
///         Err(e) => eprintln!("recovery stopped: {e}"),
///     }
/// }
/// ```
pub struct WalReader {
    /// Sorted list of segment files to read, lowest index first.
    segments: Vec<PathBuf>,
    /// Index into `segments` of the currently open file.
    segment_cursor: usize,
    /// Handle to the currently open segment file.
    current: Option<File>,
    /// Current byte offset within the segment (for error reporting).
    offset: u64,
    /// Set to `true` once a torn or bad entry is encountered; stops iteration.
    done: bool,
}

impl WalReader {
    /// Opens a WAL directory for sequential reading.
    ///
    /// Segments are read in ascending index order.  If the directory is empty
    /// or contains no WAL segments, the reader returns immediately on the first
    /// `next()` call (producing no entries).
    ///
    /// # Errors
    ///
    /// Returns [`PersistenceError::WalOpen`] if the directory cannot be read.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let dir = dir.as_ref();
        let mut segments: Vec<PathBuf> = Vec::new();

        // Tolerate a non-existent directory (fresh install, no WAL yet).
        if dir.exists() {
            let entries = std::fs::read_dir(dir).map_err(PersistenceError::WalOpen)?;
            let mut indexed: Vec<(u32, PathBuf)> = Vec::new();
            for entry in entries {
                let entry = entry.map_err(PersistenceError::WalOpen)?;
                let name = entry.file_name();
                let s = name.to_string_lossy();
                if let Some(idx) = parse_segment_name(&s) {
                    indexed.push((idx, entry.path()));
                }
            }
            indexed.sort_by_key(|(idx, _)| *idx);
            segments = indexed.into_iter().map(|(_, p)| p).collect();
        }

        Ok(Self {
            segments,
            segment_cursor: 0,
            current: None,
            offset: 0,
            done: false,
        })
    }

    /// Reads the next valid entry from the WAL.
    ///
    /// Returns:
    /// - `Some(Ok((seq, mutation)))` for a good entry.
    /// - `Some(Err(PersistenceError::TornEntry { .. }))` for a torn final
    ///   entry; the reader is now exhausted.
    /// - `None` once all segments have been read or a torn entry was already
    ///   returned.
    pub fn next_entry(&mut self) -> Option<Result<(u64, WorldMutation), PersistenceError>> {
        if self.done {
            return None;
        }

        loop {
            // Ensure we have an open file.
            if self.current.is_none() {
                if self.segment_cursor >= self.segments.len() {
                    return None;
                }
                let path = &self.segments[self.segment_cursor];
                match File::open(path) {
                    Ok(f) => {
                        self.current = Some(f);
                        self.offset = 0;
                    }
                    Err(e) => {
                        self.done = true;
                        return Some(Err(PersistenceError::WalRead(e)));
                    }
                }
            }

            let file = self.current.as_mut().expect("checked above");
            match read_entry(file, &mut self.offset) {
                Ok(Some((seq, mutation))) => return Some(Ok((seq, mutation))),
                Ok(None) => {
                    // End of this segment — move on.
                    self.current = None;
                    self.segment_cursor += 1;
                    // Don't reset offset; each segment tracks its own.
                }
                Err(PersistenceError::TornEntry { seq }) => {
                    self.done = true;
                    return Some(Err(PersistenceError::TornEntry { seq }));
                }
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            }
        }
    }

    /// Drains all readable entries into a `Vec`, stopping cleanly at the first
    /// torn entry.
    ///
    /// Convenient for tests and for loading a save that may have been
    /// interrupted.
    ///
    /// # Errors
    ///
    /// Returns an error only for non-torn failures (bad magic, checksum
    /// mismatch, OS read error, decode error).  A torn entry at the tail
    /// is silently discarded and treated as a clean EOF.
    pub fn recover(mut self) -> Result<Vec<(u64, WorldMutation)>, PersistenceError> {
        let mut out = Vec::new();
        loop {
            match self.next_entry() {
                None => return Ok(out),
                // A torn tail entry is the expected kill-9 outcome — treat as EOF.
                Some(Err(PersistenceError::TornEntry { .. })) => return Ok(out),
                Some(Err(e)) => return Err(e),
                Some(Ok(entry)) => out.push(entry),
            }
        }
    }
}

// ── Frame I/O helpers ─────────────────────────────────────────────────────────

/// Reads one entry frame from `file`, advancing `offset`.
///
/// Returns:
/// - `Ok(Some((seq, mutation)))` for a complete valid entry.
/// - `Ok(None)` at clean end-of-file (zero bytes remaining).
/// - `Err(PersistenceError::TornEntry { .. })` if the file ends mid-frame.
/// - `Err(PersistenceError::BadMagic { .. })` for a corrupt magic cookie.
/// - `Err(PersistenceError::ChecksumMismatch { .. })` for a corrupt payload.
/// - `Err(PersistenceError::WalRead(_))` for OS I/O failures.
/// - `Err(PersistenceError::Decode(_))` if bincode decode fails.
fn read_entry(
    file: &mut File,
    offset: &mut u64,
) -> Result<Option<(u64, WorldMutation)>, PersistenceError> {
    // -- Read fixed header --
    let mut header = [0u8; FRAME_HEADER_LEN];
    let start_offset = *offset;

    match read_exact_or_eof(file, &mut header)? {
        ReadResult::Eof => return Ok(None),
        ReadResult::Partial => {
            return Err(PersistenceError::TornEntry { seq: None });
        }
        ReadResult::Full => {}
    }
    *offset += FRAME_HEADER_LEN as u64;

    // -- Validate magic --
    let magic: [u8; 4] = header[0..4].try_into().expect("slice is exactly 4");
    if magic != FRAME_MAGIC {
        return Err(PersistenceError::BadMagic {
            found: magic,
            offset: start_offset,
        });
    }

    // -- Parse fields --
    let seq = u64::from_le_bytes(header[4..12].try_into().expect("8 bytes"));
    // timestamp is parsed but not returned (used only to construct the mutation
    // in a future replay-with-time scenario; for now we just skip it).
    let _ts = u64::from_le_bytes(header[12..20].try_into().expect("8 bytes"));
    let payload_len = u32::from_le_bytes(header[20..24].try_into().expect("4 bytes")) as usize;
    let stored_crc = u32::from_le_bytes(header[24..28].try_into().expect("4 bytes"));

    // -- Read payload --
    let mut payload = vec![0u8; payload_len];
    match read_exact_or_eof(file, &mut payload)? {
        ReadResult::Eof | ReadResult::Partial => {
            return Err(PersistenceError::TornEntry { seq: Some(seq) });
        }
        ReadResult::Full => {}
    }
    *offset += payload_len as u64;

    // -- Verify CRC --
    // CRC covers: magic(4) + seq(8) + ts(8) + payload_len(4) + payload(N)
    // i.e. the header bytes BEFORE the crc field, plus the payload.
    let computed_crc = {
        let mut h = crc32fast::Hasher::new();
        h.update(&header[0..24]); // everything before the stored crc
        h.update(&payload);
        h.finalize()
    };
    if computed_crc != stored_crc {
        return Err(PersistenceError::ChecksumMismatch { seq });
    }

    // -- Decode payload --
    let (mutation, _): (WorldMutation, usize) =
        bincode::serde::decode_from_slice(&payload, bincode::config::standard())
            .map_err(PersistenceError::Decode)?;

    Ok(Some((seq, mutation)))
}

/// Result of a `read_exact_or_eof` attempt.
enum ReadResult {
    /// All requested bytes were read.
    Full,
    /// Zero bytes were read — clean end-of-file.
    Eof,
    /// Some bytes were read but fewer than requested — truncated file.
    Partial,
}

/// Reads exactly `buf.len()` bytes, distinguishing clean EOF from partial reads.
fn read_exact_or_eof(file: &mut File, buf: &mut [u8]) -> Result<ReadResult, PersistenceError> {
    let mut total = 0;
    while total < buf.len() {
        match file
            .read(&mut buf[total..])
            .map_err(PersistenceError::WalRead)?
        {
            0 => {
                return Ok(if total == 0 {
                    ReadResult::Eof
                } else {
                    ReadResult::Partial
                });
            }
            n => total += n,
        }
    }
    Ok(ReadResult::Full)
}

// ── Segment file helpers ───────────────────────────────────────────────────────

/// Returns the path for segment `index` inside `dir`.
///
/// Example: `segment_path("/saves/wal", 3)` → `/saves/wal/wal_0003.bin`.
fn segment_path(dir: &Path, index: u32) -> PathBuf {
    dir.join(format!("wal_{index:04}.bin"))
}

/// Parses a segment file name of the form `wal_NNNN.bin` and returns `NNNN`.
///
/// Returns `None` for any name that does not match the pattern.
fn parse_segment_name(name: &str) -> Option<u32> {
    let stem = name.strip_prefix("wal_")?.strip_suffix(".bin")?;
    if stem.len() == 4 {
        stem.parse::<u32>().ok()
    } else {
        None
    }
}

/// Scans a segment file and returns the sequence number that the *next*
/// entry should receive (i.e. the last valid seq + 1, or 0 if the file is
/// empty / completely torn).
fn scan_last_seq(path: &Path) -> Result<u64, PersistenceError> {
    let mut file = File::open(path).map_err(PersistenceError::WalOpen)?;
    let mut last_seq: Option<u64> = None;
    let mut offset = 0u64;

    loop {
        match read_entry(&mut file, &mut offset) {
            Ok(Some((seq, _))) => last_seq = Some(seq),
            Ok(None) => break,
            Err(PersistenceError::TornEntry { .. }) => break,
            Err(e) => return Err(e),
        }
    }

    Ok(last_seq.map(|s| s + 1).unwrap_or(0))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::mutation::{PlayerTransformSnapshot, StaminaSnapshot, WorldMutation};
    use tempfile::TempDir;

    // Helper: write N mutations and return the sequence numbers.
    fn write_mutations(writer: &mut WalWriter, count: u64) -> Vec<u64> {
        (0..count)
            .map(|i| {
                let m = WorldMutation::Stamina(StaminaSnapshot {
                    current: i as f32,
                    max: 100.0,
                });
                writer.append(&m).expect("append should succeed")
            })
            .collect()
    }

    // Helper: read all entries from a directory via WalReader::recover.
    fn recover_all(dir: &Path) -> Vec<(u64, WorldMutation)> {
        WalReader::open(dir)
            .expect("WalReader::open should succeed")
            .recover()
            .expect("recover should succeed")
    }

    /// A freshly opened WAL in an empty directory must start at segment 1
    /// and sequence 0.
    #[test]
    fn wal_starts_fresh_in_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let mut writer = WalWriter::open(tmp.path()).expect("WalWriter::open");
        assert_eq!(writer.segment_index, 1);
        assert_eq!(writer.next_seq, 0);

        let seq = writer
            .append(&WorldMutation::Stamina(StaminaSnapshot {
                current: 1.0,
                max: 100.0,
            }))
            .expect("first append");
        assert_eq!(seq, 0);
    }

    /// Sequence numbers must be monotonically increasing across appends.
    #[test]
    fn wal_sequence_numbers_are_monotone() {
        let tmp = TempDir::new().unwrap();
        let mut writer = WalWriter::open(tmp.path()).expect("WalWriter::open");
        let seqs = write_mutations(&mut writer, 50);
        let expected: Vec<u64> = (0..50).collect();
        assert_eq!(seqs, expected);
    }

    /// Recovery must yield exactly the entries that were written.
    #[test]
    fn wal_roundtrip_recover() {
        let tmp = TempDir::new().unwrap();
        let mut writer = WalWriter::open(tmp.path()).expect("WalWriter::open");

        let mutations: Vec<WorldMutation> = (0..10)
            .map(|i| {
                WorldMutation::PlayerTransform(PlayerTransformSnapshot {
                    translation: [i as f32, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                })
            })
            .collect();

        for m in &mutations {
            writer.append(m).expect("append");
        }
        writer.flush().expect("flush");
        drop(writer);

        let recovered = recover_all(tmp.path());
        assert_eq!(recovered.len(), mutations.len());
        for (i, (seq, _)) in recovered.iter().enumerate() {
            assert_eq!(*seq, i as u64);
        }
    }

    /// A writer opened against a directory that already has a segment must
    /// resume with the next sequence number after the last valid entry.
    #[test]
    fn wal_resumes_sequence_after_reopen() {
        let tmp = TempDir::new().unwrap();

        {
            let mut writer = WalWriter::open(tmp.path()).expect("first open");
            write_mutations(&mut writer, 5);
            writer.flush().expect("flush");
        }

        // Reopen and write more.
        let mut writer2 = WalWriter::open(tmp.path()).expect("second open");
        assert_eq!(writer2.next_seq, 5, "should resume at seq 5");
        let seq = writer2
            .append(&WorldMutation::Stamina(StaminaSnapshot {
                current: 99.0,
                max: 100.0,
            }))
            .expect("append after reopen");
        assert_eq!(seq, 5);

        drop(writer2);

        let recovered = recover_all(tmp.path());
        assert_eq!(recovered.len(), 6);
    }

    /// A torn final entry (truncated at the byte level) must be silently
    /// discarded by recovery, preserving all earlier complete entries.
    #[test]
    fn wal_torn_entry_is_discarded_on_recovery() {
        let tmp = TempDir::new().unwrap();

        {
            let mut writer = WalWriter::open(tmp.path()).expect("open");
            write_mutations(&mut writer, 5);
            writer.flush().expect("flush");
        }

        // Truncate the last 4 bytes of the segment to simulate a torn write.
        let seg = segment_path(tmp.path(), 1);
        let len = std::fs::metadata(&seg).unwrap().len();
        let truncated_file = std::fs::OpenOptions::new().write(true).open(&seg).unwrap();
        truncated_file.set_len(len - 4).unwrap();
        drop(truncated_file);

        // Recovery must return only 4 entries (the last one is torn).
        let recovered = recover_all(tmp.path());
        assert_eq!(
            recovered.len(),
            4,
            "torn final entry must be discarded; got {} entries",
            recovered.len()
        );
        // The 4 intact entries must have seq 0..3.
        for (i, (seq, _)) in recovered.iter().enumerate() {
            assert_eq!(*seq, i as u64);
        }
    }

    /// Truncating the entry so severely that even the magic bytes are missing
    /// must still be handled — treated as a torn entry with seq=None.
    #[test]
    fn wal_empty_tail_is_clean() {
        let tmp = TempDir::new().unwrap();

        {
            let mut writer = WalWriter::open(tmp.path()).expect("open");
            write_mutations(&mut writer, 3);
            writer.flush().expect("flush");
        }

        // Append a few random garbage bytes (simulating a torn header).
        let seg = segment_path(tmp.path(), 1);
        let mut file = std::fs::OpenOptions::new().append(true).open(&seg).unwrap();
        file.write_all(b"\x00\x01\x02").unwrap();
        drop(file);

        // Partial magic should trigger TornEntry (not BadMagic) because the
        // read is partial; either way recover() must swallow it cleanly.
        let recovered = recover_all(tmp.path());
        assert_eq!(recovered.len(), 3);
    }

    /// Explicit flush must not return an error on a healthy filesystem.
    #[test]
    fn wal_flush_succeeds() {
        let tmp = TempDir::new().unwrap();
        let mut writer = WalWriter::open(tmp.path()).expect("open");
        write_mutations(&mut writer, 3);
        writer.flush().expect("flush must succeed");
    }

    /// `parse_segment_name` must accept exactly the right filename pattern.
    #[test]
    fn parse_segment_name_valid_and_invalid() {
        assert_eq!(parse_segment_name("wal_0001.bin"), Some(1));
        assert_eq!(parse_segment_name("wal_0042.bin"), Some(42));
        assert_eq!(parse_segment_name("wal_9999.bin"), Some(9999));
        assert_eq!(parse_segment_name("wal_001.bin"), None); // 3 digits
        assert_eq!(parse_segment_name("wal_00001.bin"), None); // 5 digits
        assert_eq!(parse_segment_name("wal_0001.bin.bak"), None);
        assert_eq!(parse_segment_name("snapshot.bin"), None);
        assert_eq!(parse_segment_name(""), None);
    }
}
