# Creating decks

`pptxboss-write` builds decks with one master, four layouts (title, title
and content, title only, blank), a theme, and slides made of titles,
subtitles, bullet lists, paragraphs, text boxes, tables, pictures and
speaker notes. Output is deterministic: fixed timestamps, fixed part order,
ids in insertion order. Every deck it writes reads back through the core
and passes the verifier with no findings.

## CLI

```sh
pptxboss create blank out.pptx --slides 3
pptxboss create text out.pptx --title "Hello" --bullet "one" --bullet "two" --notes "say hi"
pptxboss create md out.pptx slides.md
```

## Rust

```rust
use pptxboss_write::{Metadata, Paragraph, Presentation, Rect, Slide, SlideSize};

let deck = Presentation::new()
    .size(SlideSize::WIDESCREEN)
    .metadata(Metadata { title: Some("Review".into()), ..Metadata::default() })
    .slide(Slide::title_slide("Quarterly review", Some("Q3 2026")))
    .slide(Slide::titled("Highlights").bullet("Revenue up").sub_bullet("in every region", 1).notes("Pause here"))
    .slide(Slide::titled("Free form")
        .text_box(Rect::inches(1.0, 1.5, 5.0, 1.0), vec![Paragraph::text("Bold claim").bold().size(28)])
        .picture(std::fs::read("chart.png")?, Rect::inches(7.0, 1.5, 5.0, 3.0))
        .table(Rect::inches(1.0, 4.0, 11.0, 2.0), vec![vec!["Region".into(), "Growth".into()], vec!["EMEA".into(), "12%".into()]], true));
deck.write_to("review.pptx")?;
```

The layout is inferred when not set: a title with body text uses Title and
Content, a title alone uses Title Only, no title uses Blank.

## Round trip

```rust
let bytes = deck.to_bytes()?;
let doc = pptxboss_core::Document::load(bytes.clone())?;
assert_eq!(doc.slide(1)?.text(), "Highlights\nRevenue up\nin every region");
assert!(pptxboss_check::check_bytes(bytes, &Default::default())?.findings.is_empty());
```

## Limitations

Pictures must be PNG, JPEG, GIF, BMP or TIFF; their box is given
explicitly. Tables use one built-in style and equal column widths. There
is no chart, diagram or embedded object creation, and no editing of
existing decks.
