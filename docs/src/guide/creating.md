# Creating decks

`pptxboss-write` builds decks with one master, four layouts (title, title
and content, title only, blank), a theme of twelve colors, two fonts, a
type scale and optional embedded font files, and slides made of titles, subtitles, bullet lists, paragraphs
of formatted runs, text boxes, tables, pictures, solid, gradient or picture
backgrounds and speaker notes. Content given without a position is placed
by the layout engine. Output is deterministic: fixed timestamps, fixed
part order, ids in insertion order. Every deck it writes reads back
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
Content, a title with a subtitle uses Title, a title alone uses Title Only,
no title uses Blank. Title slides center the title in the first accent
color; bullets take the same color.

## Layout engine

Bullets, paragraphs and blocks are measured with font metrics embedded in
the crate (Carlito for Calibri, Liberation Sans for Arial and other
sans-serif fonts, Liberation Serif for Times New Roman and other serif
fonts, Liberation Mono for monospaced fonts, Caladea for Cambria, Gelasio
for Georgia) and placed below the title. When a slide is full the body
scale steps down, never below the theme's minimum size; what still does
not fit continues on the next slide with the same title, background and
layout, split evenly over the slides it needs. A table carries its header
row onto every continuation. A title that does not fit its frame shrinks
on its own. Body text that fits at full size on a titled slide is written
exactly as before; a slide without a title places its body from the top
margin, and bullets in free text boxes take the first accent color like
the body's.

```rust
use pptxboss_write::{Block, Slide};

Slide::titled("Text beside a picture")
    .columns(vec![
        Block::bullets(["Seven twelfths for the text", "Five for the picture"]),
        Block::picture_described(std::fs::read("chart.png")?, "cold starts by week"),
    ])
    .block(Block::table(rows, true));
```

A `Block` is text (`Block::text`, `Block::bullets`), a picture
(`Block::picture`), a table (`Block::table`) or `Block::columns`, which
sets its children side by side: two children of which one is a picture
split seven to five in the text's favor, otherwise columns are equal.
Blocks stack below the body in the order they are added. A picture keeps
its aspect ratio inside its column; a table sizes each row to its cells.
Pictures, tables and text boxes given a `Rect` stay where they are put.

The type scale lives on the theme: `TypeScale { display, title, subtitle,
body, table, minimum }` in points, with defaults of 54, 44, 24, 28, 16 and
18. Body text at level 0 takes `body`; each deeper level is four points
smaller, down to ten points below `body`.

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

A `Theme` holds the twelve colors, a title font and a body font. Twelve
presets ship: `office` (the default), `dark`, `slate`, `forest`, `sunset`,
`midnight`, `mocha`, `dracula`, `nord`, `tokyo`, `clay` and `mono`.
`mocha`, `dracula`, `nord` and `tokyo` carry the palettes of the editor
themes of the same names; `midnight` is near-black with an emerald accent;
`clay` and `mono` are light. Backgrounds are solid colors, linear gradients
or pictures and apply to the master through the theme, to one layout, or
to one slide. A dark background needs light text: the dark presets are
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

### Embedding fonts

A deck set in a font the viewer's machine lacks falls back to another
font, and the layout changes with it. `Theme::embed_font` stores a
TrueType or OpenType file in the deck as Embedded OpenType, the container
PowerPoint reads, so the deck renders in that font anywhere. The font data
is MicroType Express compressed, the way PowerPoint itself stores it, which
brings a file to roughly a third of its size; a file the coder cannot
handle is stored as it is. Files of one family fill its regular, bold,
italic and bold italic slots by the bold and italic flags they declare, so
a family with more weights than those four keeps the last file given for
each slot. Writing fails for a font whose license forbids embedding (the
`fsType` restricted bit) and for font collections.

```rust
use pptxboss_write::Theme;

let theme = Theme::mono()
    .font("Inter")
    .embed_font(std::fs::read("Inter-Regular.ttf")?)
    .embed_font(std::fs::read("Inter-Bold.ttf")?);
```

```sh
pptxboss create md out.pptx slides.md --font Inter --embed-font Inter-Regular.ttf --embed-font Inter-Bold.ttf
```

LibreOffice reads embedded fonts from 25.8 on, and only in builds with
EOT support (libeot), as the Debian and Fedora packages are; the builds
from libreoffice.org lack it, so a render there still substitutes.

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
`write.Theme("mine", colors={"accent1": "#123456"}, font="Georgia",
sizes={"body": 24, "minimum": 16}, embed_fonts=[open("Georgia.ttf", "rb").read()])`
builds a custom theme; `theme.embed_font(data)` adds a font file later;
`write.Theme.presets()` lists the presets.

Content without coordinates goes to the layout engine: `slide.picture(data)`
and `slide.table(rows)` place themselves, `slide.columns(...)` sets blocks
side by side and `slide.block(...)` stacks one. A block is a list of lines
(strings become bullets, a `Paragraph` keeps its formatting), a
`write.Picture`, a `write.Table` or a list of blocks.

```python
deck.add(
    write.Slide("Text beside a picture")
    .columns(["Seven twelfths for the text", "Five for the picture"], write.Picture(chart_png, description="cold starts"))
    .table(rows)
)
```

## Round trip

```rust
let bytes = deck.to_bytes()?;
let doc = pptxboss_core::Document::load(bytes.clone())?;
assert_eq!(doc.slide(1)?.text(), "Highlights\nRevenue up\nin every region");
assert!(pptxboss_check::check_bytes(bytes, &Default::default())?.findings.is_empty());
```

## Limitations

Pictures must be PNG, JPEG, GIF, BMP or TIFF; a TIFF placed by the layout
engine is assumed to be four by three. Tables have equal column widths, a
header rule in the first accent color and hairline row rules, and no cell
fills. Titles, subtitles and table cells are plain text. Text measurement
uses the metrics of the fonts named above; text set in another font is
measured as Liberation Sans or Liberation Serif, so its line count may
differ from the rendered one. Columns are never split across slides.
There is no chart, diagram or embedded object creation, and no editing of
existing decks.
