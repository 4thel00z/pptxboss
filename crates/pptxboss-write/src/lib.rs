//! Creates `.pptx` decks from the ECMA-376 specification: a presentation
//! with one master, four layouts and a theme of twelve colors, two fonts
//! and a type scale, slides built from titles, bullet lists, paragraphs of
//! formatted runs, tables and pictures, solid, gradient or picture
//! backgrounds, and speaker notes.
//!
//! Content without a position is placed by the layout engine: text is
//! measured with embedded font metrics, blocks stack down the slide or
//! sit side by side in columns, the type scale shrinks to a floor when a
//! slide is full, and what still does not fit continues on the next slide.
//! Font files handed to the theme are stored in the deck as Embedded
//! OpenType, the form PowerPoint reads.
//!
//! Output is deterministic: fixed timestamps, entries in a fixed order,
//! ids assigned in order of insertion. The result reads back through
//! `pptxboss-core` and passes `pptxboss-check` with no findings.

use std::path::Path;

mod fonts;
mod image;
mod layout;
mod lzcomp;
mod markdown;
mod metrics;
mod metrics_data;
mod mtx;
mod parts;
mod style;
mod xml;
mod zipw;

pub use markdown::from_markdown;

/// English Metric Units per inch.
pub const EMU_PER_INCH: i64 = 914_400;
/// English Metric Units per point.
pub const EMU_PER_POINT: i64 = 12_700;

/// Errors from building or writing a deck.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Picture bytes are not a format the writer can embed.
    #[error("unsupported image format for picture {0}")]
    UnsupportedImage(usize),
    /// A table row has more cells than the table has columns.
    #[error("table row {row} has {cells} cells but the table has {columns} columns")]
    RaggedTable {
        row: usize,
        cells: usize,
        columns: usize,
    },
    /// A background picture is not a format the writer can embed.
    #[error("unsupported image format for background picture")]
    UnsupportedBackgroundImage,
    /// Font bytes are not a single TrueType or OpenType font.
    #[error("unsupported font: {0}")]
    UnsupportedFont(String),
    /// The font's license forbids embedding it (OS/2 fsType restricted).
    #[error("font {0:?} does not permit embedding")]
    FontEmbeddingRestricted(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Slide dimensions in EMU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlideSize {
    pub cx: i64,
    pub cy: i64,
}

impl SlideSize {
    /// 13.333 by 7.5 inches, PowerPoint's default widescreen size.
    pub const WIDESCREEN: SlideSize = SlideSize {
        cx: 12_192_000,
        cy: 6_858_000,
    };
    /// 10 by 7.5 inches.
    pub const STANDARD: SlideSize = SlideSize {
        cx: 9_144_000,
        cy: 6_858_000,
    };

    fn type_name(self) -> Option<&'static str> {
        match self {
            SlideSize::WIDESCREEN => Some("screen16x9"),
            SlideSize::STANDARD => Some("screen4x3"),
            _ => None,
        }
    }
}

/// The slide layouts the writer ships.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Centered title and subtitle.
    Title,
    /// Title with one content placeholder.
    TitleAndContent,
    /// Title only.
    TitleOnly,
    /// No placeholders.
    Blank,
}

impl Layout {
    fn index(self) -> usize {
        match self {
            Layout::Title => 1,
            Layout::TitleAndContent => 2,
            Layout::TitleOnly => 3,
            Layout::Blank => 4,
        }
    }
}

/// A rectangle on the slide, in EMU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub cx: i64,
    pub cy: i64,
}

impl Rect {
    pub const fn new(x: i64, y: i64, cx: i64, cy: i64) -> Self {
        Self { x, y, cx, cy }
    }

    /// A rectangle given in inches.
    pub fn inches(x: f64, y: f64, w: f64, h: f64) -> Self {
        let emu = |v: f64| (v * EMU_PER_INCH as f64).round() as i64;
        Self {
            x: emu(x),
            y: emu(y),
            cx: emu(w),
            cy: emu(h),
        }
    }
}

/// An RGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses `#RRGGBB` or `RRGGBB`, any case.
    pub fn hex(text: &str) -> Option<Rgb> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        if digits.len() != 6 || !digits.is_ascii() {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
        Some(Rgb::new(channel(0)?, channel(2)?, channel(4)?))
    }

    /// `RRGGBB` in upper case.
    pub fn to_hex(self) -> String {
        format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

/// The twelve color slots of a theme (ECMA-376 20.1.6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemeColor {
    Dark1,
    Light1,
    Dark2,
    Light2,
    Accent1,
    Accent2,
    Accent3,
    Accent4,
    Accent5,
    Accent6,
    Hyperlink,
    FollowedHyperlink,
}

