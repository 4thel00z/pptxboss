//! Builds legacy PowerPoint 97-2003 `.ppt` files in memory: a minimal
//! compound file with `Current User`, `PowerPoint Document` and `Pictures`
//! streams, laid out from the MS-CFB, MS-PPT and MS-ODRAW specifications.
//! Like the rest of the testkit it depends on nothing under test.

/// A slide of a [`PptDeck`].
#[derive(Clone, Debug, Default)]
pub struct PptSlide {
    title: Option<String>,
    /// `(indent level, text)` body paragraphs, stored as outline text.
    bullets: Vec<(u8, String)>,
    /// Free text boxes with their text inline in the shape.
    text_boxes: Vec<String>,
    notes: Option<String>,
    hidden: bool,
    /// One-based indexes into the deck's pictures.
    pictures: Vec<usize>,
}

impl PptSlide {
    pub fn titled(title: &str) -> Self {
        Self {
            title: Some(title.to_string()),
            ..Self::default()
        }
    }

    pub fn bullet(mut self, text: &str) -> Self {
        self.bullets.push((0, text.to_string()));
        self
    }

    pub fn sub_bullet(mut self, text: &str, level: u8) -> Self {
        self.bullets.push((level, text.to_string()));
        self
    }

    pub fn text_box(mut self, text: &str) -> Self {
        self.text_boxes.push(text.to_string());
        self
    }

