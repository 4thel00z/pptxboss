//! The lenient document layer: a presentation located through the package
//! relationships, its slides in `sldIdLst` order, and their content.
//!
//! A `Document` is single-threaded (it caches parsed parts behind
//! `RefCell`). [`Document::seed`] hands out a `Send + Sync` handle from
//! which any thread can build its own `Document` over the same archive,
//! which is how [`Document::map_slides`] spreads work across cores.

use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::model::{Content, PlaceholderKind, SlideContent, TextBody};
use crate::opc::{Relationships, TargetMode};
use crate::package::{Package, PackageSeed};
use crate::pml::{content_type, RelKind};
use crate::presentation::Presentation;
use crate::slide::{parse_slide, SlideReport};
use crate::text::{write_content_text, ExtractReport, TextOptions};

/// How the presentation part was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Located {
    /// Through the `officeDocument` relationship of the package (13.3.6).
    Relationship,
    /// By scanning content types for a presentation main type.
    ContentType,
    /// At `/ppt/presentation.xml` with no declaration pointing there.
    ConventionalPath,
}

/// What the document layer had to work around.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentDefects {
    pub located: Located,
    /// `sldId` entries whose relationship did not resolve to a part.
    pub unresolved_slides: Vec<(usize, String)>,
    /// The slide list was rebuilt from `slide` relationships because `sldIdLst` was empty or absent.
    pub slides_recovered_from_rels: bool,
}

/// A slide as listed by the presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideRef {
    /// The Slide part name.
    pub part: String,
    /// `sldId/@id`.
    pub id: Option<u32>,
    /// The relationship id from the presentation part.
    pub rel_id: String,
}

struct Shared {
    presentation_part: String,
    presentation: Presentation,
    slides: Vec<SlideRef>,
    defects: DocumentDefects,
    package: PackageSeed,
}

/// A thread-safe handle from which a [`Document`] is rebuilt; see [`Document::from_seed`].
#[derive(Clone)]
pub struct DocumentSeed {
    shared: Arc<Shared>,
    threads: usize,
}

impl DocumentSeed {
    /// The worker limit for [`Document::map_slides`]; 0 means every core.
    pub fn threads(&self) -> usize {
        self.threads
    }
}

/// An open presentation.
pub struct Document {
    package: Package,
    shared: Arc<Shared>,
    threads: usize,
}

