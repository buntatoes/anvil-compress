// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0-OR-Commercial
// Copyright (c) 2026 Buntato
//! Robustness checks for the DEFLATE, zlib, and gzip decoders.
//!
//! Inputs come from a seeded generator, so a failure names a case number
//! that reproduces on every run. Each test stops after a short time budget.
//! A decoder may return an error for any of these inputs. It must not panic,
//! hang, or write past the output cap.

use anvil_compress::{
    compress_deflate, compress_gzip, compress_zlib, decompress_deflate_bounded,
    decompress_gzip_bounded, decompress_zlib_bounded,
};

const OUTPUT_CAP: usize = 1 << 20;

// ---- shared harness (same text in each repo's robustness test) ----

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Wall-clock budget for one robustness test. Cases run until it is spent.
/// Set `ANVIL_ROBUSTNESS_MS` for a longer local soak run.
fn test_budget() -> Duration {
    let millis = std::env::var("ANVIL_ROBUSTNESS_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1500);
    Duration::from_millis(millis)
}

/// One input that runs longer than this counts as a hang.
const CASE_LIMIT: Duration = Duration::from_secs(10);

/// Upper bound on cases per test, so a fast machine does not loop forever.
const MAX_CASES: usize = 200_000;

/// xorshift64 generator. A fixed seed gives the same inputs on every run.
struct Rng(u64);

#[allow(dead_code)] // Not every test file uses every helper.
impl Rng {
    fn new(seed: u64) -> Self {
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Value in `0..n`. Returns 0 when `n` is 0.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    fn byte(&mut self) -> u8 {
        self.next_u64() as u8
    }

    fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    fn bytes(&mut self, max_len: usize) -> Vec<u8> {
        let len = self.below(max_len + 1);
        (0..len).map(|_| self.byte()).collect()
    }
}

/// Apply 1 to 8 random edits: bit flips, byte overwrites, boundary values,
/// inserts, deletes, duplicated runs, and truncation.
fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut out = seed.to_vec();
    let edits = 1 + rng.below(8);
    for _ in 0..edits {
        let len = out.len();
        match rng.below(8) {
            0 if len > 0 => {
                let i = rng.below(len);
                out[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len);
                out[i] = rng.byte();
            }
            2 if len > 0 => {
                let i = rng.below(len);
                out[i] = rng.pick(&[0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF]);
            }
            3 if len >= 4 => {
                let i = rng.below(len - 3);
                let v = rng.pick(&[0u32, 1, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFF, 0xFFFF]);
                out[i..i + 4].copy_from_slice(&v.to_be_bytes());
            }
            4 => {
                let i = rng.below(len + 1);
                let extra = rng.bytes(16);
                out.splice(i..i, extra);
            }
            5 if len > 0 => {
                let i = rng.below(len);
                let n = 1 + rng.below((len - i).min(32));
                out.drain(i..i + n);
            }
            6 if len > 0 => {
                let i = rng.below(len);
                let n = 1 + rng.below((len - i).min(64));
                let run = out[i..i + n].to_vec();
                let at = rng.below(len + 1);
                out.splice(at..at, run);
            }
            _ => {
                let keep = rng.below(len + 1);
                out.truncate(keep);
            }
        }
    }
    out
}

fn preview(input: &[u8]) -> String {
    let shown: Vec<String> = input.iter().take(96).map(|b| format!("{b:02x}")).collect();
    let more = if input.len() > 96 { " ..." } else { "" };
    format!("{} bytes: {}{}", input.len(), shown.join(" "), more)
}

/// Run `target` on `input` in its own thread. Fails the test on a panic or
/// when the case runs past [`CASE_LIMIT`]. A hung thread is left behind and
/// ends with the test process.
fn check_case(label: &str, case: usize, input: Vec<u8>, target: fn(&[u8])) {
    let (done_tx, done_rx) = mpsc::channel();
    let shared = std::sync::Arc::new(input);
    let worker_input = shared.clone();
    let handle = thread::Builder::new()
        .name(format!("{label}-{case}"))
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            target(&worker_input);
            let _ = done_tx.send(());
        })
        .expect("spawn case thread");
    match done_rx.recv_timeout(CASE_LIMIT) {
        Ok(()) => {
            let _ = handle.join();
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = handle.join();
            panic!(
                "{label}: case {case} panicked on input {}",
                preview(&shared)
            );
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "{label}: case {case} did not finish within {:?} on input {}",
                CASE_LIMIT,
                preview(&shared)
            );
        }
    }
}

