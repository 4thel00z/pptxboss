//! PresentationML structure rules (ECMA-376 Part 1, clauses 13.3, 19.2,
//! 19.3, 19.7 and 20.1.2.2.8).

use std::collections::HashMap;

use pptxboss_core::mce::children;
use pptxboss_core::opc::Relationships;
use pptxboss_core::pml::{content_type, RelKind};
use pptxboss_core::presentation::Presentation;
use pptxboss_core::xml::{unescape_attr, Event, Ns, Reader};
use pptxboss_core::Package;

use crate::{Context, Finding, Rule, Severity, Sink};

pub const PML001: Rule = Rule {
    code: "PML001",
    severity: Severity::Error,
    clause: "Part 1 13.3.6",
    summary: "The Presentation part's root element is p:presentation",
};
pub const PML002: Rule = Rule {
    code: "PML002",
    severity: Severity::Error,
    clause: "Part 1 13.3.10",
    summary: "The presentation lists at least one slide master",
};
pub const PML003: Rule = Rule {
    code: "PML003",
    severity: Severity::Error,
    clause: "Part 1 13.3.7",
    summary: "Exactly one Presentation Properties part is related from the Presentation part",
};
pub const PML004: Rule = Rule {
    code: "PML004",
    severity: Severity::Error,
    clause: "Part 1 19.7.13",
    summary: "sldId/@id lies in [256, 2147483647]",
};
pub const PML005: Rule = Rule {
    code: "PML005",
    severity: Severity::Error,
    clause: "Part 1 19.2.1.33",
    summary: "sldId/@id values are unique",
};
pub const PML006: Rule = Rule {
    code: "PML006",
    severity: Severity::Error,
    clause: "Part 1 13.3.8",
    summary: "Every sldId/@r:id resolves to a Slide part",
};
pub const PML007: Rule = Rule {
    code: "PML007",
    severity: Severity::Error,
    clause: "Part 1 13.3.8",
    summary: "Slide relationship targets have the slide content type",
};
pub const PML008: Rule = Rule {
    code: "PML008",
    severity: Severity::Error,
    clause: "Part 1 19.7.16",
    summary: "sldMasterId/@id and sldLayoutId/@id are at least 2147483648",
};
pub const PML009: Rule = Rule {
    code: "PML009",
    severity: Severity::Error,
    clause: "Part 1 19.2.1.36",
    summary: "Master and layout ids are unique throughout the presentation",
};
pub const PML010: Rule = Rule {
    code: "PML010",
    severity: Severity::Error,
    clause: "Part 1 19.7.17",
    summary: "sldSz/@cx and @cy lie in [914400, 51206400]",
};
pub const PML011: Rule = Rule {
    code: "PML011",
    severity: Severity::Warning,
    clause: "Part 1 19.7.18",
    summary: "sldSz/@type agrees with the aspect ratio of cx and cy",
};
pub const PML012: Rule = Rule {
    code: "PML012",
    severity: Severity::Error,
    clause: "Part 1 19.2.1.22",
    summary: "The presentation carries a notesSz element",
};
pub const PML013: Rule = Rule {
    code: "PML013",
    severity: Severity::Warning,
    clause: "Part 1 13.3.4",
    summary: "A notes master relationship and notesMasterIdLst appear together",
};
pub const PML014: Rule = Rule {
    code: "PML014",
    severity: Severity::Error,
    clause: "Part 1 13.3",
    summary: "Each PresentationML part's root element matches its content type",
};
pub const PML015: Rule = Rule {
    code: "PML015",
    severity: Severity::Error,
    clause: "Part 1 13.3.9",
    summary: "Every slide has a slideLayout relationship",
};
pub const PML016: Rule = Rule {
    code: "PML016",
    severity: Severity::Error,
    clause: "Part 1 13.3.10",
    summary: "Every slide layout has a slideMaster relationship",
};
pub const PML017: Rule = Rule {
    code: "PML017",
    severity: Severity::Error,
    clause: "Part 1 13.3.9",
    summary: "Every sldLayoutId/@r:id in a master resolves to a Slide Layout part",
};
pub const PML018: Rule = Rule {
    code: "PML018",
    severity: Severity::Error,
    clause: "Part 1 13.3.4",
    summary: "Every notes slide has a notesMaster relationship",
};
pub const PML019: Rule = Rule {
    code: "PML019",
    severity: Severity::Error,
    clause: "Part 1 20.1.2.2.8",
    summary: "cNvPr/@id values are unique within a part",
};
pub const PML020: Rule = Rule {
    code: "PML020",
    severity: Severity::Error,
    clause: "Part 1 22.8.2.1",
    summary: "Every r:id, r:embed, r:link and diagram reference names a relationship of its part",
};
pub const PML021: Rule = Rule { code: "PML021", severity: Severity::Error, clause: "Part 1 13.3.6", summary: "sldMasterId, notesMasterId and handoutMasterId relationships resolve to parts of the right content type" };
pub const PML022: Rule = Rule {
    code: "PML022",
    severity: Severity::Info,
    clause: "Part 1 13.3.8",
    summary: "The presentation has at least one slide",
};
pub const PML023: Rule = Rule {
    code: "PML023",
    severity: Severity::Warning,
    clause: "Part 1 13.3",
    summary: "Slide-family parts relate only to the part kinds the specification lists",
};

