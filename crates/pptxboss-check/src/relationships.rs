//! Relationships rules (ECMA-376 Part 2, 6.5) and the package-level
//! relationship graph (8.2, 15.2.16, Part 1 13.2 and 13.3.6).

use std::collections::HashSet;

use pptxboss_core::opc::{
    equivalence_key, is_rels_part, source_of_rels, TargetMode, CORE_PROPERTIES_CONTENT_TYPE,
};
use pptxboss_core::pml::{content_type, RelKind};
use pptxboss_core::xml::is_ncname;
use pptxboss_core::Package;

use crate::{Context, Finding, Rule, Severity, Sink};

pub const REL001: Rule = Rule {
    code: "REL001",
    severity: Severity::Error,
    clause: "Part 2 6.5.3.1",
    summary: "Relationships parts are well-formed XML with a Relationships root",
};
pub const REL002: Rule = Rule {
    code: "REL002",
    severity: Severity::Error,
    clause: "Part 2 6.5.3.4",
    summary: "Relationship Ids are unique within their Relationships part",
};
pub const REL003: Rule = Rule {
    code: "REL003",
    severity: Severity::Error,
    clause: "Part 2 C.5",
    summary: "Every Relationship carries Id, Type and Target, and a valid TargetMode",
};
pub const REL004: Rule = Rule {
    code: "REL004",
    severity: Severity::Error,
    clause: "Part 2 6.5.3.4",
    summary: "Internal relationship targets resolve to a part in the package",
};
pub const REL005: Rule = Rule {
    code: "REL005",
    severity: Severity::Error,
    clause: "Part 2 6.5.2.1",
    summary: "No relationship points at a Relationships part",
};
pub const REL006: Rule = Rule {
    code: "REL006",
    severity: Severity::Error,
    clause: "Part 2 6.5.3.4",
    summary: "Relationship Ids are XML names (xsd:ID)",
};
pub const REL007: Rule = Rule {
    code: "REL007",
    severity: Severity::Warning,
    clause: "Part 2 6.5.2.3",
    summary: "Every Relationships part belongs to a part that exists",
};
pub const REL008: Rule = Rule {
    code: "REL008",
    severity: Severity::Warning,
    clause: "Part 2 6.5.1",
    summary: "Every part is reachable from the package through relationships",
};
pub const REL009: Rule = Rule {
    code: "REL009",
    severity: Severity::Error,
    clause: "Part 2 6.5.3.4",
    summary: "Internal targets are relative references, not absolute IRIs",
};
pub const REL010: Rule = Rule {
    code: "REL010",
    severity: Severity::Error,
    clause: "Part 2 6.5.3.4",
    summary: "Relationship types are non-empty IRIs",
};
pub const PKG001: Rule = Rule {
    code: "PKG001",
    severity: Severity::Error,
    clause: "Part 1 13.3.6",
    summary: "The package relationships name exactly one Presentation part",
};
pub const PKG002: Rule = Rule {
    code: "PKG002",
    severity: Severity::Error,
    clause: "Part 1 13.3.6",
    summary: "The Presentation part has a presentation main content type",
};
pub const PKG003: Rule = Rule { code: "PKG003", severity: Severity::Error, clause: "Part 2 8.2", summary: "At most one core properties relationship, from the package, to a part with the core properties media type" };
pub const PKG004: Rule = Rule {
    code: "PKG004",
    severity: Severity::Error,
    clause: "Part 1 15.2.16",
    summary: "At most one thumbnail relationship from the package and per part",
};
pub const PKG005: Rule = Rule {
    code: "PKG005",
    severity: Severity::Error,
    clause: "Part 1 15.2.12.3",
    summary:
        "At most one extended properties and one custom properties relationship from the package",
};

pub static RELATIONSHIP_RULES: [Rule; 15] = [
    REL001, REL002, REL003, REL004, REL005, REL006, REL007, REL008, REL009, REL010, PKG001, PKG002,
    PKG003, PKG004, PKG005,
];

