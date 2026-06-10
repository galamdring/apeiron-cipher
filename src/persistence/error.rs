//! Error types for the persistence layer.
//!
//! All WAL and save-file operations return [`PersistenceError`].  The enum is
//! deliberately granular: each variant carries the root cause so the caller
//! can decide whether to retry, surface a warning, or refuse to proceed.
//!
//! # Why no `From<io::Error>` impl?
//!
//! The same [`std::io::Error`] can originate from write, read, open, or sync
//! operations.  Deriving `From` would let `?` silently swallow that context.
//! Call-sites must tag the error explicitly — [`PersistenceError::WalWrite`],
//! [`PersistenceError::WalRead`], etc. — so a bug report or log line
//! immediately identifies the operation that failed.

use std::fmt;

/// Every error that can arise from WAL writes, segment file management, entry
/// decoding, and recovery.
///
/// Returned by [`super::wal::WalWriter::append`], [`super::wal::WalWriter::flush`],
/// and [`super::wal::WalReader::recover`].
#[derive(Debug)]
pub enum PersistenceError {
    /// The WAL writer failed to write, flush, or `fdatasync` a segment file.
    ///
    /// When this is returned by [`super::wal::WalWriter::append`], the
    /// mutation has NOT been committed to stable storage.  The caller should
    /// treat the game session as no longer persistent and warn the player.
    WalWrite(std::io::Error),

    /// A segment file could not be opened, created, or stat'd.
    ///
    /// Common causes: the WAL directory does not exist and could not be
    /// created, the directory is on a read-only filesystem, or the process
    /// lacks permission.
    WalOpen(std::io::Error),

    /// A read from a segment file failed with an OS error.
    ///
    /// Distinct from [`PersistenceError::TornEntry`]: this variant means the
    /// I/O syscall itself returned an error, not that the entry is simply
    /// truncated.
    WalRead(std::io::Error),

    /// A WAL entry is truncated mid-write — the file ends before the entry's
    /// payload or checksum bytes are fully present.
    ///
    /// This is the **expected** outcome of a `kill -9` or power loss mid-write.
    /// The recovery reader MUST silently discard this final partial entry and
    /// stop scanning; all earlier complete entries are durable.
    TornEntry {
        /// Sequence number of the torn entry, if the eight sequence bytes were
        /// fully written before the truncation.  `None` if the truncation
        /// happened before the sequence field was complete.
        seq: Option<u64>,
    },

    /// The CRC-32 checksum stored in an entry does not match the checksum
    /// computed over its header and payload bytes.
    ///
    /// This indicates silent data corruption or an overwrite by a non-WAL
    /// process.  Recovery MUST treat this as a fatal integrity violation for
    /// the affected segment and stop reading.
    ChecksumMismatch {
        /// Sequence number of the corrupted entry.
        seq: u64,
    },

    /// The four-byte magic cookie at the start of an entry frame did not match
    /// the expected value `0x57414C31` ("WAL1" in ASCII).
    ///
    /// This usually means the read position has drifted to garbage bytes —
    /// either a torn previous entry left trailing garbage, or the file is not
    /// a WAL segment at all.
    BadMagic {
        /// The four bytes that were read instead of the expected magic.
        found: [u8; 4],
        /// Byte offset within the segment file where the unexpected bytes
        /// were encountered.
        offset: u64,
    },

    /// A [`WorldMutation`][super::mutation::WorldMutation] could not be
    /// serialised to bincode bytes before writing to the WAL.
    Encode(bincode::error::EncodeError),

    /// Bincode bytes stored in the WAL could not be decoded back into a
    /// [`WorldMutation`][super::mutation::WorldMutation].
    Decode(bincode::error::DecodeError),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistenceError::WalWrite(e) => write!(f, "WAL write error: {e}"),
            PersistenceError::WalOpen(e) => write!(f, "WAL open error: {e}"),
            PersistenceError::WalRead(e) => write!(f, "WAL read error: {e}"),
            PersistenceError::TornEntry { seq: Some(s) } => {
                write!(f, "torn WAL entry at seq {s} (truncated mid-write)")
            }
            PersistenceError::TornEntry { seq: None } => {
                write!(f, "torn WAL entry (sequence field not recoverable)")
            }
            PersistenceError::ChecksumMismatch { seq } => write!(
                f,
                "WAL entry seq {seq}: CRC-32 mismatch — data corruption detected"
            ),
            PersistenceError::BadMagic { found, offset } => write!(
                f,
                "WAL bad magic at offset {offset}: expected 0x57414c31 (\"WAL1\"), \
                 found 0x{:02x}{:02x}{:02x}{:02x}",
                found[0], found[1], found[2], found[3]
            ),
            PersistenceError::Encode(e) => write!(f, "WAL bincode encode error: {e}"),
            PersistenceError::Decode(e) => write!(f, "WAL bincode decode error: {e}"),
        }
    }
}

impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PersistenceError::WalWrite(e)
            | PersistenceError::WalOpen(e)
            | PersistenceError::WalRead(e) => Some(e),
            PersistenceError::Encode(e) => Some(e),
            PersistenceError::Decode(e) => Some(e),
            PersistenceError::TornEntry { .. }
            | PersistenceError::ChecksumMismatch { .. }
            | PersistenceError::BadMagic { .. } => None,
        }
    }
}
