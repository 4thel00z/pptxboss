//! The `pptxboss.write` submodule: build decks from Python.
//!
//! `Presentation` and `Slide` are mutable builders; coordinates are given
//! in inches, colors as `#RRGGBB` or a theme slot name. Content without
//! coordinates is placed by the layout engine. `to_bytes()` and `save()`
//! release the GIL while serializing.

use std::collections::HashMap;
use std::path::PathBuf;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use pptxboss_write::{
    Align, Background as CoreBackground, Block, Color, GradientStop, Layout, Metadata,
    Paragraph as CoreParagraph, Presentation as CorePresentation, Rect, Rgb, Run as CoreRun,
    SchemeColor, Slide as CoreSlide, SlideSize, Theme as CoreTheme, TypeScale,
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

fn slot_names() -> String {
    SchemeColor::ALL
        .iter()
        .map(|slot| slot.name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `#RRGGBB`, `RRGGBB` or a theme slot name.
fn color_from(text: &str) -> PyResult<Color> {
    if let Some(color) = Color::hex(text) {
        return Ok(color);
    }
    if let Some(slot) = SchemeColor::from_name(text) {
        return Ok(Color::Scheme(slot));
    }
    Err(PyValueError::new_err(format!(
        "unknown color {text:?}; use #RRGGBB or one of {}",
        slot_names()
    )))
}

fn color_name(color: Option<Color>) -> Option<String> {
    match color? {
        Color::Rgb(rgb) => Some(format!("#{}", rgb.to_hex())),
        Color::Scheme(slot) => Some(slot.name().to_string()),
    }
}

fn rgb_from(text: &str) -> PyResult<Rgb> {
    Rgb::hex(text)
        .ok_or_else(|| PyValueError::new_err(format!("theme color {text:?} must be #RRGGBB")))
}

fn align_from(name: &str) -> PyResult<Align> {
    match name {
        "left" => Ok(Align::Left),
        "center" => Ok(Align::Center),
        "right" => Ok(Align::Right),
        "justify" => Ok(Align::Justify),
        other => Err(PyValueError::new_err(format!(
            "unknown align {other:?}; use left, center, right or justify"
        ))),
    }
}

fn align_name(align: Align) -> &'static str {
    match align {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
        Align::Justify => "justify",
    }
}

/// Keyword formatting applied to plain-string items.
#[derive(Clone, Copy, Default)]
struct Style<'a> {
    bold: bool,
    italic: bool,
    size: Option<u32>,
    color: Option<&'a str>,
}

impl Style<'_> {
    fn apply(&self, mut run: CoreRun) -> PyResult<CoreRun> {
        run.bold = self.bold;
        run.italic = self.italic;
        run.size = self.size;
        run.color = self.color.map(color_from).transpose()?;
        Ok(run)
    }
}

/// Paragraph content: a string or a list of strings and runs.
#[derive(FromPyObject)]
enum Content {
    Text(String),
    Pieces(Vec<Piece>),
}

#[derive(FromPyObject)]
enum Piece {
    Run(Run),
    Text(String),
}

fn runs_from(content: Content, style: Style<'_>) -> PyResult<Vec<CoreRun>> {
    let pieces = match content {
        Content::Text(text) => vec![Piece::Text(text)],
        Content::Pieces(pieces) => pieces,
    };
    pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Run(run) => Ok(run.inner),
            Piece::Text(text) => style.apply(CoreRun::text(text)),
        })
        .collect()
}

/// A run of text with one set of character properties; `size` is in
/// points, `color` is `#RRGGBB` or a theme slot name, `link` an absolute URL.
#[pyclass(module = "pptxboss.write")]
#[derive(Clone)]
pub struct Run {
    inner: CoreRun,
}