fn has_scheme(target: &str) -> bool {
    let Some(colon) = target.find(':') else {
        return false;
    };
    let scheme = &target[..colon];
    !scheme.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && !target[..colon].contains('/')
}

pub fn run(ctx: &Context<'_>, sink: &mut Sink<'_>) {
    let package = ctx.package;
    let mut reachable: HashSet<String> = HashSet::new();
    let mut sources: Vec<String> = vec!["/".to_string()];
    for part in package.parts() {
        if is_rels_part(&part.name) {
            match source_of_rels(&part.name) {
                Some(source) if source == "/" || package.has_part(&source) => {}
                _ => sink.push(
                    Finding::new(&REL007, "Relationships part for a part that does not exist")
                        .in_part(&part.name),
                ),
            }
            continue;
        }
        sources.push(part.name.clone());
    }
    for source in &sources {
        check_rels(package, source, &mut reachable, sink);
    }
    for part in package.parts() {
        if is_rels_part(&part.name) {
            continue;
        }
        if !reachable.contains(&equivalence_key(&part.name)) {
            sink.push(
                Finding::new(&REL008, "part is not the target of any relationship")
                    .in_part(&part.name),
            );
        }
    }
    package_graph(package, sink);
}

fn check_rels(
    package: &Package,
    source: &str,
    reachable: &mut HashSet<String>,
    sink: &mut Sink<'_>,
) {
    let rels_name = pptxboss_core::opc::rels_part_name(source);
    let rels = match package.rels(source) {
        Ok(rels) => rels,
        Err(err) => {
            sink.push(Finding::new(&REL001, err.to_string()).in_part(&rels_name));
            return;
        }
    };
    if package.has_part(&rels_name) {
        sink.part_read();
    }
    if rels.wrong_namespace {
        sink.push(
            Finding::new(
                &REL001,
                "root element is not Relationships in the relationships namespace",
            )
            .in_part(&rels_name),
        );
    }
    for defect in &rels.defects {
        sink.push(
            Finding::new(&REL003, defect.msg)
                .in_part(&rels_name)
                .at(format!("byte {}", defect.offset)),
        );
    }
    let items: Vec<_> = rels.iter().collect();
    for &index in &rels.duplicate_ids {
        if let Some(rel) = items.get(index) {
            sink.push(
                Finding::new(&REL002, format!("Id {} is declared more than once", rel.id))
                    .in_part(&rels_name)
                    .at(format!("byte {}", rel.offset)),
            );
        }
    }
    let mut thumbnails = 0;
    for rel in &items {
        if !is_ncname(rel.id.as_bytes()) {
            sink.push(
                Finding::new(&REL006, format!("Id {:?} is not an XML name", rel.id))
                    .in_part(&rels_name)
                    .at(format!("byte {}", rel.offset)),
            );
        }
        if rel.rel_type.trim().is_empty() {
            sink.push(
                Finding::new(
                    &REL010,
                    format!("relationship {} has an empty Type", rel.id),
                )
                .in_part(&rels_name),
            );
        }
        if RelKind::of(&rel.rel_type) == RelKind::Thumbnail {
            thumbnails += 1;
        }
        if rel.mode == TargetMode::External {
            continue;
        }
        if has_scheme(&rel.target) {
            sink.push(
                Finding::new(
                    &REL009,
                    format!(
                        "relationship {} has Internal mode but an absolute target {}",
                        rel.id, rel.target
                    ),
                )
                .in_part(&rels_name),
            );
            continue;
        }
        let Some(target) = rels.resolve(rel) else {
            continue;
        };
        if is_rels_part(&target) {
            sink.push(
                Finding::new(
                    &REL005,
                    format!(
                        "relationship {} targets the Relationships part {target}",
                        rel.id
                    ),
                )
                .in_part(&rels_name),
            );
            continue;
        }
        match package.has_part(&target) {
            true => {
                reachable.insert(equivalence_key(&target));
            }
            false => sink.push(
                Finding::new(
                    &REL004,
                    format!(
                        "relationship {} ({}) targets {target}, which does not exist",
                        rel.id,
                        short_type(&rel.rel_type)
                    ),
                )
                .in_part(&rels_name),
            ),
        }
    }
    if thumbnails > 1 {
        sink.push(
            Finding::new(&PKG004, format!("{thumbnails} thumbnail relationships"))
                .in_part(&rels_name),
        );
    }
}