impl SchemeColor {
    pub const ALL: [SchemeColor; 12] = [
        SchemeColor::Dark1,
        SchemeColor::Light1,
        SchemeColor::Dark2,
        SchemeColor::Light2,
        SchemeColor::Accent1,
        SchemeColor::Accent2,
        SchemeColor::Accent3,
        SchemeColor::Accent4,
        SchemeColor::Accent5,
        SchemeColor::Accent6,
        SchemeColor::Hyperlink,
        SchemeColor::FollowedHyperlink,
    ];

    /// The slot's name: `dark1`, `light1`, `dark2`, `light2`, `accent1` to
    /// `accent6`, `hyperlink`, `followed_hyperlink`.
    pub fn name(self) -> &'static str {
        match self {
            SchemeColor::Dark1 => "dark1",
            SchemeColor::Light1 => "light1",
            SchemeColor::Dark2 => "dark2",
            SchemeColor::Light2 => "light2",
            SchemeColor::Accent1 => "accent1",
            SchemeColor::Accent2 => "accent2",
            SchemeColor::Accent3 => "accent3",
            SchemeColor::Accent4 => "accent4",
            SchemeColor::Accent5 => "accent5",
            SchemeColor::Accent6 => "accent6",
            SchemeColor::Hyperlink => "hyperlink",
            SchemeColor::FollowedHyperlink => "followed_hyperlink",
        }
    }

    pub fn from_name(name: &str) -> Option<SchemeColor> {
        SchemeColor::ALL
            .into_iter()
            .find(|slot| slot.name() == name)
    }

    /// The slot that takes this one's place when a theme is inverted.
    pub(crate) fn swapped(self) -> SchemeColor {
        match self {
            SchemeColor::Dark1 => SchemeColor::Light1,
            SchemeColor::Light1 => SchemeColor::Dark1,
            SchemeColor::Dark2 => SchemeColor::Light2,
            SchemeColor::Light2 => SchemeColor::Dark2,
            other => other,
        }
    }

    /// The ECMA-376 token used in XML.
    pub(crate) fn xml(self) -> &'static str {
        match self {
            SchemeColor::Dark1 => "dk1",
            SchemeColor::Light1 => "lt1",
            SchemeColor::Dark2 => "dk2",
            SchemeColor::Light2 => "lt2",
            SchemeColor::Accent1 => "accent1",
            SchemeColor::Accent2 => "accent2",
            SchemeColor::Accent3 => "accent3",
            SchemeColor::Accent4 => "accent4",
            SchemeColor::Accent5 => "accent5",
            SchemeColor::Accent6 => "accent6",
            SchemeColor::Hyperlink => "hlink",
            SchemeColor::FollowedHyperlink => "folHlink",
        }
    }
}

/// A color: fixed RGB or a theme slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Rgb(Rgb),
    Scheme(SchemeColor),
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::Rgb(Rgb::new(r, g, b))
    }

    /// Parses `#RRGGBB` or `RRGGBB`.
    pub fn hex(text: &str) -> Option<Color> {
        Rgb::hex(text).map(Color::Rgb)
    }
}

/// A run of text with one set of character properties.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Font size in points; None inherits.
    pub size: Option<u32>,
    pub color: Option<Color>,
    pub font: Option<String>,
    /// An absolute URL the run links to.
    pub link: Option<String>,
}

impl Run {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub fn strike(mut self) -> Self {
        self.strike = true;
        self
    }

    pub fn size(mut self, points: u32) -> Self {
        self.size = Some(points);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn font(mut self, font: impl Into<String>) -> Self {
        self.font = Some(font.into());
        self
    }

    pub fn link(mut self, url: impl Into<String>) -> Self {
        self.link = Some(url.into());
        self
    }
}

/// Horizontal paragraph alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

/// One paragraph of a text body: runs plus paragraph properties.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    /// Indent level 0 to 8.
    pub level: u8,
    /// Show a bullet character; false for plain paragraphs.
    pub bullet: bool,
    pub align: Align,
    /// Space before the paragraph in points.
    pub space_before: Option<u32>,
    /// Space after the paragraph in points.
    pub space_after: Option<u32>,
}

impl Paragraph {
    /// A plain paragraph with one run.
    pub fn text(text: impl Into<String>) -> Self {
        Self::runs(vec![Run::text(text)])
    }

    /// A bulleted paragraph with one run.
    pub fn bullet(text: impl Into<String>, level: u8) -> Self {
        Self::bullet_runs(vec![Run::text(text)], level)
    }

    pub fn runs(runs: Vec<Run>) -> Self {
        Self {
            runs,
            ..Self::default()
        }
    }

    pub fn bullet_runs(runs: Vec<Run>, level: u8) -> Self {
        Self {
            runs,
            level: level.min(8),
            bullet: true,
            ..Self::default()
        }
    }

    /// Appends a run.
    pub fn run(mut self, run: Run) -> Self {
        self.runs.push(run);
        self
    }

    fn each_run(mut self, apply: impl Fn(&mut Run)) -> Self {
        self.runs.iter_mut().for_each(apply);
        self
    }

    /// Bold on every run.
    pub fn bold(self) -> Self {
        self.each_run(|run| run.bold = true)
    }

