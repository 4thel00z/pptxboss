//! Prints the text of legacy .ppt files through the ppt module directly.

use pptxboss_core::cfb::Compound;
use pptxboss_core::ppt::LegacyDeck;
use pptxboss_core::text::{write_content_text, TextOptions};
use pptxboss_core::zip::FileSource;

fn main() {
    let verbose = std::env::var("VERBOSE").is_ok();
    let mut ok = 0;
    let mut failed = 0;
    let mut slides_total = 0;
    let mut chars_total = 0;
    for path in std::env::args().skip(1) {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let result = FileSource::open(&path)
            .map_err(|e| e.to_string())
            .and_then(|source| Compound::open(&source).map_err(|e| e.to_string()))
            .and_then(|compound| LegacyDeck::open(&compound).map_err(|e| e.to_string()));
        let deck = match result {
            Ok(deck) => deck,
            Err(err) => {
                failed += 1;
                println!("{name}: ERROR {err}");
                continue;
            }
        };
        ok += 1;
        let mut text = String::new();
        let mut pictures = 0;
        let mut errors = 0;
        for index in 0..deck.slide_count() {
            match deck.slide(index) {
                Ok(slide) => {
                    let mut out = String::new();
                    write_content_text(
                        &slide.content,
                        &TextOptions::default(),
                        &mut |_| None,
                        &mut out,
                    );
                    for shape in slide.content.walk() {
                        if let pptxboss_core::Content::Picture(_) = shape.content {
                            pictures += 1;
                        }
                    }
                    if verbose {
                        println!(
                            "--- slide {} (id {}{}) title={:?}",
                            index + 1,
                            slide.id,
                            if slide.hidden { ", hidden" } else { "" },
                            slide.content.title()
                        );
                        println!("{out}");
                        if let Ok(Some(notes)) = deck.notes(index) {
                            println!("[notes] {}", notes.text());
                        }
                    }
                    chars_total += out.chars().count();
                    text.push_str(&out);
                }
                Err(_) => errors += 1,
            }
        }
        slides_total += deck.slide_count();
        println!(
            "{name}: slides={} chars={} pictures={pictures} slide_errors={errors} size={:?}",
            deck.slide_count(),
            text.chars().count(),
            deck.slide_size
        );
    }
    println!("summary: ok={ok} failed={failed} slides={slides_total} chars={chars_total}");
}
