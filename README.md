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
the archive, accepts data descriptors and Zip64 records, reads UTF-16 parts and
interleaved pieces, tolerates broken content types and relationships,
recovers the slide list when the presentation part does not list it, and
skips what it cannot read instead of refusing, reporting every skip.

## Highlights

- **Clean-room engine**: implemented from ECMA-376 Parts 1 to 4 in safe Rust.
  The ZIP container, DEFLATE decoder, CRC-32, XML tokenizer, Open Packaging
  Conventions and PresentationML model are all in-tree; the reader has no
  compression dependency.
- **Reads only what it needs**: the central directory is parsed once, then
  parts are read with positioned reads. Extracting text from a 42 MB deck
  never touches its 40 MB of media.
- **Fast on one core, faster on all**: a deck opens with one positioned
  read for small files and one per part otherwise, and slides are spread
  across cores with a work-stealing counter, each worker with private caches
  over a shared archive. `--threads 1` keeps everything on the calling
  thread and is still the fastest engine measured ([benchmarks](#benchmarks)).
- **Strict and Transitional alike**: both namespace families resolve to the
  same element ids, and `mc:AlternateContent` is resolved per Part 3.
- **Two views of a package**: `Package` keeps every defect as written for
  the verifier; `Document` reads around them and says what it skipped.
- **The whole deck, not just the slides**: speaker notes, comments of both
  flavours (the 2006 comments part and the threaded 2018 one) with their
  authors, sections, core and application properties, embedded objects with
  their bytes, pictures with their image parts, hyperlinks, and alternative
  text on request. Chart titles, series, categories and values and the text
  of SmartArt diagrams come out with the slide text.
- **Markdown output**: `pptxboss markdown` renders a deck as a heading per
  slide, bullets with their levels, GFM tables, images, chart tables and
  diagram outlines, with notes and comments as block quotes on request.
- **A lean verifier**: `pptxboss check` runs 72 structural rules from
  ECMA-376 Parts 1 and 2 over the container, part names, content types,
  relationships, required parts, id ranges and XML well-formedness. Every
  finding carries a stable code, a severity and the clause it enforces.
  Real PowerPoint output verifies clean; the rules were calibrated against
  790 public test decks.
- **Fastest measured**: 1,121 files/s extracting text over a 631-file
  public corpus, and 1,062 files/s when held to one thread: about 3x the
  next fastest Rust engine either way and 27x the most-used Python library,
  with paragraph-for-paragraph agreement on every gated file
  ([benchmarks](#benchmarks)).
- **Reads Strict decks**: the Open XML SDK's Strict-namespace test decks,
  which most readers refuse, read and verify like any other.

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
pptxboss text --comments --alt-text deck.pptx   # comments after each slide; alt text of pictures
pptxboss markdown --notes deck.pptx # the deck as Markdown, notes as block quotes
pptxboss text --json deck.pptx      # [{"number": 1, "text": "..."}, ...]
pptxboss text --threads 1 deck.pptx # cap the worker threads (default: every core)
pptxboss check deck.pptx            # verify against ECMA-376; exit 1 on errors
pptxboss check --json --quiet deck.pptx
pptxboss rules                      # every rule with its code, severity and clause
pptxboss create text out.pptx --title "Hello" --bullet "one" --bullet "two" --notes "say hi"
pptxboss create md out.pptx deck.md # '#' title slide, '##' content slides, list items, Notes:
pptxboss create blank out.pptx --slides 3
```

Text semantics: shapes in z-order (which ECMA-376 makes the reading order),
paragraphs one per line, line breaks preserved, fields included, table rows
one per line with tab-separated cells, groups descended, chart titles and
data as rows, diagram nodes one per line, hidden shapes and
date/footer/slide-number placeholders left out unless asked for. Text is
never inherited from a layout or master, so empty placeholders stay empty.

```python
import pptxboss

doc = pptxboss.Document("deck.pptx")   # threads=1 to stay on one core
print(doc.slide_count, doc.slide_size)
for slide in doc:                      # slides parse lazily
    print(slide.number, slide.title)
    print(slide.text())                # z-order, one paragraph per line
    print(slide.notes())               # speaker notes or None
    for table in slide.tables():       # rows of cell texts
        print(table)
    for image in slide.images():       # pictures with their image parts
        data = slide.image_bytes(image)
    for comment in slide.comments():   # author, date, text; replies flagged
        print(comment.author, comment.text)
    for obj in slide.embedded_objects():  # p:oleObj with prog_id and part
        data = slide.object_bytes(obj)
    for chart in slide.charts():       # title, kinds, series with categories and values
        print(chart.title, [s.name for s in chart.series])
    for diagram in slide.diagrams():   # SmartArt as (level, text) items
        print(diagram.items)
print(doc.markdown(notes=True))        # the deck as Markdown
props = doc.core_properties()          # title, creator, created, modified, ...
for section in doc.sections():         # name and zero-based slide indexes
    print(section.name, section.slides)
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

**pptxboss is the fastest library measured, on one thread as well as on
all cores: about 3x the next fastest Rust engine and 27x python-pptx, with
paragraph-for-paragraph agreement on every file that passes the gate.**

Text extraction from Python over the 737 `.pptx` files of the LibreOffice,
Apache POI, python-pptx, pandoc and Open XML SDK test suites, best of 3 per
file after a warm-up pass, aggregated over the 631 files every engine
handled, Apple M3 Pro. A file counts only when pptxboss reports nothing
skipped and its per-slide paragraphs match python-pptx after whitespace
normalization: 636 files pass, and not one is excluded for a disagreement.
The 101 exclusions are 86 Strict-namespace decks python-pptx cannot open,
11 fuzzer-minimized archives, one encrypted deck, and two fuzzer cases
pptxboss reports as unreadable.

| Library | files/s | slides/s |
|---|--:|--:|
| pptxboss, all cores | 1,121 | 2,434 |
| pptxboss, one thread (`threads=1`) | 1,062 | 2,306 |
| office-oxide | 352 | 765 |
| undoc | 207 | 448 |
| kreuzberg | 165 | 357 |
| python-pptx | 41 | 89 |
| markitdown | 5.2 | 11 |

<details>
<summary>Method and fine print</summary>

Every engine is called from Python through its own adapter. pptxboss
spreads a deck's slides across cores unless `threads=1` holds it to the
calling thread; the one-thread row is the like-for-like comparison, since
the other Rust engines run one thread per file (office-oxide's wheel was
measured at 0.9 to 1.1 CPU seconds per wall second). The test-suite corpus
is small files, so the rows are dominated by per-file cost: opening the
file, one positioned read for the whole archive when it is small, parsing
the directory, the relationships and a few slides.

On two real-world PowerPoint decks (7 and 43 slides, 3 MB and 42 MB) the
same harness, best of 40:

| Deck | office-oxide | pptxboss, one thread | pptxboss, all cores |
|---|--:|--:|--:|
| 43 slides, wall | 36.5 ms | 10.8 ms | 3.9 ms |
| 43 slides, CPU | 39.0 ms | 12.1 ms | 13.7 ms |
| 7 slides, wall | 9.0 ms | 2.4 ms | 1.7 ms |
| 7 slides, CPU | 9.8 ms | 2.6 ms | 4.0 ms |

In-process, the 43-slide deck opens in 1.4 ms with positioned reads,
tokenizes its 443 KiB of slide XML in about 4 ms, and yields its text in
11.5 ms on one thread and 4.2 ms on twelve.

The gate compares pptxboss against python-pptx only, because the other
engines do not expose per-slide paragraphs. Numbers are machine-dependent;
reproduce with [`benchmarks/bench.py`](benchmarks/README.md) after
`benchmarks/corpora/fetch_public.sh`. Engine versions are recorded in
`benchmarks/results.json`.

</details>

## What's inside

| Crate | What it does |
|---|---|
| `pptxboss-core` | ZIP container with positioned reads, DEFLATE decoder, CRC-32, XML pull tokenizer, OPC package model, PresentationML document model, charts, diagrams, comments, properties, text and Markdown extraction |
| `pptxboss-check` | The verifier: 72 clause-numbered rules over package and presentation structure |
| `pptxboss-write` | Creates decks: titles, bullets, paragraphs, text boxes, tables, pictures, notes; Markdown to slides; deterministic output that verifies clean |
| `pptxboss-cli` | The `pptxboss` binary |
| `pptxboss-py` | The `pptxboss._pptxboss` extension module behind the Python package |
| `pptxboss-testkit` | In-memory ZIP and deck builders for tests; not published |

## Limitations

- Encrypted packages (OLE compound files with `EncryptionInfo`) and legacy
  binary `.ppt` files are detected and refused with a clear error, not read.
- Interleaved ("piece") items are reassembled, but no public test deck uses
  them; the only evidence is the testkit fixture built from the OPC text.
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