    /// Italic on every run.
    pub fn italic(self) -> Self {
        self.each_run(|run| run.italic = true)
    }

    /// Underline on every run.
    pub fn underline(self) -> Self {
        self.each_run(|run| run.underline = true)
    }

    /// Strikethrough on every run.
    pub fn strike(self) -> Self {
        self.each_run(|run| run.strike = true)
    }

    /// Size in points on every run.
    pub fn size(self, points: u32) -> Self {
        self.each_run(|run| run.size = Some(points))
    }

    /// Color on every run.
    pub fn color(self, color: Color) -> Self {
        self.each_run(|run| run.color = Some(color))
    }

    /// Font on every run.
    pub fn font(self, font: impl Into<String>) -> Self {
        let font = font.into();
        self.each_run(|run| run.font = Some(font.clone()))
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn space_before(mut self, points: u32) -> Self {
        self.space_before = Some(points);
        self
    }

    pub fn space_after(mut self, points: u32) -> Self {
        self.space_after = Some(points);
        self
    }

    /// The runs' text joined.
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

/// A picture to embed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub data: Vec<u8>,
    pub rect: Rect,
    pub name: String,
    pub description: Option<String>,
}

/// A table of text cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub rows: Vec<Vec<String>>,
    pub rect: Rect,
    /// Style the first row as a header.
    pub header: bool,
}

/// Content placed on a slide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A free text box.
    Text {
        paragraphs: Vec<Paragraph>,
        rect: Rect,
    },
    Picture(Picture),
    Table(Table),
}

/// Content the layout engine places: blocks stack down the slide below
/// the title and the body; a `Columns` block sets its children side by
/// side. Text is measured with the theme's fonts, pictures keep their
/// aspect ratio, tables size their rows to their cells.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Text(Vec<Paragraph>),
    Picture {
        data: Vec<u8>,
        description: Option<String>,
    },
    Table {
        rows: Vec<Vec<String>>,
        header: bool,
    },
    Columns(Vec<Block>),
}

impl Block {
    pub fn text(paragraphs: Vec<Paragraph>) -> Self {
        Block::Text(paragraphs)
    }

    /// One bulleted paragraph per item.
    pub fn bullets<S: Into<String>>(items: impl IntoIterator<Item = S>) -> Self {
        Block::Text(
            items
                .into_iter()
                .map(|item| Paragraph::bullet(item, 0))
                .collect(),
        )
    }

    /// A picture (PNG, JPEG, GIF, BMP or TIFF bytes) sized to its column.
    pub fn picture(data: Vec<u8>) -> Self {
        Block::Picture {
            data,
            description: None,
        }
    }

    pub fn picture_described(data: Vec<u8>, description: impl Into<String>) -> Self {
        Block::Picture {
            data,
            description: Some(description.into()),
        }
    }

    /// A table; every row must have the same number of cells as the first.
    pub fn table(rows: Vec<Vec<String>>, header: bool) -> Self {
        Block::Table { rows, header }
    }

    /// Blocks side by side. Two blocks of which one is a picture split
    /// seven to five in the text's favor; otherwise columns are equal.
    pub fn columns(blocks: Vec<Block>) -> Self {
        Block::Columns(blocks)
    }
}

/// One slide under construction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slide {
    pub layout: Option<Layout>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    /// Paragraphs of the body placeholder.
    pub body: Vec<Paragraph>,
    /// Content placed by the layout engine, below the body.
    pub blocks: Vec<Block>,
    /// Content at fixed positions.
    pub shapes: Vec<Shape>,
    pub notes: Option<String>,
    pub hidden: bool,
    /// This slide's own background; None shows the layout's or the master's.
    pub background: Option<Background>,
    /// Swap light and dark text slots on this slide relative to the theme.
    pub inverted: bool,
}

impl Slide {
    pub fn new() -> Self {
        Self::default()
    }

    /// A title slide with a centered title and optional subtitle.
    pub fn title_slide(title: impl Into<String>, subtitle: Option<&str>) -> Self {
        Self {
            layout: Some(Layout::Title),
            title: Some(title.into()),
            subtitle: subtitle.map(str::to_string),
            ..Self::default()
        }
    }

