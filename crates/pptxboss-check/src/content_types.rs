//! Content types stream rules (ECMA-376 Part 2, 6.2.3 and 7.2.3).

use pptxboss_core::opc::{is_rels_part, RELATIONSHIPS_CONTENT_TYPE};

use crate::{Context, Finding, Rule, Severity, Sink};

pub const CTY001: Rule = Rule {
    code: "CTY001",
    severity: Severity::Error,
    clause: "Part 2 7.2.3.2.1",
    summary: "Every part other than a Relationships part has a content type",
};
pub const CTY002: Rule = Rule {
    code: "CTY002",
    severity: Severity::Error,
    clause: "Part 2 7.2.3.2.1",
    summary: "At most one Default element per extension",
};
pub const CTY003: Rule = Rule {
    code: "CTY003",
    severity: Severity::Error,
    clause: "Part 2 7.2.3.2.1",
    summary: "At most one Override element per part name",
};
pub const CTY004: Rule = Rule {
    code: "CTY004",
    severity: Severity::Warning,
    clause: "Part 2 7.2.3.2.5",
    summary: "Every Override names a part that exists",
};
pub const CTY005: Rule = Rule { code: "CTY005", severity: Severity::Error, clause: "Part 2 7.2.3.2.1", summary: "The content types stream holds only Default and Override elements in the content types namespace" };
pub const CTY006: Rule = Rule {
    code: "CTY006",
    severity: Severity::Error,
    clause: "Part 2 6.2.3",
    summary: "Content types are media types of the form type/subtype",
};
pub const CTY007: Rule = Rule {
    code: "CTY007",
    severity: Severity::Error,
    clause: "Part 2 6.5.2.1",
    summary: "Relationships parts have the relationships media type",
};
pub const CTY008: Rule = Rule {
    code: "CTY008",
    severity: Severity::Error,
    clause: "Part 2 7.2.3.2.4",
    summary: "Default extensions contain no dot or slash",
};

pub static CONTENT_TYPE_RULES: [Rule; 8] = [
    CTY001, CTY002, CTY003, CTY004, CTY005, CTY006, CTY007, CTY008,
];

fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// `type/subtype` optionally followed by `;` parameters (RFC 7231 3.1.1.1).
pub fn is_media_type(text: &str) -> bool {
    let main = text.split(';').next().unwrap_or("").trim();
    let Some((kind, subtype)) = main.split_once('/') else {
        return false;
    };
    is_token(kind) && is_token(subtype)
}

pub fn run(ctx: &Context<'_>, sink: &mut Sink<'_>) {
    let package = ctx.package;
    let types = package.content_types();
    if package.defects().content_types_missing
        || package.defects().content_types_error.is_some()
        || package.defects().content_types_unreadable.is_some()
    {
        return;
    }
    if types.wrong_namespace {
        sink.push(
            Finding::new(
                &CTY005,
                "root element is not Types in the content types namespace",
            )
            .in_part("/[Content_Types].xml"),
        );
    }
    for defect in &types.defects {
        sink.push(
            Finding::new(&CTY005, defect.msg)
                .in_part("/[Content_Types].xml")
                .at(format!("byte {}", defect.offset)),
        );
    }
    for &index in &types.duplicate_defaults {
        let (extension, _) = &types.defaults()[index];
        sink.push(
            Finding::new(&CTY002, format!("second Default for extension {extension}"))
                .in_part("/[Content_Types].xml"),
        );
    }
    for &index in &types.duplicate_overrides {
        let (part_name, _) = &types.overrides()[index];
        sink.push(
            Finding::new(&CTY003, format!("second Override for {part_name}"))
                .in_part("/[Content_Types].xml"),
        );
    }
    for (extension, content_type) in types.defaults() {
        if extension.contains('.') || extension.contains('/') || extension.is_empty() {
            sink.push(
                Finding::new(&CTY008, format!("Default extension {extension:?}"))
                    .in_part("/[Content_Types].xml"),
            );
        }
        if !is_media_type(content_type) {
            sink.push(
                Finding::new(
                    &CTY006,
                    format!("Default for {extension} has content type {content_type:?}"),
                )
                .in_part("/[Content_Types].xml"),
            );
        }
        if extension.eq_ignore_ascii_case("rels")
            && !content_type.eq_ignore_ascii_case(RELATIONSHIPS_CONTENT_TYPE)
        {
            sink.push(
                Finding::new(&CTY007, format!("Default for rels is {content_type}"))
                    .in_part("/[Content_Types].xml"),
            );
        }
    }
    for (part_name, content_type) in types.overrides() {
        if !is_media_type(content_type) {
            sink.push(
                Finding::new(
                    &CTY006,
                    format!("Override has content type {content_type:?}"),
                )
                .in_part(part_name),
            );
        }
        if !package.has_part(part_name) {
            sink.push(
                Finding::new(&CTY004, "Override names a part that does not exist")
                    .in_part(part_name),
            );
        }
        if is_rels_part(part_name) && !content_type.eq_ignore_ascii_case(RELATIONSHIPS_CONTENT_TYPE)
        {
            sink.push(
                Finding::new(
                    &CTY007,
                    format!("Relationships part declared as {content_type}"),
                )
                .in_part(part_name),
            );
        }
    }
    for part in package.parts() {
        if is_rels_part(&part.name) {
            continue;
        }
        if package.content_type_of(&part.name).is_none() {
            sink.push(
                Finding::new(
                    &CTY001,
                    "no Default or Override gives this part a content type",
                )
                .in_part(&part.name),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_media_type;

    #[test]
    fn media_type_syntax() {
        assert!(is_media_type("application/xml"));
        assert!(is_media_type(
            "application/vnd.openxmlformats-officedocument.presentationml.slide+xml"
        ));
        assert!(is_media_type("text/plain; charset=utf-8"));
        assert!(!is_media_type(""));
        assert!(!is_media_type("application"));
        assert!(!is_media_type("application/"));
        assert!(!is_media_type("app lication/xml"));
    }
}
