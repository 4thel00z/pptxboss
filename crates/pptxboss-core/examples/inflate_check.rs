//! Compares the in-tree inflater with zlib-rs on every deflated entry of the given archives.
//!
//! `cargo run --release -p pptxboss-core --example inflate_check -- *.pptx`

use std::io::Read;

use pptxboss_core::zip::Archive;

fn main() {
    let mut entries = 0usize;
    let mut mismatches = 0usize;
    let mut bytes = 0usize;
    let mut ours_failed = 0usize;
    let mut both_failed = 0usize;
    for path in std::env::args().skip(1) {
        let Ok(archive) = Archive::open_path(&path) else {
            continue;
        };
        for entry in archive.entries() {
            if entry.method != 8 {
                continue;
            }
            let mut raw = Vec::new();
            if archive.read_raw(entry, &mut raw).is_err() {
                continue;
            }
            entries += 1;
            let mut ours = Vec::new();
            let ours =
                pptxboss_core::inflate::inflate(&raw, entry.uncompressed_size as usize, &mut ours)
                    .map(|_| ours);
            let mut reference = Vec::new();
            let reference = flate2::read::DeflateDecoder::new(&raw[..])
                .read_to_end(&mut reference)
                .map(|_| reference);
            match (ours, reference) {
                (Ok(a), Ok(b)) => {
                    bytes += a.len();
                    if a != b {
                        mismatches += 1;
                        println!(
                            "MISMATCH {path} {} ({} vs {} bytes)",
                            entry.name,
                            a.len(),
                            b.len()
                        );
                    }
                }
                (Err(err), Ok(b)) => {
                    ours_failed += 1;
                    println!(
                        "OURS FAILED {path} {}: {err} (reference {} bytes)",
                        entry.name,
                        b.len()
                    );
                }
                (Ok(a), Err(err)) => {
                    println!(
                        "REFERENCE FAILED {path} {}: {err} (ours {} bytes)",
                        entry.name,
                        a.len()
                    );
                }
                (Err(_), Err(_)) => both_failed += 1,
            }
        }
    }
    println!(
        "{entries} deflated entries, {bytes} bytes inflated, {mismatches} mismatches, {ours_failed} ours-only failures, {both_failed} failed in both"
    );
}