    pub fn notes(mut self, text: &str) -> Self {
        self.notes = Some(text.to_string());
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// Places the deck's picture with one-based index `index` on the slide.
    pub fn picture(mut self, index: usize) -> Self {
        self.pictures.push(index);
        self
    }
}

/// Builds a `.ppt`; `build()` returns the compound file bytes.
#[derive(Clone, Debug, Default)]
pub struct PptDeck {
    slides: Vec<PptSlide>,
    /// PNG pictures, one BLIP each.
    pictures: Vec<Vec<u8>>,
    /// Slide size in master units (1/576 inch).
    size: (i32, i32),
}

const RT_DOCUMENT: u16 = 0x03e8;
const RT_DOCUMENT_ATOM: u16 = 0x03e9;
const RT_END_DOCUMENT_ATOM: u16 = 0x03ea;
const RT_SLIDE: u16 = 0x03ee;
const RT_SLIDE_ATOM: u16 = 0x03ef;
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
const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0ff0;
const RT_USER_EDIT_ATOM: u16 = 0x0ff5;
const RT_CURRENT_USER_ATOM: u16 = 0x0ff6;
const RT_PERSIST_DIRECTORY_ATOM: u16 = 0x1772;

const OA_DGG_CONTAINER: u16 = 0xf000;
const OA_BSTORE_CONTAINER: u16 = 0xf001;
const OA_DG_CONTAINER: u16 = 0xf002;
const OA_SPGR_CONTAINER: u16 = 0xf003;
const OA_SP_CONTAINER: u16 = 0xf004;
const OA_FDGG_BLOCK: u16 = 0xf006;
const OA_FBSE: u16 = 0xf007;
const OA_FDG: u16 = 0xf008;
const OA_FSPGR: u16 = 0xf009;
const OA_FSP: u16 = 0xf00a;
const OA_FOPT: u16 = 0xf00b;
const OA_CLIENT_TEXTBOX: u16 = 0xf00d;
const OA_CLIENT_ANCHOR: u16 = 0xf010;
const OA_CLIENT_DATA: u16 = 0xf011;
const OA_BLIP_PNG: u16 = 0xf01e;

const PT_TITLE: u8 = 0x0d;
const PT_BODY: u8 = 0x0e;
const PT_NOTES_BODY: u8 = 0x0c;
const MSOSPT_RECTANGLE: u16 = 1;
const MSOSPT_PICTURE_FRAME: u16 = 75;
const MSOSPT_TEXT_BOX: u16 = 202;

fn record(ver: u16, instance: u16, kind: u16, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + data.len());
    out.extend_from_slice(&((instance << 4) | ver).to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn atom(kind: u16, data: &[u8]) -> Vec<u8> {
    record(0, 0, kind, data)
}

fn container(kind: u16, instance: u16, children: &[Vec<u8>]) -> Vec<u8> {
    record(0xf, instance, kind, &children.concat())
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// `TextHeaderAtom` + `TextCharsAtom` + a `StyleTextPropAtom` carrying one
/// paragraph run per line with its indent level.
fn text_records(text_type: u32, paragraphs: &[(u8, String)]) -> Vec<Vec<u8>> {
    let joined: Vec<String> = paragraphs.iter().map(|(_, text)| text.clone()).collect();
    let chars = joined.join("\r");
    let mut style = Vec::new();
    let mut remaining = chars.encode_utf16().count() + 1;
    for (index, (level, text)) in paragraphs.iter().enumerate() {
        let mut count = text.encode_utf16().count() + 1;
        if index + 1 == paragraphs.len() {
            count = remaining;
        }
        remaining = remaining.saturating_sub(count);
        style.extend_from_slice(&(count as u32).to_le_bytes());
        style.extend_from_slice(&u16::from(*level).to_le_bytes());
        style.extend_from_slice(&0u32.to_le_bytes());
    }
    let total = chars.encode_utf16().count() as u32 + 1;
    style.extend_from_slice(&total.to_le_bytes());
    style.extend_from_slice(&0u32.to_le_bytes());
    vec![
        atom(RT_TEXT_HEADER_ATOM, &text_type.to_le_bytes()),
        atom(RT_TEXT_CHARS_ATOM, &utf16(&chars)),
        atom(RT_STYLE_TEXT_PROP_ATOM, &style),
    ]
}

fn client_anchor(top: i16, left: i16, right: i16, bottom: i16) -> Vec<u8> {
    let mut data = Vec::new();
    for value in [top, left, right, bottom] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    atom(OA_CLIENT_ANCHOR, &data)
}

fn fsp(spid: u32, shape_type: u16, flags: u32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&spid.to_le_bytes());
    data.extend_from_slice(&flags.to_le_bytes());
    record(2, shape_type, OA_FSP, &data)
}

/// An `OfficeArtFOPT` with the given simple and complex properties.
fn fopt(simple: &[(u16, u32)], complex: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut entries = Vec::new();
    let mut blobs = Vec::new();
    for (id, value) in simple {
        entries.extend_from_slice(&id.to_le_bytes());
        entries.extend_from_slice(&value.to_le_bytes());
    }
    for (id, data) in complex {
        entries.extend_from_slice(&(id | 0x8000).to_le_bytes());
        entries.extend_from_slice(&(data.len() as u32).to_le_bytes());
        blobs.extend_from_slice(data);
    }
    entries.extend_from_slice(&blobs);
    record(3, (simple.len() + complex.len()) as u16, OA_FOPT, &entries)
}

fn placeholder(kind: u8, position: u32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&position.to_le_bytes());
    data.push(kind);
    data.push(0);
    data.extend_from_slice(&0u16.to_le_bytes());
    atom(RT_PLACEHOLDER_ATOM, &data)
}

fn patriarch(spid: u32) -> Vec<u8> {
    let mut spgr = Vec::new();
    for value in [0i32, 0, 0, 0] {
        spgr.extend_from_slice(&value.to_le_bytes());
    }
    container(
        OA_SP_CONTAINER,
        0,
        &[record(1, 0, OA_FSPGR, &spgr), fsp(spid, 0, 0x0005)],
    )
}

impl PptDeck {
    pub fn new() -> Self {
        Self {
            size: (5760, 4320),
            ..Self::default()
        }
    }

    pub fn slide(mut self, slide: PptSlide) -> Self {
        self.slides.push(slide);
        self
    }

    /// Adds a PNG picture; slides refer to it by its one-based index.
    pub fn picture(mut self, png: &[u8]) -> Self {
        self.pictures.push(png.to_vec());
        self
    }

    /// Slide size in master units (1/576 inch); the default is 10 x 7.5 inches.
    pub fn size(mut self, width: i32, height: i32) -> Self {
        self.size = (width, height);
        self
    }