    pub fn titled(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::default()
        }
    }

    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Adds a bulleted line to the body placeholder.
    pub fn bullet(mut self, text: impl Into<String>) -> Self {
        self.body.push(Paragraph::bullet(text, 0));
        self
    }

    /// Adds an indented bulleted line to the body placeholder.
    pub fn sub_bullet(mut self, text: impl Into<String>, level: u8) -> Self {
        self.body.push(Paragraph::bullet(text, level));
        self
    }

    /// Adds a plain paragraph to the body placeholder.
    pub fn paragraph(mut self, text: impl Into<String>) -> Self {
        self.body.push(Paragraph::text(text));
        self
    }

    pub fn body_paragraph(mut self, paragraph: Paragraph) -> Self {
        self.body.push(paragraph);
        self
    }

    /// Adds a block for the layout engine to place.
    pub fn block(mut self, block: Block) -> Self {
        self.blocks.push(block);
        self
    }

    /// Adds blocks side by side.
    pub fn columns(mut self, blocks: Vec<Block>) -> Self {
        self.blocks.push(Block::Columns(blocks));
        self
    }

    /// Adds a free text box.
    pub fn text_box(mut self, rect: Rect, paragraphs: Vec<Paragraph>) -> Self {
        self.shapes.push(Shape::Text { paragraphs, rect });
        self
    }

    /// Adds a picture; the format is sniffed from the bytes (PNG, JPEG, GIF, BMP, TIFF).
    pub fn picture(mut self, data: Vec<u8>, rect: Rect) -> Self {
        let name = format!("Picture {}", self.shapes.len() + 1);
        self.shapes.push(Shape::Picture(Picture {
            data,
            rect,
            name,
            description: None,
        }));
        self
    }

    pub fn picture_described(
        mut self,
        data: Vec<u8>,
        rect: Rect,
        description: impl Into<String>,
    ) -> Self {
        let name = format!("Picture {}", self.shapes.len() + 1);
        self.shapes.push(Shape::Picture(Picture {
            data,
            rect,
            name,
            description: Some(description.into()),
        }));
        self
    }

    /// Adds a table; every row must have the same number of cells as the first.
    pub fn table(mut self, rect: Rect, rows: Vec<Vec<String>>, header: bool) -> Self {
        self.shapes.push(Shape::Table(Table { rows, rect, header }));
        self
    }

    pub fn notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    pub fn background(mut self, background: Background) -> Self {
        self.background = Some(background);
        self
    }

    /// Light text on this slide when the theme is not inverted, and the reverse.
    pub fn inverted(mut self) -> Self {
        self.inverted = true;
        self
    }

    /// The layout to bind: the explicit one, else inferred from the title,
    /// the subtitle and whether the layout engine placed a body.
    pub(crate) fn layout_for(&self, has_body: bool) -> Layout {
        if let Some(layout) = self.layout {
            return layout;
        }
        match (&self.title, has_body, self.subtitle.is_some()) {
            (Some(_), true, _) => Layout::TitleAndContent,
            (Some(_), false, true) => Layout::Title,
            (Some(_), false, false) => Layout::TitleOnly,
            (None, _, _) => Layout::Blank,
        }
    }
}

/// One stop of a gradient; `position` is a percentage 0 to 100.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GradientStop {
    pub position: u8,
    pub color: Color,
}

/// A background fill.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Background {
    Solid(Color),
    /// A linear gradient; `angle` is in degrees, 0 runs left to right, 90 top to bottom.
    Gradient {
        stops: Vec<GradientStop>,
        angle: u16,
    },
    /// Picture bytes (PNG, JPEG, GIF, BMP or TIFF) stretched over the slide.
    Picture(Vec<u8>),
}

impl Background {
    pub fn solid(color: Color) -> Self {
        Background::Solid(color)
    }

    pub fn gradient(stops: Vec<GradientStop>, angle: u16) -> Self {
        Background::Gradient {
            stops,
            angle: angle % 360,
        }
    }

    /// A two-stop gradient from `from` to `to`.
    pub fn linear(from: Color, to: Color, angle: u16) -> Self {
        Self::gradient(
            vec![
                GradientStop {
                    position: 0,
                    color: from,
                },
                GradientStop {
                    position: 100,
                    color: to,
                },
            ],
            angle,
        )
    }

    pub fn picture(data: Vec<u8>) -> Self {
        Background::Picture(data)
    }
}

/// A background for one layout.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutBackground {
    pub layout: Layout,
    pub background: Background,
    /// Swap light and dark text slots on this layout.
    pub inverted: bool,
}

/// The twelve colors of a theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorScheme {
    pub dark1: Rgb,
    pub light1: Rgb,
    pub dark2: Rgb,
    pub light2: Rgb,
    pub accent1: Rgb,
    pub accent2: Rgb,
    pub accent3: Rgb,
    pub accent4: Rgb,
    pub accent5: Rgb,
    pub accent6: Rgb,
    pub hyperlink: Rgb,
    pub followed_hyperlink: Rgb,
}

impl ColorScheme {
    pub fn get(&self, slot: SchemeColor) -> Rgb {
        match slot {
            SchemeColor::Dark1 => self.dark1,
            SchemeColor::Light1 => self.light1,
            SchemeColor::Dark2 => self.dark2,
            SchemeColor::Light2 => self.light2,
            SchemeColor::Accent1 => self.accent1,
            SchemeColor::Accent2 => self.accent2,
            SchemeColor::Accent3 => self.accent3,
            SchemeColor::Accent4 => self.accent4,
            SchemeColor::Accent5 => self.accent5,
            SchemeColor::Accent6 => self.accent6,
            SchemeColor::Hyperlink => self.hyperlink,
            SchemeColor::FollowedHyperlink => self.followed_hyperlink,
        }
    }

