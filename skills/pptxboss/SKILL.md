---
name: pptxboss
description: Use when reading, extracting, verifying, or creating PowerPoint .pptx (and reading legacy .ppt) files with pptxboss, the from-scratch Rust PresentationML engine with a CLI and Python bindings. Triggers include extracting text, titles, speaker notes or tables from a deck, listing or saving the pictures on a slide, checking a .pptx against ECMA-376 (validation, lint, repair prompts), creating a deck from Markdown or from arguments, and inspecting a deck's slides and size.
---

# pptxboss

pptxboss reads and verifies `.pptx` decks, reads legacy `.ppt` decks, and
creates new ones. It is a clean-room implementation of ECMA-376 (and of the
MS-PPT binary format) in safe Rust: no PowerPoint, no Java, no C. The reader is lenient and reports what it skips; the verifier is
strict and cites the clause behind every finding.

## Install

```sh
pip install pptxboss          # Python package
cargo install pptxboss-cli    # the pptxboss binary
pptxboss skill install        # this file into ./.claude/skills/pptxboss/
pptxboss skill install --global   # into ~/.claude/skills/pptxboss/
npx skills add 4thel00z/pptxboss   # the same file through skills.sh
```

## CLI

```sh
pptxboss info deck.pptx                 # slide count, size, properties, sections, one line per slide with flags
pptxboss info --json deck.pptx
pptxboss text deck.pptx                 # slide text; slides separated by a blank line
pptxboss text --notes --headings deck.pptx
pptxboss text --json deck.pptx          # [{"number": 1, "text": "..."}]
pptxboss text --slides 2-4,7 deck.pptx  # only those slides, in that order; also on info and markdown
pptxboss text --furniture deck.pptx     # include date/footer/slide-number placeholders
pptxboss text --comments --alt-text deck.pptx   # comments after each slide; alt text of pictures
pptxboss text --no-charts --no-diagrams deck.pptx   # slide text without chart data or SmartArt
pptxboss markdown --notes deck.pptx     # the deck as Markdown: headings, bullets, tables, images, charts
pptxboss text --threads 1 deck.pptx     # cap worker threads (default: every core)
pptxboss check deck.pptx                # exit 0 clean, 1 errors, 2 unreadable
pptxboss check --json --quiet deck.pptx
pptxboss rules                          # every rule: severity, code, clause, summary
pptxboss create text out.pptx --title "T" --bullet "a" --bullet "b" --notes "n"
pptxboss create md out.pptx slides.md   # or '-' to read Markdown from stdin
pptxboss create md out.pptx slides.md --theme dark --font Inter   # presets: office, dark, slate, forest, sunset, midnight, mocha, dracula, nord, tokyo, clay, mono
pptxboss create blank out.pptx --slides 3 --standard
```

Text semantics: shapes in z-order, one paragraph per line, line breaks
kept, fields included, table rows one per line with tab-separated cells,
groups descended, chart title and data as rows, diagram nodes one per
line. Hidden shapes and date/footer/slide-number placeholders are left out
unless asked. Text is never inherited from layouts or masters.

Markdown for `create md`: `#` starts a title slide and the next paragraph
is its subtitle; `##` or `###` starts a content slide; `-`, `*`, `+`, `1.`
lines are bullets and indentation sets the level; `Notes:` starts speaker
notes; `---` starts an untitled slide; other lines are body paragraphs. A
slide with more bullets than fit shrinks them to the theme's minimum size
and continues on the next slide with the same title.

## Python

