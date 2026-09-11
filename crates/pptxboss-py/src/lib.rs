//! Python bindings for pptxboss, compiled as the extension module
//! `pptxboss._pptxboss` and re-exported by the `pptxboss` package.
//!
//! `Document` and `Slide` are frozen pyclasses usable from any Python
//! thread. Every call that reads the archive releases the GIL and works on
//! a private materialization of the document's shareable core
//! (`DocumentSeed`), so calls from different threads run in parallel.

use std::path::PathBuf;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use pptxboss_core::model::{Content, PlaceholderKind, Shape as CoreShape, SlideContent};
use pptxboss_core::text::write_content_text;
use pptxboss_core::{Document as CoreDocument, DocumentSeed, SlideReport, TextOptions};

create_exception!(
    pptxboss,
    PptxError,
    PyException,
    "Raised for any PowerPoint processing error (bad data, unreadable parts, I/O)."
);

mod write;

pub(crate) fn pptx_err(err: impl std::fmt::Display) -> PyErr {
    PptxError::new_err(err.to_string())
}

fn options(notes: bool, furniture: bool, hidden_shapes: bool, hidden_slides: bool) -> TextOptions {
    TextOptions {
        notes,
        furniture,
        hidden_shapes,
        hidden_slides,
        ..TextOptions::default()
    }
}

/// An open presentation.
#[pyclass(frozen, module = "pptxboss")]
struct Document {
    seed: DocumentSeed,
    slide_count: usize,
    path: Option<String>,
}

impl Document {
    fn core(&self) -> CoreDocument {
        CoreDocument::from_seed(self.seed.clone())
    }

    fn resolve_index(&self, index: isize) -> PyResult<usize> {
        let count = self.slide_count as isize;
        let resolved = match index < 0 {
            true => index + count,
            false => index,
        };
        if resolved < 0 || resolved >= count {
            return Err(PyIndexError::new_err(format!(
                "slide index {index} out of range for {count} slides"
            )));
        }
        Ok(resolved as usize)
    }
}

#[pymethods]
impl Document {
    /// Opens a deck from a path, or from bytes with `data=`. `threads` caps
    /// the workers used by whole-deck calls; None or 0 means every core.
    #[new]
    #[pyo3(signature = (path=None, *, data=None, threads=None))]
    fn new(
        py: Python<'_>,
        path: Option<PathBuf>,
        data: Option<Vec<u8>>,
        threads: Option<usize>,
    ) -> PyResult<Self> {
        let threads = threads.unwrap_or(0);
        let opened = match (path.as_ref(), data) {
            (Some(path), None) => py.allow_threads(|| {
                CoreDocument::open(path)
                    .map(|doc| doc.with_threads(threads))
                    .map(|doc| (doc.seed(), doc.slide_count()))
            }),
            (None, Some(data)) => py.allow_threads(|| {
                CoreDocument::load(data)
                    .map(|doc| doc.with_threads(threads))
                    .map(|doc| (doc.seed(), doc.slide_count()))
            }),
            _ => {
                return Err(PyValueError::new_err(
                    "pass either a path or data=, not both",
                ))
            }
        };
        let (seed, slide_count) = opened.map_err(pptx_err)?;
        Ok(Self {
            seed,
            slide_count,
            path: path.map(|path| path.display().to_string()),
        })
    }

    /// Number of slides in presentation order.
    #[getter]
    fn slide_count(&self) -> usize {
        self.slide_count
    }

    /// The worker cap for whole-deck calls; 0 means every core.
    #[getter]
    fn threads(&self) -> usize {
        self.seed.threads()
    }

    #[getter]
    fn path(&self) -> Option<String> {
        self.path.clone()
    }

    /// The Presentation part name, usually `/ppt/presentation.xml`.
    #[getter]
    fn presentation_part(&self) -> String {
        self.core().presentation_part().to_string()
    }

    /// Slide size in EMU as `(cx, cy)`, if declared.
    #[getter]
    fn slide_size(&self) -> Option<(i64, i64)> {
        self.core()
            .presentation()
            .slide_size
            .as_ref()
            .map(|size| (size.cx, size.cy))
    }

    /// The declared slide size type such as `screen16x9`, if any.
    #[getter]
    fn slide_size_type(&self) -> Option<String> {
        self.core()
            .presentation()
            .slide_size
            .as_ref()
            .and_then(|size| size.kind.clone())
    }

