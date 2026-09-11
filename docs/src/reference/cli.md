# CLI reference

Every command takes a path to a `.pptx` (or `.ppt`, except `check`).
Warnings go to stderr as `warning:` lines; errors as `error:` lines. Exit code 1 means the input could not be read,
2 means bad usage, except for `check`, which uses 1 for findings at error
severity and 2 for an unreadable file.

`--threads N`, before or after the subcommand, caps the worker threads
used to parse slides; the default is every core, or the value of
`PPTXBOSS_THREADS` when that is set. `--threads 1` keeps everything on the
calling thread.

## `pptxboss info FILE [--json]`

Format (`pptx` or `ppt`), slide count, presentation part, slide size in EMU
and inches with its declared type, master count, part count, the core
properties that are set
(title, subject, creator, modified by, created, modified, application),
one `section:` line per section with its slide numbers, then one line per
slide: number, title or `(no title)`, and flags among `hidden`, `notes`,
`pictures`, `tables`, `objects`, `comments`. A slide that fails to parse
shows `(unreadable: reason)`.

## `pptxboss text FILE [--notes] [--comments] [--alt-text] [--no-charts] [--no-diagrams] [--furniture] [--hidden-shapes] [--skip-hidden] [--headings] [--json]`

Slide text, slides separated by a blank line; empty slides are skipped
unless `--headings` or `--json` is given. `--comments` appends
`[comment] Author: text` lines (replies as `[reply]`) after a slide's text
and notes; `--alt-text` adds the alternative text of pictures and other
shapes that have no text. Chart data and diagram text are included unless
`--no-charts` or `--no-diagrams` is given.

## `pptxboss markdown FILE [--notes] [--comments] [--skip-hidden] [--hidden-shapes] [--furniture] [--no-headings] [--no-images]`

The deck as Markdown on stdout: a `## Title` heading per slide, bullets,
paragraphs, GFM tables, images, chart tables and diagram outlines, slides
separated by a rule; `--notes` and `--comments` add block quotes. Warnings
go to stderr as for `text`.

## `pptxboss check FILE [--json] [--quiet] [--max-findings N] [--no-crc]`

Findings sorted most severe first, then a summary line
`FILE: ok|not ok: E error(s), W warning(s), P part(s) checked`.

## `pptxboss rules [--json]`

Every rule: severity, code, clause, summary.

## `pptxboss create blank OUT [--slides N] [--standard]`

## `pptxboss create text OUT --title T [--bullet B]... [--notes N] [--standard]`

## `pptxboss create md OUT INPUT [--standard] [--font F]`

`INPUT` may be `-` for standard input.

## `pptxboss skill install [--global]`, `pptxboss skill show`

Installs the bundled agent skill into `./.claude/skills/pptxboss/SKILL.md`
or `~/.claude/skills/pptxboss/SKILL.md`, or prints it.
