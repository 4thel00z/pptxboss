//! Walks the record structure of legacy .ppt files to check the reader's
//! understanding of the format against real files: Current User atom,
//! user-edit chain, persist directory, document container tiling, slide
//! list, slide containers, notes and pictures.
//!
//! `cargo run --release -p pptxboss-core --example ppt_walk -- *.ppt`

use pptxboss_core::cfb::Compound;
use pptxboss_core::zip::FileSource;

const RT_DOCUMENT: u16 = 0x03e8;
const RT_SLIDE: u16 = 0x03ee;
const RT_NOTES: u16 = 0x03f0;
const RT_SLIDE_PERSIST_ATOM: u16 = 0x03f3;
const RT_TEXT_HEADER_ATOM: u16 = 0x0f9f;
const RT_SLIDE_LIST_WITH_TEXT: u16 = 0x0ff0;
const RT_USER_EDIT_ATOM: u16 = 0x0ff5;
const RT_CURRENT_USER_ATOM: u16 = 0x0ff6;
const RT_PERSIST_DIRECTORY_ATOM: u16 = 0x1772;
const RT_CRYPT_SESSION: u16 = 0x2f14;
const RT_DRAWING: u16 = 0x040c;

#[derive(Clone, Copy)]
struct Header {
    ver: u8,
    instance: u16,
    kind: u16,
    len: u32,
}

fn header(data: &[u8], at: usize) -> Option<Header> {
    let raw = data.get(at..at + 8)?;
    let word = u16::from_le_bytes([raw[0], raw[1]]);
    Some(Header {
        ver: (word & 0xf) as u8,
        instance: word >> 4,
        kind: u16::from_le_bytes([raw[2], raw[3]]),
        len: u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]),
    })
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let raw = data.get(at..at + 4)?;
    Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

/// Children `(header, data offset)` of the container at `at`; None when the children do not tile it.
fn children(data: &[u8], at: usize) -> Option<Vec<(Header, usize)>> {
    let container = header(data, at)?;
    let end = at + 8 + container.len as usize;
    if end > data.len() {
        return None;
    }
    let mut out = Vec::new();
    let mut cursor = at + 8;
    while cursor < end {
        let child = header(data, cursor)?;
        let next = cursor + 8 + child.len as usize;
        if next > end {
            return None;
        }
        out.push((child, cursor));
        cursor = next;
    }
    Some(out)
}