/// The default worker limit: `PPTXBOSS_THREADS` when set to a number, else 0 (every core).
fn threads_from_env() -> usize {
    std::env::var("PPTXBOSS_THREADS")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

impl Document {
    /// Opens a `.pptx` file with positioned reads.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_package(Package::open(path)?)
    }

    /// Opens a presentation held in memory.
    pub fn load(bytes: Vec<u8>) -> Result<Self> {
        Self::from_package(Package::from_bytes(bytes)?)
    }

    pub fn from_package(package: Package) -> Result<Self> {
        let (presentation_part, located) = locate_presentation(&package)?;
        let xml = package.read_part(&presentation_part)?;
        let presentation = Presentation::parse(&xml).map_err(|err| Error::Xml {
            part: presentation_part.clone(),
            offset: err.offset,
            msg: err.msg.to_string(),
        })?;
        let rels = package.rels(&presentation_part)?;
        let mut defects = DocumentDefects {
            located,
            unresolved_slides: Vec::new(),
            slides_recovered_from_rels: false,
        };
        let mut slides = Vec::with_capacity(presentation.slides.len());
        for (index, slide_id) in presentation.slides.iter().enumerate() {
            match rels
                .target_of(&slide_id.rel_id)
                .filter(|part| package.has_part(part))
            {
                Some(part) => slides.push(SlideRef {
                    part,
                    id: slide_id.id,
                    rel_id: slide_id.rel_id.clone(),
                }),
                None => defects
                    .unresolved_slides
                    .push((index, slide_id.rel_id.clone())),
            }
        }
        if slides.is_empty() && presentation.slides.is_empty() {
            for rel in rels.iter().filter(|rel| {
                rel.mode == TargetMode::Internal && RelKind::of(&rel.rel_type) == RelKind::Slide
            }) {
                if let Some(part) = rels.resolve(rel).filter(|part| package.has_part(part)) {
                    slides.push(SlideRef {
                        part,
                        id: None,
                        rel_id: rel.id.clone(),
                    });
                }
            }
            defects.slides_recovered_from_rels = !slides.is_empty();
        }
        let shared = Arc::new(Shared {
            presentation_part,
            presentation,
            slides,
            defects,
            package: package.seed(),
        });
        Ok(Self {
            package,
            shared,
            threads: threads_from_env(),
        })
    }

    /// The worker limit for [`Document::map_slides`]; 0 means every core.
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Limits [`Document::map_slides`] to `threads` workers; 0 restores every core.
    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads;
    }

    /// Builder form of [`Document::set_threads`].
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    /// A handle that can cross threads.
    pub fn seed(&self) -> DocumentSeed {
        DocumentSeed {
            shared: Arc::clone(&self.shared),
            threads: self.threads,
        }
    }

    /// A document over the same archive and presentation, with its own caches.
    pub fn from_seed(seed: DocumentSeed) -> Self {
        Self {
            package: Package::from_seed(seed.shared.package.clone()),
            shared: seed.shared,
            threads: seed.threads,
        }
    }

    pub fn package(&self) -> &Package {
        &self.package
    }

    pub fn presentation(&self) -> &Presentation {
        &self.shared.presentation
    }

    /// The Presentation part name, usually `/ppt/presentation.xml`.
    pub fn presentation_part(&self) -> &str {
        &self.shared.presentation_part
    }

    pub fn defects(&self) -> &DocumentDefects {
        &self.shared.defects
    }

    /// Slides in presentation order.
    pub fn slide_refs(&self) -> &[SlideRef] {
        &self.shared.slides
    }

    pub fn slide_count(&self) -> usize {
        self.shared.slides.len()
    }

    /// Parses slide `index` (zero-based, presentation order).
    pub fn slide(&self, index: usize) -> Result<Slide<'_>> {
        let slide_ref = self
            .shared
            .slides
            .get(index)
            .ok_or(Error::SlideNotFound(index))?;
        let xml = self.package.read_part(&slide_ref.part)?;
        let (content, report) = parse_slide(&xml).map_err(|err| Error::Xml {
            part: slide_ref.part.clone(),
            offset: err.offset,
            msg: err.msg.to_string(),
        })?;
        Ok(Slide {
            doc: self,
            index,
            part: slide_ref.part.clone(),
            content,
            report,
        })
    }

    /// Every slide in order; a slide that fails to parse yields its error and iteration continues.
    pub fn slides(&self) -> impl Iterator<Item = Result<Slide<'_>>> + '_ {
        (0..self.slide_count()).map(move |index| self.slide(index))
    }

    /// Reads and parses any slide-family part by name (layouts, masters, notes).
    pub fn slide_part(&self, part: &str) -> Result<(SlideContent, SlideReport)> {
        let xml = self.package.read_part(part)?;
        parse_slide(&xml).map_err(|err| Error::Xml {
            part: part.to_string(),
            offset: err.offset,
            msg: err.msg.to_string(),
        })
    }

    /// Workers for `count` slides under the configured limit.
    fn worker_count(&self, count: usize) -> usize {
        let limit = match self.threads {
            0 => std::thread::available_parallelism().map_or(1, |n| n.get()),
            limit => limit,
        };
        limit.min(count)
    }

    /// Applies `f` to every slide, spreading slides across the available
    /// cores (see [`Document::set_threads`]). Results come back in slide
    /// order. Each worker thread builds its own `Document` from a seed, so
    /// nothing is shared but the archive.
    pub fn map_slides<T, F>(&self, f: F) -> Vec<T>
    where
        T: Send,
        F: Fn(Result<Slide<'_>>) -> T + Sync,
    {
        let count = self.slide_count();
        let workers = self.worker_count(count);
        if workers <= 1 {
            return (0..count).map(|index| f(self.slide(index))).collect();
        }
        let seed = self.seed();
        let next = AtomicUsize::new(0);
        let mut results: Vec<(usize, T)> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..workers)
                .map(|_| {
                    scope.spawn(|| {
                        let doc = Document::from_seed(seed.clone());
                        let mut mine = Vec::new();
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            if index >= count {
                                return mine;
                            }
                            mine.push((index, f(doc.slide(index))));
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| match handle.join() {
                    Ok(mine) => mine,
                    Err(payload) => std::panic::resume_unwind(payload),
                })
                .collect()
        });
        results.sort_by_key(|(index, _)| *index);
        results.into_iter().map(|(_, value)| value).collect()
    }

    /// The text of every slide, in order, plus what was left out.
    pub fn slide_texts(&self, options: &TextOptions) -> (Vec<String>, ExtractReport) {
        let results = self.map_slides(|slide| match slide {
            Ok(slide) => {
                let mut report = ExtractReport::default();
                let text = slide.text_reporting(options, &mut report);
                (text, report)
            }
            Err(err) => {
                let mut report = ExtractReport::default();
                report.failed_slides.push((0, err.to_string()));
                (String::new(), report)
            }
        });
        let mut report = ExtractReport::default();
        let mut texts = Vec::with_capacity(results.len());
        for (index, (text, slide_report)) in results.into_iter().enumerate() {
            report.failed_slides.extend(
                slide_report
                    .failed_slides
                    .into_iter()
                    .map(|(_, err)| (index, err)),
            );
            report.failed_notes.extend(
                slide_report
                    .failed_notes
                    .into_iter()
                    .map(|(_, err)| (index, err)),
            );
            report.hidden_slides_skipped += slide_report.hidden_slides_skipped;
            report.unknown_graphics += slide_report.unknown_graphics;
            for uri in slide_report.unknown_graphic_uris {
                if !report.unknown_graphic_uris.contains(&uri) {
                    report.unknown_graphic_uris.push(uri);
                }
            }
            report.unknown_elements += slide_report.unknown_elements;
            texts.push(text);
        }
        (texts, report)
    }

    /// The whole deck as text: slides separated by a blank line.
    pub fn text(&self) -> String {
        self.text_reporting(&TextOptions::default()).0
    }

    pub fn text_reporting(&self, options: &TextOptions) -> (String, ExtractReport) {
        let (texts, report) = self.slide_texts(options);
        let mut out = String::with_capacity(texts.iter().map(|text| text.len() + 2).sum());
        let mut first = true;
        for text in texts.iter().filter(|text| !text.is_empty()) {
            if !first {
                out.push_str("\n\n");
            }
            first = false;
            out.push_str(text);
        }
        (out, report)
    }
}

