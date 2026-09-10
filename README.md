<h1 align="center">pptxboss</h1>

<p align="center">
  <strong>A PowerPoint engine written from scratch in Rust: read .pptx decks, extract text, notes, tables and images, verify them against ECMA-376. One core, a CLI, and pythonic bindings.</strong>
</p>

<p align="center">
  <a href="https://github.com/4thel00z/pptxboss/actions/workflows/ci.yaml"><img src="https://github.com/4thel00z/pptxboss/actions/workflows/ci.yaml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-2021-000000?logo=rust&logoColor=white" alt="Rust 2021">
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="MIT OR Apache-2.0"></a>
</p>

---

Reading a PowerPoint file should not require PowerPoint, a Java runtime, or a
pure-Python XML tree. pptxboss is a clean-room reader built from the ECMA-376
specification (Office Open XML): safe Rust, no C dependencies, no bindings to
another engine, one core behind the CLI and the Python extension. It is a
**lenient reader**: real decks are damaged, so it compensates for junk before
the archive, accepts data descriptors and Zip64 records, tolerates broken
content types and relationships, recovers the slide list when the
presentation part does not list it, and skips what it cannot read instead of
refusing, reporting every skip.

## Highlights

- **Clean-room engine**: implemented from ECMA-376 Parts 1 to 4 in safe Rust.
  The ZIP container, CRC-32, XML tokenizer, Open Packaging Conventions and
  PresentationML model are all in-tree; the only compression dependency is
  the pure-Rust `zlib-rs` inflate.
- **Reads only what it needs**: the central directory is parsed once, then
  parts are read with positioned reads. Extracting text from a 42 MB deck
  never touches its 40 MB of media.
- **Parallel by default**: slides are spread across cores with a
  work-stealing counter; each worker has private caches over a shared archive.
- **Strict and Transitional alike**: both namespace families resolve to the
  same element ids, and `mc:AlternateContent` is resolved per Part 3.
- **Two views of a package**: `Package` keeps every defect as written for
  the verifier; `Document` reads around them and says what it skipped.

## Install

```sh
cargo install pptxboss-cli
```

## Usage

```sh
pptxboss info deck.pptx             # slide count, size, one line per slide
pptxboss text deck.pptx             # slide text, slides separated by blank lines
pptxboss text --notes --headings deck.pptx
pptxboss text --json deck.pptx      # [{"number": 1, "text": "..."}, ...]
```

Text semantics: shapes in z-order (which ECMA-376 makes the reading order),
paragraphs one per line, line breaks preserved, fields included, table rows
one per line with tab-separated cells, groups descended, hidden shapes and
date/footer/slide-number placeholders left out unless asked for. Text is
never inherited from a layout or master, so empty placeholders stay empty.

```rust
use pptxboss_core::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = Document::open("deck.pptx")?;
    for slide in doc.slides() {
        let slide = slide?;
        println!("{}: {}", slide.number(), slide.title().unwrap_or_default());
        println!("{}", slide.text());
        if let Some(notes) = slide.notes_text()? {
            println!("notes: {notes}");
        }
    }
    let (text, report) = doc.text_reporting(&Default::default());
    for warning in report.warnings() {
        eprintln!("warning: {warning}");
    }
    println!("{text}");
    Ok(())
}
```

## Benchmarks

See [`benchmarks/`](benchmarks/README.md). Numbers are filled in from a quiet
run of `benchmarks/bench.py` on a real-world corpus.

## What's inside

| Crate | What it does |
|---|---|
| `pptxboss-core` | ZIP container with positioned reads, CRC-32, XML pull tokenizer, OPC package model, PresentationML document model, text extraction |
| `pptxboss-cli` | The `pptxboss` binary |
| `pptxboss-testkit` | In-memory ZIP and deck builders for tests; not published |

## Limitations

- Encrypted packages (OLE compound files with `EncryptionInfo`) and legacy
  binary `.ppt` files are detected and refused with a clear error, not read.
- UTF-16 encoded XML parts are not read.
- Interleaved ("piece") ZIP items are not reassembled.
- No rendering of slides to images.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
make ci
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option. Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project shall be dual licensed as above,
without any additional terms or conditions.
