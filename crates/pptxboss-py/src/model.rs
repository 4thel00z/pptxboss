//! Structured slide content (tables, paragraphs, runs) and the deck-level
//! records: extraction reports, document defects, the presentation model
//! and comment authors.

use pyo3::prelude::*;

use pptxboss_core::comments::CommentAuthor as CoreCommentAuthor;
use pptxboss_core::document::{DocumentDefects as CoreDocumentDefects, Located};
use pptxboss_core::model::{
    Bullet, Cell as CoreCell, Paragraph as CoreParagraph, Run as CoreRun, RunKind,
    Table as CoreTable, TextBody,
};
use pptxboss_core::{ExtractReport as CoreExtractReport, Presentation as CorePresentation};

/// A table with its cell grid, spans and merges.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct Table {
    /// Column widths in EMU.
    #[pyo3(get)]
    column_widths: Vec<i64>,
    #[pyo3(get)]
    rows: Vec<Row>,
}

impl Table {
    pub fn from_core(table: &CoreTable) -> Self {
        Self {
            column_widths: table.column_widths.clone(),
            rows: table
                .rows
                .iter()
                .map(|row| Row {
                    height: row.height,
                    cells: row.cells.iter().map(Cell::from_core).collect(),
                })
                .collect(),
        }
    }
}

#[pymethods]
impl Table {
    fn __repr__(&self) -> String {
        format!(
            "Table(rows={}, columns={})",
            self.rows.len(),
            self.column_widths.len()
        )
    }
}

/// One table row.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct Row {
    /// Row height in EMU.
    #[pyo3(get)]
    height: i64,
    #[pyo3(get)]
    cells: Vec<Cell>,
}

#[pymethods]
impl Row {
    fn __repr__(&self) -> String {
        format!("Row(cells={})", self.cells.len())
    }
}

/// One table cell. A cell merged into another has `h_merge` or `v_merge`
/// set and `is_origin` false.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct Cell {
    #[pyo3(get)]
    text: String,
    #[pyo3(get)]
    paragraphs: Vec<Paragraph>,
    #[pyo3(get)]
    grid_span: u32,
    #[pyo3(get)]
    row_span: u32,
    #[pyo3(get)]
    h_merge: bool,
    #[pyo3(get)]
    v_merge: bool,
    #[pyo3(get)]
    is_origin: bool,
}

impl Cell {
    fn from_core(cell: &CoreCell) -> Self {
        Self {
            text: cell.body.text(),
            paragraphs: paragraphs_from_body(&cell.body),
            grid_span: cell.grid_span,
            row_span: cell.row_span,
            h_merge: cell.h_merge,
            v_merge: cell.v_merge,
            is_origin: cell.is_origin(),
        }
    }
}

#[pymethods]
impl Cell {
    fn __repr__(&self) -> String {
        format!("Cell({:?})", self.text)
    }
}

pub fn paragraphs_from_body(body: &TextBody) -> Vec<Paragraph> {
    body.paragraphs.iter().map(Paragraph::from_core).collect()
}

/// One paragraph with its runs and bullet.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct Paragraph {
    #[pyo3(get)]
    text: String,
    #[pyo3(get)]
    level: u8,
    /// One of `inherited`, `none`, `char`, `auto_number`, `picture`.
    #[pyo3(get)]
    bullet: String,
    /// The bullet character when `bullet` is `char`.
    #[pyo3(get)]
    bullet_char: Option<String>,
    /// The numbering scheme and start when `bullet` is `auto_number`.
    #[pyo3(get)]
    number_scheme: Option<String>,
    #[pyo3(get)]
    number_start: Option<u32>,
    #[pyo3(get)]
    runs: Vec<Run>,
}

impl Paragraph {
    fn from_core(paragraph: &CoreParagraph) -> Self {
        let (bullet, bullet_char, number_scheme, number_start) = match &paragraph.bullet {
            Bullet::Inherited => ("inherited", None, None, None),
            Bullet::None => ("none", None, None, None),
            Bullet::Char(ch) => ("char", Some(ch.clone()), None, None),
            Bullet::AutoNumber { scheme, start_at } => {
                ("auto_number", None, Some(scheme.clone()), Some(*start_at))
            }
            Bullet::Picture => ("picture", None, None, None),
        };
        Self {
            text: paragraph.text(),
            level: paragraph.level,
            bullet: bullet.to_string(),
            bullet_char,
            number_scheme,
            number_start,
            runs: paragraph.runs.iter().map(Run::from_core).collect(),
        }
    }
}