    /// The `PowerPoint Document`, `Current User` and `Pictures` streams.
    pub fn streams(&self) -> Vec<(String, Vec<u8>)> {
        let mut pictures = Vec::new();
        let mut blip_offsets = Vec::new();
        for png in &self.pictures {
            blip_offsets.push(pictures.len() as u32);
            let mut data = vec![0u8; 16];
            data.push(0xff);
            data.extend_from_slice(png);
            pictures.extend(record(0, 0x6e0, OA_BLIP_PNG, &data));
        }
        let mut bstore_entries = Vec::new();
        for (index, png) in self.pictures.iter().enumerate() {
            let mut fbse = Vec::new();
            fbse.push(6);
            fbse.push(6);
            fbse.extend_from_slice(&[0u8; 16]);
            fbse.extend_from_slice(&0xffu16.to_le_bytes());
            fbse.extend_from_slice(&((png.len() + 25) as u32).to_le_bytes());
            fbse.extend_from_slice(&1u32.to_le_bytes());
            fbse.extend_from_slice(&blip_offsets[index].to_le_bytes());
            fbse.extend_from_slice(&[0u8; 4]);
            bstore_entries.push(record(2, 6, OA_FBSE, &fbse));
        }
        let mut fdgg = Vec::new();
        fdgg.extend_from_slice(&(1024 * (self.slides.len() as u32 + 1)).to_le_bytes());
        fdgg.extend_from_slice(&1u32.to_le_bytes());
        fdgg.extend_from_slice(&0u32.to_le_bytes());
        fdgg.extend_from_slice(&(self.slides.len() as u32).to_le_bytes());
        let mut dgg_children = vec![atom(OA_FDGG_BLOCK, &fdgg)];
        if !bstore_entries.is_empty() {
            dgg_children.push(record(
                0xf,
                bstore_entries.len() as u16,
                OA_BSTORE_CONTAINER,
                &bstore_entries.concat(),
            ));
        }
        let drawing_group = container(
            RT_DRAWING_GROUP,
            0,
            &[container(OA_DGG_CONTAINER, 0, &dgg_children)],
        );

        let mut document_atom = Vec::new();
        for value in [self.size.0, self.size.1, 5760, 4320, 1, 2] {
            document_atom.extend_from_slice(&value.to_le_bytes());
        }
        document_atom.extend_from_slice(&0u32.to_le_bytes());
        document_atom.extend_from_slice(&0u32.to_le_bytes());
        document_atom.extend_from_slice(&1u16.to_le_bytes());
        document_atom.extend_from_slice(&0u16.to_le_bytes());
        document_atom.extend_from_slice(&[0, 0, 0, 1]);

        let mut slide_list = Vec::new();
        let mut notes_list = Vec::new();
        let slide_persist_base = 2u32;
        let notes_persist_base = slide_persist_base + self.slides.len() as u32;
        let mut notes_count = 0u32;
        for (index, slide) in self.slides.iter().enumerate() {
            let slide_id = 256 + index as u32;
            let mut persist = Vec::new();
            persist.extend_from_slice(&(slide_persist_base + index as u32).to_le_bytes());
            persist.extend_from_slice(&0u32.to_le_bytes());
            let texts = usize::from(slide.title.is_some()) + usize::from(!slide.bullets.is_empty());
            persist.extend_from_slice(&(texts as u32).to_le_bytes());
            persist.extend_from_slice(&slide_id.to_le_bytes());
            persist.extend_from_slice(&0u32.to_le_bytes());
            slide_list.push(atom(RT_SLIDE_PERSIST_ATOM, &persist));
            if let Some(title) = &slide.title {
                slide_list.extend(text_records(0, &[(0, title.clone())]));
            }
            if !slide.bullets.is_empty() {
                slide_list.extend(text_records(1, &slide.bullets));
            }
            if slide.notes.is_some() {
                let mut notes_persist = Vec::new();
                notes_persist.extend_from_slice(&(notes_persist_base + notes_count).to_le_bytes());
                notes_persist.extend_from_slice(&0u32.to_le_bytes());
                notes_persist.extend_from_slice(&0u32.to_le_bytes());
                notes_persist.extend_from_slice(&(0x1000 + notes_count).to_le_bytes());
                notes_persist.extend_from_slice(&0u32.to_le_bytes());
                notes_list.push(atom(RT_SLIDE_PERSIST_ATOM, &notes_persist));
                notes_count += 1;
            }
        }
        let mut document_children = vec![atom(RT_DOCUMENT_ATOM, &document_atom), drawing_group];
        if !slide_list.is_empty() {
            document_children.push(container(RT_SLIDE_LIST_WITH_TEXT, 0, &slide_list));
        }
        if !notes_list.is_empty() {
            document_children.push(container(RT_SLIDE_LIST_WITH_TEXT, 2, &notes_list));
        }
        document_children.push(atom(RT_END_DOCUMENT_ATOM, &[]));
        let document_container = container(RT_DOCUMENT, 0, &document_children);

        let mut stream = Vec::new();
        let mut persist_offsets: Vec<(u32, u32)> = vec![(1, 0)];
        stream.extend(document_container);
        let mut notes_containers: Vec<(u32, Vec<u8>)> = Vec::new();
        notes_count = 0;
        for (index, slide) in self.slides.iter().enumerate() {
            let slide_id = 256 + index as u32;
            persist_offsets.push((slide_persist_base + index as u32, stream.len() as u32));
            stream.extend(self.slide_container(index, slide, slide_id, notes_count));
            if let Some(notes) = &slide.notes {
                let persist = notes_persist_base + notes_count;
                notes_containers.push((persist, self.notes_container(slide_id, notes, index)));
                notes_count += 1;
            }
        }
        for (persist, bytes) in notes_containers {
            persist_offsets.push((persist, stream.len() as u32));
            stream.extend(bytes);
        }
        persist_offsets.sort();
        let mut directory = Vec::new();
        for (id, offset) in &persist_offsets {
            directory.extend_from_slice(&(id | (1 << 20)).to_le_bytes());
            directory.extend_from_slice(&offset.to_le_bytes());
        }
        let directory_offset = stream.len() as u32;
        stream.extend(atom(RT_PERSIST_DIRECTORY_ATOM, &directory));
        let user_edit_offset = stream.len() as u32;
        let mut user_edit = Vec::new();
        user_edit.extend_from_slice(&256u32.to_le_bytes());
        user_edit.extend_from_slice(&0u16.to_le_bytes());
        user_edit.push(0);
        user_edit.push(3);
        user_edit.extend_from_slice(&0u32.to_le_bytes());
        user_edit.extend_from_slice(&directory_offset.to_le_bytes());
        user_edit.extend_from_slice(&1u32.to_le_bytes());
        user_edit.extend_from_slice(&(notes_persist_base + notes_count + 1).to_le_bytes());
        user_edit.extend_from_slice(&1u16.to_le_bytes());
        user_edit.extend_from_slice(&0u16.to_le_bytes());
        stream.extend(atom(RT_USER_EDIT_ATOM, &user_edit));

        let mut current_user = Vec::new();
        current_user.extend_from_slice(&0x14u32.to_le_bytes());
        current_user.extend_from_slice(&0xe391_c05fu32.to_le_bytes());
        current_user.extend_from_slice(&user_edit_offset.to_le_bytes());
        current_user.extend_from_slice(&4u16.to_le_bytes());
        current_user.extend_from_slice(&0x03f4u16.to_le_bytes());
        current_user.push(3);
        current_user.push(0);
        current_user.extend_from_slice(&0u16.to_le_bytes());
        current_user.extend_from_slice(b"test");
        current_user.extend_from_slice(&8u32.to_le_bytes());
        current_user.extend_from_slice(&utf16("test"));
        let current_user = atom(RT_CURRENT_USER_ATOM, &current_user);

        let mut streams = vec![
            ("Current User".to_string(), current_user),
            ("PowerPoint Document".to_string(), stream),
        ];
        if !pictures.is_empty() {
            streams.push(("Pictures".to_string(), pictures));
        }
        streams
    }