    pub fn set(&mut self, slot: SchemeColor, color: Rgb) {
        let field = match slot {
            SchemeColor::Dark1 => &mut self.dark1,
            SchemeColor::Light1 => &mut self.light1,
            SchemeColor::Dark2 => &mut self.dark2,
            SchemeColor::Light2 => &mut self.light2,
            SchemeColor::Accent1 => &mut self.accent1,
            SchemeColor::Accent2 => &mut self.accent2,
            SchemeColor::Accent3 => &mut self.accent3,
            SchemeColor::Accent4 => &mut self.accent4,
            SchemeColor::Accent5 => &mut self.accent5,
            SchemeColor::Accent6 => &mut self.accent6,
            SchemeColor::Hyperlink => &mut self.hyperlink,
            SchemeColor::FollowedHyperlink => &mut self.followed_hyperlink,
        };
        *field = color;
    }

    /// Twelve `RRGGBB` values in slot order.
    const fn from_hex_table(table: [u32; 12]) -> Self {
        const fn c(v: u32) -> Rgb {
            Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8)
        }
        Self {
            dark1: c(table[0]),
            light1: c(table[1]),
            dark2: c(table[2]),
            light2: c(table[3]),
            accent1: c(table[4]),
            accent2: c(table[5]),
            accent3: c(table[6]),
            accent4: c(table[7]),
            accent5: c(table[8]),
            accent6: c(table[9]),
            hyperlink: c(table[10]),
            followed_hyperlink: c(table[11]),
        }
    }
}

impl Default for ColorScheme {
    /// The Office scheme.
    fn default() -> Self {
        Self::from_hex_table([
            0x000000, 0xFFFFFF, 0x44546A, 0xE7E6E6, 0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000,
            0x5B9BD5, 0x70AD47, 0x0563C1, 0x954F72,
        ])
    }
}

/// Font sizes in points. Body text at level 0 takes `body`; each deeper
/// level is four points smaller, down to ten points below `body`. When a
/// slide is full, the layout engine scales body text down, never below
/// `minimum`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeScale {
    /// The title of a title slide.
    pub display: u32,
    pub title: u32,
    pub subtitle: u32,
    pub body: u32,
    pub table: u32,
    pub minimum: u32,
}

impl Default for TypeScale {
    fn default() -> Self {
        Self {
            display: 54,
            title: 44,
            subtitle: 24,
            body: 28,
            table: 16,
            minimum: 18,
        }
    }
}

impl TypeScale {
    /// The body size at an indent level.
    pub fn body_level(&self, level: u8) -> u32 {
        let stepped = self.body.saturating_sub(4 * level as u32);
        stepped.max(self.body.saturating_sub(10)).max(1)
    }
}

/// Colors, fonts, type scale and backgrounds shared by every slide.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub colors: ColorScheme,
    /// Title font.
    pub major_font: String,
    /// Body font.
    pub minor_font: String,
    pub scale: TypeScale,
    /// Swap the light and dark slots when the theme is written, so the
    /// background takes `dark1` and text takes `light1`.
    pub inverted: bool,
    /// Master background; None writes the theme background reference.
    pub background: Option<Background>,
    pub layout_backgrounds: Vec<LayoutBackground>,
    /// TrueType or OpenType font files stored in the deck, so viewers
    /// without the fonts installed still render it in them.
    pub embedded_fonts: Vec<Vec<u8>>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::office()
    }
}

impl Theme {
    pub const PRESETS: [&'static str; 12] = [
        "office", "dark", "slate", "forest", "sunset", "midnight", "mocha", "dracula", "nord",
        "tokyo", "clay", "mono",
    ];

