//! Opens every given file repeatedly on one thread, for profilers.
//!
//! `ITER=30 cargo run --release -p pptxboss-core --example open_loop -- deck.pptx [more.pptx ...]`

use pptxboss_core::Document;

fn main() {
    let iterations: usize = std::env::var("ITER")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(30);
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let mut total = 0usize;
    for _ in 0..iterations {
        for path in &paths {
            let Ok(doc) = Document::open(path).map(|doc| doc.with_threads(1)) else {
                continue;
            };
            for index in 0..doc.slide_count() {
                if let Ok(slide) = doc.slide(index) {
                    total += slide.text().len();
                }
            }
        }
    }
    println!("{total}");
}
