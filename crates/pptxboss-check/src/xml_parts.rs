//! XML rules for every XML part (ECMA-376 Part 2, 6.2.5; Part 4, clause 7).

use pptxboss_core::encoding::utf16_xml_to_utf8;
use pptxboss_core::opc::is_rels_part;
use pptxboss_core::xml::{well_formed, Event, Reader};

use crate::{Context, Finding, Rule, Severity, Sink};

pub const XML001: Rule = Rule {
    code: "XML001",
    severity: Severity::Error,
    clause: "Part 2 6.2.5",
    summary: "XML parts are well-formed, namespace-well-formed and carry no DTD",
};
pub const XML002: Rule = Rule {
    code: "XML002",
    severity: Severity::Error,
    clause: "Part 2 6.2.5",
    summary: "XML declarations name UTF-8 or UTF-16 and nothing else",
};
pub const XML003: Rule = Rule {
    code: "XML003",
    severity: Severity::Warning,
    clause: "Part 4 7",
    summary: "All parts use the same conformance class of namespaces, Strict or Transitional",
};
pub const XML004: Rule = Rule {
    code: "XML004",
    severity: Severity::Info,
    clause: "Part 2 6.2.5",
    summary: "XML parts are UTF-8 or UTF-16; a UTF-16 part is noted and read after transcoding",
};

pub static XML_RULES: [Rule; 4] = [XML001, XML002, XML003, XML004];

/// True for content types that denote XML.
pub fn is_xml_content_type(content_type: &str) -> bool {
    let main = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    main.ends_with("+xml") || main == "application/xml" || main == "text/xml"
}

fn declared_encoding(data: &[u8]) -> Option<String> {
    let head = data.get(..data.len().min(200))?;
    let text = std::str::from_utf8(head)
        .ok()
        .or_else(|| std::str::from_utf8(&head[..head.len().saturating_sub(4)]).ok())?;
    let decl = text.strip_prefix('\u{feff}').unwrap_or(text);
    let decl = decl.strip_prefix("<?xml")?;
    let end = decl.find("?>")?;
    let decl = &decl[..end];
    let pos = decl.find("encoding")?;
    let rest = decl[pos + "encoding".len()..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let quote = rest.chars().next()?;
    let rest = &rest[1..];
    let close = rest.find(quote)?;
    Some(rest[..close].to_string())
}

pub fn run(ctx: &Context<'_>, sink: &mut Sink<'_>) {
    let package = ctx.package;
    let mut strict_parts = 0usize;
    let mut transitional_parts = 0usize;
    let mut example_strict = None;
    let mut example_transitional = None;
    for part in package.parts() {
        let xml = is_rels_part(&part.name)
            || package
                .content_type_of(&part.name)
                .is_some_and(is_xml_content_type);
        if !xml {
            continue;
        }
        let mut stored = Vec::new();
        if let Err(err) = package.read_part_bytes(&part.name, &mut stored) {
            sink.push(
                Finding::new(&XML001, format!("part could not be read: {err}")).in_part(&part.name),
            );
            continue;
        }
        sink.part_read();
        let data = match utf16_xml_to_utf8(&stored) {
            Some(utf8) => {
                sink.push(Finding::new(&XML004, "part is UTF-16 encoded").in_part(&part.name));
                utf8
            }
            None => stored,
        };
        if let Some(encoding) = declared_encoding(&data) {
            if !encoding.eq_ignore_ascii_case("utf-8") && !encoding.eq_ignore_ascii_case("utf-16") {
                sink.push(
                    Finding::new(&XML002, format!("declared encoding is {encoding}"))
                        .in_part(&part.name),
                );
            }
        }
        if ctx.options.xml_well_formed {
            if let Err(err) = well_formed(&data) {
                sink.push(
                    Finding::new(&XML001, err.msg)
                        .in_part(&part.name)
                        .at(format!("byte {}", err.offset)),
                );
                continue;
            }
        }
        let mut reader = Reader::new(&data);
        loop {
            match reader.next() {
                Ok(Event::Start(_)) | Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        if reader.saw_strict() {
            strict_parts += 1;
            example_strict.get_or_insert_with(|| part.name.clone());
        }
        if reader.saw_transitional() {
            transitional_parts += 1;
            example_transitional.get_or_insert_with(|| part.name.clone());
        }
    }
    if strict_parts > 0 && transitional_parts > 0 {
        sink.push(Finding::new(&XML003, format!(
            "{strict_parts} part(s) use Strict namespaces (e.g. {}) and {transitional_parts} use Transitional (e.g. {})",
            example_strict.unwrap_or_default(),
            example_transitional.unwrap_or_default()
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_content_types() {
        assert!(is_xml_content_type(
            "application/vnd.openxmlformats-officedocument.presentationml.slide+xml"
        ));
        assert!(is_xml_content_type("application/xml"));
        assert!(is_xml_content_type("Text/XML; charset=utf-8"));
        assert!(!is_xml_content_type("image/png"));
        assert!(!is_xml_content_type(
            "application/vnd.openxmlformats-officedocument.presentationml.printerSettings"
        ));
    }

    #[test]
    fn declared_encodings() {
        assert_eq!(
            declared_encoding(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a/>"#)
                .as_deref(),
            Some("UTF-8")
        );
        assert_eq!(
            declared_encoding(b"\xef\xbb\xbf<?xml version='1.0' encoding='iso-8859-1'?><a/>")
                .as_deref(),
            Some("iso-8859-1")
        );
        assert_eq!(declared_encoding(br#"<?xml version="1.0"?><a/>"#), None);
        assert_eq!(declared_encoding(b"<a/>"), None);
    }
}