    fn slide_container(
        &self,
        index: usize,
        slide: &PptSlide,
        slide_id: u32,
        notes_index: u32,
    ) -> Vec<u8> {
        let mut slide_atom = Vec::new();
        slide_atom.extend_from_slice(&1u32.to_le_bytes());
        slide_atom.extend_from_slice(&[PT_TITLE, PT_BODY, 0, 0, 0, 0, 0, 0]);
        slide_atom.extend_from_slice(&0x8000_0000u32.to_le_bytes());
        let notes_ref = match slide.notes.is_some() {
            true => 0x1000 + notes_index,
            false => 0,
        };
        slide_atom.extend_from_slice(&notes_ref.to_le_bytes());
        slide_atom.extend_from_slice(&0x0007u16.to_le_bytes());
        slide_atom.extend_from_slice(&0u16.to_le_bytes());
        let mut children = vec![record(2, 0, RT_SLIDE_ATOM, &slide_atom)];
        if slide.hidden {
            let mut info = vec![0u8; 16];
            info[10..12].copy_from_slice(&0x0004u16.to_le_bytes());
            children.push(atom(RT_SLIDE_SHOW_SLIDE_INFO_ATOM, &info));
        }
        let mut spid = 1024 * (index as u32 + 1);
        let mut shapes = vec![patriarch(spid)];
        spid += 1;
        let mut outline_index = 0i32;
        if slide.title.is_some() {
            shapes.push(container(
                OA_SP_CONTAINER,
                0,
                &[
                    fsp(spid, MSOSPT_RECTANGLE, 0x0a00),
                    fopt(&[], &[(0x0380, utf16("Title 1\0"))]),
                    client_anchor(360, 360, 5400, 1200),
                    container(OA_CLIENT_DATA, 0, &[placeholder(PT_TITLE, 0)]),
                    container(
                        OA_CLIENT_TEXTBOX,
                        0,
                        &[atom(RT_OUTLINE_TEXT_REF_ATOM, &outline_index.to_le_bytes())],
                    ),
                ],
            ));
            spid += 1;
            outline_index += 1;
        }
        if !slide.bullets.is_empty() {
            shapes.push(container(
                OA_SP_CONTAINER,
                0,
                &[
                    fsp(spid, MSOSPT_RECTANGLE, 0x0a00),
                    fopt(&[], &[(0x0380, utf16("Body 2\0"))]),
                    client_anchor(1400, 360, 5400, 4000),
                    container(OA_CLIENT_DATA, 0, &[placeholder(PT_BODY, 1)]),
                    container(
                        OA_CLIENT_TEXTBOX,
                        0,
                        &[atom(RT_OUTLINE_TEXT_REF_ATOM, &outline_index.to_le_bytes())],
                    ),
                ],
            ));
            spid += 1;
        }
        for text in &slide.text_boxes {
            shapes.push(container(
                OA_SP_CONTAINER,
                0,
                &[
                    fsp(spid, MSOSPT_TEXT_BOX, 0x0a00),
                    fopt(&[], &[(0x0380, utf16("TextBox\0"))]),
                    client_anchor(4000, 360, 3000, 4300),
                    container(OA_CLIENT_TEXTBOX, 0, &text_records(4, &[(0, text.clone())])),
                ],
            ));
            spid += 1;
        }
        for picture in &slide.pictures {
            shapes.push(container(
                OA_SP_CONTAINER,
                0,
                &[
                    fsp(spid, MSOSPT_PICTURE_FRAME, 0x0a00),
                    fopt(
                        &[(0x0104 | 0x4000, *picture as u32)],
                        &[(0x0380, utf16("Picture\0")), (0x0381, utf16("A dot\0"))],
                    ),
                    client_anchor(1000, 4000, 5000, 2000),
                ],
            ));
            spid += 1;
        }
        let mut fdg = Vec::new();
        fdg.extend_from_slice(&(shapes.len() as u32).to_le_bytes());
        fdg.extend_from_slice(&spid.to_le_bytes());
        let drawing = container(
            RT_DRAWING,
            0,
            &[container(
                OA_DG_CONTAINER,
                0,
                &[
                    record(0, index as u16 + 1, OA_FDG, &fdg),
                    container(OA_SPGR_CONTAINER, 0, &shapes),
                ],
            )],
        );
        children.push(drawing);
        let _ = slide_id;
        container(RT_SLIDE, 0, &children)
    }