#[pymethods]
impl Run {
    #[new]
    #[pyo3(signature = (text, *, bold=false, italic=false, underline=false, strike=false, size=None, color=None, font=None, link=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        text: String,
        bold: bool,
        italic: bool,
        underline: bool,
        strike: bool,
        size: Option<u32>,
        color: Option<&str>,
        font: Option<String>,
        link: Option<String>,
    ) -> PyResult<Self> {
        let mut inner = CoreRun::text(text);
        inner.bold = bold;
        inner.italic = italic;
        inner.underline = underline;
        inner.strike = strike;
        inner.size = size;
        inner.color = color.map(color_from).transpose()?;
        inner.font = font;
        inner.link = link;
        Ok(Self { inner })
    }

    #[getter]
    fn text(&self) -> String {
        self.inner.text.clone()
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
    fn underline(&self) -> bool {
        self.inner.underline
    }

    #[getter]
    fn strike(&self) -> bool {
        self.inner.strike
    }

    #[getter]
    fn size(&self) -> Option<u32> {
        self.inner.size
    }

    #[getter]
    fn color(&self) -> Option<String> {
        color_name(self.inner.color)
    }

    #[getter]
    fn font(&self) -> Option<String> {
        self.inner.font.clone()
    }

    #[getter]
    fn link(&self) -> Option<String> {
        self.inner.link.clone()
    }

    fn __repr__(&self) -> String {
        format!("write.Run({:?})", self.inner.text)
    }
}

/// One paragraph: a string or a list of strings and runs, plus level,
/// bullet, alignment and spacing in points. Keyword formatting applies to
/// plain strings; a `Run` keeps its own.
#[pyclass(module = "pptxboss.write")]
#[derive(Clone)]
pub struct Paragraph {
    inner: CoreParagraph,
}

#[pymethods]
impl Paragraph {
    #[new]
    #[pyo3(signature = (text, *, level=0, bullet=false, align="left", bold=false, italic=false, size=None, color=None, space_before=None, space_after=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        text: Content,
        level: u8,
        bullet: bool,
        align: &str,
        bold: bool,
        italic: bool,
        size: Option<u32>,
        color: Option<&str>,
        space_before: Option<u32>,
        space_after: Option<u32>,
    ) -> PyResult<Self> {
        let runs = runs_from(
            text,
            Style {
                bold,
                italic,
                size,
                color,
            },
        )?;
        let mut inner = match bullet {
            true => CoreParagraph::bullet_runs(runs, level),
            false => CoreParagraph::runs(runs),
        };
        inner.level = level.min(8);
        inner.align = align_from(align)?;
        inner.space_before = space_before;
        inner.space_after = space_after;
        Ok(Self { inner })
    }

    /// The runs' text joined.
    #[getter]
    fn text(&self) -> String {
        self.inner.plain_text()
    }

    #[getter]
    fn runs(&self) -> Vec<Run> {
        self.inner
            .runs
            .iter()
            .cloned()
            .map(|inner| Run { inner })
            .collect()
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
    fn align(&self) -> &'static str {
        align_name(self.inner.align)
    }

    fn __repr__(&self) -> String {
        format!("write.Paragraph({:?})", self.inner.plain_text())
    }
}

/// A background: a solid color, a linear gradient or a picture.
#[pyclass(module = "pptxboss.write")]
#[derive(Clone)]
pub struct Background {
    inner: CoreBackground,
}

#[pymethods]
impl Background {
    #[staticmethod]
    fn solid(color: &str) -> PyResult<Self> {
        Ok(Self {
            inner: CoreBackground::solid(color_from(color)?),
        })
    }

    /// `stops` are `(percent, color)` pairs; `angle` is in degrees, 90 runs top to bottom.
    #[staticmethod]
    #[pyo3(signature = (stops, angle=90))]
    fn gradient(stops: Vec<(u8, String)>, angle: u16) -> PyResult<Self> {
        let stops = stops
            .into_iter()
            .map(|(position, color)| {
                Ok(GradientStop {
                    position,
                    color: color_from(&color)?,
                })
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            inner: CoreBackground::gradient(stops, angle),
        })
    }

    /// A two-stop gradient from `start` to `end`.
    #[staticmethod]
    #[pyo3(signature = (start, end, angle=90))]
    fn linear(start: &str, end: &str, angle: u16) -> PyResult<Self> {
        Ok(Self {
            inner: CoreBackground::linear(color_from(start)?, color_from(end)?, angle),
        })
    }

    /// Picture bytes (PNG, JPEG, GIF, BMP or TIFF) stretched over the slide.
    #[staticmethod]
    fn picture(data: Vec<u8>) -> Self {
        Self {
            inner: CoreBackground::picture(data),
        }
    }