```python
import pptxboss

doc = pptxboss.Document("deck.pptx")          # or Document(data=bytes), threads=1 for one core
doc.slide_count; len(doc); doc.slide_size; doc.slide_size_type
for slide in doc:                              # lazy, parsed on demand
    slide.number; slide.title; slide.hidden; slide.name
    slide.text()                               # z-order text, options: furniture=, hidden_shapes=
    slide.paragraphs()                         # every non-empty paragraph incl. table cells
    slide.notes()                              # speaker notes or None
    slide.comments()                           # Comment(author, initials, date, text, reply)
    slide.charts()                             # Chart(title, kinds, series=[ChartSeries(name, categories, values)])
    slide.diagrams()                           # Diagram(items=[(level, text), ...])
    slide.markdown(notes=True)                 # this slide as Markdown
    slide.embedded_objects()                   # EmbeddedObject(prog_id, part, ...); slide.object_bytes(obj)
    slide.tables()                             # list of rows of cell texts
    for shape in slide.shapes():               # kind: text|picture|table|group|chart|diagram|ole|...
        shape.text; shape.placeholder; shape.frame; shape.children
        shape.paragraphs                       # Paragraph(runs=[Run(bold, italic, size, hyperlink, ...)]) for text shapes
        shape.table                            # Table(rows=[Row(cells=[Cell(text, grid_span, row_span, is_origin)])]) for tables
    for image in slide.images():               # pictures with their image parts
        data = slide.image_bytes(image); image.content_type
    slide.hyperlink("rId3")                    # URL or internal part for a relationship id
doc.slides()                                   # all slides, parsed in parallel
doc.titles()
doc.core_properties(); doc.app_properties()    # docProps metadata or None
doc.sections()                                 # Section(name, slides) with zero-based indexes
doc.text(notes=True, comments=True, alt_text=True)  # whole deck, blank line between slides
doc.markdown(notes=True, comments=True)        # whole deck as Markdown, slides separated by ---
text, warnings = doc.text_reporting()          # warnings: what the reader skipped
text, report = doc.extract(indexes=[0, 2])     # chosen slides; ExtractReport(failed_slides, is_complete, warnings, ...)
doc.slide_texts(hidden_slides=False)
doc.slide_texts(indexes=[2, 0]); doc.markdown(indexes=[1])  # zero-based, negatives from the end, written order
doc.defects; doc.presentation(); doc.comment_authors()      # how slides were found; sldId/master lists; authors
package = doc.package()                        # raw parts, content types, relationships (or Package(path))
package.parts(); package.read("/ppt/presentation.xml"); package.rels("/ppt/presentation.xml"); package.defects

findings = pptxboss.check("deck.pptx")         # verifier; most severe first
for f in findings:
    f.severity, f.code, f.clause, f.part, f.message
report = pptxboss.check_report("deck.pptx")    # .findings, .parts_checked, .truncated, .errors, .is_clean, .codes
pptxboss.rules()                               # every rule with code, severity, clause, summary
```

Errors raise `pptxboss.PptxError`. Documents are frozen and thread-safe;
heavy calls release the GIL. The `.pyi` stubs are the authoritative API.

## Rust

Crates: `pptxboss-core` (container, package, document model, text),
`pptxboss-check` (verifier), `pptxboss-write` (deck creation),
`pptxboss-cli`. `Document::open` reads only the parts it needs;
`Document::map_slides` spreads work across cores, `Document::with_threads`
caps it.

## Gotchas

- A slide that fails to parse becomes a warning and an empty text, never an
  error for the whole deck; read `text_reporting()` or stderr for it.
- Comments and alternative text are off by default in every text call so
  the output stays comparable with the slide content; pass `--comments`,
  `--alt-text` or the matching keyword arguments.
- Legacy `.ppt` files read through every command and API (text, notes,
  titles, pictures, hidden flags, Markdown); `check` refuses them because
  the verifier covers ECMA-376 packages only. Password-protected files of
  either format are refused, not decrypted.
- `check` exit code 1 means errors were found; warnings alone exit 0.
  Decks saved by PowerPoint pass with no findings; files from other writers
  often have duplicate shape ids (PML019) or directory entries (ZIP006).
- Tables in extracted text use tabs between cells; `slide.tables()` gives
  rows of cell text and `shape.table` the grid with spans and merges.
- `create` writes deterministic files: the same input gives identical bytes.

## Links

- Repository: https://github.com/4thel00z/pptxboss
- Standard: ECMA-376 5th edition, Parts 1 to 4 (free download from Ecma)