    /// Office colors, Calibri, not inverted, no backgrounds.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            colors: ColorScheme::default(),
            major_font: "Calibri".to_string(),
            minor_font: "Calibri".to_string(),
            scale: TypeScale::default(),
            inverted: false,
            background: None,
            layout_backgrounds: Vec::new(),
            embedded_fonts: Vec::new(),
        }
    }

    pub fn office() -> Self {
        Self::new("office")
    }

    pub fn dark() -> Self {
        Self::new("dark")
            .colors(ColorScheme::from_hex_table([
                0x1E1E1E, 0xF5F5F5, 0x2D2D2D, 0xD0D0D0, 0x4FC3F7, 0xFFB74D, 0x81C784, 0xBA68C8,
                0xE57373, 0xFFF176, 0x82B1FF, 0xCE93D8,
            ]))
            .inverted()
    }

    pub fn slate() -> Self {
        Self::new("slate")
            .colors(ColorScheme::from_hex_table([
                0x1F2933, 0xF5F7FA, 0x3E4C59, 0xCBD2D9, 0x2F80ED, 0x56CCF2, 0x27AE60, 0xF2C94C,
                0xEB5757, 0x9B51E0, 0x2F80ED, 0x9B51E0,
            ]))
            .fonts("Georgia", "Calibri")
    }

    pub fn forest() -> Self {
        Self::new("forest").colors(ColorScheme::from_hex_table([
            0x1B2B1B, 0xF4F8F2, 0x2F4F2F, 0xD9E4D2, 0x2E7D32, 0x66BB6A, 0xA5D6A7, 0xFFB300,
            0x8D6E63, 0x26A69A, 0x1B5E20, 0x4E342E,
        ]))
    }

    pub fn sunset() -> Self {
        Self::new("sunset")
            .colors(ColorScheme::from_hex_table([
                0x2B1B2F, 0xFFF8F0, 0x5D2E46, 0xF3D9C7, 0xFF7043, 0xFFCA28, 0xEC407A, 0xAB47BC,
                0x26C6DA, 0x9CCC65, 0xD84315, 0x6A1B9A,
            ]))
            .fonts("Georgia", "Calibri")
            .inverted()
            .background(Background::linear(
                Color::rgb(0x2B, 0x1B, 0x2F),
                Color::rgb(0x5D, 0x2E, 0x46),
                90,
            ))
    }

    /// Near-black with an emerald accent, in the style of developer tool sites.
    pub fn midnight() -> Self {
        Self::new("midnight")
            .colors(ColorScheme::from_hex_table([
                0x0A0A0A, 0xFAFAFA, 0x141417, 0x9AA0A6, 0x34D399, 0x60A5FA, 0xFBBF24, 0xF87171,
                0xA78BFA, 0x2DD4BF, 0x34D399, 0x6EE7B7,
            ]))
            .inverted()
    }

    /// The Catppuccin Mocha palette: soft dark blue with mauve and blue accents.
    pub fn mocha() -> Self {
        Self::new("mocha")
            .colors(ColorScheme::from_hex_table([
                0x1E1E2E, 0xCDD6F4, 0x313244, 0xA6ADC8, 0xCBA6F7, 0x89B4FA, 0xA6E3A1, 0xFAB387,
                0xF38BA8, 0x94E2D5, 0x89B4FA, 0xB4BEFE,
            ]))
            .inverted()
    }

    /// The Dracula palette: dark grey with purple, pink and cyan accents.
    pub fn dracula() -> Self {
        Self::new("dracula")
            .colors(ColorScheme::from_hex_table([
                0x282A36, 0xF8F8F2, 0x44475A, 0x6272A4, 0xBD93F9, 0xFF79C6, 0x8BE9FD, 0x50FA7B,
                0xFFB86C, 0xFF5555, 0x8BE9FD, 0xBD93F9,
            ]))
            .inverted()
    }

    /// The Nord palette: blue-grey with frost and aurora accents.
    pub fn nord() -> Self {
        Self::new("nord")
            .colors(ColorScheme::from_hex_table([
                0x2E3440, 0xECEFF4, 0x3B4252, 0xD8DEE9, 0x88C0D0, 0x81A1C1, 0xA3BE8C, 0xEBCB8B,
                0xD08770, 0xB48EAD, 0x88C0D0, 0x5E81AC,
            ]))
            .inverted()
    }

    /// The Tokyo Night palette: deep navy with blue and violet accents.
    pub fn tokyo() -> Self {
        Self::new("tokyo")
            .colors(ColorScheme::from_hex_table([
                0x1A1B26, 0xC0CAF5, 0x24283B, 0xA9B1D6, 0x7AA2F7, 0xBB9AF7, 0x7DCFFF, 0x9ECE6A,
                0xFF9E64, 0xF7768E, 0x7AA2F7, 0xBB9AF7,
            ]))
            .inverted()
    }

    /// Warm cream with ink text and a terracotta accent; serif titles.
    pub fn clay() -> Self {
        Self::new("clay")
            .colors(ColorScheme::from_hex_table([
                0x141413, 0xFAF9F5, 0x5E5D59, 0xF0EEE6, 0xD97757, 0x6A9BCC, 0x788C5D, 0xC2A34E,
                0x8B6BB5, 0x5B8C7D, 0x6A9BCC, 0x8B6BB5,
            ]))
            .fonts("Georgia", "Calibri")
    }

    /// White with near-black text and one blue accent.
    pub fn mono() -> Self {
        Self::new("mono").colors(ColorScheme::from_hex_table([
            0x0A0A0A, 0xFFFFFF, 0x666666, 0xEAEAEA, 0x0070F3, 0x7928CA, 0xFF0080, 0xF5A623,
            0x50E3C2, 0xEE0000, 0x0070F3, 0x7928CA,
        ]))
    }

    /// A preset by name; see [`Theme::PRESETS`].
    pub fn preset(name: &str) -> Option<Theme> {
        match name {
            "office" => Some(Self::office()),
            "dark" => Some(Self::dark()),
            "slate" => Some(Self::slate()),
            "forest" => Some(Self::forest()),
            "sunset" => Some(Self::sunset()),
            "midnight" => Some(Self::midnight()),
            "mocha" => Some(Self::mocha()),
            "dracula" => Some(Self::dracula()),
            "nord" => Some(Self::nord()),
            "tokyo" => Some(Self::tokyo()),
            "clay" => Some(Self::clay()),
            "mono" => Some(Self::mono()),
            _ => None,
        }
    }

    pub fn color(mut self, slot: SchemeColor, color: Rgb) -> Self {
        self.colors.set(slot, color);
        self
    }

    pub fn colors(mut self, colors: ColorScheme) -> Self {
        self.colors = colors;
        self
    }

    pub fn fonts(mut self, major: impl Into<String>, minor: impl Into<String>) -> Self {
        self.major_font = major.into();
        self.minor_font = minor.into();
        self
    }

    /// One font for titles and body.
    pub fn font(self, font: impl Into<String>) -> Self {
        let font = font.into();
        self.fonts(font.clone(), font)
    }

    pub fn scale(mut self, scale: TypeScale) -> Self {
        self.scale = scale;
        self
    }

    pub fn inverted(mut self) -> Self {
        self.inverted = true;
        self
    }

    /// The master background; every layout and slide without its own shows it.
    pub fn background(mut self, background: Background) -> Self {
        self.background = Some(background);
        self
    }

    /// A background for one layout; `inverted` swaps light and dark text
    /// slots on that layout relative to the theme.
    pub fn layout_background(
        mut self,
        layout: Layout,
        background: Background,
        inverted: bool,
    ) -> Self {
        self.layout_backgrounds
            .retain(|entry| entry.layout != layout);
        self.layout_backgrounds.push(LayoutBackground {
            layout,
            background,
            inverted,
        });
        self
    }

    /// Stores a font file (TrueType or OpenType) in the deck. Files of one
    /// family fill its regular, bold, italic and bold italic slots by the
    /// style the font declares. Writing fails for a font whose license
    /// forbids embedding.
    pub fn embed_font(mut self, data: Vec<u8>) -> Self {
        self.embedded_fonts.push(data);
        self
    }

    pub(crate) fn layout_background_for(&self, layout: Layout) -> Option<&LayoutBackground> {
        self.layout_backgrounds
            .iter()
            .find(|entry| entry.layout == layout)
    }
}

