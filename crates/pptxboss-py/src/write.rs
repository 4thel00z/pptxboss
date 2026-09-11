//! The `pptxboss.write` submodule: build decks from Python.
//!
//! `Presentation` and `Slide` are mutable builders; coordinates are given
//! in inches. `to_bytes()` and `save()` release the GIL while serializing.

use std::path::PathBuf;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use pptxboss_write::{
    Layout, Metadata, Paragraph, Presentation as CorePresentation, Rect, Slide as CoreSlide,
    SlideSize,
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

fn size_from(name: &str) -> PyResult<SlideSize> {
    match name {
        "widescreen" | "16:9" => Ok(SlideSize::WIDESCREEN),
        "standard" | "4:3" => Ok(SlideSize::STANDARD),
        other => Err(PyValueError::new_err(format!(
            "unknown size {other:?}; use widescreen or standard"
        ))),
    }
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

    /// Adds a bullet to the body placeholder.
    #[pyo3(signature = (text, level=0))]
    fn bullet(mut slf: PyRefMut<'_, Self>, text: String, level: u8) -> PyRefMut<'_, Self> {
        slf.inner.body.push(Paragraph::bullet(text, level));
        slf
    }

    /// Adds a plain paragraph to the body placeholder.
    fn paragraph(mut slf: PyRefMut<'_, Self>, text: String) -> PyRefMut<'_, Self> {
        slf.inner.body.push(Paragraph::text(text));
        slf
    }

    /// Adds a text box at `(x, y)` inches, `w` by `h` inches; one paragraph per line.
    #[pyo3(signature = (x, y, w, h, lines, *, bullets=false, bold=false, size=None))]
    #[allow(clippy::too_many_arguments)]
    fn text_box(
        mut slf: PyRefMut<'_, Self>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        lines: Vec<String>,
        bullets: bool,
        bold: bool,
        size: Option<u32>,
    ) -> PyRefMut<'_, Self> {
        let paragraphs = lines
            .into_iter()
            .map(|line| {
                let mut paragraph = match bullets {
                    true => Paragraph::bullet(line, 0),
                    false => Paragraph::text(line),
                };
                paragraph.bold = bold;
                paragraph.size = size;
                paragraph
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
    #[new]
    #[pyo3(signature = (*, size="widescreen", font=None, title=None, creator=None))]
    fn new(
        size: &str,
        font: Option<String>,
        title: Option<String>,
        creator: Option<String>,
    ) -> PyResult<Self> {
        let mut inner = CorePresentation::new().size(size_from(size)?);
        if let Some(font) = font {
            inner = inner.font(font);
        }
        let mut metadata = Metadata::default();
        metadata.title = title;
        if let Some(creator) = creator {
            metadata.creator = creator;
        }
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
#[pyo3(signature = (markdown, *, size="widescreen", font=None))]
fn from_markdown(markdown: &str, size: &str, font: Option<String>) -> PyResult<Presentation> {
    let mut inner = pptxboss_write::from_markdown(markdown).size(size_from(size)?);
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
    module.add_function(wrap_pyfunction!(from_markdown, &module)?)?;
    parent.add_submodule(&module)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pptxboss._pptxboss.write", &module)?;
    Ok(())
}
