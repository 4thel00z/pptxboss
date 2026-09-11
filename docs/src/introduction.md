# Introduction

pptxboss reads, verifies and creates PowerPoint `.pptx` files. It is a
clean-room implementation of ECMA-376 (Office Open XML) in safe Rust: the
ZIP container, the CRC-32, the XML tokenizer, the Open Packaging
Conventions, the DEFLATE decoder and the PresentationML model are all
written from the specification; the reader has no compression dependency.
One core sits behind the command line, the Rust crates and the Python
extension.

## Leniency

Real decks are damaged. The reader compensates for junk before the
archive, accepts data descriptors and Zip64 records, rebuilds a missing
central directory from local file headers, tolerates broken content types
and relationships, recovers the slide list from relationships when the
presentation part does not list it, and skips what it cannot read instead
of refusing. Leniency never hides what it cost: every lossy operation has a
reporting twin, and the CLI prints what was skipped to stderr.

## Two views of a package

`Package` is the raw Open Packaging Conventions view: every item, content
type and relationship exactly as written, defects included. `Document`
sits on top and is lenient. The verifier reads both, which is how it can
report the defects the reader silently works around.

## Scope

Text extraction with paragraphs, line breaks, fields, tables, groups,
charts and diagrams; Markdown output; speaker notes; comments of both
flavours; sections; core and application properties; titles; pictures with
their image parts; embedded objects; hyperlinks; alternative text; slide
structure as a shape tree; a verifier with 72 clause-numbered rules; deck
creation with titles, bullets, paragraphs, text boxes, tables, pictures and
notes; Markdown to slides. Rendering slides to images is out of scope.

## Chapters

The guide chapters each cover one task, CLI first, then Python, then Rust,
and end with what the operation leaves out. The references are inventories.
