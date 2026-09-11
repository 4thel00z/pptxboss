//! Times the layers of opening a deck: archive, package, document.
//!
//! `cargo run --release -p pptxboss-core --example open_phases -- deck.pptx [more.pptx ...]`

use std::sync::Arc;
use std::time::Instant;

use pptxboss_core::zip::{Archive, FileSource};
use pptxboss_core::{Document, Package};

fn main() {
    for path in std::env::args().skip(1) {
        let mut best = [f64::MAX; 4];
        for _ in 0..10 {
            let start = Instant::now();
            let source = Arc::new(FileSource::open(&path).expect("opens"));
            let t_open = start.elapsed().as_secs_f64() * 1e6;
            let start = Instant::now();
            let archive = Archive::open(source).expect("archive");
            let t_archive = start.elapsed().as_secs_f64() * 1e6;
            let start = Instant::now();
            let package = Package::from_archive(archive).expect("package");
            let t_package = start.elapsed().as_secs_f64() * 1e6;
            let start = Instant::now();
            let doc = Document::from_package(package).expect("document");
            let t_document = start.elapsed().as_secs_f64() * 1e6;
            std::hint::black_box(doc.slide_count());
            for (slot, value) in best
                .iter_mut()
                .zip([t_open, t_archive, t_package, t_document])
            {
                *slot = slot.min(value);
            }
        }
        println!(
            "{:8.1} file  {:8.1} archive  {:8.1} package  {:8.1} document  {}",
            best[0], best[1], best[2], best[3], path
        );
    }
}
