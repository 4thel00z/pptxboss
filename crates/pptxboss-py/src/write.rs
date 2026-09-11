//! The `pptxboss.write` submodule: build decks from Python.
//!
//! `Presentation` and `Slide` are mutable builders; coordinates are given
//! in inches. `to_bytes()` and `save()` release the GIL while serializing.

use std::path::PathBuf;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use pptxboss_write::{
    Layout, Metadata, Paragraph as CoreParagraph, Presentation as CorePresentation, Rect,
    Slide as CoreSlide, SlideSize,
};

use crate::pptx_err;

fn layout_from(name: Option<&str>) -> PyResult<Option<Layout>> {
    Ok(match name {
        None => None,
        Some("title") => Some(Layout::Title),
        Some("title_and_content") | Some("content") => Some(Layout::TitleAndContent),
        Some("title_only") => Some(Layout::TitleOnly),
        Some("blank") => Some(Layout::Blank),
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "unknown layout {other:?}; use title, title_and_content, title_only or blank"
            )))
        }
    })
}

/// A slide size: a preset name or `(width, height)` in inches.
#[derive(FromPyObject)]
enum SizeArg {
    Inches((f64, f64)),
    Name(String),
}

fn widescreen() -> SizeArg {
    SizeArg::Name("widescreen".to_string())
}

fn size_from(size: &SizeArg) -> PyResult<SlideSize> {
    match size {
        SizeArg::Inches((width, height)) => {
            let rect = Rect::inches(0.0, 0.0, *width, *height);
            Ok(SlideSize {
                cx: rect.cx,
                cy: rect.cy,
            })
        }
        SizeArg::Name(name) => match name.as_str() {
            "widescreen" | "16:9" => Ok(SlideSize::WIDESCREEN),
            "standard" | "4:3" => Ok(SlideSize::STANDARD),
            other => Err(PyValueError::new_err(format!(
                "unknown size {other:?}; use widescreen, standard or (width, height) in inches"
            ))),
        },
    }
}

fn styled(
    mut paragraph: CoreParagraph,
    bold: bool,
    italic: bool,
    size: Option<u32>,
) -> CoreParagraph {
    paragraph.bold = bold;
    paragraph.italic = italic;
    paragraph.size = size;
    paragraph
}

/// One paragraph with its formatting; `size` is in points.
#[pyclass(module = "pptxboss.write")]
#[derive(Clone)]
pub struct Paragraph {
    inner: CoreParagraph,
}

#[pymethods]
impl Paragraph {
    #[new]
    #[pyo3(signature = (text, *, level=0, bullet=false, bold=false, italic=false, size=None))]
    fn new(
        text: String,
        level: u8,
        bullet: bool,
        bold: bool,
        italic: bool,
        size: Option<u32>,
    ) -> Self {
        let mut inner = match bullet {
            true => CoreParagraph::bullet(text, level),
            false => CoreParagraph::text(text),
        };
        inner.level = level;
        Self {
            inner: styled(inner, bold, italic, size),
        }
    }

    #[getter]
    fn text(&self) -> String {
        self.inner.text.clone()
    }

    #[getter]
    fn level(&self) -> u8 {
        self.inner.level
    }

    #[getter]
    fn bullet(&self) -> bool {
        self.inner.bullet
    }

    #[getter]
    fn bold(&self) -> bool {
        self.inner.bold
    }

    #[getter]
    fn italic(&self) -> bool {
        self.inner.italic
    }

    #[getter]
    fn size(&self) -> Option<u32> {
        self.inner.size
    }

    fn __repr__(&self) -> String {
        format!("write.Paragraph({:?})", self.inner.text)
    }
}

/// A text box line: a plain string or a formatted `Paragraph`.
#[derive(FromPyObject)]
enum Line {
    Paragraph(Paragraph),
    Text(String),
}

/// One slide under construction.
#[pyclass(module = "pptxboss.write")]
#[derive(Clone)]
pub struct Slide {
    inner: CoreSlide,
}

#[pymethods]
impl Slide {
    #[new]
    #[pyo3(signature = (title=None, *, subtitle=None, layout=None, notes=None, hidden=false))]
    fn new(
        title: Option<String>,
        subtitle: Option<String>,
        layout: Option<&str>,
        notes: Option<String>,
        hidden: bool,
    ) -> PyResult<Self> {
        let layout = layout_from(layout)?;
        let mut inner = CoreSlide::new();
        inner.title = title;
        inner.subtitle = subtitle;
        inner.layout = layout;
        inner.notes = notes;
        inner.hidden = hidden;
        Ok(Self { inner })
    }

