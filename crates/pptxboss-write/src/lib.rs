//! Creates `.pptx` decks from the ECMA-376 specification: a presentation
//! with one master, four layouts and a theme, slides built from titles,
//! bullet lists, paragraphs, tables and pictures, and speaker notes.
//!
//! Output is deterministic: fixed timestamps, entries in a fixed order,
//! ids assigned in order of insertion. The result reads back through
//! `pptxboss-core` and passes `pptxboss-check` with no findings.

use std::path::Path;

mod markdown;
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

/// One slide under construction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slide {
    pub layout: Option<Layout>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    /// Paragraphs of the body placeholder.
    pub body: Vec<Paragraph>,
    pub shapes: Vec<Shape>,
    pub notes: Option<String>,
    pub hidden: bool,
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

    fn effective_layout(&self) -> Layout {
        if let Some(layout) = self.layout {
            return layout;
        }
        match (&self.title, self.body.is_empty()) {
            (Some(_), false) => Layout::TitleAndContent,
            (Some(_), true) => Layout::TitleOnly,
            (None, _) => Layout::Blank,
        }
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
    /// Body font family for the theme; PowerPoint substitutes when absent.
    pub font: String,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            size: SlideSize::WIDESCREEN,
            slides: Vec::new(),
            metadata: Metadata::default(),
            font: "Calibri".to_string(),
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

    pub fn font(mut self, font: impl Into<String>) -> Self {
        self.font = font.into();
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
    fn layouts_are_inferred_from_content() {
        assert_eq!(
            Slide::titled("t").bullet("b").effective_layout(),
            Layout::TitleAndContent
        );
        assert_eq!(Slide::titled("t").effective_layout(), Layout::TitleOnly);
        assert_eq!(Slide::new().effective_layout(), Layout::Blank);
        assert_eq!(
            Slide::title_slide("t", None).effective_layout(),
            Layout::Title
        );
        assert_eq!(
            Slide::new()
                .layout(Layout::Blank)
                .bullet("x")
                .effective_layout(),
            Layout::Blank
        );
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
    fn rect_in_inches() {
        assert_eq!(
            Rect::inches(1.0, 0.5, 2.0, 0.25),
            Rect::new(914400, 457200, 1828800, 228600)
        );
    }
}
