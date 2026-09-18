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

## Where to go next

[Installation](./installation.md) covers the wheel, the binary and the crates;
the [Quickstart](./quickstart.md) shows the CLI, Python and Rust doing real
work. The guide then takes one task per chapter:

- [Extracting text](./guide/text.md): slide text, reading order, slide
  selection, warnings for what was skipped.
- [Notes, tables and structure](./guide/structure.md): speaker notes,
  tables, the shape tree, titles and hyperlinks.
- [Extracting images](./guide/images.md): each slide's pictures and the
  bytes of their image parts, read only when asked for.
- [Verifying a deck](./guide/verifying.md): the clause-numbered rules, their
  codes and severities.
- [Creating decks](./guide/creating.md): titles, bullets, text boxes, tables,
  pictures and notes from the CLI and Rust.
- [Markdown to slides](./guide/markdown.md): headings become slides, list
  items bullets and `Notes:` lines speaker notes.

The reference section holds the [CLI reference](./reference/cli.md), the
[Python API](./reference/python.md), the [Rust crates](./reference/rust.md),
the [verifier rules](./reference/rules.md) and the list of
[limitations](./reference/limitations.md).

pptxboss is dual-licensed under MIT or Apache-2.0, at your option.