    fn __repr__(&self) -> String {
        match &self.inner {
            CoreBackground::Solid(color) => {
                format!("write.Background.solid({:?})", color_name(Some(*color)))
            }
            CoreBackground::Gradient { stops, angle } => {
                format!(
                    "write.Background.gradient(stops={}, angle={angle})",
                    stops.len()
                )
            }
            CoreBackground::Picture(data) => {
                format!("write.Background.picture({} bytes)", data.len())
            }
            _ => "write.Background".to_string(),
        }
    }
}

const SIZE_NAMES: [&str; 6] = ["display", "title", "subtitle", "body", "table", "minimum"];

fn scale_from(sizes: HashMap<String, u32>) -> PyResult<TypeScale> {
    let mut scale = TypeScale::default();
    let mut sizes: Vec<(String, u32)> = sizes.into_iter().collect();
    sizes.sort();
    for (name, points) in sizes {
        let slot = match name.as_str() {
            "display" => &mut scale.display,
            "title" => &mut scale.title,
            "subtitle" => &mut scale.subtitle,
            "body" => &mut scale.body,
            "table" => &mut scale.table,
            "minimum" => &mut scale.minimum,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown size {other:?}; use one of {}",
                    SIZE_NAMES.join(", ")
                )))
            }
        };
        if points == 0 {
            return Err(PyValueError::new_err(format!(
                "size {name:?} must be positive"
            )));
        }
        *slot = points;
    }
    Ok(scale)
}

/// Colors, fonts, type scale and backgrounds shared by every slide.
/// `colors` maps slot names (dark1, light1, dark2, light2, accent1 to
/// accent6, hyperlink, followed_hyperlink) to `#RRGGBB`; `sizes` maps
/// display, title, subtitle, body, table and minimum to points.
#[pyclass(module = "pptxboss.write")]
#[derive(Clone)]
pub struct Theme {
    inner: CoreTheme,
}

#[pymethods]
impl Theme {
    #[new]
    #[pyo3(signature = (name="pptxboss", *, colors=None, major_font=None, minor_font=None, font=None, sizes=None, inverted=false, background=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        name: &str,
        colors: Option<HashMap<String, String>>,
        major_font: Option<String>,
        minor_font: Option<String>,
        font: Option<String>,
        sizes: Option<HashMap<String, u32>>,
        inverted: bool,
        background: Option<Background>,
    ) -> PyResult<Self> {
        let mut inner = CoreTheme::new(name);
        if let Some(sizes) = sizes {
            inner = inner.scale(scale_from(sizes)?);
        }
        let mut colors: Vec<(String, String)> = colors.unwrap_or_default().into_iter().collect();
        colors.sort();
        for (slot, value) in colors {
            let slot = SchemeColor::from_name(&slot).ok_or_else(|| {
                PyValueError::new_err(format!(
                    "unknown theme slot {slot:?}; use one of {}",
                    slot_names()
                ))
            })?;
            inner = inner.color(slot, rgb_from(&value)?);
        }
        if let Some(font) = font {
            inner = inner.font(font);
        }
        if let Some(major) = major_font {
            inner.major_font = major;
        }
        if let Some(minor) = minor_font {
            inner.minor_font = minor;
        }
        if inverted {
            inner = inner.inverted();
        }
        if let Some(background) = background {
            inner = inner.background(background.inner);
        }
        Ok(Self { inner })
    }

    /// A preset by name; see `presets()`.
    #[staticmethod]
    fn preset(name: &str) -> PyResult<Self> {
        CoreTheme::preset(name)
            .map(|inner| Self { inner })
            .ok_or_else(|| {
                PyValueError::new_err(format!(
                    "unknown theme {name:?}; use one of {}",
                    CoreTheme::PRESETS.join(", ")
                ))
            })
    }

    #[staticmethod]
    fn presets() -> Vec<&'static str> {
        CoreTheme::PRESETS.to_vec()
    }

    /// A background for one layout; `inverted` swaps light and dark text on it.
    #[pyo3(signature = (layout, background, *, inverted=false))]
    fn layout_background<'py>(
        mut slf: PyRefMut<'py, Self>,
        layout: &str,
        background: Background,
        inverted: bool,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let layout = layout_from(Some(layout))?
            .ok_or_else(|| PyValueError::new_err("layout is required"))?;
        slf.inner =
            std::mem::take(&mut slf.inner).layout_background(layout, background.inner, inverted);
        Ok(slf)
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn major_font(&self) -> String {
        self.inner.major_font.clone()
    }

    #[getter]
    fn minor_font(&self) -> String {
        self.inner.minor_font.clone()
    }

    #[getter]
    fn inverted(&self) -> bool {
        self.inner.inverted
    }

    #[getter]
    fn colors(&self) -> HashMap<String, String> {
        SchemeColor::ALL
            .iter()
            .map(|slot| {
                (
                    slot.name().to_string(),
                    format!("#{}", self.inner.colors.get(*slot).to_hex()),
                )
            })
            .collect()
    }

    /// Font sizes in points by name.
    #[getter]
    fn sizes(&self) -> HashMap<String, u32> {
        let scale = self.inner.scale;
        SIZE_NAMES
            .iter()
            .zip([
                scale.display,
                scale.title,
                scale.subtitle,
                scale.body,
                scale.table,
                scale.minimum,
            ])
            .map(|(name, points)| (name.to_string(), points))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("write.Theme({:?})", self.inner.name)
    }
}

