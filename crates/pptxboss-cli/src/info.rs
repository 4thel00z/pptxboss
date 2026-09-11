//! `pptxboss info`.

use std::io::Write;
use std::path::Path;

use pptxboss_core::Document;
use serde::Serialize;

use crate::Failure;

#[derive(Serialize)]
struct SlideInfo {
    number: usize,
    part: String,
    title: Option<String>,
    hidden: bool,
    shapes: usize,
    pictures: usize,
    tables: usize,
    objects: usize,
    has_notes: bool,
    has_comments: bool,
    error: Option<String>,
}

#[derive(Serialize)]
struct SectionInfo {
    name: String,
    /// One-based slide numbers.
    slides: Vec<usize>,
}

#[derive(Serialize)]
struct Properties {
    title: Option<String>,
    subject: Option<String>,
    creator: Option<String>,
    last_modified_by: Option<String>,
    created: Option<String>,
    modified: Option<String>,
    application: Option<String>,
    app_version: Option<String>,
}

#[derive(Serialize)]
struct Info {
    file: String,
    /// `pptx` for an ECMA-376 package, `ppt` for a legacy binary presentation.
    format: &'static str,
    presentation_part: String,
    slides: usize,
    slide_size_emu: Option<(i64, i64)>,
    slide_size_inches: Option<(f64, f64)>,
    slide_size_type: Option<String>,
    masters: usize,
    parts: usize,
    properties: Properties,
    sections: Vec<SectionInfo>,
    slide_list: Vec<SlideInfo>,
}

fn collect(doc: &Document, file: &Path) -> Info {
    let presentation = doc.presentation();
    let slide_list = doc.map_slides(|slide| match slide {
        Ok(slide) => {
            let shapes: Vec<_> = slide.content.walk().collect();
            SlideInfo {
                number: slide.number(),
                part: slide.part.clone(),
                title: slide.title().map(|title| title.trim().to_string()),
                hidden: slide.is_hidden(),
                shapes: shapes.len(),
                pictures: shapes
                    .iter()
                    .filter(|shape| matches!(shape.content, pptxboss_core::Content::Picture(_)))
                    .count(),
                tables: shapes
                    .iter()
                    .filter(|shape| matches!(shape.content, pptxboss_core::Content::Table(_)))
                    .count(),
                objects: shapes
                    .iter()
                    .filter(|shape| matches!(shape.content, pptxboss_core::Content::Ole(_)))
                    .count(),
                has_notes: slide.notes_part().ok().flatten().is_some(),
                has_comments: slide.comments_part().ok().flatten().is_some(),
                error: None,
            }
        }
        Err(err) => SlideInfo {
            number: 0,
            part: String::new(),
            title: None,
            hidden: false,
            shapes: 0,
            pictures: 0,
            tables: 0,
            objects: 0,
            has_notes: false,
            has_comments: false,
            error: Some(err.to_string()),
        },
    });
    let slide_list = slide_list
        .into_iter()
        .enumerate()
        .map(|(index, mut info)| {
            if info.number == 0 {
                info.number = index + 1;
                info.part = doc.slide_refs()[index].part.clone();
            }
            info
        })
        .collect();
    let size = presentation.slide_size.as_ref();
    let core = doc.core_properties().ok().flatten().unwrap_or_default();
    let app = doc.app_properties().ok().flatten().unwrap_or_default();
    Info {
        file: file.display().to_string(),
        format: match doc.is_legacy() {
            true => "ppt",
            false => "pptx",
        },
        presentation_part: doc.presentation_part().to_string(),
        slides: doc.slide_count(),
        slide_size_emu: size.map(|size| (size.cx, size.cy)),
        slide_size_inches: size.map(|size| (size.cx as f64 / 914400.0, size.cy as f64 / 914400.0)),
        slide_size_type: size.and_then(|size| size.kind.clone()),
        masters: presentation.masters.len(),
        parts: doc.package().map_or(0, |package| package.parts().len()),
        properties: Properties {
            title: core.title,
            subject: core.subject,
            creator: core.creator,
            last_modified_by: core.last_modified_by,
            created: core.created,
            modified: core.modified,
            application: app.application,
            app_version: app.app_version,
        },
        sections: doc
            .sections()
            .into_iter()
            .map(|section| SectionInfo {
                name: section.name,
                slides: section.slides.iter().map(|index| index + 1).collect(),
            })
            .collect(),
        slide_list,
    }
}

