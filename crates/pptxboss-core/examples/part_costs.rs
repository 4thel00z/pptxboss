//! Times inflate and parse separately for the parts read when a deck opens.
//!
//! `cargo run --release -p pptxboss-core --example part_costs -- deck.pptx`

use std::time::Instant;

use pptxboss_core::opc::{ContentTypes, Relationships};
use pptxboss_core::presentation::Presentation;
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
    for path in std::env::args().skip(1) {
        let archive = Archive::open_path(&path).expect("archive");
        println!("{path}: {} entries", archive.entries().len());
        let t_dir = best(50, || {
            std::hint::black_box(Archive::open_path(&path).expect("archive"));
        });
        println!("  {t_dir:7.1} us  file open + central directory");
        for name in [
            "[Content_Types].xml",
            "_rels/.rels",
            "ppt/_rels/presentation.xml.rels",
            "ppt/presentation.xml",
        ] {
            let Some(entry) = archive.entry(name) else {
                println!("  missing {name}");
                continue;
            };
            let mut out = Vec::new();
            let t_inflate = best(50, || archive.read(entry, &mut out).expect("reads"));
            let t_parse = best(50, || match name {
                "[Content_Types].xml" => {
                    std::hint::black_box(ContentTypes::parse(&out).ok());
                }
                "ppt/presentation.xml" => {
                    std::hint::black_box(Presentation::parse(&out).ok());
                }
                _ => {
                    std::hint::black_box(Relationships::parse("/x", &out).ok());
                }
            });
            println!(
                "  {t_inflate:7.1} us inflate  {t_parse:7.1} us parse  {:6} -> {:6} bytes  {name}",
                entry.compressed_size,
                out.len()
            );
        }
    }
}
