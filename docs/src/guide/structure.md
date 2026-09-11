# Notes, tables and structure

## Speaker notes

Notes are stored in a notes slide part linked from the slide. The reader
takes the `body` placeholder of that part; when there is none, every text
shape other than the slide image and the date, footer and slide-number
placeholders.

```sh
pptxboss text --notes deck.pptx
```

```python
doc[1].notes()          # str or None
```

```rust
slide.notes_text()?     // Option<String>
```

## Tables

```python
for rows in slide.tables():
    for row in rows:
        print(row)          # cell texts, merged-away cells omitted
```

In Rust a table is `Content::Table(Table)` with `column_widths` and `rows`
of `Cell { body, grid_span, row_span, h_merge, v_merge }`. `Cell::is_origin`
is false for cells merged into another.

## The shape tree

```python
for shape in slide.shapes():
    shape.kind          # text, picture, table, group, chart, diagram, ole, connector, content_part, unknown
    shape.id, shape.name, shape.hidden, shape.placeholder, shape.placeholder_index
    shape.text          # for text shapes
    shape.frame         # (x, y, cx, cy) in EMU when the shape has a transform
    shape.children      # for groups
```

In Rust, `SlideContent::shapes` holds the top-level `Shape` values and
`SlideContent::walk()` yields every shape in document order, descending
into groups. `Shape::placeholder` holds the placeholder kind and index;
`Shape::is_title` is true for title and centered-title placeholders.

## Titles

`Slide::title()` returns the first title placeholder's text. Decks that
put their titles in plain text boxes have no title placeholder, and the
info listing shows `(no title)` for them.

## Hyperlinks

Runs hold the relationship id of a click hyperlink in `RunProps::hyperlink`;
`Slide::hyperlink_target(rel_id)` resolves it to an external URL or an
internal part name.