    /// Adds a bullet to the body placeholder; `size` is in points.
    #[pyo3(signature = (text, level=0, *, bold=false, italic=false, size=None))]
    fn bullet(
        mut slf: PyRefMut<'_, Self>,
        text: String,
        level: u8,
        bold: bool,
        italic: bool,
        size: Option<u32>,
    ) -> PyRefMut<'_, Self> {
        let paragraph = styled(CoreParagraph::bullet(text, level), bold, italic, size);
        slf.inner.body.push(paragraph);
        slf
    }

    /// Adds a plain paragraph to the body placeholder; `size` is in points.
    #[pyo3(signature = (text, *, bold=false, italic=false, size=None))]
    fn paragraph(
        mut slf: PyRefMut<'_, Self>,
        text: String,
        bold: bool,
        italic: bool,
        size: Option<u32>,
    ) -> PyRefMut<'_, Self> {
        let paragraph = styled(CoreParagraph::text(text), bold, italic, size);
        slf.inner.body.push(paragraph);
        slf
    }

    /// Adds a text box at `(x, y)` inches, `w` by `h` inches; one paragraph
    /// per line. A plain string takes the keyword formatting; a `Paragraph`
    /// keeps its own.
    #[pyo3(signature = (x, y, w, h, lines, *, bullets=false, bold=false, italic=false, size=None))]
    #[allow(clippy::too_many_arguments)]
    fn text_box(
        mut slf: PyRefMut<'_, Self>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        lines: Vec<Line>,
        bullets: bool,
        bold: bool,
        italic: bool,
        size: Option<u32>,
    ) -> PyRefMut<'_, Self> {
        let paragraphs = lines
            .into_iter()
            .map(|line| match line {
                Line::Paragraph(paragraph) => paragraph.inner,
                Line::Text(text) => {
                    let paragraph = match bullets {
                        true => CoreParagraph::bullet(text, 0),
                        false => CoreParagraph::text(text),
                    };
                    styled(paragraph, bold, italic, size)
                }
            })
            .collect();
        slf.inner.shapes.push(pptxboss_write::Shape::Text {
            paragraphs,
            rect: Rect::inches(x, y, w, h),
        });
        slf
    }

    /// Adds a table of cell texts; every row must have the same length.
    #[pyo3(signature = (x, y, w, h, rows, *, header=true))]
    fn table(
        mut slf: PyRefMut<'_, Self>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        rows: Vec<Vec<String>>,
        header: bool,
    ) -> PyRefMut<'_, Self> {
        slf.inner = std::mem::take(&mut slf.inner).table(Rect::inches(x, y, w, h), rows, header);
        slf
    }

    /// Adds a picture (PNG, JPEG, GIF, BMP or TIFF bytes).
    #[pyo3(signature = (data, x, y, w, h, *, description=None))]
    fn picture(
        mut slf: PyRefMut<'_, Self>,
        data: Vec<u8>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        description: Option<String>,
    ) -> PyRefMut<'_, Self> {
        let rect = Rect::inches(x, y, w, h);
        slf.inner = match description {
            Some(description) => {
                std::mem::take(&mut slf.inner).picture_described(data, rect, description)
            }
            None => std::mem::take(&mut slf.inner).picture(data, rect),
        };
        slf
    }

    #[getter]
    fn title(&self) -> Option<String> {
        self.inner.title.clone()
    }

    #[getter]
    fn notes(&self) -> Option<String> {
        self.inner.notes.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "write.Slide(title={:?}, body={}, shapes={})",
            self.inner.title,
            self.inner.body.len(),
            self.inner.shapes.len()
        )
    }
}

/// A deck under construction.
#[pyclass(module = "pptxboss.write")]
pub struct Presentation {
    inner: CorePresentation,
}

#[pymethods]
impl Presentation {
    /// `size` is `"widescreen"`, `"standard"` or `(width, height)` in inches;
    /// `timestamp` is a W3C-DTF string used for both created and modified.
    #[new]
    #[pyo3(signature = (*, size=widescreen(), font=None, title=None, creator=None, subject=None, keywords=None, timestamp=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        size: SizeArg,
        font: Option<String>,
        title: Option<String>,
        creator: Option<String>,
        subject: Option<String>,
        keywords: Option<String>,
        timestamp: Option<String>,
    ) -> PyResult<Self> {
        let mut inner = CorePresentation::new().size(size_from(&size)?);
        if let Some(font) = font {
            inner = inner.font(font);
        }
        let defaults = Metadata::default();
        let metadata = Metadata {
            title,
            creator: creator.unwrap_or(defaults.creator),
            subject,
            keywords,
            timestamp: timestamp.unwrap_or(defaults.timestamp),
        };
        inner = inner.metadata(metadata);
        Ok(Self { inner })
    }

    /// Appends a slide.
    fn add<'py>(mut slf: PyRefMut<'py, Self>, slide: &Slide) -> PyRefMut<'py, Self> {
        slf.inner.slides.push(slide.inner.clone());
        slf
    }

    #[getter]
    fn slide_count(&self) -> usize {
        self.inner.slides.len()
    }

    fn __len__(&self) -> usize {
        self.inner.slides.len()
    }

    /// The `.pptx` bytes.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let inner = self.inner.clone();
        let bytes = py
            .allow_threads(move || inner.to_bytes())
            .map_err(pptx_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Writes the deck to `path`.
    fn save(&self, py: Python<'_>, path: PathBuf) -> PyResult<()> {
        let inner = self.inner.clone();
        py.allow_threads(move || inner.write_to(path))
            .map_err(pptx_err)
    }

    fn __repr__(&self) -> String {
        format!("write.Presentation(slides={})", self.inner.slides.len())
    }
}

/// Builds a presentation from Markdown text.
#[pyfunction]
#[pyo3(signature = (markdown, *, size=widescreen(), font=None))]
fn from_markdown(markdown: &str, size: SizeArg, font: Option<String>) -> PyResult<Presentation> {
    let mut inner = pptxboss_write::from_markdown(markdown).size(size_from(&size)?);
    if let Some(font) = font {
        inner = inner.font(font);
    }
    Ok(Presentation { inner })
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let module = PyModule::new(py, "write")?;
    module.add_class::<Presentation>()?;
    module.add_class::<Slide>()?;
    module.add_class::<Paragraph>()?;
    module.add_function(wrap_pyfunction!(from_markdown, &module)?)?;
    parent.add_submodule(&module)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pptxboss._pptxboss.write", &module)?;
    Ok(())
}
