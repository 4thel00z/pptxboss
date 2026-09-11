//! Legacy PowerPoint 97-2003 binary presentations (MS-PPT, MS-ODRAW) held
//! in a compound file. Everything is reached through the persist directory
//! built from the user-edit chain, never by scanning the stream, so stale
//! records from earlier edits are ignored as the specification requires.
//! Slides are mapped onto the same shape model as PresentationML slides.

use std::sync::Arc;

use crate::cfb::Compound;
use crate::error::{Error, Result};
use crate::hash::FastMap;
use crate::model::{
    Bullet, Content, Emu, OleObject, Paragraph, Picture, Placeholder, PlaceholderKind, Run,
    RunKind, RunProps, Shape, SlideContent, SlideKind, TextBody, Transform,
};

const RT_DOCUMENT: u16 = 0x03e8;
const RT_DOCUMENT_ATOM: u16 = 0x03e9;
const RT_SLIDE: u16 = 0x03ee;
const RT_NOTES: u16 = 0x03f0;
const RT_NOTES_ATOM: u16 = 0x03f1;
const RT_SLIDE_PERSIST_ATOM: u16 = 0x03f3;
const RT_SLIDE_SHOW_SLIDE_INFO_ATOM: u16 = 0x03f9;
const RT_DRAWING_GROUP: u16 = 0x040b;
const RT_DRAWING: u16 = 0x040c;
const RT_PLACEHOLDER_ATOM: u16 = 0x0bc3;
const RT_OUTLINE_TEXT_REF_ATOM: u16 = 0x0f9e;
const RT_TEXT_HEADER_ATOM: u16 = 0x0f9f;
const RT_TEXT_CHARS_ATOM: u16 = 0x0fa0;
const RT_STYLE_TEXT_PROP_ATOM: u16 = 0x0fa1;
const RT_MASTER_TEXT_PROP_ATOM: u16 = 0x0fa2;
const RT_TEXT_BYTES_ATOM: u16 = 0x0fa8;
const RT_CSTRING: u16 = 0x0fba;
const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0ff0;
const RT_USER_EDIT_ATOM: u16 = 0x0ff5;
const RT_CURRENT_USER_ATOM: u16 = 0x0ff6;
const RT_PERSIST_DIRECTORY_ATOM: u16 = 0x1772;
const RT_CRYPT_SESSION10_CONTAINER: u16 = 0x2f14;

const OA_DGG_CONTAINER: u16 = 0xf000;
const OA_BSTORE_CONTAINER: u16 = 0xf001;
const OA_DG_CONTAINER: u16 = 0xf002;
const OA_SPGR_CONTAINER: u16 = 0xf003;
const OA_SP_CONTAINER: u16 = 0xf004;
const OA_FBSE: u16 = 0xf007;
const OA_FSP: u16 = 0xf00a;
const OA_FOPT: u16 = 0xf00b;
const OA_CLIENT_TEXTBOX: u16 = 0xf00d;
const OA_CHILD_ANCHOR: u16 = 0xf00f;
const OA_CLIENT_ANCHOR: u16 = 0xf010;
const OA_CLIENT_DATA: u16 = 0xf011;
const OA_SECONDARY_FOPT: u16 = 0xf121;
const OA_TERTIARY_FOPT: u16 = 0xf122;

const HEADER_TOKEN_ENCRYPTED: u32 = 0xf3d1_c4df;
const NO_PLACEHOLDER: u32 = 0xffff_ffff;
const MSOSPT_TEXT_BOX: u16 = 202;

/// One English Metric Unit per master unit (1/576 inch), as a rational: 914400 / 576.
const EMU_PER_MASTER_UNIT_NUM: i64 = 3175;
const EMU_PER_MASTER_UNIT_DEN: i64 = 2;

/// A record header plus where its data lies in the stream.
#[derive(Clone, Copy, Debug)]
struct Rec {
    ver: u8,
    instance: u16,
    kind: u16,
    /// Offset of the first data byte.
    start: usize,
    len: usize,
}

impl Rec {
    fn end(&self) -> usize {
        self.start + self.len
    }

    fn is_container(&self) -> bool {
        self.ver == 0xf
    }
}

fn header_at(data: &[u8], at: usize) -> Option<Rec> {
    let raw = data.get(at..at + 8)?;
    let word = u16::from_le_bytes([raw[0], raw[1]]);
    let len = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
    Some(Rec {
        ver: (word & 0xf) as u8,
        instance: word >> 4,
        kind: u16::from_le_bytes([raw[2], raw[3]]),
        start: at + 8,
        len,
    })
}

/// The records tiling `start..end`, stopping at the first that overruns.
fn records(data: &[u8], start: usize, end: usize) -> Vec<Rec> {
    let end = end.min(data.len());
    let mut out = Vec::new();
    let mut cursor = start;
    while cursor + 8 <= end {
        let Some(rec) = header_at(data, cursor) else {
            break;
        };
        if rec.end() > end {
            break;
        }
        out.push(rec);
        cursor = rec.end();
    }
    out
}