    fn __len__(&self) -> usize {
        self.slide_count
    }

    fn __repr__(&self) -> String {
        match &self.path {
            Some(path) => format!("Document({path:?}, slides={})", self.slide_count),
            None => format!("Document(<bytes>, slides={})", self.slide_count),
        }
    }

    /// Parses one slide; negative indexes count from the end.
    fn slide(&self, py: Python<'_>, index: isize) -> PyResult<Slide> {
        let index = self.resolve_index(index)?;
        let seed = self.seed.clone();
        py.allow_threads(|| Slide::load(seed, index))
            .map_err(pptx_err)
    }

    fn __getitem__(&self, py: Python<'_>, index: isize) -> PyResult<Slide> {
        self.slide(py, index)
    }

    fn __iter__(&self) -> SlideIter {
        SlideIter {
            seed: self.seed.clone(),
            count: self.slide_count,
            next: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Every slide, parsed in parallel across cores.
    fn slides(&self, py: Python<'_>) -> PyResult<Vec<Slide>> {
        let seed = self.seed.clone();
        let slides = py.allow_threads(|| {
            let doc = CoreDocument::from_seed(seed.clone());
            doc.map_slides(|slide| {
                slide
                    .map(|slide| {
                        Slide::from_parsed(
                            seed.clone(),
                            slide.index,
                            slide.part.clone(),
                            slide.content,
                            slide.report,
                        )
                    })
                    .map_err(|err| err.to_string())
            })
        });
        slides
            .into_iter()
            .map(|slide| slide.map_err(pptx_err))
            .collect()
    }

    /// The text of the whole deck: slides separated by a blank line.
    #[pyo3(signature = (*, notes=false, furniture=false, hidden_shapes=false, hidden_slides=true))]
    fn text(
        &self,
        py: Python<'_>,
        notes: bool,
        furniture: bool,
        hidden_shapes: bool,
        hidden_slides: bool,
    ) -> String {
        let options = options(notes, furniture, hidden_shapes, hidden_slides);
        let seed = self.seed.clone();
        py.allow_threads(|| CoreDocument::from_seed(seed).text_reporting(&options).0)
    }

    /// The text plus one warning line per problem the reader skipped.
    #[pyo3(signature = (*, notes=false, furniture=false, hidden_shapes=false, hidden_slides=true))]
    fn text_reporting(
        &self,
        py: Python<'_>,
        notes: bool,
        furniture: bool,
        hidden_shapes: bool,
        hidden_slides: bool,
    ) -> (String, Vec<String>) {
        let options = options(notes, furniture, hidden_shapes, hidden_slides);
        let seed = self.seed.clone();
        let (text, report) =
            py.allow_threads(|| CoreDocument::from_seed(seed).text_reporting(&options));
        (text, report.warnings())
    }

    /// One string per slide, in order, extracted in parallel.
    #[pyo3(signature = (*, notes=false, furniture=false, hidden_shapes=false, hidden_slides=true))]
    fn slide_texts(
        &self,
        py: Python<'_>,
        notes: bool,
        furniture: bool,
        hidden_shapes: bool,
        hidden_slides: bool,
    ) -> Vec<String> {
        let options = options(notes, furniture, hidden_shapes, hidden_slides);
        let seed = self.seed.clone();
        py.allow_threads(|| CoreDocument::from_seed(seed).slide_texts(&options).0)
    }

    /// The title of every slide (None where a slide has no title placeholder).
    fn titles(&self, py: Python<'_>) -> Vec<Option<String>> {
        let seed = self.seed.clone();
        py.allow_threads(|| {
            CoreDocument::from_seed(seed)
                .map_slides(|slide| slide.ok().and_then(|slide| slide.title()))
        })
    }
}

/// Iterates a document's slides lazily.
#[pyclass(frozen, module = "pptxboss")]
struct SlideIter {
    seed: DocumentSeed,
    count: usize,
    next: std::sync::atomic::AtomicUsize,
}

#[pymethods]
impl SlideIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Slide>> {
        let index = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if index >= self.count {
            return Ok(None);
        }
        let seed = self.seed.clone();
        py.allow_threads(|| Slide::load(seed, index))
            .map(Some)
            .map_err(pptx_err)
    }
}

/// One parsed slide.
#[pyclass(frozen, module = "pptxboss")]
struct Slide {
    seed: DocumentSeed,
    index: usize,
    part: String,
    content: SlideContent,
    report: SlideReport,
}

impl Slide {
    fn load(seed: DocumentSeed, index: usize) -> pptxboss_core::Result<Self> {
        let doc = CoreDocument::from_seed(seed.clone());
        let slide = doc.slide(index)?;
        Ok(Self::from_parsed(
            seed,
            index,
            slide.part.clone(),
            slide.content,
            slide.report,
        ))
    }

