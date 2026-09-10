<h1 align="center">pptxboss</h1>

<p align="center">
  <strong>A PowerPoint engine written from scratch in Rust: read .pptx decks, extract text, notes, tables and images, verify them against ECMA-376. One core, a CLI, and pythonic bindings.</strong>
</p>

<p align="center">
  <a href="https://github.com/4thel00z/pptxboss/actions/workflows/ci.yaml"><img src="https://github.com/4thel00z/pptxboss/actions/workflows/ci.yaml/badge.svg" alt="CI"></a>
  <a href="https://github.com/4thel00z/pptxboss/actions/workflows/python-ci.yml"><img src="https://github.com/4thel00z/pptxboss/actions/workflows/python-ci.yml/badge.svg" alt="python-ci"></a>
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
- **A lean verifier**: `pptxboss check` runs 71 structural rules from
  ECMA-376 Parts 1 and 2 over the container, part names, content types,
  relationships, required parts, id ranges and XML well-formedness. Every
  finding carries a stable code, a severity and the clause it enforces.
  Real PowerPoint output verifies clean; the rules were calibrated against
  790 public test decks.
- **Fastest measured**: about 2.7x the throughput of the next fastest
  library from Python, with paragraph-for-paragraph agreement on the
  extracted text ([benchmarks](#benchmarks)).

## Install

```sh
pip install pptxboss          # Python package with the extension module
cargo install pptxboss-cli    # the pptxboss binary
```

## Usage

```sh
pptxboss info deck.pptx             # slide count, size, one line per slide
pptxboss text deck.pptx             # slide text, slides separated by blank lines
pptxboss text --notes --headings deck.pptx
pptxboss text --json deck.pptx      # [{"number": 1, "text": "..."}, ...]
pptxboss check deck.pptx            # verify against ECMA-376; exit 1 on errors
pptxboss check --json --quiet deck.pptx
pptxboss rules                      # every rule with its code, severity and clause
pptxboss create text out.pptx --title "Hello" --bullet "one" --bullet "two" --notes "say hi"
pptxboss create md out.pptx deck.md # '#' title slide, '##' content slides, list items, Notes:
pptxboss create blank out.pptx --slides 3
```

Text semantics: shapes in z-order (which ECMA-376 makes the reading order),
paragraphs one per line, line breaks preserved, fields included, table rows
one per line with tab-separated cells, groups descended, hidden shapes and
date/footer/slide-number placeholders left out unless asked for. Text is
never inherited from a layout or master, so empty placeholders stay empty.

```python
import pptxboss

doc = pptxboss.Document("deck.pptx")
print(doc.slide_count, doc.slide_size)
for slide in doc:                      # slides parse lazily
    print(slide.number, slide.title)
    print(slide.text())                # z-order, one paragraph per line
    print(slide.notes())               # speaker notes or None
    for table in slide.tables():       # rows of cell texts
        print(table)
    for image in slide.images():       # pictures with their image parts
        data = slide.image_bytes(image)
text, warnings = doc.text_reporting()  # whole deck, plus what was skipped

for finding in pptxboss.check("deck.pptx"):   # the verifier, most severe first
    print(finding.severity, finding.code, finding.clause, finding.message)
```

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

## Create decks

```rust
use pptxboss_write::{Presentation, Rect, Slide};

let deck = Presentation::new()
    .slide(Slide::title_slide("Quarterly review", Some("Q3 2026")))
    .slide(Slide::titled("Highlights").bullet("Revenue up").sub_bullet("in every region", 1).notes("Pause here"))
    .slide(Slide::titled("Numbers").table(Rect::inches(1.0, 1.8, 11.0, 2.0), vec![vec!["Region".into(), "Growth".into()], vec!["EMEA".into(), "12%".into()]], true));
deck.write_to("review.pptx")?;
```

Output is deterministic (fixed timestamps, fixed part order), reads back
through `pptxboss-core`, and passes `pptxboss check` with no findings.

## Benchmarks

Text extraction from Python, two real-world PowerPoint decks (7 and 43
slides, 3 MB and 42 MB), best of 5 per file, warm cache, Apple M3 Pro. A
file counts only when every engine handles it and the per-slide paragraphs
agree with python-pptx after whitespace normalization; both files pass.

| Library | slides/s | files/s |
|---|--:|--:|
| pptxboss | 2,473 | 98.9 |
| office-oxide | 909 | 36.3 |
| undoc | 180 | 7.2 |
| python-pptx | 136 | 5.4 |
| kreuzberg | 94 | 3.7 |
| markitdown | 32 | 1.3 |

pptxboss spreads slides across cores; the other Rust engines run one
thread per file. In-process, the 43-slide deck opens in 1.8 ms and yields
its text in 12 ms on one thread and 5.8 ms on twelve. Numbers are
machine-dependent; reproduce with [`benchmarks/bench.py`](benchmarks/README.md).

## What's inside

| Crate | What it does |
|---|---|
| `pptxboss-core` | ZIP container with positioned reads, CRC-32, XML pull tokenizer, OPC package model, PresentationML document model, text extraction |
| `pptxboss-check` | The verifier: 71 clause-numbered rules over package and presentation structure |
| `pptxboss-write` | Creates decks: titles, bullets, paragraphs, text boxes, tables, pictures, notes; Markdown to slides; deterministic output that verifies clean |
| `pptxboss-cli` | The `pptxboss` binary |
| `pptxboss-py` | The `pptxboss._pptxboss` extension module behind the Python package |
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
maturin develop && pytest
make ci
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your
option. Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project shall be dual licensed as above,
without any additional terms or conditions.
