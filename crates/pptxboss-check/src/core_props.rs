//! Core properties rules (ECMA-376 Part 2, clause 8).

use pptxboss_core::mce::children;
use pptxboss_core::pml::RelKind;
use pptxboss_core::xml::{Event, Ns, Reader};

use crate::{Context, Finding, Rule, Severity, Sink};

pub const CPR001: Rule = Rule {
    code: "CPR001",
    severity: Severity::Error,
    clause: "Part 2 8.3.3",
    summary: "The core properties root is cp:coreProperties without attributes",
};
pub const CPR002: Rule = Rule {
    code: "CPR002",
    severity: Severity::Error,
    clause: "Part 2 8.3.4.3",
    summary: "dcterms:created and dcterms:modified carry xsi:type=\"dcterms:W3CDTF\"",
};
pub const CPR003: Rule = Rule {
    code: "CPR003",
    severity: Severity::Error,
    clause: "Part 2 8.3.4.1",
    summary: "Core property elements are not repeated",
};
pub const CPR004: Rule = Rule {
    code: "CPR004",
    severity: Severity::Warning,
    clause: "Part 2 C.3",
    summary: "Only the fifteen core property elements appear, in their namespaces",
};

pub static CORE_PROPERTY_RULES: [Rule; 4] = [CPR001, CPR002, CPR003, CPR004];

const CP_ELEMENTS: &[&[u8]] = &[
    b"category",
    b"contentStatus",
    b"keywords",
    b"lastModifiedBy",
    b"lastPrinted",
    b"revision",
    b"version",
];
const DC_ELEMENTS: &[&[u8]] = &[
    b"creator",
    b"description",
    b"identifier",
    b"language",
    b"subject",
    b"title",
];
const DCTERMS_ELEMENTS: &[&[u8]] = &[b"created", b"modified"];

pub fn run(ctx: &Context<'_>, sink: &mut Sink<'_>) {
    let package = ctx.package;
    let Ok(rels) = package.package_rels() else {
        return;
    };
    let found = rels
        .iter()
        .filter(|rel| RelKind::of(&rel.rel_type) == RelKind::CoreProperties)
        .find_map(|rel| rels.resolve(rel))
        .filter(|part| package.has_part(part));
    let Some(part) = found else {
        return;
    };
    let Ok(data) = package.read_part(&part) else {
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
    if !root.name.is(Ns::Cp, b"coreProperties") {
        sink.push(
            Finding::new(
                &CPR001,
                format!("root element is {}", root.name.qualified()),
            )
            .in_part(&part),
        );
        return;
    }
    if reader
        .attrs(&root)
        .any(|attr| attr.name.prefix != b"xmlns" && attr.name.local != b"xmlns")
    {
        sink.push(Finding::new(&CPR001, "root element has attributes").in_part(&part));
    }
    let mut seen: Vec<String> = Vec::new();
    let mut findings = Vec::new();
    let _ = children(&mut reader, &mut |reader, child| {
        let known = match child.name.ns {
            Ns::Cp => CP_ELEMENTS.contains(&child.name.local),
            Ns::Dc => DC_ELEMENTS.contains(&child.name.local),
            Ns::Dcterms => DCTERMS_ELEMENTS.contains(&child.name.local),
            _ => false,
        };
        let qualified = child.name.qualified();
        if !known {
            findings.push(
                Finding::new(&CPR004, format!("unexpected element {qualified}"))
                    .in_part(&part)
                    .at(format!("byte {}", child.offset)),
            );
            return Ok(());
        }
        if seen.contains(&qualified) {
            findings.push(
                Finding::new(&CPR003, format!("{qualified} appears more than once"))
                    .in_part(&part)
                    .at(format!("byte {}", child.offset)),
            );
        }
        seen.push(qualified.clone());
        if child.name.ns == Ns::Dcterms {
            let ok = reader
                .attr(&child, Ns::Xsi, b"type")
                .is_some_and(|value| value == b"dcterms:W3CDTF");
            if !ok {
                findings.push(
                    Finding::new(
                        &CPR002,
                        format!("{qualified} lacks xsi:type=\"dcterms:W3CDTF\""),
                    )
                    .in_part(&part)
                    .at(format!("byte {}", child.offset)),
                );
            }
        }
        Ok(())
    });
    for finding in findings {
        sink.push(finding);
    }
}
