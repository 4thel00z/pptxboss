//! The presentation part (`p:presentation`, ECMA-376 Part 1, 19.2.1.26):
//! the slide and master id lists that give the deck its order, and the
//! slide and notes sizes.

use crate::mce::children;
use crate::model::Emu;
use crate::opc::Defect;
use crate::slide::parse_coordinate;
use crate::xml::{unescape_attr, Event, Ns, Reader, Start, XmlError};

/// One `p:sldId` (19.2.1.33).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideId {
    /// `id`; `None` when absent or not a number.
    pub id: Option<u32>,
    /// `r:id` naming the Slide part relationship.
    pub rel_id: String,
    pub offset: usize,
}

/// One `p:sldMasterId` (19.2.1.36).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterId {
    pub id: Option<u32>,
    pub rel_id: String,
    pub offset: usize,
}

/// `p:sldSz` (19.2.1.39).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideSize {
    pub cx: Emu,
    pub cy: Emu,
    /// `type`, e.g. `screen16x9`; `None` means `custom`.
    pub kind: Option<String>,
}

/// The parsed presentation part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Presentation {
    pub slides: Vec<SlideId>,
    pub masters: Vec<MasterId>,
    /// `r:id` of the notes master, if listed.
    pub notes_master: Option<String>,
    /// `r:id` of the handout master, if listed.
    pub handout_master: Option<String>,
    pub slide_size: Option<SlideSize>,
    pub notes_size: Option<(Emu, Emu)>,
    /// `firstSlideNum`; 1 when omitted.
    pub first_slide_num: i32,
    pub rtl: bool,
    /// False when the root is not `p:presentation`.
    pub root_ok: bool,
    pub defects: Vec<Defect>,
}

impl Presentation {
    pub fn parse(xml: &[u8]) -> Result<Self, XmlError> {
        let mut reader = Reader::new(xml);
        let root = loop {
            match reader.next()? {
                Event::Start(start) => break start,
                Event::Eof => {
                    return Err(XmlError {
                        offset: xml.len(),
                        msg: "no root element",
                    })
                }
                _ => {}
            }
        };
        let mut presentation = Presentation {
            first_slide_num: 1,
            root_ok: root.name.is(Ns::Pml, b"presentation"),
            ..Presentation::default()
        };
        presentation.first_slide_num = attr_i32(&reader, &root, b"firstSlideNum").unwrap_or(1);
        presentation.rtl = matches!(reader.attr(&root, Ns::None, b"rtl"), Some(b"1" | b"true"));
        children(&mut reader, &mut |reader, child| {
            if child.name.ns != Ns::Pml {
                return Ok(());
            }
            match child.name.local {
                b"sldIdLst" => children(reader, &mut |reader, item| {
                    if !item.name.is(Ns::Pml, b"sldId") {
                        return Ok(());
                    }
                    match reader
                        .attr(&item, Ns::Rel, b"id")
                        .map(unescape_attr)
                        .filter(|id| !id.is_empty())
                    {
                        Some(rel_id) => presentation.slides.push(SlideId {
                            id: attr_u32(reader, &item, b"id"),
                            rel_id,
                            offset: item.offset,
                        }),
                        None => presentation.defects.push(Defect {
                            offset: item.offset,
                            msg: "sldId without r:id",
                        }),
                    }
                    Ok(())
                }),
                b"sldMasterIdLst" => children(reader, &mut |reader, item| {
                    if !item.name.is(Ns::Pml, b"sldMasterId") {
                        return Ok(());
                    }
                    match reader
                        .attr(&item, Ns::Rel, b"id")
                        .map(unescape_attr)
                        .filter(|id| !id.is_empty())
                    {
                        Some(rel_id) => presentation.masters.push(MasterId {
                            id: attr_u32(reader, &item, b"id"),
                            rel_id,
                            offset: item.offset,
                        }),
                        None => presentation.defects.push(Defect {
                            offset: item.offset,
                            msg: "sldMasterId without r:id",
                        }),
                    }
                    Ok(())
                }),
                b"notesMasterIdLst" => children(reader, &mut |reader, item| {
                    if item.name.is(Ns::Pml, b"notesMasterId")
                        && presentation.notes_master.is_none()
                    {
                        presentation.notes_master =
                            reader.attr(&item, Ns::Rel, b"id").map(unescape_attr);
                    }
                    Ok(())
                }),
                b"handoutMasterIdLst" => children(reader, &mut |reader, item| {
                    if item.name.is(Ns::Pml, b"handoutMasterId")
                        && presentation.handout_master.is_none()
                    {
                        presentation.handout_master =
                            reader.attr(&item, Ns::Rel, b"id").map(unescape_attr);
                    }
                    Ok(())
                }),
                b"sldSz" => {
                    let cx = reader
                        .attr(&child, Ns::None, b"cx")
                        .and_then(parse_coordinate);
                    let cy = reader
                        .attr(&child, Ns::None, b"cy")
                        .and_then(parse_coordinate);
                    match (cx, cy) {
                        (Some(cx), Some(cy)) => {
                            presentation.slide_size = Some(SlideSize {
                                cx,
                                cy,
                                kind: reader.attr(&child, Ns::None, b"type").map(unescape_attr),
                            });
                        }
                        _ => presentation.defects.push(Defect {
                            offset: child.offset,
                            msg: "sldSz without numeric cx and cy",
                        }),
                    }
                    Ok(())
                }
                b"notesSz" => {
                    let cx = reader
                        .attr(&child, Ns::None, b"cx")
                        .and_then(parse_coordinate);
                    let cy = reader
                        .attr(&child, Ns::None, b"cy")
                        .and_then(parse_coordinate);
                    match (cx, cy) {
                        (Some(cx), Some(cy)) => presentation.notes_size = Some((cx, cy)),
                        _ => presentation.defects.push(Defect {
                            offset: child.offset,
                            msg: "notesSz without numeric cx and cy",
                        }),
                    }
                    Ok(())
                }
                _ => Ok(()),
            }
        })?;
        Ok(presentation)
    }
}

