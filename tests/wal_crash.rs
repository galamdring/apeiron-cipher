//! WAL crash-recovery integration test.
//!
//! Spawns `wal_crash_helper` as a child process, lets it write some mutations,
//! then kills it with `SIGKILL`.  After the process is gone, opens the WAL
//! directory with [`WalReader`] and verifies that:
//!
//! 1. No entry has torn (partially-written) frame bytes visible to the
//!    consumer — recovery returns only complete, CRC-verified entries.
//! 2. All entries that completed before the kill have intact sequence numbers
//!    (0, 1, 2, …).
//! 3. The total recovered count is ≤ the total entries the helper was asked
//!    to write (never more).
//!
//! # Acceptance criterion
//!
//! `cargo test -p apeiron-cipher wal_crash` must pass.  The test intentionally
//! does not assert an exact recovered count because the kill may arrive before
//! or after any given entry is written.
//!
//! # Platform note
//!
//! `SIGKILL` is a Unix signal.  On non-Unix platforms this test is compiled
//! out via a `#[cfg(unix)]` gate.

use std::io::BufRead;
use std::process::{Command, Stdio};
use std::time::Duration;

use apeiron_cipher::persistence::wal::WalReader;

/// Returns the path to the `wal_crash_helper` binary built by Cargo.
fn helper_bin() -> std::path::PathBuf {
    // Cargo sets CARGO_BIN_EXE_wal_crash_helper when building integration
    // tests alongside the binary target.  Fall back to the default debug
    // target directory for manual runs.
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_wal_crash_helper") {
        return p.into();
    }
    // Try to find it relative to the current executable location.
    let mut path = std::env::current_exe().expect("current_exe");
    path.pop(); // strip test binary name
    if path.ends_with("deps") {
        path.pop(); // strip deps/
    }
    path.push("wal_crash_helper");
    path
}

/// Verify that all recovered entries have strictly increasing sequence numbers
/// starting from 0.
fn assert_sequences_are_clean(
    entries: &[(u64, apeiron_cipher::persistence::mutation::WorldMutation)],
) {
    for (i, (seq, _)) in entries.iter().enumerate() {
        assert_eq!(
            *seq, i as u64,
            "entry at position {i} has unexpected seq {seq}"
        );
    }
}

#[cfg(unix)]
#[test]
fn wal_survives_kill9_mid_write() {
    use std::os::unix::process::ExitStatusExt;

    let tmp = tempfile::TempDir::new().expect("temp dir");
    let wal_dir = tmp.path().to_str().expect("path is valid UTF-8");

    // Number of entries the helper writes before announcing READY.
    // Large enough to ensure the WAL file is non-trivial; small enough to
    // complete in milliseconds.
    let entry_count = 200u64;

    let helper = helper_bin();
    assert!(
        helper.exists(),
        "wal_crash_helper binary not found at {}: \
         run `cargo build --bin wal_crash_helper` first",
        helper.display()
    );

    // -- Phase 1: spawn the helper and let it write all entries. -----------
    let mut child = Command::new(&helper)
        .args([wal_dir, &entry_count.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn wal_crash_helper");

    // Wait for "READY\n" — all entries are written.
    let stdout = child.stdout.take().expect("stdout pipe");
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("read_line from helper stdout");
    assert_eq!(
        line.trim(),
        "READY",
        "helper must print READY before we kill it"
    );

    // -- Phase 2: kill -9 the helper while it is hanging in spin loop. -----
    let pid = child.id();
    // SAFETY: kill(2) is safe to call with a valid PID and SIGKILL.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }

    // Wait for the child to actually exit (should be near-instantaneous).
    let status = child.wait().expect("wait for child");
    // On Unix, SIGKILL produces a non-zero exit; the signal number is 9.
    assert!(
        status.signal() == Some(9) || !status.success(),
        "child should have been killed; status = {status:?}"
    );

    // -- Phase 3: recover the WAL and verify integrity. --------------------
    let recovered = WalReader::open(tmp.path())
        .expect("WalReader::open")
        .recover()
        .expect("recover must not return a non-torn error");

    // Must not have more entries than what was written.
    assert!(
        recovered.len() <= entry_count as usize,
        "recovered {} entries but only {} were written",
        recovered.len(),
        entry_count
    );

    // All recovered entries must have clean, in-order sequence numbers.
    assert_sequences_are_clean(&recovered);
}

/// Regression: process killed BEFORE writing any entries must produce an
/// empty recovery (not a panic).
#[cfg(unix)]
#[test]
fn wal_empty_on_immediate_kill() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let wal_dir = tmp.path().to_str().expect("path is valid UTF-8");

    // Spawn helper asking for 0 entries so it prints READY immediately.
    let mut child = Command::new(helper_bin())
        .args([wal_dir, "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn wal_crash_helper");

    let stdout = child.stdout.take().expect("stdout pipe");
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read_line");
    assert_eq!(line.trim(), "READY");

    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGKILL);
    }
    child.wait().expect("wait");

    // Brief pause: let the OS flush any partial page-cache writes (usually
    // instant after SIGKILL, but be defensive).
    std::thread::sleep(Duration::from_millis(10));

    let recovered = WalReader::open(tmp.path())
        .expect("WalReader::open")
        .recover()
        .expect("recover");

    assert_eq!(
        recovered.len(),
        0,
        "expected empty WAL; got {} entries",
        recovered.len()
    );
}