#[pymethods]
impl Paragraph {
    fn __repr__(&self) -> String {
        format!("Paragraph(level={}, text={:?})", self.level, self.text)
    }
}

/// One run of a paragraph with its formatting as written; None means not set.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct Run {
    /// One of `text`, `line_break`, `field`.
    #[pyo3(get)]
    kind: String,
    /// The field type when `kind` is `field`.
    #[pyo3(get)]
    field: Option<String>,
    #[pyo3(get)]
    text: String,
    #[pyo3(get)]
    bold: Option<bool>,
    #[pyo3(get)]
    italic: Option<bool>,
    #[pyo3(get)]
    underline: Option<bool>,
    #[pyo3(get)]
    strike: Option<bool>,
    /// Font size in hundredths of a point, as written.
    #[pyo3(get)]
    size: Option<u32>,
    /// The relationship id of a click hyperlink; `Slide.hyperlink` resolves it.
    #[pyo3(get)]
    hyperlink: Option<String>,
    #[pyo3(get)]
    lang: Option<String>,
    #[pyo3(get)]
    typeface: Option<String>,
}

impl Run {
    fn from_core(run: &CoreRun) -> Self {
        let (kind, field) = match &run.kind {
            RunKind::Text => ("text", None),
            RunKind::LineBreak => ("line_break", None),
            RunKind::Field(kind) => ("field", Some(kind.clone())),
        };
        Self {
            kind: kind.to_string(),
            field,
            text: run.text.clone(),
            bold: run.props.bold,
            italic: run.props.italic,
            underline: run.props.underline,
            strike: run.props.strike,
            size: run.props.size,
            hyperlink: run.props.hyperlink.clone(),
            lang: run.props.lang.clone(),
            typeface: run.props.typeface.clone(),
        }
    }
}

#[pymethods]
impl Run {
    fn __repr__(&self) -> String {
        format!("Run({:?})", self.text)
    }
}

/// What text extraction skipped or could not read.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct ExtractReport {
    inner: CoreExtractReport,
}

impl ExtractReport {
    pub fn from_core(inner: CoreExtractReport) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl ExtractReport {
    /// `(slide index, error)` for slides whose part could not be read.
    #[getter]
    fn failed_slides(&self) -> Vec<(usize, String)> {
        self.inner.failed_slides.clone()
    }

    #[getter]
    fn failed_notes(&self) -> Vec<(usize, String)> {
        self.inner.failed_notes.clone()
    }

    #[getter]
    fn failed_comments(&self) -> Vec<(usize, String)> {
        self.inner.failed_comments.clone()
    }

    /// Chart or diagram parts that could not be read.
    #[getter]
    fn failed_frames(&self) -> Vec<(usize, String)> {
        self.inner.failed_frames.clone()
    }

    /// Hidden slides left out because the options excluded them.
    #[getter]
    fn hidden_slides_skipped(&self) -> u32 {
        self.inner.hidden_slides_skipped
    }

    #[getter]
    fn unknown_graphics(&self) -> u32 {
        self.inner.unknown_graphics
    }

    #[getter]
    fn unknown_graphic_uris(&self) -> Vec<String> {
        self.inner.unknown_graphic_uris.clone()
    }

    #[getter]
    fn unknown_elements(&self) -> u32 {
        self.inner.unknown_elements
    }

    /// True when nothing was dropped for a reason other than the options.
    #[getter]
    fn is_complete(&self) -> bool {
        self.inner.is_complete()
    }

    /// One line per problem, as the CLI prints them.
    #[getter]
    fn warnings(&self) -> Vec<String> {
        self.inner.warnings()
    }

    fn __repr__(&self) -> String {
        format!(
            "ExtractReport(complete={}, warnings={})",
            self.inner.is_complete(),
            self.inner.warnings().len()
        )
    }
}

/// What the document layer worked around to find the slides.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct DocumentDefects {
    /// How the presentation part was found: `relationship`, `content_type`,
    /// `conventional_path` or `legacy_stream`.
    #[pyo3(get)]
    located: String,
    /// `(position, relationship id)` of slide entries that resolve to nothing.
    #[pyo3(get)]
    unresolved_slides: Vec<(usize, String)>,
    #[pyo3(get)]
    slides_recovered_from_rels: bool,
}

