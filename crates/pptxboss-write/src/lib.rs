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

/// One paragraph of a text body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paragraph {
    pub text: String,
    /// Indent level 0 to 8.
    pub level: u8,
    /// Show a bullet character; false for plain paragraphs.
    pub bullet: bool,
    pub bold: bool,
    pub italic: bool,
    /// Font size in points; None inherits from the layout.
    pub size: Option<u32>,
}

impl Paragraph {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: 0,
            bullet: false,
            bold: false,
            italic: false,
            size: None,
        }
    }

    pub fn bullet(text: impl Into<String>, level: u8) -> Self {
        Self {
            text: text.into(),
            level: level.min(8),
            bullet: true,
            bold: false,
            italic: false,
            size: None,
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

    pub fn size(mut self, points: u32) -> Self {
        self.size = Some(points);
        self
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
            writer.add(&part.name, &part.data, part.compress);
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
    fn rect_in_inches() {
        assert_eq!(
            Rect::inches(1.0, 0.5, 2.0, 0.25),
            Rect::new(914400, 457200, 1828800, 228600)
        );
    }
}