/// Generate cases from `seed` until the budget is spent and check each one.
fn run_cases(
    label: &str,
    seed: u64,
    mut generate: impl FnMut(&mut Rng) -> Vec<u8>,
    target: fn(&[u8]),
) {
    let mut rng = Rng::new(seed);
    let budget = test_budget();
    let started = Instant::now();
    let mut case = 0;
    while case < MAX_CASES && started.elapsed() < budget {
        let input = generate(&mut rng);
        check_case(label, case, input, target);
        case += 1;
    }
    eprintln!("{label}: {case} cases in {:?}", started.elapsed());
}

/// Plain text with long repeats, so the encoders emit back-references.
fn sample_text(rng: &mut Rng) -> Vec<u8> {
    let words: [&[u8]; 6] = [
        b"anvil ",
        b"forge ",
        b"spark ",
        b"\n",
        b"0123456789",
        b"aaaaaaaa",
    ];
    let mut out = Vec::new();
    let count = rng.below(200);
    for _ in 0..count {
        out.extend_from_slice(rng.pick(&words[..]));
    }
    if rng.one_in(3) {
        out.extend(rng.bytes(64));
    }
    out
}

fn decode_all(input: &[u8]) {
    let decoded = [
        decompress_deflate_bounded(input, OUTPUT_CAP),
        decompress_zlib_bounded(input, OUTPUT_CAP),
        decompress_gzip_bounded(input, OUTPUT_CAP),
    ];
    for out in decoded.into_iter().flatten() {
        assert!(
            out.len() <= OUTPUT_CAP,
            "output cap exceeded: {}",
            out.len()
        );
    }
}

#[test]
fn random_bytes_do_not_panic() {
    run_cases(
        "compress-random",
        1,
        |rng| {
            let mut input = rng.bytes(256);
            // Steer some inputs past the header checks.
            match rng.below(4) {
                0 if input.len() >= 2 => input[..2].copy_from_slice(&[0x78, 0x9C]),
                1 if input.len() >= 3 => input[..3].copy_from_slice(&[0x1F, 0x8B, 0x08]),
                _ => {}
            }
            input
        },
        decode_all,
    );
}

#[test]
fn mutated_streams_do_not_panic() {
    run_cases(
        "compress-mutated",
        2,
        |rng| {
            let text = sample_text(rng);
            let packed = match rng.below(3) {
                0 => compress_deflate(&text),
                1 => compress_zlib(&text),
                _ => compress_gzip(&text),
            };
            mutate(rng, &packed)
        },
        decode_all,
    );
}

#[test]
fn round_trip_holds_for_random_text() {
    run_cases("compress-round-trip", 3, sample_text, |text| {
        let packed = compress_deflate(text);
        assert_eq!(
            decompress_deflate_bounded(&packed, OUTPUT_CAP).unwrap(),
            text
        );
        let packed = compress_zlib(text);
        assert_eq!(decompress_zlib_bounded(&packed, OUTPUT_CAP).unwrap(), text);
        let packed = compress_gzip(text);
        assert_eq!(decompress_gzip_bounded(&packed, OUTPUT_CAP).unwrap(), text);
    });
}

#[test]
fn output_cap_is_enforced_on_highly_repetitive_input() {
    // 4 MiB of zeros packs into a few kilobytes. A 64 KiB cap must refuse it.
    let packed = compress_zlib(&vec![0u8; 4 << 20]);
    assert!(decompress_zlib_bounded(&packed, 64 * 1024).is_err());
}