pub static PRESENTATION_RULES: [Rule; 23] = [
    PML001, PML002, PML003, PML004, PML005, PML006, PML007, PML008, PML009, PML010, PML011, PML012,
    PML013, PML014, PML015, PML016, PML017, PML018, PML019, PML020, PML021, PML022, PML023,
];

const SLIDE_ID_MIN: u32 = 256;
const SLIDE_ID_MAX: u32 = 2_147_483_647;
const MASTER_ID_MIN: u32 = 2_147_483_648;
const SIZE_MIN: i64 = 914_400;
const SIZE_MAX: i64 = 51_206_400;

/// The root element a content type requires, when it is a slide-family or presentation part.
fn expected_root(ct: &str) -> Option<(Ns, &'static str)> {
    let ct = ct.split(';').next().unwrap_or("").trim();
    if content_type::is_presentation_main(ct) {
        return Some((Ns::Pml, "presentation"));
    }
    let table = [
        (content_type::SLIDE, Ns::Pml, "sld"),
        (content_type::SLIDE_LAYOUT, Ns::Pml, "sldLayout"),
        (content_type::SLIDE_MASTER, Ns::Pml, "sldMaster"),
        (content_type::NOTES_SLIDE, Ns::Pml, "notes"),
        (content_type::NOTES_MASTER, Ns::Pml, "notesMaster"),
        (content_type::HANDOUT_MASTER, Ns::Pml, "handoutMaster"),
        (content_type::PRES_PROPS, Ns::Pml, "presentationPr"),
        (content_type::VIEW_PROPS, Ns::Pml, "viewPr"),
        (content_type::TABLE_STYLES, Ns::Dml, "tblStyleLst"),
        (content_type::COMMENT_AUTHORS, Ns::Pml, "cmAuthorLst"),
        (content_type::COMMENTS, Ns::Pml, "cmLst"),
        (content_type::THEME, Ns::Dml, "theme"),
    ];
    table
        .iter()
        .find(|(known, _, _)| known.eq_ignore_ascii_case(ct))
        .map(|(_, ns, root)| (*ns, *root))
}

pub fn run(ctx: &Context<'_>, sink: &mut Sink<'_>) {
    let package = ctx.package;
    check_presentation(package, sink);
    for part in package.parts() {
        let Some(ct) = package.content_type_of(&part.name) else {
            continue;
        };
        let Some((ns, root)) = expected_root(ct) else {
            continue;
        };
        check_part(package, &part.name, ns, root, sink);
    }
}

fn presentation_part(package: &Package) -> Option<String> {
    let rels = package.package_rels().ok()?;
    let found = rels
        .iter()
        .filter(|rel| RelKind::of(&rel.rel_type) == RelKind::OfficeDocument)
        .find_map(|rel| rels.resolve(rel))
        .filter(|part| package.has_part(part));
    found
}