fn locate_presentation(package: &Package) -> Result<(String, Located)> {
    let rels = package.package_rels()?;
    let by_rel = rels
        .iter()
        .filter(|rel| RelKind::of(&rel.rel_type) == RelKind::OfficeDocument)
        .filter_map(|rel| rels.resolve(rel))
        .find(|part| package.has_part(part));
    if let Some(part) = by_rel {
        return Ok((part, Located::Relationship));
    }
    let by_type = package
        .parts()
        .iter()
        .find(|part| {
            package
                .content_type_of(&part.name)
                .is_some_and(content_type::is_presentation_main)
        })
        .map(|part| part.name.clone());
    if let Some(part) = by_type {
        return Ok((part, Located::ContentType));
    }
    if package.has_part("/ppt/presentation.xml") {
        return Ok((
            "/ppt/presentation.xml".to_string(),
            Located::ConventionalPath,
        ));
    }
    Err(Error::NotAPresentation)
}

/// An image referenced from a slide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRef {
    /// `cNvPr/@id` of the picture shape.
    pub shape_id: u32,
    pub rel_id: String,
    /// The image part, when the relationship is internal and resolves.
    pub part: Option<String>,
    /// The image part's declared content type.
    pub content_type: Option<String>,
    /// The external target for linked images.
    pub external: Option<String>,
}

/// One parsed slide, bound to its document.
pub struct Slide<'d> {
    doc: &'d Document,
    pub index: usize,
    /// The Slide part name.
    pub part: String,
    pub content: SlideContent,
    pub report: SlideReport,
}

impl<'d> Slide<'d> {
    /// One-based position in the deck.
    pub fn number(&self) -> usize {
        self.index + 1
    }