    fn notes_container(&self, slide_id: u32, notes: &str, index: usize) -> Vec<u8> {
        let mut notes_atom = Vec::new();
        notes_atom.extend_from_slice(&slide_id.to_le_bytes());
        notes_atom.extend_from_slice(&0x0007u16.to_le_bytes());
        notes_atom.extend_from_slice(&0u16.to_le_bytes());
        let spid = 1024 * (self.slides.len() as u32 + index as u32 + 2);
        let shapes = vec![
            patriarch(spid),
            container(
                OA_SP_CONTAINER,
                0,
                &[
                    fsp(spid + 1, MSOSPT_RECTANGLE, 0x0a00),
                    client_anchor(3000, 360, 5400, 5000),
                    container(OA_CLIENT_DATA, 0, &[placeholder(PT_NOTES_BODY, 0)]),
                    container(
                        OA_CLIENT_TEXTBOX,
                        0,
                        &text_records(2, &[(0, notes.to_string())]),
                    ),
                ],
            ),
        ];
        let mut fdg = Vec::new();
        fdg.extend_from_slice(&(shapes.len() as u32).to_le_bytes());
        fdg.extend_from_slice(&(spid + 1).to_le_bytes());
        let drawing = container(
            RT_DRAWING,
            0,
            &[container(
                OA_DG_CONTAINER,
                0,
                &[
                    record(0, (self.slides.len() + index + 1) as u16, OA_FDG, &fdg),
                    container(OA_SPGR_CONTAINER, 0, &shapes),
                ],
            )],
        );
        container(
            RT_NOTES,
            0,
            &[record(1, 0, RT_NOTES_ATOM, &notes_atom), drawing],
        )
    }

