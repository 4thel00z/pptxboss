# Creating decks

`pptxboss-write` builds decks with one master, four layouts (title, title
and content, title only, blank), a theme of twelve colors and two fonts,
and slides made of titles, subtitles, bullet lists, paragraphs of
formatted runs, text boxes, tables, pictures, solid, gradient or picture
backgrounds and speaker notes. Output is deterministic: fixed timestamps,
fixed part order, ids in insertion order. Every deck it writes reads back
through the core and passes the verifier with no findings.

## CLI

```sh
pptxboss create blank out.pptx --slides 3
pptxboss create text out.pptx --title "Hello" --bullet "one" --bullet "two" --notes "say hi"
pptxboss create md out.pptx slides.md
pptxboss create md out.pptx slides.md --theme dark --font Inter
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

## Styling

A paragraph is a list of runs. Each run carries bold, italic, underline,
strikethrough, size in points, a color, a font and an optional hyperlink.
Colors are RGB values or one of the twelve theme slots (dark1, light1,
dark2, light2, accent1 to accent6, hyperlink, followed_hyperlink).
Paragraphs take an alignment and space before and after in points.

```rust
use pptxboss_write::{Align, Color, Paragraph, Run, SchemeColor};

Paragraph::runs(vec![
    Run::text("Revenue "),
    Run::text("up 12%").bold().color(Color::Scheme(SchemeColor::Accent1)),
    Run::text(" (source)").link("https://example.com/report"),
])
.align(Align::Center)
.space_after(12)
```

A `Theme` holds the twelve colors, a title font and a body font. Five
presets ship: `office` (the default), `dark`, `slate`, `forest` and
`sunset`. Backgrounds are solid colors, linear gradients or pictures and
apply to the master through the theme, to one layout, or to one slide.
A dark background needs light text: the `dark` and `sunset` presets are
inverted, which swaps the light and dark slots for the whole deck, and
`Slide::inverted` or the `inverted` argument of `Theme::layout_background`
swaps them again for one layout or slide.

```rust
use pptxboss_write::{Background, Color, Layout, Presentation, Rgb, SchemeColor, Slide, Theme};

let theme = Theme::slate()
    .color(SchemeColor::Accent1, Rgb::new(0x2F, 0x80, 0xED))
    .fonts("Georgia", "Inter")
    .layout_background(
        Layout::Title,
        Background::linear(Color::rgb(0x1F, 0x29, 0x33), Color::rgb(0x3E, 0x4C, 0x59), 90),
        true,
    );
let deck = Presentation::new()
    .theme(theme)
    .slide(Slide::title_slide("Slate", Some("light title on a dark gradient")))
    .slide(Slide::titled("Photo").background(Background::picture(std::fs::read("bg.png")?)).inverted());
```

### Python

```python
from pptxboss import write

theme = write.Theme.preset("dark").layout_background("title", write.Background.linear("#101010", "accent1"))
deck = write.Presentation(theme=theme)
deck.add(write.Slide("Dark deck", subtitle="Q3", layout="title"))
deck.add(
    write.Slide("Highlights", background=write.Background.solid("#FFFFFF"), inverted=True)
    .bullet(["Revenue ", write.Run("up 12%", bold=True, color="accent1")])
    .paragraph("Source: finance", align="right", size=12, color="#666666")
)
deck.save("dark.pptx")
```

Colors in Python are `"#RRGGBB"` strings or slot names.
`write.Theme("mine", colors={"accent1": "#123456"}, font="Georgia")` builds
a custom theme; `write.Theme.presets()` lists the presets.

## Round trip

```rust
let bytes = deck.to_bytes()?;
let doc = pptxboss_core::Document::load(bytes.clone())?;
assert_eq!(doc.slide(1)?.text(), "Highlights\nRevenue up\nin every region");
assert!(pptxboss_check::check_bytes(bytes, &Default::default())?.findings.is_empty());
```

## Limitations

Pictures must be PNG, JPEG, GIF, BMP or TIFF; their box is given
explicitly. Tables use one built-in style and equal column widths. Titles,
subtitles and table cells are plain text. There is no chart, diagram or
embedded object creation, and no editing of existing decks.
