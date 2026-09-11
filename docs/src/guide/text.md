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
pptxboss text --slides 2-4,7 deck.pptx  # only those slides, in that order
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
cores, or across the cap set with `Document::with_threads`; `slide_texts`
and `text_reporting` are built on it. `map_slides_at`, `slide_texts_at`
and `markdown_at` take a list of zero-based indices instead and keep the
written order, which is what `--slides` uses.

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

Parts are UTF-8 or UTF-16; a UTF-16 part (byte order mark or `<?` in
either byte order) is transcoded before parsing, and the verifier notes it.
Text uses the `_xHHHH_` escape convention for control characters, which
the reader decodes.

## Charts and diagrams

A chart contributes its title and a table of its cached data: a header of
series names, then one row per category with each series' value, cells
separated like table cells. Series with differing categories (scatter and
bubble charts) become one row each. A diagram (SmartArt) contributes one
line per node in tree order. Both are on by default and switched off with
`TextOptions::charts` and `TextOptions::diagrams` (`--no-charts`,
`--no-diagrams`). `Slide::charts` and `Slide::diagrams` return the data
structured.

## Markdown

`Document::markdown` and `pptxboss markdown` render the deck: a `##`
heading per slide (the title, or `Slide N`), bullets with their levels,
plain paragraphs, bold, italic and links, GFM tables, `![alt](part)`
images, a `**Chart: title**` table per chart, diagram outlines, embedded
objects as italic labels, and speaker notes and comments as block quotes
when asked. Slides are separated by a rule. Paragraphs that inherit their
bullet from the list style are bullets inside body placeholders and plain
text elsewhere.

## Comments and alternative text

`TextOptions::comments` appends each slide's comments after its text and
notes, one line each: `[comment] Author: text`, replies indented as
`[reply]`. `TextOptions::alt_text` emits the `descr` of shapes that have
no text of their own (pictures, charts, diagrams, embedded objects). Both
are off by default so extracted text stays paragraph-for-paragraph
comparable with the slide content. `Slide::comments` returns the same
comments structured, with author, initials and date.