    /// The `.ppt` file: a version-3 compound file holding the streams.
    pub fn build(&self) -> Vec<u8> {
        let streams = self.streams();
        let refs: Vec<(&str, &[u8])> = streams
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice()))
            .collect();
        compound_file(&refs)
    }
}

const SECTOR: usize = 512;
const MINI_SECTOR: usize = 64;
const MINI_CUTOFF: usize = 4096;
const ENDOFCHAIN: u32 = 0xffff_fffe;
const FREESECT: u32 = 0xffff_ffff;
const FATSECT: u32 = 0xffff_fffd;
const NOSTREAM: u32 = 0xffff_ffff;

/// A minimal version-3 compound file with the given root-level streams:
/// sector 0 the FAT, then the directory, the mini FAT, the mini stream
/// holding streams under the cutoff, then the large streams. Every
/// directory node is black and the sibling tree is balanced.
pub fn compound_file(streams: &[(&str, &[u8])]) -> Vec<u8> {
    let mut mini_stream = Vec::new();
    let mut mini_fat: Vec<u32> = Vec::new();
    let mut entries: Vec<(&str, u32, u64)> = Vec::new();
    let mut large: Vec<&[u8]> = Vec::new();
    for (name, data) in streams {
        if data.len() < MINI_CUTOFF && !data.is_empty() {
            let first = (mini_stream.len() / MINI_SECTOR) as u32;
            let count = data.len().div_ceil(MINI_SECTOR);
            mini_stream.extend_from_slice(data);
            mini_stream.resize(mini_stream.len().div_ceil(MINI_SECTOR) * MINI_SECTOR, 0);
            for i in 0..count {
                mini_fat.push(match i + 1 == count {
                    true => ENDOFCHAIN,
                    false => first + i as u32 + 1,
                });
            }
            entries.push((name, first, data.len() as u64));
            continue;
        }
        entries.push((name, u32::MAX, data.len() as u64));
        large.push(data);
    }
    let directory_sectors = (1 + entries.len()).div_ceil(SECTOR / 128);
    let mini_fat_sectors = (mini_fat.len() * 4).div_ceil(SECTOR);
    let mini_stream_sectors = mini_stream.len().div_ceil(SECTOR);
    let large_sectors: usize = large.iter().map(|d| d.len().div_ceil(SECTOR)).sum();
    let total = 1 + directory_sectors + mini_fat_sectors + mini_stream_sectors + large_sectors;
    assert!(total <= SECTOR / 4, "fixture too large for one FAT sector");
    let mut fat = vec![FREESECT; SECTOR / 4];
    fat[0] = FATSECT;
    let mut next = 1u32;
    let mut chain = |count: usize, fat: &mut Vec<u32>| -> u32 {
        let start = next;
        for i in 0..count {
            fat[(start as usize) + i] = match i + 1 == count {
                true => ENDOFCHAIN,
                false => start + i as u32 + 1,
            };
        }
        next += count as u32;
        start
    };
    let directory_start = chain(directory_sectors, &mut fat);
    let mini_fat_start = match mini_fat_sectors {
        0 => ENDOFCHAIN,
        n => chain(n, &mut fat),
    };
    let mini_stream_start = match mini_stream_sectors {
        0 => ENDOFCHAIN,
        n => chain(n, &mut fat),
    };
    let mut large_iter = large.iter();
    for entry in entries.iter_mut().filter(|entry| entry.1 == u32::MAX) {
        let data = large_iter.next().expect("a large stream per entry");
        entry.1 = match data.is_empty() {
            true => ENDOFCHAIN,
            false => chain(data.len().div_ceil(SECTOR), &mut fat),
        };
    }

    let mut out = Vec::with_capacity((total + 1) * SECTOR);
    out.extend_from_slice(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
    out.extend_from_slice(&[0u8; 16]);
    for value in [0x003eu16, 3, 0xfffe, 9, 6] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&[0u8; 6]);
    for value in [
        0u32,
        1,
        directory_start,
        0,
        MINI_CUTOFF as u32,
        mini_fat_start,
        mini_fat_sectors as u32,
        ENDOFCHAIN,
        0,
        0,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for _ in 1..109 {
        out.extend_from_slice(&FREESECT.to_le_bytes());
    }
    assert_eq!(out.len(), SECTOR);
    for value in &fat {
        out.extend_from_slice(&value.to_le_bytes());
    }

    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&i| {
        let name = entries[i].0;
        (name.encode_utf16().count(), name.to_ascii_uppercase())
    });
    let mut links = vec![(NOSTREAM, NOSTREAM); entries.len()];
    let root_child = build_tree(&order, &mut links);
    let mut directory = Vec::new();
    directory.extend(directory_entry(
        "Root Entry",
        5,
        NOSTREAM,
        NOSTREAM,
        root_child,
        match mini_stream.is_empty() {
            true => ENDOFCHAIN,
            false => mini_stream_start,
        },
        mini_stream.len() as u64,
    ));
    for (index, (name, start, size)) in entries.iter().enumerate() {
        directory.extend(directory_entry(
            name,
            2,
            links[index].0,
            links[index].1,
            NOSTREAM,
            *start,
            *size,
        ));
    }
    while directory.len() < directory_sectors * SECTOR {
        directory.extend(directory_entry("", 0, NOSTREAM, NOSTREAM, NOSTREAM, 0, 0));
    }
    out.extend_from_slice(&directory);
    if mini_fat_sectors > 0 {
        let mut bytes = Vec::new();
        for value in &mini_fat {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.resize(mini_fat_sectors * SECTOR, 0xff);
        out.extend_from_slice(&bytes);
    }
    if mini_stream_sectors > 0 {
        let mut bytes = mini_stream.clone();
        bytes.resize(mini_stream_sectors * SECTOR, 0);
        out.extend_from_slice(&bytes);
    }
    for data in &large {
        let mut bytes = data.to_vec();
        bytes.resize(data.len().div_ceil(SECTOR) * SECTOR, 0);
        out.extend_from_slice(&bytes);
    }
    out
}

/// Builds a balanced sibling tree over `order`; returns the root's directory id.
fn build_tree(order: &[usize], links: &mut [(u32, u32)]) -> u32 {
    if order.is_empty() {
        return NOSTREAM;
    }
    let middle = order.len() / 2;
    let node = order[middle];
    let left = build_tree(&order[..middle], links);
    let right = build_tree(&order[middle + 1..], links);
    links[node] = (left, right);
    node as u32 + 1
}

fn directory_entry(
    name: &str,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
) -> Vec<u8> {
    let mut entry = vec![0u8; 128];
    let units: Vec<u16> = name.encode_utf16().take(31).collect();
    for (i, unit) in units.iter().enumerate() {
        entry[i * 2..i * 2 + 2].copy_from_slice(&unit.to_le_bytes());
    }
    let name_len = match name.is_empty() {
        true => 0u16,
        false => (units.len() as u16 + 1) * 2,
    };
    entry[0x40..0x42].copy_from_slice(&name_len.to_le_bytes());
    entry[0x42] = kind;
    entry[0x43] = 1;
    entry[0x44..0x48].copy_from_slice(&left.to_le_bytes());
    entry[0x48..0x4c].copy_from_slice(&right.to_le_bytes());
    entry[0x4c..0x50].copy_from_slice(&child.to_le_bytes());
    entry[0x74..0x78].copy_from_slice(&start.to_le_bytes());
    entry[0x78..0x80].copy_from_slice(&size.to_le_bytes());
    entry
}