/// A picture (PNG, JPEG, GIF, BMP or TIFF bytes) for the layout engine to
/// place; it keeps its aspect ratio.
#[pyclass(module = "pptxboss.write")]
#[derive(Clone)]
pub struct Picture {
    data: Vec<u8>,
    description: Option<String>,
}

#[pymethods]
impl Picture {
    #[new]
    #[pyo3(signature = (data, *, description=None))]
    fn new(data: Vec<u8>, description: Option<String>) -> Self {
        Self { data, description }
    }

    #[getter]
    fn description(&self) -> Option<String> {
        self.description.clone()
    }

    fn __repr__(&self) -> String {
        format!("write.Picture({} bytes)", self.data.len())
    }
}

/// A table of cell texts for the layout engine to place; rows take the
/// height their cells need, and a long table continues on the next slide
/// with its header.
#[pyclass(module = "pptxboss.write")]
#[derive(Clone)]
pub struct Table {
    rows: Vec<Vec<String>>,
    header: bool,
}

#[pymethods]
impl Table {
    #[new]
    #[pyo3(signature = (rows, *, header=true))]
    fn new(rows: Vec<Vec<String>>, header: bool) -> Self {
        Self { rows, header }
    }

    #[getter]
    fn rows(&self) -> Vec<Vec<String>> {
        self.rows.clone()
    }

    #[getter]
    fn header(&self) -> bool {
        self.header
    }

    fn __repr__(&self) -> String {
        format!("write.Table({} rows)", self.rows.len())
    }
}

/// A block for the layout engine: lines of text (strings become bullets,
/// a `Paragraph` keeps its own formatting), a `Picture`, a `Table`, or a
/// list of blocks set side by side.
#[derive(FromPyObject)]
enum BlockArg {
    Picture(Picture),
    Table(Table),
    Lines(Vec<Line>),
    Columns(Vec<BlockArg>),
}

fn block_from(block: BlockArg) -> Block {
    match block {
        BlockArg::Picture(picture) => Block::Picture {
            data: picture.data,
            description: picture.description,
        },
        BlockArg::Table(table) => Block::table(table.rows, table.header),
        BlockArg::Lines(lines) => Block::text(
            lines
                .into_iter()
                .map(|line| match line {
                    Line::Paragraph(paragraph) => paragraph.inner,
                    Line::Text(text) => CoreParagraph::bullet(text, 0),
                })
                .collect(),
        ),
        BlockArg::Columns(children) => {
            Block::columns(children.into_iter().map(block_from).collect())
        }
    }
}

