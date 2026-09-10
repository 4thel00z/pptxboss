//! The slide content model: the shape tree of a slide-family part with its
//! text bodies, pictures, tables and groups (ECMA-376 Part 1, 19.3 and
//! 21.1). Only what a reader needs to extract content and structure is
//! kept; formatting beyond run-level emphasis is not modelled.

/// A length in English Metric Units: 914400 per inch, 12700 per point.
pub type Emu = i64;

/// Position and size of a shape (20.1.7.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transform {
    pub x: Emu,
    pub y: Emu,
    pub cx: Emu,
    pub cy: Emu,
    /// Rotation in 60,000ths of a degree, clockwise.
    pub rot: i64,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// A group's child coordinate space (20.1.7.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChildSpace {
    pub x: Emu,
    pub y: Emu,
    pub cx: Emu,
    pub cy: Emu,
}

/// A placeholder declaration (19.3.1.36).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placeholder {
    /// `type`; `obj` when omitted.
    pub kind: PlaceholderKind,
    /// `idx`; 0 when omitted.
    pub idx: u32,
}

/// `ST_PlaceholderType` (19.7.10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaceholderKind {
    Title,
    Body,
    CenterTitle,
    Subtitle,
    DateTime,
    SlideNumber,
    Footer,
    Header,
    #[default]
    Object,
    Chart,
    Table,
    ClipArt,
    Diagram,
    Media,
    Picture,
    SlideImage,
    Other,
}

impl PlaceholderKind {
    pub fn parse(value: &[u8]) -> PlaceholderKind {
        match value {
            b"title" => PlaceholderKind::Title,
            b"body" => PlaceholderKind::Body,
            b"ctrTitle" => PlaceholderKind::CenterTitle,
            b"subTitle" => PlaceholderKind::Subtitle,
            b"dt" => PlaceholderKind::DateTime,
            b"sldNum" => PlaceholderKind::SlideNumber,
            b"ftr" => PlaceholderKind::Footer,
            b"hdr" => PlaceholderKind::Header,
            b"obj" => PlaceholderKind::Object,
            b"chart" => PlaceholderKind::Chart,
            b"tbl" => PlaceholderKind::Table,
            b"clipArt" => PlaceholderKind::ClipArt,
            b"dgm" => PlaceholderKind::Diagram,
            b"media" => PlaceholderKind::Media,
            b"pic" => PlaceholderKind::Picture,
            b"sldImg" => PlaceholderKind::SlideImage,
            _ => PlaceholderKind::Other,
        }
    }

    /// True for the title and centered title placeholders.
    pub fn is_title(self) -> bool {
        matches!(self, PlaceholderKind::Title | PlaceholderKind::CenterTitle)
    }

    /// True for the date, footer and slide number placeholders, whose text
    /// is presentation furniture rather than slide content.
    pub fn is_furniture(self) -> bool {
        matches!(
            self,
            PlaceholderKind::DateTime
                | PlaceholderKind::SlideNumber
                | PlaceholderKind::Footer
                | PlaceholderKind::Header
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PlaceholderKind::Title => "title",
            PlaceholderKind::Body => "body",
            PlaceholderKind::CenterTitle => "ctrTitle",
            PlaceholderKind::Subtitle => "subTitle",
            PlaceholderKind::DateTime => "dt",
            PlaceholderKind::SlideNumber => "sldNum",
            PlaceholderKind::Footer => "ftr",
            PlaceholderKind::Header => "hdr",
            PlaceholderKind::Object => "obj",
            PlaceholderKind::Chart => "chart",
            PlaceholderKind::Table => "tbl",
            PlaceholderKind::ClipArt => "clipArt",
            PlaceholderKind::Diagram => "dgm",
            PlaceholderKind::Media => "media",
            PlaceholderKind::Picture => "pic",
            PlaceholderKind::SlideImage => "sldImg",
            PlaceholderKind::Other => "other",
        }
    }
}

/// Bullet setting of a paragraph (21.1.2.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Bullet {
    /// Nothing specified on the paragraph; inherited from the list style.
    #[default]
    Inherited,
    None,
    Char(String),
    AutoNumber {
        scheme: String,
        start_at: u32,
    },
    Picture,
}

/// Run-level character properties that survive extraction (21.1.2.3.9).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunProps {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    /// Font size in hundredths of a point.
    pub size: Option<u32>,
    /// Relationship id of a click hyperlink.
    pub hyperlink: Option<String>,
    pub lang: Option<String>,
    pub typeface: Option<String>,
}

/// What a run is (21.1.2.3.8, 21.1.2.2.4, 21.1.2.2.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunKind {
    Text,
    /// `a:br`: a line break within the paragraph.
    LineBreak,
    /// `a:fld` with its `type`; the text is the cached value.
    Field(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub kind: RunKind,
    pub text: String,
    pub props: RunProps,
}

impl Run {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: RunKind::Text,
            text: text.into(),
            props: RunProps::default(),
        }
    }
}

/// One `a:p` (21.1.2.2.6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paragraph {
    /// Indent level 0 to 8.
    pub level: u8,
    pub bullet: Bullet,
    pub runs: Vec<Run>,
}

impl Paragraph {
    /// The paragraph's text with line breaks as `\n`.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.write_text(&mut out);
        out
    }

    pub fn write_text(&self, out: &mut String) {
        for run in &self.runs {
            match run.kind {
                RunKind::LineBreak => out.push('\n'),
                _ => out.push_str(&run.text),
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.runs
            .iter()
            .all(|run| run.text.is_empty() && run.kind != RunKind::LineBreak)
    }
}

/// A text body (21.1.2.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextBody {
    pub paragraphs: Vec<Paragraph>,
}