/// Document properties written to `docProps/core.xml` and `app.xml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub title: Option<String>,
    pub creator: String,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    /// W3C-DTF timestamp such as `2026-01-02T03:04:05Z`, used for both created and modified.
    pub timestamp: String,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            title: None,
            creator: "pptxboss".to_string(),
            subject: None,
            keywords: None,
            timestamp: "2000-01-01T00:00:00Z".to_string(),
        }
    }
}

/// A deck under construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presentation {
    pub size: SlideSize,
    pub slides: Vec<Slide>,
    pub metadata: Metadata,
    pub theme: Theme,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            size: SlideSize::WIDESCREEN,
            slides: Vec::new(),
            metadata: Metadata::default(),
            theme: Theme::default(),
        }
    }
}

impl Presentation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn size(mut self, size: SlideSize) -> Self {
        self.size = size;
        self
    }

    pub fn slide(mut self, slide: Slide) -> Self {
        self.slides.push(slide);
        self
    }

    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// One font for titles and body, keeping the rest of the theme.
    pub fn font(mut self, font: impl Into<String>) -> Self {
        self.theme = self.theme.font(font);
        self
    }

    /// The package bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let parts = parts::build(self)?;
        let mut writer = zipw::ZipWriter::new();
        for part in parts {
            writer.add(&part.name, &part.data, part.compress)?;
        }
        Ok(writer.finish())
    }

    /// Writes the package to `path`.
    pub fn write_to(&self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, self.to_bytes()?)?;
        Ok(())
    }
}

/// Image formats the writer embeds, sniffed from magic bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Tiff,
}

