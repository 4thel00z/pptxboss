//! Counts positioned reads and bytes per phase over a set of files.
//!
//! `cargo run --release -p pptxboss-core --example read_counts -- deck.pptx [more.pptx ...]`

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pptxboss_core::zip::{FileSource, Source};
use pptxboss_core::{Document, Package};

struct Counting {
    inner: FileSource,
    calls: AtomicU64,
    bytes: AtomicU64,
}

impl Source for Counting {
    fn len(&self) -> u64 {
        self.inner.len()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(buf.len() as u64, Ordering::Relaxed);
        self.inner.read_at(offset, buf)
    }
}

/// Open time, slide time, slide count and path of one file.
type Row = (f64, f64, usize, String);

#[derive(Default)]
struct Phase {
    calls: u64,
    bytes: u64,
    time: Duration,
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let mut open = Phase::default();
    let mut slides = Phase::default();
    let mut files = 0u64;
    let mut slide_count = 0u64;
    let mut file_bytes = 0u64;
    let mut rows: Vec<Row> = Vec::new();
    for path in &paths {
        let Ok(inner) = FileSource::open(path) else {
            continue;
        };
        file_bytes += inner.len();
        let source = Arc::new(Counting {
            inner,
            calls: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        });
        let open_before = open.time.as_secs_f64() * 1e6;
        let slides_before = slides.time.as_secs_f64() * 1e6;
        let start = Instant::now();
        let Ok(package) = Package::from_source(source.clone()) else {
            continue;
        };
        let Ok(doc) = Document::from_package(package) else {
            continue;
        };
        open.time += start.elapsed();
        open.calls += source.calls.swap(0, Ordering::Relaxed);
        open.bytes += source.bytes.swap(0, Ordering::Relaxed);
        let start = Instant::now();
        for index in 0..doc.slide_count() {
            if let Ok(slide) = doc.slide(index) {
                std::hint::black_box(slide.text());
            }
        }
        slides.time += start.elapsed();
        slides.calls += source.calls.swap(0, Ordering::Relaxed);
        slides.bytes += source.bytes.swap(0, Ordering::Relaxed);
        slide_count += doc.slide_count() as u64;
        files += 1;
        rows.push((
            open.time.as_secs_f64() * 1e6 - open_before,
            slides.time.as_secs_f64() * 1e6 - slides_before,
            doc.slide_count(),
            path.clone(),
        ));
    }
    rows.sort_by(|a, b| (b.0 + b.1).total_cmp(&(a.0 + a.1)));
    let median = |pick: &dyn Fn(&Row) -> f64| {
        let mut values: Vec<f64> = rows.iter().map(pick).collect();
        values.sort_by(|a, b| a.total_cmp(b));
        (values[values.len() / 2], values[values.len() * 9 / 10])
    };
    let (open_median, open_p90) = median(&|row| row.0);
    let (slides_median, slides_p90) = median(&|row| row.1);
    println!("open   median {open_median:8.1} us  p90 {open_p90:8.1} us");
    println!("slides median {slides_median:8.1} us  p90 {slides_p90:8.1} us");
    for (open_us, slides_us, count, path) in rows.iter().take(8) {
        println!("  {open_us:8.1} {slides_us:9.1} {count:3} {path}");
    }
    let per = |phase: &Phase| {
        format!(
            "{:6.1} calls  {:9.0} bytes  {:8.1} us",
            phase.calls as f64 / files as f64,
            phase.bytes as f64 / files as f64,
            phase.time.as_secs_f64() * 1e6 / files as f64
        )
    };
    println!(
        "{files} files, {slide_count} slides, {:.0} bytes/file on average",
        file_bytes as f64 / files as f64
    );
    println!("open   per file: {}", per(&open));
    println!("slides per file: {}", per(&slides));
}