impl TextBody {
    /// Paragraph texts joined by `\n`.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.write_text(&mut out);
        out
    }

    pub fn write_text(&self, out: &mut String) {
        for (i, paragraph) in self.paragraphs.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            paragraph.write_text(out);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paragraphs.iter().all(Paragraph::is_empty)
    }
}

/// A table cell (21.1.3.16).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub body: TextBody,
    pub grid_span: u32,
    pub row_span: u32,
    /// Merged into the cell to its left; carries no content of its own.
    pub h_merge: bool,
    /// Merged into the cell above it.
    pub v_merge: bool,
}

impl Cell {
    /// False for cells merged away into another cell.
    pub fn is_origin(&self) -> bool {
        !self.h_merge && !self.v_merge
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub height: Emu,
    pub cells: Vec<Cell>,
}

/// A table (21.1.3.13).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub column_widths: Vec<Emu>,
    pub rows: Vec<Row>,
}

/// A picture's image reference (20.1.8.13).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picture {
    /// `r:embed`: relationship id of an image part in the package.
    pub embed: Option<String>,
    /// `r:link`: relationship id of an external image.
    pub link: Option<String>,
    /// Relationship id of an attached audio or video clip.
    pub media: Option<String>,
}

/// An embedded object frame (19.3.2.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OleObject {
    pub prog_id: Option<String>,
    pub rel_id: Option<String>,
    pub preview: Option<Picture>,
}

/// What a shape holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    /// `p:sp`: possibly with a text body; empty when the shape has none.
    Text(TextBody),
    /// `p:pic`.
    Picture(Picture),
    /// `p:grpSp`: children in z-order, and the group's child coordinate space.
    Group(Vec<Shape>, Option<ChildSpace>),
    /// `p:graphicFrame` holding `a:tbl`.
    Table(Table),
    /// `p:graphicFrame` holding `c:chart`, with its relationship id.
    Chart(Option<String>),
    /// `p:graphicFrame` holding a diagram, with the diagram data relationship id.
    Diagram(Option<String>),
    /// `p:graphicFrame` holding `p:oleObj`.
    Ole(OleObject),
    /// `p:cxnSp`: a connector; no content.
    Connector,
    /// `p:contentPart` with its relationship id.
    ContentPart(Option<String>),
    /// A `p:graphicFrame` whose `graphicData` URI the reader does not know.
    UnknownGraphic(String),
}

/// One node of the shape tree (19.3.1.45).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    /// `cNvPr/@id`.
    pub id: u32,
    /// `cNvPr/@name`.
    pub name: String,
    /// `cNvPr/@hidden`.
    pub hidden: bool,
    /// `cNvPr/@descr`, the alternative text.
    pub description: Option<String>,
    /// `cNvPr/hlinkClick/@r:id`.
    pub hyperlink: Option<String>,
    pub placeholder: Option<Placeholder>,
    pub transform: Option<Transform>,
    /// `cNvSpPr/@txBox`: the shape is a text box.
    pub text_box: bool,
    pub content: Content,
}

impl Shape {
    /// The shape's own text body, if it is a text shape.
    pub fn text_body(&self) -> Option<&TextBody> {
        match &self.content {
            Content::Text(body) => Some(body),
            _ => None,
        }
    }

    /// True when the placeholder is a title or centered title.
    pub fn is_title(&self) -> bool {
        self.placeholder
            .as_ref()
            .is_some_and(|ph| ph.kind.is_title())
    }
}

/// The root element kinds that carry a shape tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideKind {
    Slide,
    Layout,
    Master,
    Notes,
    NotesMaster,
    HandoutMaster,
}

impl SlideKind {
    /// The root element name for this kind.
    pub fn root_name(self) -> &'static str {
        match self {
            SlideKind::Slide => "sld",
            SlideKind::Layout => "sldLayout",
            SlideKind::Master => "sldMaster",
            SlideKind::Notes => "notes",
            SlideKind::NotesMaster => "notesMaster",
            SlideKind::HandoutMaster => "handoutMaster",
        }
    }

    pub fn from_root(local: &[u8]) -> Option<SlideKind> {
        match local {
            b"sld" => Some(SlideKind::Slide),
            b"sldLayout" => Some(SlideKind::Layout),
            b"sldMaster" => Some(SlideKind::Master),
            b"notes" => Some(SlideKind::Notes),
            b"notesMaster" => Some(SlideKind::NotesMaster),
            b"handoutMaster" => Some(SlideKind::HandoutMaster),
            _ => None,
        }
    }
}

/// The parsed content of one slide-family part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideContent {
    pub kind: SlideKind,
    /// `cSld/@name`.
    pub name: Option<String>,
    /// `sld/@show`; hidden slides are `false`.
    pub show: bool,
    /// Shapes in z-order, which is also document and reading order.
    pub shapes: Vec<Shape>,
}

impl SlideContent {
    /// Every shape in document order, descending into groups.
    pub fn walk(&self) -> impl Iterator<Item = &Shape> {
        let mut stack: Vec<&Shape> = self.shapes.iter().rev().collect();
        std::iter::from_fn(move || {
            let shape = stack.pop()?;
            if let Content::Group(children, _) = &shape.content {
                stack.extend(children.iter().rev());
            }
            Some(shape)
        })
    }

    /// The first title placeholder's text, if any.
    pub fn title(&self) -> Option<String> {
        self.walk()
            .find(|shape| shape.is_title())
            .and_then(Shape::text_body)
            .map(TextBody::text)
            .filter(|text| !text.trim().is_empty())
    }
}
