//! Criterion benches over synthetic decks from the testkit: open, text
//! extraction sequential and parallel, and the slide parser alone.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pptxboss_core::slide::parse_slide;
use pptxboss_core::{Document, TextOptions};
use pptxboss_testkit::{Deck, DeckSlide};

fn deck(slides: usize, bullets: usize) -> Vec<u8> {
    let mut deck = Deck::new();
    for i in 0..slides {
        let mut slide = DeckSlide::titled(&format!("Slide {i}: a title with some words in it"));
        for j in 0..bullets {
            slide = slide.bullet(&format!(
                "Bullet {j} on slide {i}: the quick brown fox jumps over the lazy dog & friends"
            ));
        }
        if i % 3 == 0 {
            slide = slide.notes("Speaker notes for this slide, a couple of sentences long so they cost something to parse.");
        }
        deck = deck.slide(slide);
    }
    deck.build()
}

fn bench_open(c: &mut Criterion) {
    let bytes = deck(50, 8);
    let mut group = c.benchmark_group("open");
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    group.bench_function("50 slides in memory", |b| {
        b.iter(|| Document::load(bytes.clone()).unwrap().slide_count())
    });
    group.finish();
}

fn bench_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("text");
    for (slides, bullets) in [(5usize, 6usize), (50, 8), (200, 8)] {
        let bytes = deck(slides, bullets);
        group.throughput(Throughput::Elements(slides as u64));
        group.bench_with_input(
            BenchmarkId::new("sequential", format!("{slides}x{bullets}")),
            &bytes,
            |b, bytes| {
                b.iter(|| {
                    let doc = Document::load(bytes.clone()).unwrap();
                    let mut total = 0;
                    for index in 0..doc.slide_count() {
                        total += doc.slide(index).unwrap().text().len();
                    }
                    total
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("parallel", format!("{slides}x{bullets}")),
            &bytes,
            |b, bytes| {
                b.iter(|| {
                    Document::load(bytes.clone())
                        .unwrap()
                        .text_reporting(&TextOptions::default())
                        .0
                        .len()
                })
            },
        );
    }
    group.finish();
}

fn bench_parse(c: &mut Criterion) {
    let bytes = deck(1, 20);
    let doc = Document::load(bytes).unwrap();
    let xml = doc.package().read_part("/ppt/slides/slide1.xml").unwrap();
    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(xml.len() as u64));
    group.bench_function("slide xml, 20 bullets", |b| {
        b.iter(|| parse_slide(&xml).unwrap().0.shapes.len())
    });
    group.finish();
}

criterion_group!(benches, bench_open, bench_text, bench_parse);
criterion_main!(benches);