    pub fn document(&self) -> &'d Document {
        self.doc
    }

    /// True for slides marked hidden.
    pub fn is_hidden(&self) -> bool {
        !self.content.show
    }

    /// The slide's relationships.
    pub fn rels(&self) -> Result<Rc<Relationships>> {
        self.doc.package.rels(&self.part)
    }

    /// The first title placeholder's text.
    pub fn title(&self) -> Option<String> {
        self.content.title()
    }

    /// The slide text with default options and no notes.
    pub fn text(&self) -> String {
        let mut out = String::new();
        write_content_text(&self.content, &TextOptions::default(), &mut out);
        out
    }

    /// The slide text with `options`, recording problems in `report`.
    pub fn text_reporting(&self, options: &TextOptions, report: &mut ExtractReport) -> String {
        report.unknown_graphics += self.report.unknown_graphics.len() as u32;
        for uri in &self.report.unknown_graphics {
            if !report.unknown_graphic_uris.contains(uri) {
                report.unknown_graphic_uris.push(uri.clone());
            }
        }
        report.unknown_elements += self.report.unknown_elements;
        if self.is_hidden() && !options.hidden_slides {
            report.hidden_slides_skipped += 1;
            return String::new();
        }
        let mut out = String::new();
        write_content_text(&self.content, options, &mut out);
        if !options.notes {
            return out;
        }
        match self.notes() {
            Ok(Some(notes)) if !notes.is_empty() => {
                if !out.is_empty() {
                    out.push('\n');
                }
                notes.write_text(&mut out);
            }
            Ok(_) => {}
            Err(err) => report.failed_notes.push((self.index, err.to_string())),
        }
        out
    }

    /// The part name of the first relationship of `kind`, when internal.
    fn related_part(&self, kind: RelKind) -> Result<Option<String>> {
        let rels = self.rels()?;
        let found = rels
            .iter()
            .filter(|rel| RelKind::of(&rel.rel_type) == kind)
            .find_map(|rel| rels.resolve(rel));
        Ok(found)
    }

    /// The Notes Slide part, if the slide has one.
    pub fn notes_part(&self) -> Result<Option<String>> {
        self.related_part(RelKind::NotesSlide)
    }

    /// The Slide Layout part.
    pub fn layout_part(&self) -> Result<Option<String>> {
        self.related_part(RelKind::SlideLayout)
    }

    /// The speaker notes: the body placeholder of the notes slide, or,
    /// when there is none, every text shape on it other than the slide
    /// image and furniture.
    pub fn notes(&self) -> Result<Option<TextBody>> {
        let Some(part) = self.notes_part()? else {
            return Ok(None);
        };
        let (content, _) = self.doc.slide_part(&part)?;
        let body_placeholder = content
            .walk()
            .find(|shape| {
                shape
                    .placeholder
                    .as_ref()
                    .is_some_and(|ph| ph.kind == PlaceholderKind::Body)
            })
            .and_then(|shape| shape.text_body().cloned());
        if let Some(body) = body_placeholder {
            return Ok(Some(body));
        }
        let mut merged = TextBody::default();
        for shape in content.walk() {
            let skip = shape
                .placeholder
                .as_ref()
                .is_some_and(|ph| ph.kind == PlaceholderKind::SlideImage || ph.kind.is_furniture());
            if skip {
                continue;
            }
            if let Some(body) = shape.text_body() {
                merged.paragraphs.extend(body.paragraphs.iter().cloned());
            }
        }
        Ok(Some(merged))
    }

    /// The speaker notes as plain text, if any.
    pub fn notes_text(&self) -> Result<Option<String>> {
        Ok(self
            .notes()?
            .filter(|body| !body.is_empty())
            .map(|body| body.text()))
    }

    /// Every picture on the slide with its image part resolved.
    pub fn images(&self) -> Result<Vec<ImageRef>> {
        let rels = self.rels()?;
        let mut images = Vec::new();
        for shape in self.content.walk() {
            let picture = match &shape.content {
                Content::Picture(picture) => picture,
                Content::Ole(ole) => match &ole.preview {
                    Some(picture) => picture,
                    None => continue,
                },
                _ => continue,
            };
            let Some(rel_id) = picture.embed.as_ref().or(picture.link.as_ref()) else {
                continue;
            };
            let rel = rels.get(rel_id);
            let part = rel
                .and_then(|rel| rels.resolve(rel))
                .filter(|part| self.doc.package.has_part(part));
            let content_type = part
                .as_deref()
                .and_then(|part| self.doc.package.content_type_of(part))
                .map(str::to_string);
            let external = rel
                .filter(|rel| rel.mode == TargetMode::External)
                .map(|rel| rel.target.clone());
            images.push(ImageRef {
                shape_id: shape.id,
                rel_id: rel_id.clone(),
                part,
                content_type,
                external,
            });
        }
        Ok(images)
    }

    /// The bytes of an image part referenced from this slide.
    pub fn image_bytes(&self, image: &ImageRef) -> Result<Vec<u8>> {
        let part = image
            .part
            .as_deref()
            .ok_or_else(|| Error::MissingRelationship {
                part: self.part.clone(),
                id: image.rel_id.clone(),
            })?;
        let mut out = Vec::new();
        self.doc.package.read_part_into(part, &mut out)?;
        Ok(out)
    }

    /// The target of a hyperlink relationship: an external URL, or an internal part name.
    pub fn hyperlink_target(&self, rel_id: &str) -> Result<Option<String>> {
        let rels = self.rels()?;
        Ok(rels.get(rel_id).map(|rel| match rel.mode {
            TargetMode::External => rel.target.clone(),
            TargetMode::Internal => rels.resolve(rel).unwrap_or_default(),
        }))
    }
}