/// `1-3, 5` for a run of one-based slide numbers.
fn number_ranges(numbers: &[usize]) -> String {
    let mut ranges: Vec<String> = Vec::new();
    let mut i = 0;
    while i < numbers.len() {
        let start = numbers[i];
        let mut end = start;
        while i + 1 < numbers.len() && numbers[i + 1] == end + 1 {
            end = numbers[i + 1];
            i += 1;
        }
        ranges.push(match start == end {
            true => start.to_string(),
            false => format!("{start}-{end}"),
        });
        i += 1;
    }
    ranges.join(", ")
}

pub fn run(doc: &Document, file: &Path, json: bool) -> Result<(), Failure> {
    let info = collect(doc, file);
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &info).map_err(|err| Failure {
            message: err.to_string(),
            code: 1,
        })?;
        writeln!(out)?;
        return Ok(());
    }
    writeln!(out, "file:         {}", info.file)?;
    writeln!(out, "format:       {}", info.format)?;
    writeln!(out, "presentation: {}", info.presentation_part)?;
    writeln!(out, "slides:       {}", info.slides)?;
    if let (Some((cx, cy)), Some((w, h))) = (info.slide_size_emu, info.slide_size_inches) {
        let kind = info
            .slide_size_type
            .as_deref()
            .map(|kind| format!(" ({kind})"))
            .unwrap_or_default();
        writeln!(
            out,
            "slide size:   {cx} x {cy} EMU = {w:.2} x {h:.2} in{kind}"
        )?;
    }
    writeln!(out, "masters:      {}", info.masters)?;
    writeln!(out, "parts:        {}", info.parts)?;
    let properties = [
        ("title", &info.properties.title),
        ("subject", &info.properties.subject),
        ("creator", &info.properties.creator),
        ("modified by", &info.properties.last_modified_by),
        ("created", &info.properties.created),
        ("modified", &info.properties.modified),
        ("application", &info.properties.application),
    ];
    for (label, value) in properties {
        if let Some(value) = value {
            writeln!(out, "{:<13} {value}", format!("{label}:"))?;
        }
    }
    for section in &info.sections {
        let slides = match section.slides.is_empty() {
            true => "no slides".to_string(),
            false => format!("slides {}", number_ranges(&section.slides)),
        };
        writeln!(out, "section:      {} ({slides})", section.name)?;
    }
    if info.slide_list.is_empty() {
        return Ok(());
    }
    writeln!(out)?;
    for slide in &info.slide_list {
        let mut flags = Vec::new();
        if slide.hidden {
            flags.push("hidden");
        }
        if slide.has_notes {
            flags.push("notes");
        }
        if slide.pictures > 0 {
            flags.push("pictures");
        }
        if slide.tables > 0 {
            flags.push("tables");
        }
        if slide.objects > 0 {
            flags.push("objects");
        }
        if slide.has_comments {
            flags.push("comments");
        }
        let flags = match flags.is_empty() {
            true => String::new(),
            false => format!(" [{}]", flags.join(", ")),
        };
        match &slide.error {
            Some(err) => writeln!(out, "{:>4}  (unreadable: {err})", slide.number)?,
            None => writeln!(
                out,
                "{:>4}  {}{flags}",
                slide.number,
                slide.title.as_deref().unwrap_or("(no title)")
            )?,
        }
    }
    Ok(())
}