fn check_presentation(package: &Package, sink: &mut Sink<'_>) {
    let Some(part) = presentation_part(package) else {
        return;
    };
    let Ok(data) = package.read_part(&part) else {
        return;
    };
    sink.part_read();
    let presentation = match Presentation::parse(&data) {
        Ok(presentation) => presentation,
        Err(_) => return,
    };
    if !presentation.root_ok {
        sink.push(Finding::new(&PML001, "root element is not p:presentation").in_part(&part));
        return;
    }
    let Ok(rels) = package.rels(&part) else {
        return;
    };
    for defect in &presentation.defects {
        sink.push(
            Finding::new(&PML006, defect.msg)
                .in_part(&part)
                .at(format!("byte {}", defect.offset)),
        );
    }
    if presentation.masters.is_empty() {
        sink.push(Finding::new(&PML002, "sldMasterIdLst lists no slide master").in_part(&part));
    }
    if presentation.slides.is_empty() {
        sink.push(Finding::new(&PML022, "the presentation has no slides").in_part(&part));
    }
    if presentation.notes_size.is_none() {
        sink.push(Finding::new(&PML012, "notesSz is missing").in_part(&part));
    }
    let pres_props = rels
        .iter()
        .filter(|rel| RelKind::of(&rel.rel_type) == RelKind::PresProps)
        .filter_map(|rel| rels.resolve(rel))
        .filter(|target| package.has_part(target))
        .count();
    if pres_props != 1 {
        sink.push(
            Finding::new(&PML003, format!("{pres_props} presProps relationship(s)")).in_part(&part),
        );
    }
    if let Some(size) = &presentation.slide_size {
        for (axis, value) in [("cx", size.cx), ("cy", size.cy)] {
            if !(SIZE_MIN..=SIZE_MAX).contains(&value) {
                sink.push(
                    Finding::new(&PML010, format!("sldSz/@{axis} is {value}")).in_part(&part),
                );
            }
        }
        if let Some(kind) = &size.kind {
            let expected: Option<(i64, i64)> = match kind.as_str() {
                "screen4x3" => Some((4, 3)),
                "screen16x9" => Some((16, 9)),
                "screen16x10" => Some((16, 10)),
                _ => None,
            };
            if let Some((w, h)) = expected {
                let ratio = size.cx as f64 / size.cy.max(1) as f64;
                let want = w as f64 / h as f64;
                if (ratio - want).abs() > 0.01 {
                    sink.push(
                        Finding::new(
                            &PML011,
                            format!("sldSz/@type is {kind} but cx:cy is {}:{}", size.cx, size.cy),
                        )
                        .in_part(&part),
                    );
                }
            }
        }
    }
    let mut seen_slide_ids: HashMap<u32, usize> = HashMap::new();
    for slide in &presentation.slides {
        let location = format!("byte {}", slide.offset);
        if let Some(id) = slide.id {
            if !(SLIDE_ID_MIN..=SLIDE_ID_MAX).contains(&id) {
                sink.push(
                    Finding::new(&PML004, format!("sldId/@id is {id}"))
                        .in_part(&part)
                        .at(&location),
                );
            }
            if let Some(first) = seen_slide_ids.insert(id, slide.offset) {
                sink.push(
                    Finding::new(
                        &PML005,
                        format!("sldId/@id {id} repeats the id at byte {first}"),
                    )
                    .in_part(&part)
                    .at(&location),
                );
            }
        } else {
            sink.push(
                Finding::new(&PML004, "sldId has no numeric id")
                    .in_part(&part)
                    .at(&location),
            );
        }
        match rels.get(&slide.rel_id) {
            None => sink.push(
                Finding::new(
                    &PML006,
                    format!(
                        "sldId r:id {} is not a relationship of the presentation",
                        slide.rel_id
                    ),
                )
                .in_part(&part)
                .at(&location),
            ),
            Some(rel) => match rels.resolve(rel).filter(|target| package.has_part(target)) {
                None => sink.push(
                    Finding::new(
                        &PML006,
                        format!(
                            "sldId r:id {} targets {}, which does not exist",
                            slide.rel_id, rel.target
                        ),
                    )
                    .in_part(&part)
                    .at(&location),
                ),
                Some(target) => {
                    if RelKind::of(&rel.rel_type) != RelKind::Slide {
                        sink.push(
                            Finding::new(
                                &PML007,
                                format!("relationship {} is not of the slide type", slide.rel_id),
                            )
                            .in_part(&part)
                            .at(&location),
                        );
                    }
                    match package.content_type_of(&target) {
                        Some(ct) if ct.eq_ignore_ascii_case(content_type::SLIDE) => {}
                        Some(ct) => sink.push(
                            Finding::new(&PML007, format!("content type is {ct}")).in_part(&target),
                        ),
                        None => {}
                    }
                }
            },
        }
    }
    let mut seen_master_ids: HashMap<u32, usize> = HashMap::new();
    for master in &presentation.masters {
        let location = format!("byte {}", master.offset);
        if let Some(id) = master.id {
            if id < MASTER_ID_MIN {
                sink.push(
                    Finding::new(&PML008, format!("sldMasterId/@id is {id}"))
                        .in_part(&part)
                        .at(&location),
                );
            }
            if let Some(first) = seen_master_ids.insert(id, master.offset) {
                sink.push(
                    Finding::new(
                        &PML009,
                        format!("sldMasterId/@id {id} repeats the id at byte {first}"),
                    )
                    .in_part(&part)
                    .at(&location),
                );
            }
        }
        check_typed_target(
            package,
            &rels,
            &part,
            &master.rel_id,
            RelKind::SlideMaster,
            content_type::SLIDE_MASTER,
            "sldMasterId",
            &location,
            sink,
            &mut seen_master_ids,
        );
    }
    if let Some(rel_id) = &presentation.notes_master {
        check_typed_target(
            package,
            &rels,
            &part,
            rel_id,
            RelKind::NotesMaster,
            content_type::NOTES_MASTER,
            "notesMasterId",
            "notesMasterIdLst",
            sink,
            &mut HashMap::new(),
        );
    }
    if let Some(rel_id) = &presentation.handout_master {
        check_typed_target(
            package,
            &rels,
            &part,
            rel_id,
            RelKind::HandoutMaster,
            content_type::HANDOUT_MASTER,
            "handoutMasterId",
            "handoutMasterIdLst",
            sink,
            &mut HashMap::new(),
        );
    }
    let notes_master_rels = rels
        .iter()
        .filter(|rel| RelKind::of(&rel.rel_type) == RelKind::NotesMaster)
        .count();
    if notes_master_rels > 0 && presentation.notes_master.is_none() {
        sink.push(
            Finding::new(
                &PML013,
                "a notesMaster relationship exists but notesMasterIdLst is absent",
            )
            .in_part(&part),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn check_typed_target(
    package: &Package,
    rels: &Relationships,
    part: &str,
    rel_id: &str,
    kind: RelKind,
    expected_ct: &str,
    element: &str,
    location: &str,
    sink: &mut Sink<'_>,
    _seen: &mut HashMap<u32, usize>,
) {
    match rels.get(rel_id) {
        None => sink.push(
            Finding::new(
                &PML021,
                format!("{element} r:id {rel_id} is not a relationship of the presentation"),
            )
            .in_part(part)
            .at(location),
        ),
        Some(rel) => match rels.resolve(rel).filter(|target| package.has_part(target)) {
            None => sink.push(
                Finding::new(
                    &PML021,
                    format!(
                        "{element} r:id {rel_id} targets {}, which does not exist",
                        rel.target
                    ),
                )
                .in_part(part)
                .at(location),
            ),
            Some(target) => {
                if RelKind::of(&rel.rel_type) != kind {
                    sink.push(
                        Finding::new(
                            &PML021,
                            format!("{element} relationship {rel_id} has type {}", rel.rel_type),
                        )
                        .in_part(part)
                        .at(location),
                    );
                }
                if let Some(ct) = package.content_type_of(&target) {
                    if !ct.eq_ignore_ascii_case(expected_ct) {
                        sink.push(
                            Finding::new(
                                &PML021,
                                format!("{element} target has content type {ct}"),
                            )
                            .in_part(&target),
                        );
                    }
                }
            }
        },
    }
}

fn check_part(
    package: &Package,
    part: &str,
    expected_ns: Ns,
    expected_root: &str,
    sink: &mut Sink<'_>,
) {
    let Ok(data) = package.read_part(part) else {
        return;
    };
    sink.part_read();
    let mut reader = Reader::new(&data);
    let root = loop {
        match reader.next() {
            Ok(Event::Start(start)) => break start,
            Ok(Event::Eof) | Err(_) => return,
            _ => {}
        }
    };
    if root.name.ns != expected_ns || root.name.local != expected_root.as_bytes() {
        let prefix = match expected_ns {
            Ns::Dml => "a",
            _ => "p",
        };
        sink.push(
            Finding::new(
                &PML014,
                format!(
                    "root element is {}, expected {prefix}:{expected_root}",
                    root.name.qualified()
                ),
            )
            .in_part(part),
        );
    }
    if expected_ns != Ns::Pml {
        return;
    }
    let rels = package.rels(part).ok();
    let mut ids: HashMap<u32, usize> = HashMap::new();
    let mut findings: Vec<Finding> = Vec::new();
    let mut layout_ids: Vec<(String, usize)> = Vec::new();
    let mut master_id_values: Vec<(u32, usize)> = Vec::new();
    let mut walk = |reader: &mut Reader<'_>,
                    start: pptxboss_core::xml::Start<'_>|
     -> Result<(), pptxboss_core::xml::XmlError> {
        if start.name.is(Ns::Pml, b"cNvPr") {
            if let Some(id) = reader
                .attr(&start, Ns::None, b"id")
                .and_then(|v| std::str::from_utf8(v).ok()?.trim().parse::<u32>().ok())
            {
                if let Some(first) = ids.insert(id, start.offset) {
                    findings.push(
                        Finding::new(
                            &PML019,
                            format!("cNvPr/@id {id} repeats the id at byte {first}"),
                        )
                        .in_part(part)
                        .at(format!("byte {}", start.offset)),
                    );
                }
            }
        }
        if start.name.is(Ns::Pml, b"sldLayoutId") {
            if let Some(rel_id) = reader.attr(&start, Ns::Rel, b"id") {
                layout_ids.push((unescape_attr(rel_id), start.offset));
            }
            if let Some(id) = reader
                .attr(&start, Ns::None, b"id")
                .and_then(|v| std::str::from_utf8(v).ok()?.trim().parse::<u32>().ok())
            {
                master_id_values.push((id, start.offset));
            }
        }
        for attr in reader.attrs(&start) {
            if attr.name.ns != Ns::Rel || attr.raw_value.is_empty() {
                continue;
            }
            let rel_id = unescape_attr(attr.raw_value);
            let resolves = rels
                .as_ref()
                .is_some_and(|rels| rels.get(&rel_id).is_some());
            if !resolves {
                findings.push(
                    Finding::new(
                        &PML020,
                        format!(
                            "r:{} = {rel_id} names no relationship of this part",
                            String::from_utf8_lossy(attr.name.local)
                        ),
                    )
                    .in_part(part)
                    .at(format!("byte {}", start.offset)),
                );
            }
        }
        Ok(())
    };
    let _ = walk_all(&mut reader, &mut walk);
    for finding in findings {
        sink.push(finding);
    }
    for (id, offset) in &master_id_values {
        if *id < MASTER_ID_MIN {
            sink.push(
                Finding::new(&PML008, format!("sldLayoutId/@id is {id}"))
                    .in_part(part)
                    .at(format!("byte {offset}")),
            );
        }
    }
    let Some(rels) = rels else {
        return;
    };
    let has_kind = |kind: RelKind| rels.iter().any(|rel| RelKind::of(&rel.rel_type) == kind);
    match expected_root {
        "sld" if !has_kind(RelKind::SlideLayout) => {
            sink.push(Finding::new(&PML015, "slide has no slideLayout relationship").in_part(part));
        }
        "sldLayout" if !has_kind(RelKind::SlideMaster) => {
            sink.push(
                Finding::new(&PML016, "slide layout has no slideMaster relationship").in_part(part),
            );
        }
        "notes" if !has_kind(RelKind::NotesMaster) => {
            sink.push(
                Finding::new(&PML018, "notes slide has no notesMaster relationship").in_part(part),
            );
        }
        "sldMaster" => {
            for (rel_id, offset) in &layout_ids {
                let target = rels
                    .get(rel_id)
                    .and_then(|rel| rels.resolve(rel))
                    .filter(|target| package.has_part(target));
                match target {
                    None => sink.push(
                        Finding::new(
                            &PML017,
                            format!("sldLayoutId r:id {rel_id} does not resolve to a part"),
                        )
                        .in_part(part)
                        .at(format!("byte {offset}")),
                    ),
                    Some(target) => {
                        if let Some(ct) = package.content_type_of(&target) {
                            if !ct.eq_ignore_ascii_case(content_type::SLIDE_LAYOUT) {
                                sink.push(
                                    Finding::new(
                                        &PML017,
                                        format!("sldLayoutId target has content type {ct}"),
                                    )
                                    .in_part(&target),
                                );
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    if matches!(
        expected_root,
        "sld" | "sldLayout" | "sldMaster" | "notes" | "notesMaster" | "handoutMaster"
    ) {
        for rel in rels.iter() {
            let kind = RelKind::of(&rel.rel_type);
            let allowed = !matches!(
                kind,
                RelKind::OfficeDocument
                    | RelKind::PresProps
                    | RelKind::ViewProps
                    | RelKind::TableStyles
                    | RelKind::CoreProperties
                    | RelKind::ExtendedProperties
                    | RelKind::CustomProperties
            );
            if !allowed {
                sink.push(
                    Finding::new(
                        &PML023,
                        format!("relationship {} has type {}", rel.id, rel.rel_type),
                    )
                    .in_part(pptxboss_core::opc::rels_part_name(part)),
                );
            }
        }
    }
}

fn walk_all<'a>(
    reader: &mut Reader<'a>,
    f: &mut dyn FnMut(
        &mut Reader<'a>,
        pptxboss_core::xml::Start<'a>,
    ) -> Result<(), pptxboss_core::xml::XmlError>,
) -> Result<(), pptxboss_core::xml::XmlError> {
    children(reader, &mut |reader, child| {
        f(reader, child)?;
        walk_all(reader, f)
    })
}
