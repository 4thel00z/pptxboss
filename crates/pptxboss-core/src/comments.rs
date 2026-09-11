//! Slide comments: the PresentationML Comments part (ECMA-376 Part 1,
//! clause 19.4, `p:cmLst`) with its Comment Authors part, and the 2018
//! comments part current PowerPoint writes (`p188:cmLst`, threaded
//! replies, rich text) with its Authors part.

use crate::mce::children;
use crate::slide::parse_text_body;
use crate::xml::{unescape_attr, Event, Ns, Reader, XmlError};

/// One entry of a comment authors part, either flavour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentAuthor {
    /// `@id`: a small integer in the 2006 part, a GUID in the 2018 part.
    pub id: String,
    pub name: String,
    pub initials: Option<String>,
}

/// A comment on a slide; replies follow their parent with `reply` set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comment {
    /// The author's name when the authors part resolves the id, else the raw id.
    pub author: Option<String>,
    pub initials: Option<String>,
    /// `@dt` (2006) or `@created` (2018) as written.
    pub date: Option<String>,
    pub text: String,
    pub reply: bool,
}

/// Parses a comment authors part of either flavour.
pub fn parse_authors(xml: &[u8]) -> Result<Vec<CommentAuthor>, XmlError> {
    let mut reader = Reader::new(xml);
    if !skip_to_root(&mut reader)? {
        return Ok(Vec::new());
    }
    let mut authors = Vec::new();
    children(&mut reader, &mut |reader, child| {
        let is_author = child.name.is(Ns::Pml, b"cmAuthor") || child.name.is(Ns::P188, b"author");
        if !is_author {
            return reader.skip_element();
        }
        let id = reader.attr(&child, Ns::None, b"id").map(unescape_attr);
        let name = reader.attr(&child, Ns::None, b"name").map(unescape_attr);
        if let (Some(id), Some(name)) = (id, name) {
            authors.push(CommentAuthor {
                id,
                name,
                initials: reader
                    .attr(&child, Ns::None, b"initials")
                    .map(unescape_attr)
                    .filter(|initials| !initials.is_empty()),
            });
        }
        reader.skip_element()
    })?;
    Ok(authors)
}

/// Parses a comments part of either flavour, resolving author ids through `authors`.
pub fn parse_comments(xml: &[u8], authors: &[CommentAuthor]) -> Result<Vec<Comment>, XmlError> {
    let mut reader = Reader::new(xml);
    if !skip_to_root(&mut reader)? {
        return Ok(Vec::new());
    }
    let mut comments = Vec::new();
    children(&mut reader, &mut |reader, child| {
        if child.name.is(Ns::Pml, b"cm") {
            let mut comment = header(reader, &child, b"dt", authors);
            children(reader, &mut |reader, item| {
                if !item.name.is(Ns::Pml, b"text") {
                    return reader.skip_element();
                }
                reader.text_content(&mut comment.text)
            })?;
            comments.push(comment);
            return Ok(());
        }
        if child.name.is(Ns::P188, b"cm") {
            let mut comment = header(reader, &child, b"created", authors);
            let mut replies = Vec::new();
            children(reader, &mut |reader, item| {
                if item.name.is(Ns::P188, b"txBody") {
                    comment.text = parse_text_body(reader)?.text();
                    return Ok(());
                }
                if item.name.is(Ns::P188, b"replyLst") {
                    return children(reader, &mut |reader, reply| {
                        if !reply.name.is(Ns::P188, b"reply") {
                            return reader.skip_element();
                        }
                        let mut comment = header(reader, &reply, b"created", authors);
                        comment.reply = true;
                        children(reader, &mut |reader, part| {
                            if !part.name.is(Ns::P188, b"txBody") {
                                return reader.skip_element();
                            }
                            comment.text = parse_text_body(reader)?.text();
                            Ok(())
                        })?;
                        replies.push(comment);
                        Ok(())
                    });
                }
                reader.skip_element()
            })?;
            comments.push(comment);
            comments.append(&mut replies);
            return Ok(());
        }
        reader.skip_element()
    })?;
    Ok(comments)
}

fn header(
    reader: &Reader<'_>,
    start: &crate::xml::Start<'_>,
    date_attr: &[u8],
    authors: &[CommentAuthor],
) -> Comment {
    let author_id = reader.attr(start, Ns::None, b"authorId").map(unescape_attr);
    let author = author_id
        .as_deref()
        .and_then(|id| authors.iter().find(|author| author.id == id));
    Comment {
        author: author.map(|author| author.name.clone()).or(author_id),
        initials: author.and_then(|author| author.initials.clone()),
        date: reader.attr(start, Ns::None, date_attr).map(unescape_attr),
        text: String::new(),
        reply: false,
    }
}

/// Positions the reader after the root start tag; false for an empty document.
fn skip_to_root(reader: &mut Reader<'_>) -> Result<bool, XmlError> {
    loop {
        match reader.next()? {
            Event::Start(_) => return Ok(true),
            Event::Eof => return Ok(false),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHORS_2006: &[u8] = br#"<p:cmAuthorLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cmAuthor id="0" name="Ada Lovelace" initials="AL" lastIdx="2" clrIdx="0"/><p:cmAuthor id="1" name="Bob" lastIdx="1" clrIdx="1"/></p:cmAuthorLst>"#;

    #[test]
    fn legacy_comments_resolve_authors_and_keep_order() {
        let authors = parse_authors(AUTHORS_2006).unwrap();
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0].initials.as_deref(), Some("AL"));
        let xml = br#"<p:cmLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cm authorId="0" dt="2024-05-01T10:00:00.000" idx="1"><p:pos x="10" y="20"/><p:text>First &amp; foremost</p:text></p:cm><p:cm authorId="9" idx="2"><p:text>Orphan</p:text></p:cm></p:cmLst>"#;
        let comments = parse_comments(xml, &authors).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].author.as_deref(), Some("Ada Lovelace"));
        assert_eq!(comments[0].date.as_deref(), Some("2024-05-01T10:00:00.000"));
        assert_eq!(comments[0].text, "First & foremost");
        assert_eq!(comments[1].author.as_deref(), Some("9"));
        assert!(!comments[1].reply);
    }

    #[test]
    fn modern_comments_carry_rich_text_and_replies() {
        let authors = parse_authors(br#"<p188:authorLst xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main"><p188:author id="{AAAA-1}" name="Ada" initials="A" userId="ada" providerId="AD"/></p188:authorLst>"#).unwrap();
        let xml = br#"<p188:cmLst xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p188:cm id="{C1}" authorId="{AAAA-1}" created="2024-05-01T10:00:00.000"><p188:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Please </a:t></a:r><a:r><a:t>fix</a:t></a:r></a:p></p188:txBody><p188:replyLst><p188:reply id="{R1}" authorId="{ZZZ}" created="2024-05-02T09:00:00.000"><p188:txBody><a:bodyPr/><a:p><a:r><a:t>Done</a:t></a:r></a:p></p188:txBody></p188:reply></p188:replyLst></p188:cm></p188:cmLst>"#;
        let comments = parse_comments(xml, &authors).unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].author.as_deref(), Some("Ada"));
        assert_eq!(comments[0].text, "Please fix");
        assert!(!comments[0].reply);
        assert_eq!(comments[1].author.as_deref(), Some("{ZZZ}"));
        assert_eq!(comments[1].text, "Done");
        assert!(comments[1].reply);
    }
}
