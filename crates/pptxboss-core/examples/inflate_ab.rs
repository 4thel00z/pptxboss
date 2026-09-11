//! Best-of-N inflate timings, in-tree decoder against zlib-rs, for named entries.
//!
//! `cargo run --release -p pptxboss-core --example inflate_ab -- deck.pptx entry [entry ...]`

use std::io::Read;
use std::time::Instant;

use flate2::{Decompress, FlushDecompress};
use pptxboss_core::zip::Archive;

fn best(runs: usize, mut f: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..runs {
        let start = Instant::now();
        f();
        best = best.min(start.elapsed().as_secs_f64() * 1e6);
    }
    best
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("path");
    let archive = Archive::open_path(&path).expect("archive");
    for name in args {
        let entry = archive.entry(&name).expect("entry");
        let mut raw = Vec::new();
        archive.read_raw(entry, &mut raw).expect("raw");
        let expected = entry.uncompressed_size as usize;
        let runs = (2_000_000 / expected.max(1)).clamp(20, 20_000);
        let mut out = Vec::with_capacity(expected + 128);
        let mut decoder = Decompress::new(false);
        let zlib = best(runs, || {
            decoder.reset(false);
            out.clear();
            decoder
                .decompress_vec(&raw, &mut out, FlushDecompress::Finish)
                .expect("inflates");
        });
        let ours = best(runs, || {
            out.clear();
            pptxboss_core::inflate::inflate(&raw, expected, &mut out).expect("inflates");
        });
        let mut check = Vec::new();
        flate2::read::DeflateDecoder::new(&raw[..])
            .read_to_end(&mut check)
            .expect("reference");
        let same = check == out;
        println!(
            "{name}: {} -> {} bytes  zlib-rs {zlib:8.2} us ({:.0} MB/s)  ours {ours:8.2} us ({:.0} MB/s)  ratio {:.2}  same={same}",
            raw.len(),
            expected,
            expected as f64 / zlib,
            expected as f64 / ours,
            zlib / ours
        );
    }
}
