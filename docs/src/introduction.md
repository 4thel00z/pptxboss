# Introduction

pptxboss reads, verifies and creates PowerPoint `.pptx` files, and reads
legacy `.ppt` files through the same API. It is a clean-room
implementation of ECMA-376 (Office Open XML) in safe Rust, with no
compression or XML dependency. The same core is used by the command line,
the Rust crates and the Python extension. The PowerPoint 97-2003 binary
format is read from the MS-CFB, MS-PPT and MS-ODRAW specifications into
the same slide model, so text, notes, titles and pictures are read from a
`.ppt` the same way.

## Leniency

Real decks are damaged. The reader tolerates bytes before the archive, a
missing central directory, and broken content types and relationships,
recovers the slide list from relationships when the presentation part does
not list it, and skips what it cannot read instead of refusing. Every skip
is reported: the CLI prints it to stderr as a `warning:` line, and
`text_reporting` returns it with the text.

## Two views of a package

`Package` is the raw Open Packaging Conventions view: every item, content
type and relationship exactly as written, defects included. `Document` is
the lenient reader built on it. The verifier reads both, which is how it
can report the defects the reader works around.

## Scope

Text extraction with paragraphs, line breaks, fields, tables, groups,
charts and diagrams; Markdown output; speaker notes; comments, threaded
comments included; sections; core and application properties; titles;
pictures with their image parts; embedded objects; hyperlinks; alternative
text; slide structure as a shape tree; a verifier with 72 clause-numbered
rules; deck creation with titles, bullets, paragraphs, text boxes, tables,
pictures and notes; Markdown to slides. Rendering slides to images is out
of scope.

## Chapters

Each guide chapter covers one task, CLI first, then Python, then Rust, and
ends with what the operation leaves out. The reference pages list every
command, class and rule.