fn walk(path: &str) -> Result<String, String> {
    let source = FileSource::open(path).map_err(|e| e.to_string())?;
    let compound = Compound::open(&source).map_err(|e| e.to_string())?;
    let current_user = compound
        .stream("Current User")
        .ok_or("no Current User stream")?;
    let document = compound
        .stream("PowerPoint Document")
        .ok_or("no PowerPoint Document stream")?;
    let pictures = compound.stream("Pictures");
    let cu = header(&current_user, 0).ok_or("short Current User")?;
    if cu.kind != RT_CURRENT_USER_ATOM {
        return Err(format!("Current User starts with 0x{:04x}", cu.kind));
    }
    let size = u32_at(&current_user, 8).unwrap_or(0);
    let token = u32_at(&current_user, 12).unwrap_or(0);
    let offset_edit = u32_at(&current_user, 16).unwrap_or(0) as usize;
    let doc_version = current_user
        .get(22..24)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .unwrap_or(0);
    let mut notes = Vec::new();
    if size != 0x14 || doc_version != 0x03f4 {
        notes.push(format!("cu size=0x{size:x} docver=0x{doc_version:x}"));
    }
    let mut edits = 0;
    let mut offset = offset_edit;
    let mut directories: Vec<Vec<(u32, u32)>> = Vec::new();
    let mut doc_persist = 0u32;
    let mut encrypt_ref = None;
    while offset != 0 {
        let ue = header(&document, offset).ok_or(format!("user edit at {offset} out of range"))?;
        if ue.kind != RT_USER_EDIT_ATOM {
            return Err(format!(
                "expected UserEditAtom at {offset}, found 0x{:04x}",
                ue.kind
            ));
        }
        if !matches!(ue.len, 0x1c | 0x20) {
            notes.push(format!("useredit len 0x{:x}", ue.len));
        }
        let last_edit = u32_at(&document, offset + 16).unwrap_or(0) as usize;
        let dir_offset = u32_at(&document, offset + 20).unwrap_or(0) as usize;
        if edits == 0 {
            doc_persist = u32_at(&document, offset + 24).unwrap_or(0);
            if ue.len == 0x20 {
                encrypt_ref = u32_at(&document, offset + 36);
            }
        }
        let dir = header(&document, dir_offset).ok_or("persist directory out of range")?;
        if dir.kind != RT_PERSIST_DIRECTORY_ATOM {
            return Err(format!(
                "expected PersistDirectoryAtom at {dir_offset}, found 0x{:04x}",
                dir.kind
            ));
        }
        let mut entries = Vec::new();
        let mut cursor = dir_offset + 8;
        let end = dir_offset + 8 + dir.len as usize;
        while cursor + 4 <= end {
            let word = u32_at(&document, cursor).unwrap_or(0);
            let persist_id = word & 0x000f_ffff;
            let count = (word >> 20) & 0xfff;
            cursor += 4;
            if count == 0 {
                notes.push("cPersist 0".into());
                break;
            }
            for i in 0..count {
                let Some(value) = u32_at(&document, cursor) else {
                    break;
                };
                entries.push((persist_id + i, value));
                cursor += 4;
            }
        }
        directories.push(entries);
        edits += 1;
        if last_edit >= offset {
            return Err("user edit chain does not decrease".into());
        }
        offset = last_edit;
        if edits > 10_000 {
            return Err("user edit chain too long".into());
        }
    }
    let mut directory = std::collections::HashMap::new();
    for entries in directories.iter().rev() {
        for (id, off) in entries {
            directory.insert(*id, *off);
        }
    }
    if token == 0xf3d1_c4df {
        return Ok(format!("ENCRYPTED (header token) edits={edits}"));
    }
    if let Some(reference) = encrypt_ref.filter(|r| *r != 0) {
        if let Some(&off) = directory.get(&reference) {
            if header(&document, off as usize).is_some_and(|h| h.kind == RT_CRYPT_SESSION) {
                return Ok(format!("ENCRYPTED (crypt session) edits={edits}"));
            }
        }
    }
    let doc_offset = *directory
        .get(&doc_persist)
        .ok_or(format!("docPersistIdRef {doc_persist} not in directory"))?
        as usize;
    let doc = header(&document, doc_offset).ok_or("document container out of range")?;
    if doc.kind != RT_DOCUMENT || doc.ver != 0xf {
        return Err(format!(
            "expected RT_Document at {doc_offset}, found 0x{:04x} ver {}",
            doc.kind, doc.ver
        ));
    }
    let doc_children = children(&document, doc_offset).ok_or("document children do not tile")?;
    let mut slide_persists = Vec::new();
    let mut text_headers = 0;
    let mut notes_persists = 0;
    let mut has_drawing_group = false;
    for (child, at) in &doc_children {
        if child.kind == 0x040b {
            has_drawing_group = true;
        }
        if child.kind != RT_SLIDE_LIST_WITH_TEXT {
            continue;
        }
        let items = children(&document, *at).ok_or("slide list does not tile")?;
        for (item, item_at) in items {
            if item.kind == RT_SLIDE_PERSIST_ATOM {
                let persist = u32_at(&document, item_at + 8).unwrap_or(0);
                match child.instance {
                    0 => slide_persists.push(persist),
                    2 => notes_persists += 1,
                    _ => {}
                }
            }
            if item.kind == RT_TEXT_HEADER_ATOM && child.instance == 0 {
                text_headers += 1;
            }
        }
    }
    let mut slides_ok = 0;
    let mut shapes = 0;
    let mut textboxes = 0;
    let mut hidden = 0;
    let mut bad_slides = 0;
    for persist in &slide_persists {
        let Some(&off) = directory.get(persist) else {
            bad_slides += 1;
            continue;
        };
        let Some(slide) = header(&document, off as usize) else {
            bad_slides += 1;
            continue;
        };
        if slide.kind != RT_SLIDE || slide.ver != 0xf {
            bad_slides += 1;
            continue;
        }
        let Some(slide_children) = children(&document, off as usize) else {
            bad_slides += 1;
            continue;
        };
        slides_ok += 1;
        for (child, child_at) in &slide_children {
            if child.kind == 0x03f9 {
                let flags = document
                    .get(child_at + 8 + 10..child_at + 8 + 12)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .unwrap_or(0);
                if flags & 0x0004 != 0 {
                    hidden += 1;
                }
            }
            if child.kind == RT_DRAWING {
                count_shapes(&document, *child_at, &mut shapes, &mut textboxes);
            }
        }
    }
    let picture_blocks = pictures.as_ref().map(|p| {
        let mut count = 0;
        let mut cursor = 0;
        while let Some(h) = header(p, cursor) {
            if h.kind != 0xf007 && !(0xf018..=0xf117).contains(&h.kind) {
                break;
            }
            count += 1;
            cursor += 8 + h.len as usize;
        }
        count
    });
    let _ = RT_NOTES;
    Ok(format!(
        "edits={edits} dir={} slides={} ok={slides_ok} bad={bad_slides} hidden={hidden} texts={text_headers} notes={notes_persists} shapes={shapes} textboxes={textboxes} pictures={} dgg={has_drawing_group}{}",
        directory.len(),
        slide_persists.len(),
        picture_blocks.map_or("-".to_string(), |n| n.to_string()),
        match notes.is_empty() {
            true => String::new(),
            false => format!(" notes: {}", notes.join("; ")),
        }
    ))
}

fn count_shapes(document: &[u8], at: usize, shapes: &mut usize, textboxes: &mut usize) {
    let Some(kids) = children(document, at) else {
        return;
    };
    for (child, child_at) in kids {
        match child.kind {
            0xf002 | 0xf003 | 0x040c => count_shapes(document, child_at, shapes, textboxes),
            0xf004 => {
                *shapes += 1;
                if let Some(parts) = children(document, child_at) {
                    if parts.iter().any(|(p, _)| p.kind == 0xf00d) {
                        *textboxes += 1;
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let mut ok = 0;
    let mut encrypted = 0;
    let mut failed = 0;
    for path in std::env::args().skip(1) {
        let name = path.rsplit('/').next().unwrap_or(&path);
        match walk(&path) {
            Ok(line) if line.starts_with("ENCRYPTED") => {
                encrypted += 1;
                println!("{name}: {line}");
            }
            Ok(line) => {
                ok += 1;
                println!("{name}: {line}");
            }
            Err(err) => {
                failed += 1;
                println!("{name}: ERROR {err}");
            }
        }
    }
    println!("summary: ok={ok} encrypted={encrypted} failed={failed}");
}
