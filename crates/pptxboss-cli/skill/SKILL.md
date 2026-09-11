---
name: pptxboss
description: Use when reading, extracting, verifying, or creating PowerPoint .pptx files with pptxboss, the from-scratch Rust PresentationML engine with a CLI and Python bindings. Triggers include extracting text, titles, speaker notes or tables from a deck, listing or saving the pictures on a slide, checking a .pptx against ECMA-376 (validation, lint, repair prompts), creating a deck from Markdown or from arguments, and inspecting a deck's slides and size.
---

# pptxboss

pptxboss reads and verifies `.pptx` decks and creates new ones. It is a
clean-room implementation of ECMA-376 in safe Rust: no PowerPoint, no Java,
no C. The reader is lenient and reports what it skips; the verifier is
strict and cites the clause behind every finding.

## Install

```sh
pip install pptxboss          # Python package
cargo install pptxboss-cli    # the pptxboss binary
pptxboss skill install        # this file into ./.claude/skills/pptxboss/
pptxboss skill install --global
```

## CLI

```sh
pptxboss info deck.pptx                 # slide count, size, one line per slide with flags
pptxboss info --json deck.pptx
pptxboss text deck.pptx                 # slide text; slides separated by a blank line
pptxboss text --notes --headings deck.pptx
pptxboss text --json deck.pptx          # [{"number": 1, "text": "..."}]
pptxboss text --furniture deck.pptx     # include date/footer/slide-number placeholders
pptxboss text --threads 1 deck.pptx     # cap worker threads (default: every core)
pptxboss check deck.pptx                # exit 0 clean, 1 errors, 2 unreadable
pptxboss check --json --quiet deck.pptx
pptxboss rules                          # every rule: severity, code, clause, summary
pptxboss create text out.pptx --title "T" --bullet "a" --bullet "b" --notes "n"
pptxboss create md out.pptx slides.md   # or '-' to read Markdown from stdin
pptxboss create blank out.pptx --slides 3 --standard
```

Text semantics: shapes in z-order, one paragraph per line, line breaks
kept, fields included, table rows one per line with tab-separated cells,
groups descended. Hidden shapes and date/footer/slide-number placeholders
are left out unless asked. Text is never inherited from layouts or masters.

Markdown for `create md`: `#` starts a title slide and the next paragraph
is its subtitle; `##` or `###` starts a content slide; `-`, `*`, `+`, `1.`
lines are bullets and indentation sets the level; `Notes:` starts speaker
notes; `---` starts an untitled slide; other lines are body paragraphs.

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
    slide.tables()                             # list of rows of cell texts
    for shape in slide.shapes():               # kind: text|picture|table|group|chart|diagram|ole|...
        shape.text; shape.placeholder; shape.frame; shape.children
    for image in slide.images():               # pictures with their image parts
        data = slide.image_bytes(image); image.content_type
    slide.hyperlink("rId3")                    # URL or internal part for a relationship id
doc.slides()                                   # all slides, parsed in parallel
doc.titles()
doc.text(notes=True)                           # whole deck, blank line between slides
text, warnings = doc.text_reporting()          # warnings: what the reader skipped
doc.slide_texts(hidden_slides=False)

findings = pptxboss.check("deck.pptx")         # verifier; most severe first
for f in findings:
    f.severity, f.code, f.clause, f.part, f.message
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
- Encrypted decks and legacy binary `.ppt` files are compound files, not
  packages; they are refused with a clear error.
- `check` exit code 1 means errors were found; warnings alone exit 0.
  Real PowerPoint output verifies clean; files from other writers often
  carry duplicate shape ids (PML019) or directory entries (ZIP006).
- Tables in extracted text use tabs between cells; pass `slide.tables()`
  for structured rows.
- `create` writes deterministic files: the same input gives identical bytes.

## Links

- Repository: https://github.com/4thel00z/pptxboss
- Standard: ECMA-376 5th edition, Parts 1 to 4 (free download from Ecma)