fn short_type(rel_type: &str) -> &str {
    rel_type.rsplit('/').next().unwrap_or(rel_type)
}

fn package_graph(package: &Package, sink: &mut Sink<'_>) {
    let Ok(rels) = package.package_rels() else {
        return;
    };
    let office: Vec<_> = rels
        .iter()
        .filter(|rel| RelKind::of(&rel.rel_type) == RelKind::OfficeDocument)
        .collect();
    match office.len() {
        0 => sink.push(Finding::new(
            &PKG001,
            "no officeDocument relationship in /_rels/.rels",
        )),
        1 => {}
        n => sink.push(
            Finding::new(&PKG001, format!("{n} officeDocument relationships"))
                .in_part("/_rels/.rels"),
        ),
    }
    for rel in &office {
        let Some(target) = rels.resolve(rel).filter(|target| package.has_part(target)) else {
            continue;
        };
        match package.content_type_of(&target) {
            Some(ct) if content_type::is_presentation_main(ct) => {}
            Some(ct) => {
                sink.push(Finding::new(&PKG002, format!("content type is {ct}")).in_part(&target))
            }
            None => {}
        }
    }
    let core: Vec<_> = rels
        .iter()
        .filter(|rel| RelKind::of(&rel.rel_type) == RelKind::CoreProperties)
        .collect();
    if core.len() > 1 {
        sink.push(
            Finding::new(
                &PKG003,
                format!("{} core properties relationships", core.len()),
            )
            .in_part("/_rels/.rels"),
        );
    }
    for rel in &core {
        let Some(target) = rels.resolve(rel).filter(|target| package.has_part(target)) else {
            continue;
        };
        match package.content_type_of(&target) {
            Some(ct) if ct.eq_ignore_ascii_case(CORE_PROPERTIES_CONTENT_TYPE) => {}
            Some(ct) => sink.push(
                Finding::new(
                    &PKG003,
                    format!("core properties part has content type {ct}"),
                )
                .in_part(&target),
            ),
            None => {}
        }
    }
    for (kind, label) in [
        (RelKind::ExtendedProperties, "extended properties"),
        (RelKind::CustomProperties, "custom properties"),
    ] {
        let count = rels
            .iter()
            .filter(|rel| RelKind::of(&rel.rel_type) == kind)
            .count();
        if count > 1 {
            sink.push(
                Finding::new(&PKG005, format!("{count} {label} relationships"))
                    .in_part("/_rels/.rels"),
            );
        }
    }
    for part in package.parts() {
        if is_rels_part(&part.name) || part.name == "/" {
            continue;
        }
        let Ok(part_rels) = package.rels(&part.name) else {
            continue;
        };
        if part_rels
            .iter()
            .filter(|rel| RelKind::of(&rel.rel_type) == RelKind::CoreProperties)
            .count()
            > 0
        {
            sink.push(
                Finding::new(
                    &PKG003,
                    "core properties relationship from a part instead of the package",
                )
                .in_part(pptxboss_core::opc::rels_part_name(&part.name)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::has_scheme;

    #[test]
    fn scheme_detection() {
        assert!(has_scheme("http://example.com/x"));
        assert!(has_scheme("file:///tmp/x"));
        assert!(!has_scheme("../media/image1.png"));
        assert!(!has_scheme("slides/slide1.xml"));
        assert!(!has_scheme("a/b:c"));
    }
}