fn children(data: &[u8], rec: &Rec) -> Vec<Rec> {
    records(data, rec.start, rec.end())
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let raw = data.get(at..at + 2)?;
    Some(u16::from_le_bytes([raw[0], raw[1]]))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let raw = data.get(at..at + 4)?;
    Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn i32_at(data: &[u8], at: usize) -> Option<i32> {
    u32_at(data, at).map(|v| v as i32)
}

fn master_units_to_emu(value: i64) -> Emu {
    value * EMU_PER_MASTER_UNIT_NUM / EMU_PER_MASTER_UNIT_DEN
}

fn ppt_error(msg: &str) -> Error {
    Error::Other(format!("ppt: {msg}"))
}

/// An outline text body from the slide list, with its text type.
#[derive(Clone, Debug)]
struct OutlineText {
    text_type: u32,
    body: TextBody,
}

#[derive(Clone, Debug)]
struct SlideEntry {
    persist: u32,
    slide_id: u32,
    outline: Vec<OutlineText>,
}

/// A picture in the blip store: where its bytes are.
#[derive(Clone, Debug)]
enum BlipSource {
    /// Offset of the BLIP record header in the Pictures stream.
    Delay(usize),
    /// Offset of a BLIP record header embedded in the document stream.
    Embedded(usize),
    Missing,
}

/// A parsed legacy presentation: the streams and the directory needed to
/// reach every live object, with slides parsed on demand.
pub struct LegacyDeck {
    document: Vec<u8>,
    pictures: Vec<u8>,
    directory: FastMap<u32, usize>,
    slides: Vec<SlideEntry>,
    /// Notes persist ids by the slide id they belong to.
    notes: FastMap<u32, u32>,
    blips: Vec<BlipSource>,
    /// Offsets of every top-level `SlideContainer` in stream order, the
    /// fallback for slides whose persist id the directory cannot resolve.
    stream_slides: Vec<usize>,
    /// True when at least one slide came from the fallback scan.
    pub slides_recovered_by_scan: bool,
    pub slide_size: Option<(Emu, Emu)>,
    pub notes_size: Option<(Emu, Emu)>,
    pub slide_size_kind: Option<&'static str>,
    pub first_slide_number: i32,
}

/// One slide as parsed from its container.
pub struct LegacySlide {
    pub id: u32,
    pub hidden: bool,
    pub name: Option<String>,
    pub content: SlideContent,
}

/// A picture's bytes with their media type.
pub struct LegacyPicture {
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

/// Whether the compound file carries a PowerPoint 97-2003 presentation.
pub fn is_presentation(compound: &Compound) -> bool {
    compound.has_stream("PowerPoint Document")
        || compound.has_stream("PP97_DUALSTORAGE/PowerPoint Document")
}

impl LegacyDeck {
    pub fn open(compound: &Compound) -> Result<Self> {
        let prefix = match compound.has_stream("PP97_DUALSTORAGE/PowerPoint Document") {
            true => "PP97_DUALSTORAGE/",
            false => "",
        };
        let document = compound
            .stream(&format!("{prefix}PowerPoint Document"))
            .ok_or_else(|| ppt_error("no PowerPoint Document stream"))?;
        let current_user = compound
            .stream(&format!("{prefix}Current User"))
            .ok_or_else(|| ppt_error("no Current User stream"))?;
        let pictures = compound
            .stream(&format!("{prefix}Pictures"))
            .unwrap_or_default();
        let cu = header_at(&current_user, 0)
            .filter(|rec| rec.kind == RT_CURRENT_USER_ATOM)
            .ok_or_else(|| {
                Error::Unsupported("PowerPoint 95 or unknown presentation format".into())
            })?;
        let header_token = u32_at(&current_user, cu.start + 4).unwrap_or(0);
        if header_token == HEADER_TOKEN_ENCRYPTED {
            return Err(Error::Encrypted(
                "legacy .ppt encrypted with a password".into(),
            ));
        }
        let mut offset = u32_at(&current_user, cu.start + 8).unwrap_or(0) as usize;
        let mut directories: Vec<Vec<(u32, usize)>> = Vec::new();
        let mut doc_persist = 0u32;
        let mut encrypt_ref = None;
        let mut edits = 0usize;
        while offset != 0 {
            let edit = header_at(&document, offset)
                .filter(|rec| rec.kind == RT_USER_EDIT_ATOM)
                .ok_or_else(|| ppt_error("user edit atom missing"))?;
            let last_edit = u32_at(&document, edit.start + 8).unwrap_or(0) as usize;
            let dir_offset = u32_at(&document, edit.start + 12).unwrap_or(0) as usize;
            if edits == 0 {
                doc_persist = u32_at(&document, edit.start + 16).unwrap_or(0);
                if edit.len >= 0x20 {
                    encrypt_ref = u32_at(&document, edit.start + 28).filter(|r| *r != 0);
                }
            }
            let dir = header_at(&document, dir_offset)
                .filter(|rec| rec.kind == RT_PERSIST_DIRECTORY_ATOM)
                .ok_or_else(|| ppt_error("persist directory missing"))?;
            let mut entries = Vec::new();
            let mut cursor = dir.start;
            while cursor + 4 <= dir.end().min(document.len()) {
                let word = u32_at(&document, cursor).unwrap_or(0);
                let persist_id = word & 0x000f_ffff;
                let count = (word >> 20) & 0xfff;
                cursor += 4;
                if count == 0 {
                    break;
                }
                for i in 0..count {
                    let Some(value) = u32_at(&document, cursor) else {
                        break;
                    };
                    entries.push((persist_id + i, value as usize));
                    cursor += 4;
                }
            }
            directories.push(entries);
            edits += 1;
            if last_edit >= offset || edits > 4096 {
                break;
            }
            offset = last_edit;
        }
        let mut directory = FastMap::default();
        for entries in directories.iter().rev() {
            for (id, value) in entries {
                directory.insert(*id, *value);
            }
        }
        if let Some(reference) = encrypt_ref {
            let encrypted = directory
                .get(&reference)
                .and_then(|&at| header_at(&document, at))
                .is_some_and(|rec| rec.kind == RT_CRYPT_SESSION10_CONTAINER);
            if encrypted {
                return Err(Error::Encrypted(
                    "legacy .ppt encrypted with a password".into(),
                ));
            }
        }
        let doc_offset = *directory
            .get(&doc_persist)
            .ok_or_else(|| ppt_error("document container not in the persist directory"))?;
        let doc_rec = header_at(&document, doc_offset)
            .filter(|rec| rec.kind == RT_DOCUMENT && rec.is_container())
            .ok_or_else(|| ppt_error("document container missing"))?;

        let mut deck = LegacyDeck {
            document,
            pictures,
            directory,
            slides: Vec::new(),
            notes: FastMap::default(),
            blips: Vec::new(),
            stream_slides: Vec::new(),
            slides_recovered_by_scan: false,
            slide_size: None,
            notes_size: None,
            slide_size_kind: None,
            first_slide_number: 1,
        };
        let doc_children = children(&deck.document, &doc_rec);
        let mut notes_persists: Vec<u32> = Vec::new();
        for child in &doc_children {
            match child.kind {
                RT_DOCUMENT_ATOM => deck.read_document_atom(child),
                RT_DRAWING_GROUP => deck.read_drawing_group(child),
                RT_SLIDE_LIST_WITH_TEXT => match child.instance {
                    0 => deck.read_slide_list(child),
                    2 => {
                        for atom in children(&deck.document, child) {
                            if atom.kind == RT_SLIDE_PERSIST_ATOM {
                                if let Some(persist) = u32_at(&deck.document, atom.start) {
                                    notes_persists.push(persist);
                                }
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        deck.recover_missing_slides();
        for persist in notes_persists {
            let Some(&at) = deck.directory.get(&persist) else {
                continue;
            };
            let Some(rec) = header_at(&deck.document, at).filter(|r| r.kind == RT_NOTES) else {
                continue;
            };
            let slide_id = children(&deck.document, &rec)
                .iter()
                .find(|c| c.kind == RT_NOTES_ATOM)
                .and_then(|atom| u32_at(&deck.document, atom.start))
                .unwrap_or(0);
            if slide_id != 0 {
                deck.notes.insert(slide_id, persist);
            }
        }
        Ok(deck)
    }

    fn read_document_atom(&mut self, rec: &Rec) {
        let data = &self.document;
        let width = i32_at(data, rec.start).unwrap_or(0);
        let height = i32_at(data, rec.start + 4).unwrap_or(0);
        if width > 0 && height > 0 {
            self.slide_size = Some((
                master_units_to_emu(i64::from(width)),
                master_units_to_emu(i64::from(height)),
            ));
        }
        let notes_width = i32_at(data, rec.start + 8).unwrap_or(0);
        let notes_height = i32_at(data, rec.start + 12).unwrap_or(0);
        if notes_width > 0 && notes_height > 0 {
            self.notes_size = Some((
                master_units_to_emu(i64::from(notes_width)),
                master_units_to_emu(i64::from(notes_height)),
            ));
        }
        self.first_slide_number = i32::from(u16_at(data, rec.start + 32).unwrap_or(1));
        self.slide_size_kind = match u16_at(data, rec.start + 34).unwrap_or(6) {
            0 => Some("screen4x3"),
            1 => Some("letter"),
            2 => Some("A4"),
            3 => Some("35mm"),
            4 => Some("overhead"),
            5 => Some("banner"),
            _ => None,
        };
    }

    /// Reads the document's blip store: one entry per picture, in order.
    fn read_drawing_group(&mut self, rec: &Rec) {
        let data = &self.document;
        for dgg in children(data, rec)
            .iter()
            .filter(|c| c.kind == OA_DGG_CONTAINER)
        {
            for store in children(data, dgg)
                .iter()
                .filter(|c| c.kind == OA_BSTORE_CONTAINER)
            {
                for block in children(data, store) {
                    let source = match block.kind {
                        OA_FBSE => {
                            let name_len =
                                data.get(block.start + 33).copied().unwrap_or(0) as usize;
                            let delay = u32_at(data, block.start + 28).unwrap_or(0xffff_ffff);
                            let embedded_at = block.start + 36 + name_len;
                            match embedded_at + 8 <= block.end() {
                                true => BlipSource::Embedded(embedded_at),
                                false if delay != 0xffff_ffff => BlipSource::Delay(delay as usize),
                                false => BlipSource::Missing,
                            }
                        }
                        0xf018..=0xf117 => BlipSource::Embedded(block.start - 8),
                        _ => BlipSource::Missing,
                    };
                    self.blips.push(source);
                }
            }
        }
    }

    /// Reads the slide list: slide order, ids and the outline text of each slide.
    fn read_slide_list(&mut self, rec: &Rec) {
        let data = &self.document;
        let items = children(data, rec);
        let mut current: Option<SlideEntry> = None;
        let mut index = 0;
        while index < items.len() {
            let item = items[index];
            if item.kind == RT_SLIDE_PERSIST_ATOM {
                if let Some(entry) = current.take() {
                    self.slides.push(entry);
                }
                current = Some(SlideEntry {
                    persist: u32_at(data, item.start).unwrap_or(0),
                    slide_id: u32_at(data, item.start + 12).unwrap_or(0),
                    outline: Vec::new(),
                });
                index += 1;
                continue;
            }
            if item.kind == RT_TEXT_HEADER_ATOM {
                let text_type = u32_at(data, item.start).unwrap_or(4);
                let mut end = index + 1;
                while end < items.len()
                    && !matches!(items[end].kind, RT_TEXT_HEADER_ATOM | RT_SLIDE_PERSIST_ATOM)
                {
                    end += 1;
                }
                let body = parse_text_body(data, &items[index + 1..end]);
                if let Some(entry) = current.as_mut() {
                    entry.outline.push(OutlineText { text_type, body });
                }
                index = end;
                continue;
            }
            index += 1;
        }
        if let Some(entry) = current {
            self.slides.push(entry);
        }
    }

    /// Points unresolved slide persist ids at unreferenced top-level slide
    /// containers, in stream order. The persist directory is authoritative
    /// (2.1.2); this only runs when it is broken, and says so.
    fn recover_missing_slides(&mut self) {
        let missing: Vec<usize> = self
            .slides
            .iter()
            .enumerate()
            .filter(|(_, slide)| {
                !self
                    .directory
                    .get(&slide.persist)
                    .and_then(|&at| header_at(&self.document, at))
                    .is_some_and(|rec| rec.kind == RT_SLIDE && rec.is_container())
            })
            .map(|(index, _)| index)
            .collect();
        if missing.is_empty() {
            return;
        }
        let referenced: Vec<usize> = self.directory.values().copied().collect();
        self.stream_slides = records(&self.document, 0, self.document.len())
            .iter()
            .filter(|rec| rec.kind == RT_SLIDE && rec.is_container())
            .map(|rec| rec.start - 8)
            .filter(|at| !referenced.contains(at))
            .collect();
        let mut spare = self.stream_slides.iter().copied();
        let mut next_id = 0x00f0_0000u32;
        for index in missing {
            let Some(at) = spare.next() else {
                break;
            };
            while self.directory.contains_key(&next_id) {
                next_id += 1;
            }
            self.directory.insert(next_id, at);
            self.slides[index].persist = next_id;
            self.slides_recovered_by_scan = true;
        }
    }

    pub fn slide_count(&self) -> usize {
        self.slides.len()
    }

    /// The slide ids in presentation order.
    pub fn slide_ids(&self) -> Vec<u32> {
        self.slides.iter().map(|slide| slide.slide_id).collect()
    }

    /// Parses slide `index` (zero-based, presentation order).
    pub fn slide(&self, index: usize) -> Result<LegacySlide> {
        let entry = self.slides.get(index).ok_or(Error::SlideNotFound(index))?;
        let at = *self
            .directory
            .get(&entry.persist)
            .ok_or_else(|| ppt_error("slide persist id not in the directory"))?;
        let rec = header_at(&self.document, at)
            .filter(|rec| rec.kind == RT_SLIDE && rec.is_container())
            .ok_or_else(|| ppt_error("slide container missing"))?;
        let mut hidden = false;
        let mut name = None;
        let mut shapes = Vec::new();
        for child in children(&self.document, &rec) {
            match child.kind {
                RT_SLIDE_SHOW_SLIDE_INFO_ATOM => {
                    hidden = u16_at(&self.document, child.start + 10).unwrap_or(0) & 0x0004 != 0;
                }
                RT_CSTRING if child.instance == 3 => {
                    name = Some(utf16_string(&self.document[child.start..child.end()]));
                }
                RT_DRAWING => {
                    shapes = self.drawing_shapes(&child, &entry.outline);
                }
                _ => {}
            }
        }
        Ok(LegacySlide {
            id: entry.slide_id,
            hidden,
            name: name.clone(),
            content: SlideContent {
                kind: SlideKind::Slide,
                name,
                show: !hidden,
                shapes,
            },
        })
    }

    /// Whether slide `index` has a notes slide.
    pub fn has_notes(&self, index: usize) -> bool {
        self.slides
            .get(index)
            .is_some_and(|slide| self.notes.contains_key(&slide.slide_id))
    }

    /// The speaker notes of slide `index`: the notes body placeholder, or
    /// every text shape on the notes slide other than the slide image.
    pub fn notes(&self, index: usize) -> Result<Option<TextBody>> {
        let entry = self.slides.get(index).ok_or(Error::SlideNotFound(index))?;
        let Some(persist) = self.notes.get(&entry.slide_id) else {
            return Ok(None);
        };
        let Some(&at) = self.directory.get(persist) else {
            return Ok(None);
        };
        let Some(rec) = header_at(&self.document, at).filter(|r| r.kind == RT_NOTES) else {
            return Ok(None);
        };
        let drawing = children(&self.document, &rec)
            .into_iter()
            .find(|c| c.kind == RT_DRAWING);
        let Some(drawing) = drawing else {
            return Ok(None);
        };
        let shapes = self.drawing_shapes(&drawing, &[]);
        let content = SlideContent {
            kind: SlideKind::Notes,
            name: None,
            show: true,
            shapes,
        };
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

    /// The shapes of a `DrawingContainer` in z-order, groups nested.
    fn drawing_shapes(&self, drawing: &Rec, outline: &[OutlineText]) -> Vec<Shape> {
        let data = &self.document;
        let mut shapes = Vec::new();
        for dg in children(data, drawing)
            .iter()
            .filter(|c| c.kind == OA_DG_CONTAINER)
        {
            for child in children(data, dg) {
                match child.kind {
                    OA_SPGR_CONTAINER => {
                        let members = children(data, &child);
                        for (position, member) in members.iter().enumerate() {
                            if position == 0 {
                                continue;
                            }
                            if let Some(shape) = self.shape_of(member, outline) {
                                shapes.push(shape);
                            }
                        }
                    }
                    OA_SP_CONTAINER => {
                        if let Some(shape) = self.shape_of(&child, outline) {
                            shapes.push(shape);
                        }
                    }
                    _ => {}
                }
            }
        }
        shapes
    }

    /// A shape (`OfficeArtSpContainer`) or group (`OfficeArtSpgrContainer`).
    fn shape_of(&self, rec: &Rec, outline: &[OutlineText]) -> Option<Shape> {
        let data = &self.document;
        if rec.kind == OA_SPGR_CONTAINER {
            let members = children(data, rec);
            let group_shape = members
                .first()
                .and_then(|first| self.shape_of(first, outline));
            let children_shapes: Vec<Shape> = members
                .iter()
                .skip(1)
                .filter_map(|member| self.shape_of(member, outline))
                .collect();
            let mut group = group_shape.unwrap_or_else(|| blank_shape(0));
            group.content = Content::Group(children_shapes, None);
            return Some(group);
        }
        if rec.kind != OA_SP_CONTAINER {
            return None;
        }
        let parts = children(data, rec);
        let fsp = parts.iter().find(|p| p.kind == OA_FSP)?;
        let spid = u32_at(data, fsp.start).unwrap_or(0);
        let flags = u32_at(data, fsp.start + 4).unwrap_or(0);
        let deleted = flags & 0x0008 != 0;
        let patriarch = flags & 0x0004 != 0;
        if deleted || patriarch {
            return None;
        }
        let ole = flags & 0x0010 != 0;
        let connector = flags & 0x0100 != 0;
        let flip_h = flags & 0x0040 != 0;
        let flip_v = flags & 0x0080 != 0;
        let shape_type = fsp.instance;
        let mut shape = blank_shape(spid);
        shape.text_box = shape_type == MSOSPT_TEXT_BOX;
        let mut pib = None;
        for fopt in parts
            .iter()
            .filter(|p| matches!(p.kind, OA_FOPT | OA_SECONDARY_FOPT | OA_TERTIARY_FOPT))
        {
            let props = parse_properties(data, fopt);
            for prop in props {
                match prop.id {
                    0x0104 if !prop.complex => pib = Some(prop.value),
                    0x0380 => {
                        if let Some(name) = prop.text(data) {
                            shape.name = name;
                        }
                    }
                    0x0381 => {
                        shape.description = prop.text(data).filter(|text| !text.is_empty());
                    }
                    _ => {}
                }
            }
        }
        let mut transform = None;
        for anchor in parts
            .iter()
            .filter(|p| matches!(p.kind, OA_CLIENT_ANCHOR | OA_CHILD_ANCHOR))
        {
            let (top, left, right, bottom) = match anchor.len {
                8 => (
                    i64::from(u16_at(data, anchor.start).unwrap_or(0) as i16),
                    i64::from(u16_at(data, anchor.start + 2).unwrap_or(0) as i16),
                    i64::from(u16_at(data, anchor.start + 4).unwrap_or(0) as i16),
                    i64::from(u16_at(data, anchor.start + 6).unwrap_or(0) as i16),
                ),
                16 => match anchor.kind {
                    OA_CLIENT_ANCHOR => (
                        i64::from(i32_at(data, anchor.start).unwrap_or(0)),
                        i64::from(i32_at(data, anchor.start + 4).unwrap_or(0)),
                        i64::from(i32_at(data, anchor.start + 8).unwrap_or(0)),
                        i64::from(i32_at(data, anchor.start + 12).unwrap_or(0)),
                    ),
                    _ => (
                        i64::from(i32_at(data, anchor.start + 4).unwrap_or(0)),
                        i64::from(i32_at(data, anchor.start).unwrap_or(0)),
                        i64::from(i32_at(data, anchor.start + 8).unwrap_or(0)),
                        i64::from(i32_at(data, anchor.start + 12).unwrap_or(0)),
                    ),
                },
                _ => continue,
            };
            if anchor.kind == OA_CLIENT_ANCHOR || transform.is_none() {
                transform = Some(Transform {
                    x: master_units_to_emu(left),
                    y: master_units_to_emu(top),
                    cx: master_units_to_emu(right - left),
                    cy: master_units_to_emu(bottom - top),
                    rot: 0,
                    flip_h,
                    flip_v,
                });
            }
        }
        shape.transform = transform;
        let mut placeholder = None;
        for client_data in parts.iter().filter(|p| p.kind == OA_CLIENT_DATA) {
            for atom in children(data, client_data) {
                if atom.kind != RT_PLACEHOLDER_ATOM {
                    continue;
                }
                let position = u32_at(data, atom.start).unwrap_or(NO_PLACEHOLDER);
                if position == NO_PLACEHOLDER {
                    continue;
                }
                let placement = data.get(atom.start + 4).copied().unwrap_or(0);
                placeholder = Some(Placeholder {
                    kind: placeholder_kind(placement),
                    idx: position,
                });
            }
        }
        let mut body: Option<(u32, TextBody)> = None;
        for textbox in parts.iter().filter(|p| p.kind == OA_CLIENT_TEXTBOX) {
            let atoms = children(data, textbox);
            if let Some(reference) = atoms.iter().find(|a| a.kind == RT_OUTLINE_TEXT_REF_ATOM) {
                let index = i32_at(data, reference.start).unwrap_or(-1);
                if index >= 0 {
                    if let Some(text) = outline.get(index as usize) {
                        body = Some((text.text_type, text.body.clone()));
                    }
                }
                continue;
            }
            if let Some(position) = atoms.iter().position(|a| a.kind == RT_TEXT_HEADER_ATOM) {
                let text_type = u32_at(data, atoms[position].start).unwrap_or(4);
                let parsed = parse_text_body(data, &atoms[position + 1..]);
                body = Some((text_type, parsed));
            }
        }
        if placeholder.is_none() {
            if let Some((text_type, _)) = &body {
                placeholder = match text_type {
                    0 => Some(Placeholder {
                        kind: PlaceholderKind::Title,
                        idx: 0,
                    }),
                    6 => Some(Placeholder {
                        kind: PlaceholderKind::CenterTitle,
                        idx: 0,
                    }),
                    _ => None,
                };
            }
        }
        shape.placeholder = placeholder;
        let picture = pib.filter(|index| *index > 0).map(|index| Picture {
            embed: Some(index.to_string()),
            link: None,
            media: None,
        });
        shape.content = match (body, picture, ole, connector) {
            (Some((_, text)), _, _, _) if !text.is_empty() => Content::Text(text),
            (_, Some(picture), true, _) => Content::Ole(OleObject {
                prog_id: None,
                rel_id: None,
                preview: Some(picture),
            }),
            (_, Some(picture), false, _) => Content::Picture(picture),
            (_, None, true, _) => Content::Ole(OleObject {
                prog_id: None,
                rel_id: None,
                preview: None,
            }),
            (_, None, false, true) => Content::Connector,
            (Some((_, text)), None, false, false) => Content::Text(text),
            (None, None, false, false) => Content::Text(TextBody::default()),
        };
        Some(shape)
    }

    /// The picture with one-based blip index `index`, decoded to its file bytes.
    pub fn picture(&self, index: usize) -> Result<Option<LegacyPicture>> {
        let Some(source) = index
            .checked_sub(1)
            .and_then(|position| self.blips.get(position))
        else {
            return Ok(None);
        };
        let (data, at): (&[u8], usize) = match source {
            BlipSource::Delay(at) => (&self.pictures, *at),
            BlipSource::Embedded(at) => (&self.document, *at),
            BlipSource::Missing => return Ok(None),
        };
        let Some(rec) = header_at(data, at) else {
            return Ok(None);
        };
        let end = rec.end().min(data.len());
        if rec.start > end {
            return Ok(None);
        }
        let payload = &data[rec.start..end];
        Ok(decode_blip(rec.kind, rec.instance, payload))
    }

    /// The media type of the picture with one-based blip index `index`.
    pub fn picture_content_type(&self, index: usize) -> Option<&'static str> {
        let source = self.blips.get(index.checked_sub(1)?)?;
        let (data, at): (&[u8], usize) = match source {
            BlipSource::Delay(at) => (&self.pictures, *at),
            BlipSource::Embedded(at) => (&self.document, *at),
            BlipSource::Missing => return None,
        };
        blip_content_type(header_at(data, at)?.kind)
    }
}

fn blank_shape(id: u32) -> Shape {
    Shape {
        id,
        name: format!("Shape {id}"),
        hidden: false,
        description: None,
        hyperlink: None,
        placeholder: None,
        transform: None,
        text_box: false,
        content: Content::Text(TextBody::default()),
    }
}

/// Maps a `PlaceholderEnum` value onto the PresentationML placeholder kinds.
fn placeholder_kind(placement: u8) -> PlaceholderKind {
    match placement {
        0x01 | 0x0d | 0x11 => PlaceholderKind::Title,
        0x03 | 0x0f => PlaceholderKind::CenterTitle,
        0x04 | 0x10 => PlaceholderKind::Subtitle,
        0x02 | 0x06 | 0x0c | 0x0e | 0x12 => PlaceholderKind::Body,
        0x07 => PlaceholderKind::DateTime,
        0x08 => PlaceholderKind::SlideNumber,
        0x09 => PlaceholderKind::Footer,
        0x0a => PlaceholderKind::Header,
        0x05 | 0x0b => PlaceholderKind::SlideImage,
        0x13 | 0x19 => PlaceholderKind::Object,
        0x14 => PlaceholderKind::Chart,
        0x15 => PlaceholderKind::Table,
        0x16 => PlaceholderKind::ClipArt,
        0x17 => PlaceholderKind::Diagram,
        0x18 => PlaceholderKind::Media,
        0x1a => PlaceholderKind::Picture,
        _ => PlaceholderKind::Other,
    }
}

/// One entry of an `OfficeArtFOPT` property table.
struct Property {
    id: u16,
    complex: bool,
    value: u32,
    /// `(start, len)` of the complex data in the stream.
    data: Option<(usize, usize)>,
}

impl Property {
    /// The complex data as a UTF-16 string without its terminator.
    fn text(&self, stream: &[u8]) -> Option<String> {
        let (start, len) = self.data?;
        let raw = stream.get(start..start + len)?;
        Some(utf16_string(raw))
    }
}

fn utf16_string(raw: &[u8]) -> String {
    let (pairs, _) = raw.as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

fn parse_properties(data: &[u8], fopt: &Rec) -> Vec<Property> {
    let count = usize::from(fopt.instance);
    let mut props = Vec::with_capacity(count);
    let mut cursor = fopt.start;
    for _ in 0..count {
        if cursor + 6 > fopt.end() {
            break;
        }
        let word = u16_at(data, cursor).unwrap_or(0);
        props.push(Property {
            id: word & 0x3fff,
            complex: word & 0x8000 != 0,
            value: u32_at(data, cursor + 2).unwrap_or(0),
            data: None,
        });
        cursor += 6;
    }
    for prop in props.iter_mut().filter(|p| p.complex) {
        let len = prop.value as usize;
        if cursor + len > fopt.end() {
            break;
        }
        prop.data = Some((cursor, len));
        cursor += len;
    }
    props
}

/// Formatting spans of a text body, one entry per character.
struct Spans {
    level: Vec<u8>,
    bullet: Vec<Bullet>,
    bold: Vec<Option<bool>>,
    italic: Vec<Option<bool>>,
    underline: Vec<Option<bool>>,
}

impl Spans {
    fn new(len: usize) -> Self {
        Self {
            level: vec![0; len],
            bullet: vec![Bullet::Inherited; len],
            bold: vec![None; len],
            italic: vec![None; len],
            underline: vec![None; len],
        }
    }
}

/// Builds a text body from the records that follow a `TextHeaderAtom`.
fn parse_text_body(data: &[u8], atoms: &[Rec]) -> TextBody {
    let mut chars: Vec<char> = Vec::new();
    for atom in atoms {
        match atom.kind {
            RT_TEXT_CHARS_ATOM => {
                let raw = &data[atom.start..atom.end().min(data.len())];
                let (pairs, _) = raw.as_chunks::<2>();
                let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
                chars = char::decode_utf16(units)
                    .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect();
                break;
            }
            RT_TEXT_BYTES_ATOM => {
                chars = data[atom.start..atom.end().min(data.len())]
                    .iter()
                    .map(|&byte| char::from(byte))
                    .collect();
                break;
            }
            _ => {}
        }
    }
    let total = chars.len() + 1;
    let mut spans = Spans::new(total);
    if let Some(style) = atoms.iter().find(|a| a.kind == RT_STYLE_TEXT_PROP_ATOM) {
        apply_style_runs(data, style, &mut spans);
    } else if let Some(master) = atoms.iter().find(|a| a.kind == RT_MASTER_TEXT_PROP_ATOM) {
        let mut cursor = master.start;
        let mut position = 0usize;
        while cursor + 6 <= master.end().min(data.len()) && position < total {
            let count = u32_at(data, cursor).unwrap_or(0) as usize;
            let level = u16_at(data, cursor + 4).unwrap_or(0).min(8) as u8;
            let end = (position + count).min(total);
            for slot in &mut spans.level[position..end] {
                *slot = level;
            }
            position = end;
            cursor += 6;
        }
    }
    let mut body = TextBody::default();
    let mut start = 0usize;
    let mut position = 0usize;
    while position <= chars.len() {
        let at_end = position == chars.len();
        if at_end || chars[position] == '\r' {
            body.paragraphs
                .push(paragraph_from(&chars[start..position], start, &spans));
            start = position + 1;
        }
        position += 1;
    }
    if body.paragraphs.len() > 1
        && body.paragraphs.last().is_some_and(Paragraph::is_empty)
        && chars.last() == Some(&'\r')
    {
        body.paragraphs.pop();
    }
    body
}

/// A paragraph from `chars` (starting at text position `offset`): runs split
/// where bold, italic or underline change, vertical tabs as line breaks.
fn paragraph_from(chars: &[char], offset: usize, spans: &Spans) -> Paragraph {
    let level = spans.level.get(offset).copied().unwrap_or(0);
    let bullet = spans
        .bullet
        .get(offset)
        .cloned()
        .unwrap_or(Bullet::Inherited);
    let mut runs: Vec<Run> = Vec::new();
    let mut current = String::new();
    let mut current_props: Option<RunProps> = None;
    let flush = |runs: &mut Vec<Run>, text: &mut String, props: &Option<RunProps>| {
        if text.is_empty() {
            return;
        }
        runs.push(Run {
            kind: RunKind::Text,
            text: std::mem::take(text),
            props: props.clone().unwrap_or_default(),
        });
    };
    for (i, &ch) in chars.iter().enumerate() {
        let position = offset + i;
        if ch == '\u{b}' {
            flush(&mut runs, &mut current, &current_props);
            runs.push(Run {
                kind: RunKind::LineBreak,
                text: String::new(),
                props: RunProps::default(),
            });
            continue;
        }
        if ch.is_control() && ch != '\t' {
            continue;
        }
        let props = RunProps {
            bold: spans.bold.get(position).copied().flatten(),
            italic: spans.italic.get(position).copied().flatten(),
            underline: spans.underline.get(position).copied().flatten(),
            ..RunProps::default()
        };
        if current_props.as_ref() != Some(&props) {
            flush(&mut runs, &mut current, &current_props);
            current_props = Some(props);
        }
        current.push(ch);
    }
    flush(&mut runs, &mut current, &current_props);
    Paragraph {
        level,
        bullet,
        runs,
    }
}

/// Reads the paragraph and character runs of a `StyleTextPropAtom`.
fn apply_style_runs(data: &[u8], atom: &Rec, spans: &mut Spans) {
    let end = atom.end().min(data.len());
    let total = spans.level.len();
    let mut cursor = atom.start;
    let mut position = 0usize;
    while position < total && cursor + 6 <= end {
        let count = u32_at(data, cursor).unwrap_or(0) as usize;
        let level = u16_at(data, cursor + 4).unwrap_or(0).min(8) as u8;
        let masks = u32_at(data, cursor + 6).unwrap_or(0);
        cursor += 10;
        let mut bullet = Bullet::Inherited;
        let mut bullet_char = None;
        let mut has_bullet = None;
        if masks & 0x0000_000f != 0 {
            let flags = u16_at(data, cursor).unwrap_or(0);
            if masks & 0x0000_0001 != 0 {
                has_bullet = Some(flags & 0x0001 != 0);
            }
            cursor += 2;
        }
        if masks & 0x0000_0080 != 0 {
            bullet_char = u16_at(data, cursor).and_then(|unit| char::from_u32(u32::from(unit)));
            cursor += 2;
        }
        if masks & 0x0000_0010 != 0 {
            cursor += 2;
        }
        if masks & 0x0000_0040 != 0 {
            cursor += 2;
        }
        if masks & 0x0000_0020 != 0 {
            cursor += 4;
        }
        for bit in [0x0800u32, 0x1000, 0x2000, 0x4000, 0x0100, 0x0400, 0x8000] {
            if masks & bit != 0 {
                cursor += 2;
            }
        }
        if masks & 0x0010_0000 != 0 {
            let tabs = u16_at(data, cursor).unwrap_or(0) as usize;
            cursor += 2 + tabs * 4;
        }
        if masks & 0x0001_0000 != 0 {
            cursor += 2;
        }
        if masks & 0x000e_0000 != 0 {
            cursor += 2;
        }
        if masks & 0x0020_0000 != 0 {
            cursor += 2;
        }
        match has_bullet {
            Some(true) => bullet = Bullet::Char(bullet_char.unwrap_or('\u{2022}').to_string()),
            Some(false) => bullet = Bullet::None,
            None => {}
        }
        let run_end = (position + count).min(total);
        for i in position..run_end {
            spans.level[i] = level;
            spans.bullet[i] = bullet.clone();
        }
        position = run_end;
        if count == 0 {
            break;
        }
    }
    position = 0;
    while position < total && cursor + 8 <= end {
        let count = u32_at(data, cursor).unwrap_or(0) as usize;
        let masks = u32_at(data, cursor + 4).unwrap_or(0);
        cursor += 8;
        let mut bold = None;
        let mut italic = None;
        let mut underline = None;
        if masks & 0x0000_3ea7 != 0 {
            let style = u16_at(data, cursor).unwrap_or(0);
            if masks & 0x0000_0001 != 0 {
                bold = Some(style & 0x0001 != 0);
            }
            if masks & 0x0000_0002 != 0 {
                italic = Some(style & 0x0002 != 0);
            }
            if masks & 0x0000_0004 != 0 {
                underline = Some(style & 0x0004 != 0);
            }
            cursor += 2;
        }
        for bit in [
            0x0001_0000u32,
            0x0020_0000,
            0x0040_0000,
            0x0080_0000,
            0x0002_0000,
        ] {
            if masks & bit != 0 {
                cursor += 2;
            }
        }
        if masks & 0x0004_0000 != 0 {
            cursor += 4;
        }
        if masks & 0x0008_0000 != 0 {
            cursor += 2;
        }
        let run_end = (position + count).min(total);
        for i in position..run_end {
            spans.bold[i] = bold;
            spans.italic[i] = italic;
            spans.underline[i] = underline;
        }
        position = run_end;
        if count == 0 {
            break;
        }
    }
}

fn blip_content_type(kind: u16) -> Option<&'static str> {
    Some(match kind {
        0xf01a => "image/x-emf",
        0xf01b => "image/x-wmf",
        0xf01c => "image/x-pict",
        0xf01d | 0xf02a => "image/jpeg",
        0xf01e => "image/png",
        0xf01f => "image/bmp",
        0xf029 => "image/tiff",
        _ => return None,
    })
}

/// The file bytes of a BLIP record's payload: UIDs stripped, metafiles
/// inflated when compressed, DIBs given a bitmap file header.
fn decode_blip(kind: u16, instance: u16, payload: &[u8]) -> Option<LegacyPicture> {
    let content_type = blip_content_type(kind)?;
    let metafile = matches!(kind, 0xf01a..=0xf01c);
    let two_uids = match kind {
        0xf01a => instance == 0x3d5,
        0xf01b => instance == 0x217,
        0xf01c => instance == 0x543,
        0xf01d | 0xf02a => matches!(instance, 0x46b | 0x6e3),
        0xf01e => instance == 0x6e1,
        0xf01f => instance == 0x7a9,
        0xf029 => instance == 0x6e5,
        _ => false,
    };
    let uid_len = match two_uids {
        true => 32,
        false => 16,
    };
    if metafile {
        let header = payload.get(uid_len..uid_len + 34)?;
        let uncompressed =
            u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let compression = header[32];
        let body = payload.get(uid_len + 34..)?;
        let bytes = match compression {
            0x00 => {
                let stream = body.get(2..)?;
                let mut out = Vec::new();
                crate::inflate::inflate(stream, uncompressed, &mut out).ok()?;
                out
            }
            _ => body.to_vec(),
        };
        return Some(LegacyPicture {
            content_type,
            bytes,
        });
    }
    let body = payload.get(uid_len + 1..)?;
    let bytes = match kind {
        0xf01f => dib_to_bmp(body),
        _ => body.to_vec(),
    };
    Some(LegacyPicture {
        content_type,
        bytes,
    })
}

/// Prepends a BITMAPFILEHEADER to a device-independent bitmap.
fn dib_to_bmp(dib: &[u8]) -> Vec<u8> {
    let header_size = u32_at(dib, 0).unwrap_or(40) as usize;
    let bit_count = u16_at(dib, 14).unwrap_or(24);
    let compression = u32_at(dib, 16).unwrap_or(0);
    let colors_used = u32_at(dib, 32).unwrap_or(0) as usize;
    let palette_entries = match colors_used {
        0 => 1usize << bit_count.min(8),
        n => n,
    };
    let palette = match bit_count {
        1..=8 => palette_entries * 4,
        _ => 0,
    };
    let masks = match (compression, header_size) {
        (3, 40) => 12,
        _ => 0,
    };
    let offset = 14 + header_size + palette + masks;
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    out.extend_from_slice(dib);
    out
}

/// Opens the compound file at `source` as a legacy deck.
pub fn open_compound(compound: &Compound) -> Result<Arc<LegacyDeck>> {
    LegacyDeck::open(compound).map(Arc::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_bodies_split_paragraphs_and_read_levels_and_bullets() {
        // TextBytesAtom "First\rSecond\x0bline" + StyleTextPropAtom with two PF runs and one CF run.
        let text = b"First\rSecond\x0bline";
        let mut stream = Vec::new();
        let text_at = stream.len();
        stream.extend_from_slice(&[0x00, 0x00, 0xa8, 0x0f]);
        stream.extend_from_slice(&(text.len() as u32).to_le_bytes());
        stream.extend_from_slice(text);
        let style_at = stream.len();
        let mut style = Vec::new();
        // PF run 1: 6 chars ("First\r"), level 0, masks hasBullet with fHasBullet = 0
        style.extend_from_slice(&6u32.to_le_bytes());
        style.extend_from_slice(&0u16.to_le_bytes());
        style.extend_from_slice(&0x0000_0001u32.to_le_bytes());
        style.extend_from_slice(&0u16.to_le_bytes());
        // PF run 2: remaining 12 chars (11 + terminator), level 1, masks hasBullet|bulletChar with bullet '-'
        style.extend_from_slice(&12u32.to_le_bytes());
        style.extend_from_slice(&1u16.to_le_bytes());
        style.extend_from_slice(&0x0000_0081u32.to_le_bytes());
        style.extend_from_slice(&1u16.to_le_bytes());
        style.extend_from_slice(&(b'-' as u16).to_le_bytes());
        // CF run: 5 chars bold, then 13 chars plain
        style.extend_from_slice(&5u32.to_le_bytes());
        style.extend_from_slice(&0x0000_0001u32.to_le_bytes());
        style.extend_from_slice(&0x0001u16.to_le_bytes());
        style.extend_from_slice(&13u32.to_le_bytes());
        style.extend_from_slice(&0x0000_0001u32.to_le_bytes());
        style.extend_from_slice(&0x0000u16.to_le_bytes());
        stream.extend_from_slice(&[0x00, 0x00, 0xa1, 0x0f]);
        stream.extend_from_slice(&(style.len() as u32).to_le_bytes());
        stream.extend_from_slice(&style);
        let atoms = records(&stream, 0, stream.len());
        assert_eq!(atoms.len(), 2);
        assert_eq!(atoms[0].start, text_at + 8);
        assert_eq!(atoms[1].start, style_at + 8);
        let body = parse_text_body(&stream, &atoms);
        assert_eq!(body.paragraphs.len(), 2);
        assert_eq!(body.paragraphs[0].text(), "First");
        assert_eq!(body.paragraphs[0].level, 0);
        assert_eq!(body.paragraphs[0].bullet, Bullet::None);
        assert_eq!(body.paragraphs[0].runs[0].props.bold, Some(true));
        assert_eq!(body.paragraphs[1].text(), "Second\nline");
        assert_eq!(body.paragraphs[1].level, 1);
        assert_eq!(body.paragraphs[1].bullet, Bullet::Char("-".into()));
        assert_eq!(body.paragraphs[1].runs[0].props.bold, Some(false));
    }

    #[test]
    fn property_tables_read_complex_strings_after_the_fixed_entries() {
        let name: Vec<u8> = "Box\0"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let mut stream = vec![0x33, 0x00, 0x0b, 0xf0];
        let body_len = 12 + name.len();
        stream.extend_from_slice(&(body_len as u32).to_le_bytes());
        stream.extend_from_slice(&(0x0104u16 | 0x4000).to_le_bytes());
        stream.extend_from_slice(&3u32.to_le_bytes());
        stream.extend_from_slice(&(0x0380u16 | 0x8000).to_le_bytes());
        stream.extend_from_slice(&(name.len() as u32).to_le_bytes());
        stream.extend_from_slice(&name);
        let mut fopt = header_at(&stream, 0).unwrap();
        fopt.instance = 2;
        let props = parse_properties(&stream, &fopt);
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].id, 0x0104);
        assert_eq!(props[0].value, 3);
        assert!(!props[0].complex);
        assert_eq!(props[1].text(&stream).as_deref(), Some("Box"));
    }

    #[test]
    fn master_units_convert_to_emu() {
        assert_eq!(master_units_to_emu(576), 914_400);
        assert_eq!(master_units_to_emu(5760), 9_144_000);
    }

    #[test]
    fn dib_gets_a_file_header() {
        let mut dib = vec![0u8; 40];
        dib[0] = 40;
        dib[14] = 24;
        dib.extend_from_slice(&[1, 2, 3]);
        let bmp = dib_to_bmp(&dib);
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32::from_le_bytes([bmp[10], bmp[11], bmp[12], bmp[13]]), 54);
        assert_eq!(bmp.len(), 14 + dib.len());
    }
}