fn attr_u32(reader: &Reader<'_>, start: &Start<'_>, local: &[u8]) -> Option<u32> {
    std::str::from_utf8(reader.attr(start, Ns::None, local)?)
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn attr_i32(reader: &Reader<'_>, start: &Start<'_>, local: &[u8]) -> Option<i32> {
    std::str::from_utf8(reader.attr(start, Ns::None, local)?)
        .ok()?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" firstSlideNum="5" rtl="1" saveSubsetFonts="1">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
<p:notesMasterIdLst><p:notesMasterId r:id="rId6"/></p:notesMasterIdLst>
<p:handoutMasterIdLst><p:handoutMasterId r:id="rId7"/></p:handoutMasterIdLst>
<p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/><p:sldId id="x" r:id="rId4"/><p:sldId id="259"/></p:sldIdLst>
<p:sldSz cx="12192000" cy="6858000" type="screen16x9"/><p:notesSz cx="6858000" cy="9144000"/>
<p:defaultTextStyle><a:defPPr/></p:defaultTextStyle>
</p:presentation>"#;

    #[test]
    fn slide_order_masters_and_sizes_are_read() {
        let presentation = Presentation::parse(XML).unwrap();
        assert!(presentation.root_ok);
        assert_eq!(presentation.first_slide_num, 5);
        assert!(presentation.rtl);
        assert_eq!(presentation.slides.len(), 3);
        assert_eq!(presentation.slides[0].id, Some(256));
        assert_eq!(presentation.slides[0].rel_id, "rId2");
        assert_eq!(presentation.slides[2].id, None);
        assert_eq!(presentation.slides[2].rel_id, "rId4");
        assert_eq!(presentation.defects.len(), 1);
        assert_eq!(presentation.defects[0].msg, "sldId without r:id");
        assert_eq!(presentation.masters.len(), 1);
        assert_eq!(presentation.masters[0].id, Some(2147483648));
        assert_eq!(presentation.notes_master.as_deref(), Some("rId6"));
        assert_eq!(presentation.handout_master.as_deref(), Some("rId7"));
        assert_eq!(
            presentation.slide_size,
            Some(SlideSize {
                cx: 12192000,
                cy: 6858000,
                kind: Some("screen16x9".into())
            })
        );
        assert_eq!(presentation.notes_size, Some((6858000, 9144000)));
    }

    #[test]
    fn a_minimal_presentation_and_a_wrong_root_parse() {
        let minimal = br#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:notesSz cx="913607" cy="913607"/></p:presentation>"#;
        let presentation = Presentation::parse(minimal).unwrap();
        assert!(presentation.slides.is_empty());
        assert_eq!(presentation.first_slide_num, 1);
        assert_eq!(presentation.notes_size, Some((913607, 913607)));
        let wrong = Presentation::parse(b"<x/>").unwrap();
        assert!(!wrong.root_ok);
        assert!(Presentation::parse(b"").is_err());
    }
}