    fn from_parsed(
        seed: DocumentSeed,
        index: usize,
        part: String,
        content: SlideContent,
        report: SlideReport,
    ) -> Self {
        Self {
            seed,
            index,
            part,
            content,
            report,
        }
    }
}

#[pymethods]
impl Slide {
    /// Zero-based position in the deck.
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// One-based position in the deck.
    #[getter]
    fn number(&self) -> usize {
        self.index + 1
    }

    /// The Slide part name, such as `/ppt/slides/slide1.xml`.
    #[getter]
    fn part(&self) -> String {
        self.part.clone()
    }

    /// True for slides marked hidden.
    #[getter]
    fn hidden(&self) -> bool {
        !self.content.show
    }

    /// `cSld/@name`, if set.
    #[getter]
    fn name(&self) -> Option<String> {
        self.content.name.clone()
    }

    /// The first title placeholder's text.
    #[getter]
    fn title(&self) -> Option<String> {
        self.content.title()
    }

    /// One line per problem the parser skipped on this slide.
    #[getter]
    fn warnings(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .report
            .unknown_graphics
            .iter()
            .map(|uri| format!("graphic frame of unknown type skipped: {uri}"))
            .collect();
        if self.report.unknown_elements > 0 {
            lines.push(format!(
                "{} element(s) in unknown namespaces skipped",
                self.report.unknown_elements
            ));
        }
        lines
    }

    fn __repr__(&self) -> String {
        format!("Slide(number={}, part={:?})", self.index + 1, self.part)
    }

    /// The slide's text: shapes in z-order, paragraphs one per line.
    #[pyo3(signature = (*, furniture=false, hidden_shapes=false))]
    fn text(&self, furniture: bool, hidden_shapes: bool) -> String {
        let mut out = String::new();
        write_content_text(
            &self.content,
            &options(false, furniture, hidden_shapes, true),
            &mut out,
        );
        out
    }