impl ImageFormat {
    pub fn sniff(data: &[u8]) -> Option<ImageFormat> {
        if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
            return Some(ImageFormat::Png);
        }
        if data.starts_with(&[0xff, 0xd8, 0xff]) {
            return Some(ImageFormat::Jpeg);
        }
        if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            return Some(ImageFormat::Gif);
        }
        if data.starts_with(b"BM") {
            return Some(ImageFormat::Bmp);
        }
        if data.starts_with(&[0x49, 0x49, 0x2a, 0x00])
            || data.starts_with(&[0x4d, 0x4d, 0x00, 0x2a])
        {
            return Some(ImageFormat::Tiff);
        }
        None
    }

    /// Width and height in pixels from the header of a PNG, JPEG, GIF or BMP.
    pub fn dimensions(data: &[u8]) -> Option<(u32, u32)> {
        image::dimensions(data)
    }

    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Gif => "gif",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Tiff => "tiff",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Tiff => "image/tiff",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_scale_steps_by_level() {
        let scale = TypeScale::default();
        assert_eq!(
            [0, 1, 2, 3, 4, 8].map(|level| scale.body_level(level)),
            [28, 24, 20, 18, 18, 18]
        );
        let small = TypeScale {
            body: 6,
            ..TypeScale::default()
        };
        assert_eq!(small.body_level(3), 1);
        assert_eq!(Theme::office().scale(small).scale.body, 6);
    }

    #[test]
    fn image_formats_are_sniffed() {
        assert_eq!(
            ImageFormat::sniff(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0]),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            ImageFormat::sniff(&[0xff, 0xd8, 0xff, 0xe0]),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(ImageFormat::sniff(b"GIF89a..."), Some(ImageFormat::Gif));
        assert_eq!(ImageFormat::sniff(b"BM...."), Some(ImageFormat::Bmp));
        assert_eq!(
            ImageFormat::sniff(&[0x4d, 0x4d, 0x00, 0x2a]),
            Some(ImageFormat::Tiff)
        );
        assert_eq!(ImageFormat::sniff(b"<svg/>"), None);
    }

    #[test]
    fn colors_parse_and_name() {
        assert_eq!(Rgb::hex("#abcdef"), Some(Rgb::new(0xab, 0xcd, 0xef)));
        assert_eq!(Rgb::hex("ABCDEF"), Some(Rgb::new(0xab, 0xcd, 0xef)));
        assert_eq!(Rgb::hex("abc"), None);
        assert_eq!(Rgb::hex("#gggggg"), None);
        assert_eq!(Rgb::new(1, 2, 255).to_hex(), "0102FF");
        for slot in SchemeColor::ALL {
            assert_eq!(SchemeColor::from_name(slot.name()), Some(slot));
        }
        assert_eq!(SchemeColor::from_name("teal"), None);
        assert_eq!(Color::hex("#000000"), Some(Color::rgb(0, 0, 0)));
    }

    #[test]
    fn paragraph_builders_apply_to_every_run() {
        let paragraph = Paragraph::text("a")
            .run(Run::text("b").italic())
            .bold()
            .size(20)
            .align(Align::Center);
        assert_eq!(paragraph.plain_text(), "ab");
        assert!(paragraph
            .runs
            .iter()
            .all(|run| run.bold && run.size == Some(20)));
        assert!(!paragraph.runs[0].italic && paragraph.runs[1].italic);
        assert_eq!(paragraph.align, Align::Center);
        assert_eq!(Paragraph::bullet("x", 12).level, 8);
        assert_eq!(Paragraph::default().plain_text(), "");
    }

    #[test]
    fn themes_and_presets() {
        assert_eq!(Theme::default(), Theme::office());
        assert_eq!(Theme::default().colors.accent1, Rgb::new(0x44, 0x72, 0xC4));
        assert!(Theme::preset("dark").unwrap().inverted);
        assert!(Theme::preset("nope").is_none());
        for name in Theme::PRESETS {
            assert_eq!(Theme::preset(name).unwrap().name, name);
        }
        for name in ["midnight", "mocha", "dracula", "nord", "tokyo"] {
            assert!(Theme::preset(name).unwrap().inverted, "{name} is dark");
        }
        for name in ["clay", "mono", "slate", "forest"] {
            assert!(!Theme::preset(name).unwrap().inverted, "{name} is light");
        }
        let theme = Theme::new("mine")
            .color(SchemeColor::Accent1, Rgb::new(1, 2, 3))
            .fonts("Georgia", "Arial")
            .layout_background(Layout::Title, Background::solid(Color::rgb(0, 0, 0)), true)
            .layout_background(Layout::Title, Background::solid(Color::rgb(9, 9, 9)), false);
        assert_eq!(theme.colors.get(SchemeColor::Accent1), Rgb::new(1, 2, 3));
        assert_eq!(
            (theme.major_font.as_str(), theme.minor_font.as_str()),
            ("Georgia", "Arial")
        );
        assert_eq!(theme.layout_backgrounds.len(), 1);
        assert_eq!(
            theme
                .layout_background_for(Layout::Title)
                .unwrap()
                .background,
            Background::solid(Color::rgb(9, 9, 9))
        );
        assert_eq!(Presentation::new().font("Inter").theme.minor_font, "Inter");
        assert_eq!(
            Background::linear(Color::rgb(0, 0, 0), Color::rgb(9, 9, 9), 405),
            Background::Gradient {
                stops: vec![
                    GradientStop {
                        position: 0,
                        color: Color::rgb(0, 0, 0)
                    },
                    GradientStop {
                        position: 100,
                        color: Color::rgb(9, 9, 9)
                    },
                ],
                angle: 45,
            }
        );
    }

    #[test]
    fn rect_in_inches() {
        assert_eq!(
            Rect::inches(1.0, 0.5, 2.0, 0.25),
            Rect::new(914400, 457200, 1828800, 228600)
        );
    }
}
