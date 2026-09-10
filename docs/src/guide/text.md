# Extracting text

## What you get

Shapes in z-order, which ECMA-376 makes both the paint order and the
reading order (19.3.1.45). Each shape's paragraphs come one per line; a
line break inside a paragraph stays a line break; field text (slide
numbers, dates) is included as cached. Tables give one line per row with
tab-separated cells, merged-away cells omitted. Groups are descended. Date,
footer, header and slide-number placeholders are left out by default, as
are hidden shapes. Text is never inherited from a layout or master, so an
empty placeholder is empty.

## CLI

```sh
pptxboss text deck.pptx                 # slides separated by a blank line
pptxboss text --headings deck.pptx      # --- slide N --- before each slide
pptxboss text --notes deck.pptx         # speaker notes after each slide
pptxboss text --furniture deck.pptx     # include date/footer/slide-number text
pptxboss text --skip-hidden deck.pptx   # leave out slides marked hidden
pptxboss text --json deck.pptx          # [{"number": 1, "text": "..."}, ...]
```

Anything the reader skipped is printed to stderr as `warning:` lines; the
exit code stays 0.

## Python

```python
doc = pptxboss.Document("deck.pptx")
doc.text()                                  # whole deck
doc.text(notes=True, hidden_slides=False)
doc.slide_texts()                           # one string per slide, in parallel
text, warnings = doc.text_reporting()       # what was skipped, one line each
doc[3].text(furniture=True)
doc[3].paragraphs()                         # every paragraph incl. table cells
```

## Rust

```rust
use pptxboss_core::{Document, TextOptions};

let doc = Document::open("deck.pptx")?;
let (text, report) = doc.text_reporting(&TextOptions { notes: true, ..TextOptions::default() });
for warning in report.warnings() {
    eprintln!("{warning}");
}
```

`Document::map_slides` runs a closure over every slide across the available
cores; `slide_texts` and `text_reporting` are built on it.

## Lenient semantics and reporting

A slide whose part is missing or malformed contributes an empty string and
an entry in `ExtractReport::failed_slides`. Graphic frames whose type the
reader does not know (anything but tables, charts, diagrams and embedded
objects) are counted in `unknown_graphics` with their URI. Elements in
unknown namespaces inside a shape tree are skipped and counted. Hidden
slides left out because of the options are counted separately and do not
make the report incomplete. `ExtractReport::is_complete` is true when
nothing was dropped for a reason other than the options.

## Encodings

Parts are UTF-8. A UTF-16 part is reported and skipped. Text uses the
`_xHHHH_` escape convention for control characters, which the reader
decodes.
