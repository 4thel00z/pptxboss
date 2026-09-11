//! Times the read path on real files, in-process, best of N.
//!
//! `cargo run --release -p pptxboss-core --example time_file -- deck.pptx [more.pptx ...]`

use std::time::{Duration, Instant};

use pptxboss_core::{Document, TextOptions};

fn best_of<T>(runs: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = None;
    for _ in 0..runs {
        let start = Instant::now();
        let value = f();
        best = best.min(start.elapsed());
        last = Some(value);
    }
    (best, last.expect("at least one run"))
}

fn main() {
    let runs: usize = std::env::var("RUNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(15);
    let options = TextOptions::default();
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).expect("readable file");
        let (open, doc) = best_of(runs, || Document::open(&path).expect("opens"));
        let (load, _) = best_of(runs, || Document::load(bytes.clone()).expect("loads"));
        let (sequential, seq_len) = best_of(runs, || {
            let doc = Document::open(&path).expect("opens");
            let mut total = 0;
            for index in 0..doc.slide_count() {
                total += doc
                    .slide(index)
                    .map(|slide| slide.text().len())
                    .unwrap_or(0);
            }
            total
        });
        let (parallel, par_len) = best_of(runs, || {
            Document::open(&path)
                .expect("opens")
                .text_reporting(&options)
                .0
                .len()
        });
        let (parse_only, _) = best_of(runs, || {
            let mut total = 0;
            for index in 0..doc.slide_count() {
                total += doc
                    .slide(index)
                    .map(|slide| slide.content.shapes.len())
                    .unwrap_or(0);
            }
            total
        });
        let (inflate_only, _) = best_of(runs, || {
            let mut out = Vec::new();
            let mut total = 0;
            for slide in doc.slide_refs() {
                doc.package()
                    .expect("package")
                    .read_part_into(&slide.part, &mut out)
                    .expect("readable part");
                total += out.len();
            }
            total
        });
        let (tokenize, slide_bytes) = best_of(runs, || {
            let mut events = 0usize;
            let mut bytes = 0usize;
            for slide in doc.slide_refs() {
                let xml = doc
                    .package()
                    .expect("package")
                    .read_part(&slide.part)
                    .expect("readable part");
                bytes += xml.len();
                let mut reader = pptxboss_core::xml::Reader::new(&xml);
                loop {
                    match reader.next().expect("well-formed") {
                        pptxboss_core::xml::Event::Eof => break,
                        _ => events += 1,
                    }
                }
            }
            (events, bytes)
        });
        println!("{path}");
        println!(
            "  slide xml {} KiB, {} events; tokenize only {:>8.2} ms",
            slide_bytes.1 / 1024,
            slide_bytes.0,
            tokenize.as_secs_f64() * 1e3
        );
        println!("  slides {:>4}  parts {:>4}  text {} chars (parallel) / {} chars (sequential, no notes)", doc.slide_count(), doc.package().map_or(0, |p| p.parts().len()), par_len, seq_len);
        println!(
            "  open (positioned)      {:>8.2} ms",
            open.as_secs_f64() * 1e3
        );
        println!(
            "  open (in-memory)       {:>8.2} ms",
            load.as_secs_f64() * 1e3
        );
        println!(
            "  inflate slide parts    {:>8.2} ms",
            inflate_only.as_secs_f64() * 1e3
        );
        println!(
            "  parse slides (cached)  {:>8.2} ms",
            parse_only.as_secs_f64() * 1e3
        );
        println!(
            "  open+text sequential   {:>8.2} ms",
            sequential.as_secs_f64() * 1e3
        );
        println!(
            "  open+text parallel     {:>8.2} ms",
            parallel.as_secs_f64() * 1e3
        );
    }
}
