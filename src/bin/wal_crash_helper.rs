//! Crash helper binary for WAL integration tests.
//!
//! This binary is spawned by `tests/wal_crash.rs` to simulate a process that
//! is killed with `SIGKILL` (`kill -9`) while writing to the WAL.
//!
//! # Usage
//!
//! ```
//! wal_crash_helper <wal_dir> <entry_count>
//! ```
//!
//! Writes `entry_count` mutations to the WAL at `wal_dir`, then hangs in a
//! `loop {}` forever so the parent test can `kill -9` it at any point.
//!
//! The test harness kills the process after the first write completes (or
//! immediately for the "crash during first write" scenario), then opens the
//! WAL with `WalReader` and verifies no torn entries are visible to the
//! consumer.

use std::env;

use apeiron_cipher::persistence::mutation::{StaminaSnapshot, WorldMutation};
use apeiron_cipher::persistence::wal::WalWriter;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: wal_crash_helper <wal_dir> <entry_count>");
        std::process::exit(1);
    }

    let wal_dir = &args[1];
    let entry_count: u64 = args[2]
        .parse()
        .expect("entry_count must be a non-negative integer");

    let mut writer = WalWriter::open(wal_dir).expect("WalWriter::open failed");

    for i in 0..entry_count {
        let mutation = WorldMutation::Stamina(StaminaSnapshot {
            current: i as f32,
            max: entry_count as f32,
        });
        writer.append(&mutation).expect("append failed");
    }

    // Signal to the parent that all entries have been written — the parent may
    // kill us at any time after this print.
    println!("READY");
    std::io::Write::flush(&mut std::io::stdout()).expect("stdout flush");

    // Hang forever so the parent can kill us with SIGKILL.
    // This ensures the process is still "alive" when killed, exercising the
    // kernel's handling of the open file descriptor on forced termination.
    loop {
        std::hint::spin_loop();
    }
}
