//! Element iteration with Markup Compatibility and Extensibility applied
//! (ECMA-376 Part 3, clause 9). An `mc:AlternateContent` is replaced by
//! its first `mc:Choice` whose `Requires` prefixes all name namespaces
//! this reader understands, else by its `mc:Fallback`, else dropped.
//! Every other element in the compatibility namespace is skipped.

use crate::xml::{Event, Ns, Reader, Start, XmlError};

type XmlResult<T> = Result<T, XmlError>;

/// Calls `f` for each child element of the element that is currently
/// open. `f` may consume the child (by iterating its own children or
/// reading its text) or leave it untouched; in either case the reader is
/// positioned after the child when `f` returns.
pub fn children<'a>(
    reader: &mut Reader<'a>,
    f: &mut dyn FnMut(&mut Reader<'a>, Start<'a>) -> XmlResult<()>,
) -> XmlResult<()> {
    children_inner(reader, f, true)
}

fn children_inner<'a>(
    reader: &mut Reader<'a>,
    f: &mut dyn FnMut(&mut Reader<'a>, Start<'a>) -> XmlResult<()>,
    apply_mce: bool,
) -> XmlResult<()> {
    let parent_depth = reader.depth();
    loop {
        match reader.next()? {
            Event::Start(start) => {
                if apply_mce && start.name.ns == Ns::Mce {
                    alternate_content(reader, &start, f)?;
                } else {
                    f(reader, start)?;
                }
                if reader.depth() > parent_depth {
                    reader.skip_element()?;
                }
            }
            Event::End(_) => {
                if reader.depth() < parent_depth {
                    return Ok(());
                }
            }
            Event::Eof => return Ok(()),
            Event::Text { .. } => {}
        }
    }
}

fn alternate_content<'a>(
    reader: &mut Reader<'a>,
    start: &Start<'a>,
    f: &mut dyn FnMut(&mut Reader<'a>, Start<'a>) -> XmlResult<()>,
) -> XmlResult<()> {
    if start.name.local != b"AlternateContent" {
        return Ok(());
    }
    let mut selected = false;
    children_inner(
        reader,
        &mut |reader, branch| {
            let take = match (branch.name.ns, branch.name.local) {
                (Ns::Mce, b"Choice") => !selected && requires_understood(reader, &branch),
                (Ns::Mce, b"Fallback") => !selected,
                _ => false,
            };
            if !take {
                return Ok(());
            }
            selected = true;
            children(reader, f)
        },
        false,
    )
}

fn requires_understood(reader: &Reader<'_>, choice: &Start<'_>) -> bool {
    let Some(requires) = reader.attr(choice, Ns::None, b"Requires") else {
        return false;
    };
    let mut prefixes = requires
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|prefix| !prefix.is_empty())
        .peekable();
    if prefixes.peek().is_none() {
        return false;
    }
    prefixes.all(|prefix| !matches!(reader.resolve(prefix), Ns::Other(_) | Ns::None))
}

/// Whether a `Start` for `mc:AlternateContent` would end up dropped: no
/// branch is selectable. Used by reports; consumes nothing.
pub fn is_alternate_content(start: &Start<'_>) -> bool {
    start.name.ns == Ns::Mce && start.name.local == b"AlternateContent"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locals(xml: &[u8]) -> Vec<String> {
        let mut reader = Reader::new(xml);
        reader.next().unwrap();
        let mut seen = Vec::new();
        children(&mut reader, &mut |reader, child| {
            seen.push(String::from_utf8_lossy(child.name.local).into_owned());
            if child.name.local == b"deep" {
                children(reader, &mut |_, grandchild| {
                    seen.push(format!(
                        "deep/{}",
                        String::from_utf8_lossy(grandchild.name.local)
                    ));
                    Ok(())
                })?;
            }
            Ok(())
        })
        .unwrap();
        assert!(matches!(reader.next().unwrap(), Event::Eof));
        seen
    }

    const NS: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main""#;

    #[test]
    fn plain_children_are_visited_whether_or_not_consumed() {
        let xml = format!(
            "<root {NS}><a/><b x=\"1\">text<inner/></b><deep><x/><y><z/></y></deep><c/></root>"
        );
        assert_eq!(
            locals(xml.as_bytes()),
            ["a", "b", "deep", "deep/x", "deep/y", "c"]
        );
    }

    #[test]
    fn fallback_is_taken_when_no_choice_is_understood() {
        let xml = format!(
            r#"<root {NS}><mc:AlternateContent><mc:Choice Requires="v"><v:shape/></mc:Choice><mc:Choice Requires="p14"><p14:thing/></mc:Choice><mc:Fallback><p:sp/><p:pic/></mc:Fallback></mc:AlternateContent><after/></root>"#
        );
        assert_eq!(locals(xml.as_bytes()), ["sp", "pic", "after"]);
    }

    #[test]
    fn the_first_understood_choice_wins() {
        let xml = format!(
            r#"<root {NS}><mc:AlternateContent><mc:Choice Requires="v"><v:shape/></mc:Choice><mc:Choice Requires="a p"><p:sp/></mc:Choice><mc:Choice Requires="p"><p:pic/></mc:Choice><mc:Fallback><p:grpSp/></mc:Fallback></mc:AlternateContent></root>"#
        );
        assert_eq!(locals(xml.as_bytes()), ["sp"]);
    }

    #[test]
    fn alternate_content_without_a_usable_branch_is_dropped() {
        let xml = format!(
            r#"<root {NS}><mc:AlternateContent><mc:Choice Requires="v"><v:shape/></mc:Choice></mc:AlternateContent><after/><mc:Ignorable/></root>"#
        );
        assert_eq!(locals(xml.as_bytes()), ["after"]);
    }

    #[test]
    fn empty_or_missing_requires_is_not_understood() {
        let xml = format!(
            r#"<root {NS}><mc:AlternateContent><mc:Choice Requires=""><a/></mc:Choice><mc:Choice><b/></mc:Choice><mc:Fallback><c/></mc:Fallback></mc:AlternateContent></root>"#
        );
        assert_eq!(locals(xml.as_bytes()), ["c"]);
    }
}