impl DocumentDefects {
    pub fn from_core(defects: &CoreDocumentDefects) -> Self {
        let located = match defects.located {
            Located::LegacyStream => "legacy_stream",
            Located::Relationship => "relationship",
            Located::ContentType => "content_type",
            Located::ConventionalPath => "conventional_path",
        };
        Self {
            located: located.to_string(),
            unresolved_slides: defects.unresolved_slides.clone(),
            slides_recovered_from_rels: defects.slides_recovered_from_rels,
        }
    }
}

#[pymethods]
impl DocumentDefects {
    fn __repr__(&self) -> String {
        format!(
            "DocumentDefects(located={:?}, unresolved_slides={})",
            self.located,
            self.unresolved_slides.len()
        )
    }
}

/// The parsed presentation part: slide and master lists, sizes and flags.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct Presentation {
    #[pyo3(get)]
    slides: Vec<SlideId>,
    #[pyo3(get)]
    masters: Vec<MasterId>,
    #[pyo3(get)]
    notes_master: Option<String>,
    #[pyo3(get)]
    handout_master: Option<String>,
    /// `(cx, cy)` in EMU.
    #[pyo3(get)]
    slide_size: Option<(i64, i64)>,
    #[pyo3(get)]
    slide_size_type: Option<String>,
    #[pyo3(get)]
    notes_size: Option<(i64, i64)>,
    #[pyo3(get)]
    first_slide_num: i32,
    #[pyo3(get)]
    rtl: bool,
}

impl Presentation {
    pub fn from_core(presentation: &CorePresentation) -> Self {
        Self {
            slides: presentation
                .slides
                .iter()
                .map(|slide| SlideId {
                    id: slide.id,
                    rel_id: slide.rel_id.clone(),
                })
                .collect(),
            masters: presentation
                .masters
                .iter()
                .map(|master| MasterId {
                    id: master.id,
                    rel_id: master.rel_id.clone(),
                })
                .collect(),
            notes_master: presentation.notes_master.clone(),
            handout_master: presentation.handout_master.clone(),
            slide_size: presentation
                .slide_size
                .as_ref()
                .map(|size| (size.cx, size.cy)),
            slide_size_type: presentation
                .slide_size
                .as_ref()
                .and_then(|size| size.kind.clone()),
            notes_size: presentation.notes_size,
            first_slide_num: presentation.first_slide_num,
            rtl: presentation.rtl,
        }
    }
}

#[pymethods]
impl Presentation {
    fn __repr__(&self) -> String {
        format!(
            "Presentation(slides={}, masters={})",
            self.slides.len(),
            self.masters.len()
        )
    }
}

/// One `p:sldId` entry: the slide id and the relationship that names its part.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct SlideId {
    #[pyo3(get)]
    id: Option<u32>,
    #[pyo3(get)]
    rel_id: String,
}

#[pymethods]
impl SlideId {
    fn __repr__(&self) -> String {
        format!("SlideId(id={:?}, rel_id={:?})", self.id, self.rel_id)
    }
}

/// One `p:sldMasterId` entry.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct MasterId {
    #[pyo3(get)]
    id: Option<u32>,
    #[pyo3(get)]
    rel_id: String,
}

#[pymethods]
impl MasterId {
    fn __repr__(&self) -> String {
        format!("MasterId(id={:?}, rel_id={:?})", self.id, self.rel_id)
    }
}

/// One comment author of either comments format.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct CommentAuthor {
    #[pyo3(get)]
    id: String,
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    initials: Option<String>,
}

impl CommentAuthor {
    pub fn from_core(author: &CoreCommentAuthor) -> Self {
        Self {
            id: author.id.clone(),
            name: author.name.clone(),
            initials: author.initials.clone(),
        }
    }
}

#[pymethods]
impl CommentAuthor {
    fn __repr__(&self) -> String {
        format!("CommentAuthor({:?})", self.name)
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Table>()?;
    m.add_class::<Row>()?;
    m.add_class::<Cell>()?;
    m.add_class::<Paragraph>()?;
    m.add_class::<Run>()?;
    m.add_class::<ExtractReport>()?;
    m.add_class::<DocumentDefects>()?;
    m.add_class::<Presentation>()?;
    m.add_class::<SlideId>()?;
    m.add_class::<MasterId>()?;
    m.add_class::<CommentAuthor>()?;
    Ok(())
}