/// A rectangle from four optional inch coordinates: all given, or none.
fn rect_from(
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    h: Option<f64>,
) -> PyResult<Option<Rect>> {
    match (x, y, w, h) {
        (Some(x), Some(y), Some(w), Some(h)) => Ok(Some(Rect::inches(x, y, w, h))),
        (None, None, None, None) => Ok(None),
        _ => Err(PyValueError::new_err(
            "give x, y, w and h together for a fixed position, or none of them to let the layout engine place it",
        )),
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
    #[pyo3(signature = (title=None, *, subtitle=None, layout=None, notes=None, hidden=false, background=None, inverted=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        title: Option<String>,
        subtitle: Option<String>,
        layout: Option<&str>,
        notes: Option<String>,
        hidden: bool,
        background: Option<Background>,
        inverted: bool,
    ) -> PyResult<Self> {
        let layout = layout_from(layout)?;
        let mut inner = CoreSlide::new();
        inner.title = title;
        inner.subtitle = subtitle;
        inner.layout = layout;
        inner.notes = notes;
        inner.hidden = hidden;
        inner.background = background.map(|background| background.inner);
        inner.inverted = inverted;
        Ok(Self { inner })
    }

    /// Adds a bullet to the body placeholder; `text` is a string or a list
    /// of strings and runs, `size` is in points.
    #[pyo3(signature = (text, level=0, *, bold=false, italic=false, size=None, color=None))]
    #[allow(clippy::too_many_arguments)]
    fn bullet<'py>(
        mut slf: PyRefMut<'py, Self>,
        text: Content,
        level: u8,
        bold: bool,
        italic: bool,
        size: Option<u32>,
        color: Option<&str>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let runs = runs_from(
            text,
            Style {
                bold,
                italic,
                size,
                color,
            },
        )?;
        slf.inner.body.push(CoreParagraph::bullet_runs(runs, level));
        Ok(slf)
    }

    /// Adds a plain paragraph to the body placeholder; `text` is a string
    /// or a list of strings and runs, `size` is in points.
    #[pyo3(signature = (text, *, bold=false, italic=false, size=None, color=None, align="left"))]
    #[allow(clippy::too_many_arguments)]
    fn paragraph<'py>(
        mut slf: PyRefMut<'py, Self>,
        text: Content,
        bold: bool,
        italic: bool,
        size: Option<u32>,
        color: Option<&str>,
        align: &str,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let runs = runs_from(
            text,
            Style {
                bold,
                italic,
                size,
                color,
            },
        )?;
        let paragraph = CoreParagraph::runs(runs).align(align_from(align)?);
        slf.inner.body.push(paragraph);
        Ok(slf)
    }

    /// Adds a text box at `(x, y)` inches, `w` by `h` inches; one paragraph
    /// per line. A plain string takes the keyword formatting; a `Paragraph`
    /// keeps its own.
    #[pyo3(signature = (x, y, w, h, lines, *, bullets=false, bold=false, italic=false, size=None, color=None))]
    #[allow(clippy::too_many_arguments)]
    fn text_box<'py>(
        mut slf: PyRefMut<'py, Self>,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        lines: Vec<Line>,
        bullets: bool,
        bold: bool,
        italic: bool,
        size: Option<u32>,
        color: Option<&str>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let style = Style {
            bold,
            italic,
            size,
            color,
        };
        let paragraphs = lines
            .into_iter()
            .map(|line| match line {
                Line::Paragraph(paragraph) => Ok(paragraph.inner),
                Line::Text(text) => {
                    let run = style.apply(CoreRun::text(text))?;
                    Ok(match bullets {
                        true => CoreParagraph::bullet_runs(vec![run], 0),
                        false => CoreParagraph::runs(vec![run]),
                    })
                }
            })
            .collect::<PyResult<Vec<_>>>()?;
        slf.inner.shapes.push(pptxboss_write::Shape::Text {
            paragraphs,
            rect: Rect::inches(x, y, w, h),
        });
        Ok(slf)
    }

    /// Adds a table of cell texts; every row must have the same length.
    /// With `x`, `y`, `w` and `h` in inches it sits at that position;
    /// without them the layout engine places it below the body.
    #[pyo3(signature = (rows, *, header=true, x=None, y=None, w=None, h=None))]
    #[allow(clippy::too_many_arguments)]
    fn table<'py>(
        mut slf: PyRefMut<'py, Self>,
        rows: Vec<Vec<String>>,
        header: bool,
        x: Option<f64>,
        y: Option<f64>,
        w: Option<f64>,
        h: Option<f64>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        slf.inner = match rect_from(x, y, w, h)? {
            Some(rect) => std::mem::take(&mut slf.inner).table(rect, rows, header),
            None => std::mem::take(&mut slf.inner).block(Block::table(rows, header)),
        };
        Ok(slf)
    }

    /// Adds a picture (PNG, JPEG, GIF, BMP or TIFF bytes). With `x`, `y`,
    /// `w` and `h` in inches it fills that box; without them the layout
    /// engine places it below the body at its own aspect ratio.
    #[pyo3(signature = (data, *, x=None, y=None, w=None, h=None, description=None))]
    #[allow(clippy::too_many_arguments)]
    fn picture<'py>(
        mut slf: PyRefMut<'py, Self>,
        data: Vec<u8>,
        x: Option<f64>,
        y: Option<f64>,
        w: Option<f64>,
        h: Option<f64>,
        description: Option<String>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let inner = std::mem::take(&mut slf.inner);
        slf.inner = match (rect_from(x, y, w, h)?, description) {
            (Some(rect), Some(description)) => inner.picture_described(data, rect, description),
            (Some(rect), None) => inner.picture(data, rect),
            (None, description) => inner.block(Block::Picture { data, description }),
        };
        Ok(slf)
    }

    /// Adds a block below the body for the layout engine to place: a list
    /// of lines (strings become bullets, a `Paragraph` keeps its
    /// formatting), a `Picture`, a `Table`, or a list of those set side by
    /// side.
    fn block<'py>(mut slf: PyRefMut<'py, Self>, block: BlockArg) -> PyRefMut<'py, Self> {
        slf.inner.blocks.push(block_from(block));
        slf
    }

    /// Adds blocks side by side; two blocks of which one is a picture
    /// split seven to five in the text's favor, otherwise columns are
    /// equal.
    #[pyo3(signature = (*columns))]
    fn columns<'py>(
        mut slf: PyRefMut<'py, Self>,
        columns: Vec<BlockArg>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        if columns.is_empty() {
            return Err(PyValueError::new_err("columns needs at least one block"));
        }
        slf.inner.blocks.push(Block::columns(
            columns.into_iter().map(block_from).collect(),
        ));
        Ok(slf)
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
            "write.Slide(title={:?}, body={}, blocks={}, shapes={})",
            self.inner.title,
            self.inner.body.len(),
            self.inner.blocks.len(),
            self.inner.shapes.len()
        )
    }
}