    /// Every non-empty paragraph on the slide, in order, including table cells.
    fn paragraphs(&self) -> Vec<String> {
        let mut out = Vec::new();
        for shape in self.content.walk() {
            match &shape.content {
                Content::Text(body) => out.extend(
                    body.paragraphs
                        .iter()
                        .map(|p| p.text())
                        .filter(|text| !text.trim().is_empty()),
                ),
                Content::Table(table) => {
                    for cell in table
                        .rows
                        .iter()
                        .flat_map(|row| row.cells.iter())
                        .filter(|cell| cell.is_origin())
                    {
                        out.extend(
                            cell.body
                                .paragraphs
                                .iter()
                                .map(|p| p.text())
                                .filter(|text| !text.trim().is_empty()),
                        );
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The speaker notes as text, or None.
    fn notes(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let seed = self.seed.clone();
        let index = self.index;
        py.allow_threads(|| CoreDocument::from_seed(seed).slide(index)?.notes_text())
            .map_err(pptx_err)
    }

    /// Every table as rows of cell texts; merged-away cells are omitted.
    fn tables(&self) -> Vec<Vec<Vec<String>>> {
        self.content
            .walk()
            .filter_map(|shape| match &shape.content {
                Content::Table(table) => Some(
                    table
                        .rows
                        .iter()
                        .map(|row| {
                            row.cells
                                .iter()
                                .filter(|cell| cell.is_origin())
                                .map(|cell| cell.body.text())
                                .collect()
                        })
                        .collect(),
                ),
                _ => None,
            })
            .collect()
    }

    /// The top-level shapes in z-order; groups carry their children.
    fn shapes(&self) -> Vec<Shape> {
        self.content.shapes.iter().map(Shape::from_core).collect()
    }

    /// Pictures on the slide with their image parts.
    fn images(&self, py: Python<'_>) -> PyResult<Vec<Image>> {
        let seed = self.seed.clone();
        let index = self.index;
        let images = py
            .allow_threads(|| CoreDocument::from_seed(seed).slide(index)?.images())
            .map_err(pptx_err)?;
        Ok(images
            .into_iter()
            .map(|image| Image {
                shape_id: image.shape_id,
                rel_id: image.rel_id,
                part: image.part,
                content_type: image.content_type,
                external: image.external,
            })
            .collect())
    }

    /// The bytes of an image part referenced from this slide.
    fn image_bytes<'py>(&self, py: Python<'py>, image: &Image) -> PyResult<Bound<'py, PyBytes>> {
        let seed = self.seed.clone();
        let part = image.part.clone().ok_or_else(|| {
            pptx_err(format!(
                "image {} is not stored in the package",
                image.rel_id
            ))
        })?;
        let bytes = py
            .allow_threads(|| {
                let doc = CoreDocument::from_seed(seed);
                let mut out = Vec::new();
                doc.package().read_part_into(&part, &mut out)?;
                Ok::<_, pptxboss_core::Error>(out)
            })
            .map_err(pptx_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// The target of a hyperlink relationship id: a URL, or an internal part name.
    fn hyperlink(&self, py: Python<'_>, rel_id: &str) -> PyResult<Option<String>> {
        let seed = self.seed.clone();
        let index = self.index;
        let rel_id = rel_id.to_string();
        py.allow_threads(|| {
            CoreDocument::from_seed(seed)
                .slide(index)?
                .hyperlink_target(&rel_id)
        })
        .map_err(pptx_err)
    }
}

/// One node of a slide's shape tree.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
struct Shape {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    name: String,
    /// One of `text`, `picture`, `table`, `group`, `chart`, `diagram`, `ole`, `connector`, `content_part`, `unknown`.
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    hidden: bool,
    #[pyo3(get)]
    placeholder: Option<String>,
    #[pyo3(get)]
    placeholder_index: Option<u32>,
    #[pyo3(get)]
    text: Option<String>,
    #[pyo3(get)]
    description: Option<String>,
    #[pyo3(get)]
    hyperlink: Option<String>,
    /// `(x, y, cx, cy)` in EMU when the shape carries a transform.
    #[pyo3(get)]
    frame: Option<(i64, i64, i64, i64)>,
    #[pyo3(get)]
    rotation: i64,
    #[pyo3(get)]
    image_rel: Option<String>,
    #[pyo3(get)]
    rows: Option<Vec<Vec<String>>>,
    #[pyo3(get)]
    children: Vec<Shape>,
}

impl Shape {
    fn from_core(shape: &CoreShape) -> Self {
        let (kind, text, image_rel, rows, children) = match &shape.content {
            Content::Text(body) => ("text", Some(body.text()), None, None, Vec::new()),
            Content::Picture(picture) => (
                "picture",
                None,
                picture.embed.clone().or_else(|| picture.link.clone()),
                None,
                Vec::new(),
            ),
            Content::Table(table) => (
                "table",
                None,
                None,
                Some(
                    table
                        .rows
                        .iter()
                        .map(|row| {
                            row.cells
                                .iter()
                                .filter(|cell| cell.is_origin())
                                .map(|cell| cell.body.text())
                                .collect()
                        })
                        .collect(),
                ),
                Vec::new(),
            ),
            Content::Group(children, _) => (
                "group",
                None,
                None,
                None,
                children.iter().map(Shape::from_core).collect(),
            ),
            Content::Chart(_) => ("chart", None, None, None, Vec::new()),
            Content::Diagram(_) => ("diagram", None, None, None, Vec::new()),
            Content::Ole(_) => ("ole", None, None, None, Vec::new()),
            Content::Connector => ("connector", None, None, None, Vec::new()),
            Content::ContentPart(_) => ("content_part", None, None, None, Vec::new()),
            Content::UnknownGraphic(_) => ("unknown", None, None, None, Vec::new()),
        };
        Self {
            id: shape.id,
            name: shape.name.clone(),
            kind: kind.to_string(),
            hidden: shape.hidden,
            placeholder: shape
                .placeholder
                .as_ref()
                .map(|ph| ph.kind.as_str().to_string()),
            placeholder_index: shape.placeholder.as_ref().map(|ph| ph.idx),
            text,
            description: shape.description.clone(),
            hyperlink: shape.hyperlink.clone(),
            frame: shape.transform.map(|t| (t.x, t.y, t.cx, t.cy)),
            rotation: shape.transform.map_or(0, |t| t.rot),
            image_rel,
            rows,
            children,
        }
    }
}

#[pymethods]
impl Shape {
    /// True when the placeholder is a title or centered title.
    #[getter]
    fn is_title(&self) -> bool {
        matches!(self.placeholder.as_deref(), Some(kind) if PlaceholderKind::parse(kind.as_bytes()).is_title())
    }

    fn __repr__(&self) -> String {
        format!(
            "Shape(id={}, kind={:?}, name={:?})",
            self.id, self.kind, self.name
        )
    }
}

/// An image referenced from a slide.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
struct Image {
    #[pyo3(get)]
    shape_id: u32,
    #[pyo3(get)]
    rel_id: String,
    #[pyo3(get)]
    part: Option<String>,
    #[pyo3(get)]
    content_type: Option<String>,
    #[pyo3(get)]
    external: Option<String>,
}

#[pymethods]
impl Image {
    fn __repr__(&self) -> String {
        format!(
            "Image(shape_id={}, part={:?}, content_type={:?})",
            self.shape_id, self.part, self.content_type
        )
    }
}

/// One verifier finding.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
struct Finding {
    /// Stable rule code such as `REL004`.
    #[pyo3(get)]
    code: String,
    /// `error`, `warning` or `info`.
    #[pyo3(get)]
    severity: String,
    /// The ECMA-376 clause the rule enforces.
    #[pyo3(get)]
    clause: String,
    #[pyo3(get)]
    part: Option<String>,
    #[pyo3(get)]
    location: Option<String>,
    #[pyo3(get)]
    message: String,
}

#[pymethods]
impl Finding {
    fn __repr__(&self) -> String {
        format!("Finding({} {} {})", self.severity, self.code, self.message)
    }

    fn __str__(&self) -> String {
        let part = self
            .part
            .as_deref()
            .map(|part| format!("{part}: "))
            .unwrap_or_default();
        format!(
            "{} {} {part}{} [{}]",
            self.severity, self.code, self.message, self.clause
        )
    }
}

/// One verifier rule.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
struct Rule {
    #[pyo3(get)]
    code: String,
    #[pyo3(get)]
    severity: String,
    #[pyo3(get)]
    clause: String,
    #[pyo3(get)]
    summary: String,
}

#[pymethods]
impl Rule {
    fn __repr__(&self) -> String {
        format!("Rule({} {} [{}])", self.severity, self.code, self.clause)
    }
}

/// Verifies a deck against ECMA-376 and returns its findings, most severe first.
#[pyfunction]
#[pyo3(signature = (path=None, *, data=None, max_findings=1000, verify_crc=true))]
fn check(
    py: Python<'_>,
    path: Option<PathBuf>,
    data: Option<Vec<u8>>,
    max_findings: usize,
    verify_crc: bool,
) -> PyResult<Vec<Finding>> {
    let options = pptxboss_check::CheckOptions {
        max_findings,
        verify_crc,
        ..pptxboss_check::CheckOptions::default()
    };
    let report = match (path, data) {
        (Some(path), None) => py.allow_threads(|| pptxboss_check::check_path(path, &options)),
        (None, Some(data)) => py.allow_threads(|| pptxboss_check::check_bytes(data, &options)),
        _ => {
            return Err(PyValueError::new_err(
                "pass either a path or data=, not both",
            ))
        }
    }
    .map_err(pptx_err)?;
    Ok(report
        .findings
        .into_iter()
        .map(|finding| Finding {
            code: finding.code.to_string(),
            severity: finding.severity.to_string(),
            clause: finding.clause.to_string(),
            part: finding.part,
            location: finding.location,
            message: finding.message,
        })
        .collect())
}

/// Every rule the verifier knows.
#[pyfunction]
fn rules() -> Vec<Rule> {
    pptxboss_check::rules()
        .into_iter()
        .map(|rule| Rule {
            code: rule.code.to_string(),
            severity: rule.severity.to_string(),
            clause: rule.clause.to_string(),
            summary: rule.summary.to_string(),
        })
        .collect()
}

#[pymodule]
fn _pptxboss(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("PptxError", m.py().get_type::<PptxError>())?;
    m.add_class::<Document>()?;
    m.add_class::<Slide>()?;
    m.add_class::<SlideIter>()?;
    m.add_class::<Shape>()?;
    m.add_class::<Image>()?;
    m.add_class::<Finding>()?;
    m.add_class::<Rule>()?;
    m.add_function(wrap_pyfunction!(check, m)?)?;
    m.add_function(wrap_pyfunction!(rules, m)?)?;
    write::register(m)?;
    Ok(())
}