/// A deck under construction.
#[pyclass(module = "pptxboss.write")]
pub struct Presentation {
    inner: CorePresentation,
}

fn with_theme_and_font(
    mut inner: CorePresentation,
    theme: Option<Theme>,
    font: Option<String>,
) -> CorePresentation {
    if let Some(theme) = theme {
        inner = inner.theme(theme.inner);
    }
    if let Some(font) = font {
        inner = inner.font(font);
    }
    inner
}

#[pymethods]
impl Presentation {
    /// `size` is `"widescreen"`, `"standard"` or `(width, height)` in inches;
    /// `font` applies over the theme; `timestamp` is a W3C-DTF string used
    /// for both created and modified.
    #[new]
    #[pyo3(signature = (*, size=widescreen(), theme=None, font=None, title=None, creator=None, subject=None, keywords=None, timestamp=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        size: SizeArg,
        theme: Option<Theme>,
        font: Option<String>,
        title: Option<String>,
        creator: Option<String>,
        subject: Option<String>,
        keywords: Option<String>,
        timestamp: Option<String>,
    ) -> PyResult<Self> {
        let inner = CorePresentation::new().size(size_from(&size)?);
        let mut inner = with_theme_and_font(inner, theme, font);
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
#[pyo3(signature = (markdown, *, size=widescreen(), theme=None, font=None))]
fn from_markdown(
    markdown: &str,
    size: SizeArg,
    theme: Option<Theme>,
    font: Option<String>,
) -> PyResult<Presentation> {
    let inner = pptxboss_write::from_markdown(markdown).size(size_from(&size)?);
    Ok(Presentation {
        inner: with_theme_and_font(inner, theme, font),
    })
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let module = PyModule::new(py, "write")?;
    module.add_class::<Presentation>()?;
    module.add_class::<Slide>()?;
    module.add_class::<Paragraph>()?;
    module.add_class::<Run>()?;
    module.add_class::<Theme>()?;
    module.add_class::<Background>()?;
    module.add_class::<Picture>()?;
    module.add_class::<Table>()?;
    module.add_function(wrap_pyfunction!(from_markdown, &module)?)?;
    parent.add_submodule(&module)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("pptxboss._pptxboss.write", &module)?;
    Ok(())
}
