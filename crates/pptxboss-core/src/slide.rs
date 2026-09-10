//! Parser for slide-family parts (`p:sld`, `p:sldLayout`, `p:sldMaster`,
//! `p:notes`, `p:notesMaster`, `p:handoutMaster`) into the content model.
//!
//! One pass over the XML, no tree. Unknown namespaces and elements are
//! skipped and counted in the [`SlideReport`]; markup compatibility is
//! applied on the way (see [`crate::mce`]).

use crate::mce::children;
use crate::model::*;
use crate::xml::{unescape_attr, Event, Ns, Reader, Start, XmlError};

type XmlResult<T> = Result<T, XmlError>;

/// What the parser skipped in one part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlideReport {
    /// `graphicData` URIs the reader does not understand, one entry each.
    pub unknown_graphics: Vec<String>,
    /// Elements in namespaces the reader does not understand, outside `extLst`.
    pub unknown_elements: u32,
}

impl SlideReport {
    pub fn is_empty(&self) -> bool {
        self.unknown_graphics.is_empty() && self.unknown_elements == 0
    }

    pub fn merge(&mut self, other: &SlideReport) {
        self.unknown_graphics
            .extend(other.unknown_graphics.iter().cloned());
        self.unknown_elements += other.unknown_elements;
    }
}

/// Parses one slide-family part.
pub fn parse_slide(xml: &[u8]) -> XmlResult<(SlideContent, SlideReport)> {
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
    let kind = match root.name.ns {
        Ns::Pml => SlideKind::from_root(root.name.local),
        _ => None,
    };
    let Some(kind) = kind else {
        return Err(XmlError {
            offset: root.offset,
            msg: "root element is not a slide, layout, master or notes element",
        });
    };
    let show = attr_bool(&reader, &root, Ns::None, b"show").unwrap_or(true);
    let mut content = SlideContent {
        kind,
        name: None,
        show,
        shapes: Vec::new(),
    };
    let mut report = SlideReport::default();
    children(&mut reader, &mut |reader, child| {
        if !child.name.is(Ns::Pml, b"cSld") {
            return Ok(());
        }
        content.name = reader
            .attr(&child, Ns::None, b"name")
            .map(unescape_attr)
            .filter(|name| !name.is_empty());
        children(reader, &mut |reader, grandchild| {
            if !grandchild.name.is(Ns::Pml, b"spTree") {
                return Ok(());
            }
            parse_shape_list(reader, &mut content.shapes, &mut report)
        })
    })?;
    Ok((content, report))
}

fn parse_shape_list<'a>(
    reader: &mut Reader<'a>,
    out: &mut Vec<Shape>,
    report: &mut SlideReport,
) -> XmlResult<()> {
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Pml {
            report.unknown_elements += 1;
            return Ok(());
        }
        let shape = match child.name.local {
            b"sp" => parse_sp(reader, report)?,
            b"pic" => parse_pic(reader, report)?,
            b"grpSp" => parse_grp_sp(reader, report)?,
            b"graphicFrame" => parse_graphic_frame(reader, report)?,
            b"cxnSp" => parse_cxn_sp(reader, report)?,
            b"contentPart" => {
                let rel = reader.attr(&child, Ns::Rel, b"id").map(unescape_attr);
                let mut shape = blank_shape(Content::ContentPart(rel));
                shape.name = String::from("Content Part");
                shape
            }
            _ => return Ok(()),
        };
        out.push(shape);
        Ok(())
    })
}

fn blank_shape(content: Content) -> Shape {
    Shape {
        id: 0,
        name: String::new(),
        hidden: false,
        description: None,
        hyperlink: None,
        placeholder: None,
        transform: None,
        text_box: false,
        content,
    }
}

#[derive(Default)]
struct NonVisual {
    id: u32,
    name: String,
    hidden: bool,
    description: Option<String>,
    hyperlink: Option<String>,
    placeholder: Option<Placeholder>,
    text_box: bool,
    media: Option<String>,
}

impl NonVisual {
    fn apply(self, shape: &mut Shape) -> Option<String> {
        shape.id = self.id;
        shape.name = self.name;
        shape.hidden = self.hidden;
        shape.description = self.description;
        shape.hyperlink = self.hyperlink;
        shape.placeholder = self.placeholder;
        shape.text_box = self.text_box;
        self.media
    }
}

fn parse_non_visual<'a>(reader: &mut Reader<'a>, report: &mut SlideReport) -> XmlResult<NonVisual> {
    let mut nv = NonVisual::default();
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Pml {
            report.unknown_elements += 1;
            return Ok(());
        }
        match child.name.local {
            b"cNvPr" => {
                nv.id = attr_u32(reader, &child, Ns::None, b"id").unwrap_or(0);
                nv.name = reader
                    .attr(&child, Ns::None, b"name")
                    .map(unescape_attr)
                    .unwrap_or_default();
                nv.hidden = attr_bool(reader, &child, Ns::None, b"hidden").unwrap_or(false);
                nv.description = reader
                    .attr(&child, Ns::None, b"descr")
                    .map(unescape_attr)
                    .filter(|descr| !descr.is_empty());
                children(reader, &mut |reader, link| {
                    if link.name.is(Ns::Dml, b"hlinkClick") {
                        nv.hyperlink = reader
                            .attr(&link, Ns::Rel, b"id")
                            .map(unescape_attr)
                            .filter(|id| !id.is_empty());
                    }
                    Ok(())
                })
            }
            b"cNvSpPr" => {
                nv.text_box = attr_bool(reader, &child, Ns::None, b"txBox").unwrap_or(false);
                Ok(())
            }
            b"nvPr" => children(reader, &mut |reader, prop| {
                match (prop.name.ns, prop.name.local) {
                    (Ns::Pml, b"ph") => {
                        let kind = reader
                            .attr(&prop, Ns::None, b"type")
                            .map(PlaceholderKind::parse)
                            .unwrap_or_default();
                        let idx = attr_u32(reader, &prop, Ns::None, b"idx").unwrap_or(0);
                        nv.placeholder = Some(Placeholder { kind, idx });
                    }
                    (Ns::Dml, b"videoFile" | b"audioFile" | b"quickTimeFile") => {
                        nv.media = reader.attr(&prop, Ns::Rel, b"link").map(unescape_attr);
                    }
                    (Ns::Dml, b"wavAudioFile") => {
                        nv.media = reader.attr(&prop, Ns::Rel, b"embed").map(unescape_attr);
                    }
                    _ => {}
                }
                Ok(())
            }),
            _ => Ok(()),
        }
    })?;
    Ok(nv)
}

fn parse_sp<'a>(reader: &mut Reader<'a>, report: &mut SlideReport) -> XmlResult<Shape> {
    let mut shape = blank_shape(Content::Text(TextBody::default()));
    let mut body = TextBody::default();
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Pml {
            report.unknown_elements += 1;
            return Ok(());
        }
        match child.name.local {
            b"nvSpPr" => {
                parse_non_visual(reader, report)?.apply(&mut shape);
            }
            b"spPr" => shape.transform = parse_sp_pr(reader)?.0,
            b"txBody" => body = parse_text_body(reader)?,
            _ => {}
        }
        Ok(())
    })?;
    shape.content = Content::Text(body);
    Ok(shape)
}

fn parse_pic<'a>(reader: &mut Reader<'a>, report: &mut SlideReport) -> XmlResult<Shape> {
    let mut shape = blank_shape(Content::Connector);
    let mut picture = Picture::default();
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Pml {
            report.unknown_elements += 1;
            return Ok(());
        }
        match child.name.local {
            b"nvPicPr" => picture.media = parse_non_visual(reader, report)?.apply(&mut shape),
            b"blipFill" => parse_blip_fill(reader, &mut picture)?,
            b"spPr" => shape.transform = parse_sp_pr(reader)?.0,
            _ => {}
        }
        Ok(())
    })?;
    shape.content = Content::Picture(picture);
    Ok(shape)
}

fn parse_blip_fill<'a>(reader: &mut Reader<'a>, picture: &mut Picture) -> XmlResult<()> {
    children(reader, &mut |reader, child| {
        if !child.name.is(Ns::Dml, b"blip") {
            return Ok(());
        }
        picture.embed = reader
            .attr(&child, Ns::Rel, b"embed")
            .map(unescape_attr)
            .filter(|id| !id.is_empty());
        picture.link = reader
            .attr(&child, Ns::Rel, b"link")
            .map(unescape_attr)
            .filter(|id| !id.is_empty());
        Ok(())
    })
}

fn parse_grp_sp<'a>(reader: &mut Reader<'a>, report: &mut SlideReport) -> XmlResult<Shape> {
    let mut shape = blank_shape(Content::Connector);
    let mut shapes = Vec::new();
    let mut child_space = None;
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Pml {
            report.unknown_elements += 1;
            return Ok(());
        }
        match child.name.local {
            b"nvGrpSpPr" => {
                parse_non_visual(reader, report)?.apply(&mut shape);
            }
            b"grpSpPr" => {
                let (transform, space) = parse_sp_pr(reader)?;
                shape.transform = transform;
                child_space = space;
            }
            b"sp" => shapes.push(parse_sp(reader, report)?),
            b"pic" => shapes.push(parse_pic(reader, report)?),
            b"grpSp" => shapes.push(parse_grp_sp(reader, report)?),
            b"graphicFrame" => shapes.push(parse_graphic_frame(reader, report)?),
            b"cxnSp" => shapes.push(parse_cxn_sp(reader, report)?),
            b"contentPart" => {
                let rel = reader.attr(&child, Ns::Rel, b"id").map(unescape_attr);
                shapes.push(blank_shape(Content::ContentPart(rel)));
            }
            _ => {}
        }
        Ok(())
    })?;
    shape.content = Content::Group(shapes, child_space);
    Ok(shape)
}

fn parse_cxn_sp<'a>(reader: &mut Reader<'a>, report: &mut SlideReport) -> XmlResult<Shape> {
    let mut shape = blank_shape(Content::Connector);
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Pml {
            report.unknown_elements += 1;
            return Ok(());
        }
        match child.name.local {
            b"nvCxnSpPr" => {
                parse_non_visual(reader, report)?.apply(&mut shape);
            }
            b"spPr" => shape.transform = parse_sp_pr(reader)?.0,
            _ => {}
        }
        Ok(())
    })?;
    Ok(shape)
}

fn parse_graphic_frame<'a>(reader: &mut Reader<'a>, report: &mut SlideReport) -> XmlResult<Shape> {
    let mut shape = blank_shape(Content::UnknownGraphic(String::new()));
    let mut content = None;
    children(reader, &mut |reader, child| {
        match (child.name.ns, child.name.local) {
            (Ns::Pml, b"nvGraphicFramePr") => {
                parse_non_visual(reader, report)?.apply(&mut shape);
            }
            (Ns::Pml, b"xfrm") => shape.transform = parse_xfrm(reader, &child)?.0,
            (Ns::Dml, b"graphic") => {
                children(reader, &mut |reader, data| {
                    if !data.name.is(Ns::Dml, b"graphicData") {
                        return Ok(());
                    }
                    let uri = reader
                        .attr(&data, Ns::None, b"uri")
                        .map(unescape_attr)
                        .unwrap_or_default();
                    content = Some(parse_graphic_data(reader, uri, report)?);
                    Ok(())
                })?;
            }
            (Ns::Pml, _) => {}
            _ => report.unknown_elements += 1,
        }
        Ok(())
    })?;
    shape.content = content.unwrap_or(Content::UnknownGraphic(String::new()));
    Ok(shape)
}

fn parse_graphic_data<'a>(
    reader: &mut Reader<'a>,
    uri: String,
    report: &mut SlideReport,
) -> XmlResult<Content> {
    let mut content = None;
    children(reader, &mut |reader, child| {
        let parsed = match (child.name.ns, child.name.local) {
            (Ns::Dml, b"tbl") => Some(Content::Table(parse_table(reader)?)),
            (Ns::Chart, b"chart") => Some(Content::Chart(
                reader.attr(&child, Ns::Rel, b"id").map(unescape_attr),
            )),
            (Ns::Dgm, b"relIds") => Some(Content::Diagram(
                reader.attr(&child, Ns::Rel, b"dm").map(unescape_attr),
            )),
            (Ns::Pml, b"oleObj") => Some(Content::Ole(parse_ole(reader, &child, report)?)),
            _ => None,
        };
        if parsed.is_some() && content.is_none() {
            content = parsed;
        }
        Ok(())
    })?;
    Ok(content.unwrap_or_else(|| {
        report.unknown_graphics.push(uri.clone());
        Content::UnknownGraphic(uri)
    }))
}

fn parse_ole<'a>(
    reader: &mut Reader<'a>,
    start: &Start<'a>,
    report: &mut SlideReport,
) -> XmlResult<OleObject> {
    let mut ole = OleObject {
        prog_id: reader.attr(start, Ns::None, b"progId").map(unescape_attr),
        rel_id: reader
            .attr(start, Ns::Rel, b"id")
            .map(unescape_attr)
            .filter(|id| !id.is_empty()),
        preview: None,
    };
    children(reader, &mut |reader, child| {
        if child.name.is(Ns::Pml, b"pic") {
            let pic = parse_pic(reader, report)?;
            if let Content::Picture(picture) = pic.content {
                ole.preview = Some(picture);
            }
        }
        Ok(())
    })?;
    Ok(ole)
}

fn parse_table<'a>(reader: &mut Reader<'a>) -> XmlResult<Table> {
    let mut table = Table::default();
    children(
        reader,
        &mut |reader, child| match (child.name.ns, child.name.local) {
            (Ns::Dml, b"tblGrid") => children(reader, &mut |reader, col| {
                if col.name.is(Ns::Dml, b"gridCol") {
                    table
                        .column_widths
                        .push(attr_coordinate(reader, &col, Ns::None, b"w").unwrap_or(0));
                }
                Ok(())
            }),
            (Ns::Dml, b"tr") => {
                let mut row = Row {
                    height: attr_coordinate(reader, &child, Ns::None, b"h").unwrap_or(0),
                    cells: Vec::new(),
                };
                children(reader, &mut |reader, cell| {
                    if !cell.name.is(Ns::Dml, b"tc") {
                        return Ok(());
                    }
                    let mut parsed = Cell {
                        body: TextBody::default(),
                        grid_span: attr_u32(reader, &cell, Ns::None, b"gridSpan").unwrap_or(1),
                        row_span: attr_u32(reader, &cell, Ns::None, b"rowSpan").unwrap_or(1),
                        h_merge: attr_bool(reader, &cell, Ns::None, b"hMerge").unwrap_or(false),
                        v_merge: attr_bool(reader, &cell, Ns::None, b"vMerge").unwrap_or(false),
                    };
                    children(reader, &mut |reader, inner| {
                        if inner.name.is(Ns::Dml, b"txBody") {
                            parsed.body = parse_text_body(reader)?;
                        }
                        Ok(())
                    })?;
                    row.cells.push(parsed);
                    Ok(())
                })?;
                table.rows.push(row);
                Ok(())
            }
            _ => Ok(()),
        },
    )?;
    Ok(table)
}

/// Parses `p:spPr` or `p:grpSpPr`: only the transform is kept.
fn parse_sp_pr<'a>(reader: &mut Reader<'a>) -> XmlResult<(Option<Transform>, Option<ChildSpace>)> {
    let mut result = (None, None);
    children(reader, &mut |reader, child| {
        if child.name.is(Ns::Dml, b"xfrm") {
            result = parse_xfrm(reader, &child)?;
        }
        Ok(())
    })?;
    Ok(result)
}

fn parse_xfrm<'a>(
    reader: &mut Reader<'a>,
    start: &Start<'a>,
) -> XmlResult<(Option<Transform>, Option<ChildSpace>)> {
    let mut transform = Transform {
        rot: attr_i64(reader, start, Ns::None, b"rot").unwrap_or(0),
        flip_h: attr_bool(reader, start, Ns::None, b"flipH").unwrap_or(false),
        flip_v: attr_bool(reader, start, Ns::None, b"flipV").unwrap_or(false),
        ..Transform::default()
    };
    let mut child_space = ChildSpace::default();
    let mut has_off_or_ext = false;
    let mut has_child = false;
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Dml {
            return Ok(());
        }
        match child.name.local {
            b"off" => {
                has_off_or_ext = true;
                transform.x = attr_coordinate(reader, &child, Ns::None, b"x").unwrap_or(0);
                transform.y = attr_coordinate(reader, &child, Ns::None, b"y").unwrap_or(0);
            }
            b"ext" => {
                has_off_or_ext = true;
                transform.cx = attr_coordinate(reader, &child, Ns::None, b"cx").unwrap_or(0);
                transform.cy = attr_coordinate(reader, &child, Ns::None, b"cy").unwrap_or(0);
            }
            b"chOff" => {
                has_child = true;
                child_space.x = attr_coordinate(reader, &child, Ns::None, b"x").unwrap_or(0);
                child_space.y = attr_coordinate(reader, &child, Ns::None, b"y").unwrap_or(0);
            }
            b"chExt" => {
                has_child = true;
                child_space.cx = attr_coordinate(reader, &child, Ns::None, b"cx").unwrap_or(0);
                child_space.cy = attr_coordinate(reader, &child, Ns::None, b"cy").unwrap_or(0);
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok((
        has_off_or_ext.then_some(transform),
        has_child.then_some(child_space),
    ))
}

/// Parses `p:txBody` or `a:txBody`.
pub fn parse_text_body<'a>(reader: &mut Reader<'a>) -> XmlResult<TextBody> {
    let mut body = TextBody::default();
    children(reader, &mut |reader, child| {
        if child.name.is(Ns::Dml, b"p") {
            body.paragraphs.push(parse_paragraph(reader)?);
        }
        Ok(())
    })?;
    Ok(body)
}

fn parse_paragraph<'a>(reader: &mut Reader<'a>) -> XmlResult<Paragraph> {
    let mut paragraph = Paragraph::default();
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Dml {
            return Ok(());
        }
        match child.name.local {
            b"pPr" => {
                paragraph.level =
                    attr_u32(reader, &child, Ns::None, b"lvl").map_or(0, |lvl| lvl.min(8) as u8);
                children(reader, &mut |reader, prop| {
                    if prop.name.ns != Ns::Dml {
                        return Ok(());
                    }
                    match prop.name.local {
                        b"buNone" => paragraph.bullet = Bullet::None,
                        b"buChar" => {
                            paragraph.bullet = Bullet::Char(
                                reader
                                    .attr(&prop, Ns::None, b"char")
                                    .map(unescape_attr)
                                    .unwrap_or_default(),
                            )
                        }
                        b"buAutoNum" => {
                            paragraph.bullet = Bullet::AutoNumber {
                                scheme: reader
                                    .attr(&prop, Ns::None, b"type")
                                    .map(unescape_attr)
                                    .unwrap_or_default(),
                                start_at: attr_u32(reader, &prop, Ns::None, b"startAt")
                                    .unwrap_or(1),
                            }
                        }
                        b"buBlip" => paragraph.bullet = Bullet::Picture,
                        _ => {}
                    }
                    Ok(())
                })
            }
            b"r" => {
                let mut run = Run {
                    kind: RunKind::Text,
                    text: String::new(),
                    props: RunProps::default(),
                };
                parse_run_children(reader, &mut run)?;
                paragraph.runs.push(run);
                Ok(())
            }
            b"br" => {
                let mut run = Run {
                    kind: RunKind::LineBreak,
                    text: String::new(),
                    props: RunProps::default(),
                };
                parse_run_children(reader, &mut run)?;
                paragraph.runs.push(run);
                Ok(())
            }
            b"fld" => {
                let field_type = reader
                    .attr(&child, Ns::None, b"type")
                    .map(unescape_attr)
                    .unwrap_or_default();
                let mut run = Run {
                    kind: RunKind::Field(field_type),
                    text: String::new(),
                    props: RunProps::default(),
                };
                parse_run_children(reader, &mut run)?;
                paragraph.runs.push(run);
                Ok(())
            }
            _ => Ok(()),
        }
    })?;
    Ok(paragraph)
}

fn parse_run_children<'a>(reader: &mut Reader<'a>, run: &mut Run) -> XmlResult<()> {
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Dml {
            return Ok(());
        }
        match child.name.local {
            b"rPr" => parse_run_props(reader, &child, &mut run.props),
            b"t" => reader.text_content(&mut run.text),
            _ => Ok(()),
        }
    })
}

fn parse_run_props<'a>(
    reader: &mut Reader<'a>,
    start: &Start<'a>,
    props: &mut RunProps,
) -> XmlResult<()> {
    props.bold = attr_bool(reader, start, Ns::None, b"b");
    props.italic = attr_bool(reader, start, Ns::None, b"i");
    props.underline = reader
        .attr(start, Ns::None, b"u")
        .map(|value| value != b"none");
    props.strike = reader
        .attr(start, Ns::None, b"strike")
        .map(|value| value != b"noStrike");
    props.size = attr_u32(reader, start, Ns::None, b"sz");
    props.lang = reader.attr(start, Ns::None, b"lang").map(unescape_attr);
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Dml {
            return Ok(());
        }
        match child.name.local {
            b"hlinkClick" => {
                props.hyperlink = reader
                    .attr(&child, Ns::Rel, b"id")
                    .map(unescape_attr)
                    .filter(|id| !id.is_empty())
            }
            b"latin" => {
                props.typeface = reader
                    .attr(&child, Ns::None, b"typeface")
                    .map(unescape_attr)
            }
            _ => {}
        }
        Ok(())
    })
}

fn attr_bool(reader: &Reader<'_>, start: &Start<'_>, ns: Ns, local: &[u8]) -> Option<bool> {
    match reader.attr(start, ns, local)? {
        b"1" | b"true" | b"on" => Some(true),
        b"0" | b"false" | b"off" => Some(false),
        _ => None,
    }
}

fn attr_u32(reader: &Reader<'_>, start: &Start<'_>, ns: Ns, local: &[u8]) -> Option<u32> {
    std::str::from_utf8(reader.attr(start, ns, local)?)
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn attr_i64(reader: &Reader<'_>, start: &Start<'_>, ns: Ns, local: &[u8]) -> Option<i64> {
    std::str::from_utf8(reader.attr(start, ns, local)?)
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn attr_coordinate(reader: &Reader<'_>, start: &Start<'_>, ns: Ns, local: &[u8]) -> Option<Emu> {
    parse_coordinate(reader.attr(start, ns, local)?)
}

/// Parses `ST_Coordinate`: an integer number of EMUs or a number with a
/// unit suffix (`in`, `cm`, `mm`, `pt`, `pc`, `pi`) per 22.9.2.15.
pub fn parse_coordinate(value: &[u8]) -> Option<Emu> {
    let text = std::str::from_utf8(value).ok()?.trim();
    if let Ok(emu) = text.parse::<i64>() {
        return Some(emu);
    }
    let unit_start = text.rfind(|c: char| c.is_ascii_digit() || c == '.')? + 1;
    let (number, unit) = text.split_at(unit_start);
    let per_unit: f64 = match unit {
        "in" => 914_400.0,
        "cm" => 360_000.0,
        "mm" => 36_000.0,
        "pt" => 12_700.0,
        "pc" | "pi" => 152_400.0,
        _ => return None,
    };
    let number: f64 = number.parse().ok()?;
    Some((number * per_unit).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart""#;

    fn slide(body: &str) -> (SlideContent, SlideReport) {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld {NS}><p:cSld name="My Slide"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{body}</p:spTree></p:cSld></p:sld>"#
        );
        parse_slide(xml.as_bytes()).unwrap()
    }

    #[test]
    fn a_title_shape_with_runs_breaks_and_fields() {
        let (content, report) = slide(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title 1" descr="alt"><a:hlinkClick r:id="rId9"/></p:cNvPr><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm rot="5400000" flipH="1"><a:off x="838200" y="365125"/><a:ext cx="10515600" cy="1325563"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr lvl="1"><a:buChar char="•"/></a:pPr><a:r><a:rPr lang="en-US" b="1" sz="4400" u="sng"><a:latin typeface="Arial"/><a:hlinkClick r:id="rId2"/></a:rPr><a:t>Hello, </a:t></a:r><a:br/><a:fld id="{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}" type="slidenum"><a:rPr lang="en-US"/><a:t>3</a:t></a:fld><a:r><a:t>World &amp; more</a:t></a:r><a:endParaRPr lang="en-US"/></a:p><a:p><a:pPr><a:buNone/></a:pPr></a:p></p:txBody></p:sp>"#,
        );
        assert!(report.is_empty());
        assert_eq!(content.kind, SlideKind::Slide);
        assert_eq!(content.name.as_deref(), Some("My Slide"));
        assert!(content.show);
        assert_eq!(content.shapes.len(), 1);
        let shape = &content.shapes[0];
        assert_eq!(shape.id, 2);
        assert_eq!(shape.name, "Title 1");
        assert_eq!(shape.description.as_deref(), Some("alt"));
        assert_eq!(shape.hyperlink.as_deref(), Some("rId9"));
        assert_eq!(
            shape.placeholder,
            Some(Placeholder {
                kind: PlaceholderKind::CenterTitle,
                idx: 0
            })
        );
        assert!(shape.is_title());
        assert_eq!(
            shape.transform,
            Some(Transform {
                x: 838200,
                y: 365125,
                cx: 10515600,
                cy: 1325563,
                rot: 5400000,
                flip_h: true,
                flip_v: false
            })
        );
        let body = shape.text_body().unwrap();
        assert_eq!(body.paragraphs.len(), 2);
        let first = &body.paragraphs[0];
        assert_eq!(first.level, 1);
        assert_eq!(first.bullet, Bullet::Char("•".into()));
        assert_eq!(first.runs.len(), 4);
        assert_eq!(first.runs[0].text, "Hello, ");
        assert_eq!(
            first.runs[0].props,
            RunProps {
                bold: Some(true),
                italic: None,
                underline: Some(true),
                strike: None,
                size: Some(4400),
                hyperlink: Some("rId2".into()),
                lang: Some("en-US".into()),
                typeface: Some("Arial".into())
            }
        );
        assert_eq!(first.runs[1].kind, RunKind::LineBreak);
        assert_eq!(first.runs[2].kind, RunKind::Field("slidenum".into()));
        assert_eq!(first.runs[2].text, "3");
        assert_eq!(first.text(), "Hello, \n3World & more");
        assert_eq!(body.paragraphs[1].bullet, Bullet::None);
        assert!(body.paragraphs[1].is_empty());
        assert_eq!(body.text(), "Hello, \n3World & more\n");
        assert_eq!(content.title().as_deref(), Some("Hello, \n3World & more\n"));
    }

    #[test]
    fn pictures_groups_tables_charts_and_unknown_graphics() {
        let (content, report) = slide(
            r#"<p:pic><p:nvPicPr><p:cNvPr id="4" name="Picture 3" hidden="1"/><p:cNvPicPr/><p:nvPr><a:videoFile r:link="rId7"/></p:nvPr></p:nvPicPr><p:blipFill><a:blip r:embed="rId3"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="1" y="2"/><a:ext cx="3" cy="4"/></a:xfrm></p:spPr></p:pic>
<p:grpSp><p:nvGrpSpPr><p:cNvPr id="5" name="Group 4"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="10" y="20"/><a:ext cx="30" cy="40"/><a:chOff x="1" y="2"/><a:chExt cx="3" cy="4"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="6" name="In group"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>inside</a:t></a:r></a:p></p:txBody></p:sp><p:cxnSp><p:nvCxnSpPr><p:cNvPr id="7" name="Connector"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr><p:spPr/></p:cxnSp></p:grpSp>
<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="8" name="Table 7"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr><p:ph type="tbl" idx="1"/></p:nvPr></p:nvGraphicFramePr><p:xfrm><a:off x="5" y="6"/><a:ext cx="7" cy="8"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1"/><a:tblGrid><a:gridCol w="100"/><a:gridCol w="200"/></a:tblGrid><a:tr h="50"><a:tc gridSpan="2"><a:txBody><a:bodyPr/><a:p><a:r><a:t>Head</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc><a:tc hMerge="1"><a:txBody><a:bodyPr/><a:p/></a:txBody></a:tc></a:tr><a:tr h="60"><a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>a</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>b</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>
<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="9" name="Chart 8"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="1" cy="1"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rId4"/></a:graphicData></a:graphic></p:graphicFrame>
<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="10" name="Mystery"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="1" cy="1"/></p:xfrm><a:graphic><a:graphicData uri="urn:mystery"><a:foo/></a:graphicData></a:graphic></p:graphicFrame>"#,
        );
        assert_eq!(report.unknown_graphics, vec!["urn:mystery"]);
        assert_eq!(report.unknown_elements, 0);
        assert_eq!(content.shapes.len(), 5);
        let pic = &content.shapes[0];
        assert!(pic.hidden);
        assert_eq!(
            pic.content,
            Content::Picture(Picture {
                embed: Some("rId3".into()),
                link: None,
                media: Some("rId7".into())
            })
        );
        assert_eq!(
            pic.transform,
            Some(Transform {
                x: 1,
                y: 2,
                cx: 3,
                cy: 4,
                rot: 0,
                flip_h: false,
                flip_v: false
            })
        );
        let Content::Group(children, space) = &content.shapes[1].content else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        assert!(children[0].text_box);
        assert_eq!(children[0].text_body().unwrap().text(), "inside");
        assert_eq!(children[1].content, Content::Connector);
        assert_eq!(
            *space,
            Some(ChildSpace {
                x: 1,
                y: 2,
                cx: 3,
                cy: 4
            })
        );
        assert_eq!(
            content.shapes[1].transform,
            Some(Transform {
                x: 10,
                y: 20,
                cx: 30,
                cy: 40,
                rot: 0,
                flip_h: false,
                flip_v: false
            })
        );
        let Content::Table(table) = &content.shapes[2].content else {
            panic!()
        };
        assert_eq!(table.column_widths, vec![100, 200]);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].cells[0].grid_span, 2);
        assert!(table.rows[0].cells[1].h_merge);
        assert!(!table.rows[0].cells[1].is_origin());
        assert_eq!(table.rows[1].cells[1].body.text(), "b");
        assert_eq!(
            content.shapes[2].placeholder,
            Some(Placeholder {
                kind: PlaceholderKind::Table,
                idx: 1
            })
        );
        assert_eq!(
            content.shapes[2].transform,
            Some(Transform {
                x: 5,
                y: 6,
                cx: 7,
                cy: 8,
                rot: 0,
                flip_h: false,
                flip_v: false
            })
        );
        assert_eq!(
            content.shapes[3].content,
            Content::Chart(Some("rId4".into()))
        );
        assert_eq!(
            content.shapes[4].content,
            Content::UnknownGraphic("urn:mystery".into())
        );
        let walked: Vec<u32> = content.walk().map(|shape| shape.id).collect();
        assert_eq!(walked, [4, 5, 6, 7, 8, 9, 10]);
        assert!(content.title().is_none());
    }

    #[test]
    fn alternate_content_falls_back_and_unknown_namespaces_are_counted() {
        let (content, report) = slide(
            r#"<mc:AlternateContent xmlns:p14="urn:p14"><mc:Choice Requires="p14"><p:sp><p:nvSpPr><p:cNvPr id="2" name="New"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>choice</a:t></a:r></a:p></p:txBody></p:sp></mc:Choice><mc:Fallback><p:sp><p:nvSpPr><p:cNvPr id="2" name="Old"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>fallback</a:t></a:r></a:p></p:txBody></p:sp></mc:Fallback></mc:AlternateContent><x:thing xmlns:x="urn:x"><x:inner/></x:thing><p:extLst><p:ext uri="{1}"><y:z xmlns:y="urn:y"/></p:ext></p:extLst>"#,
        );
        assert_eq!(content.shapes.len(), 1);
        assert_eq!(content.shapes[0].name, "Old");
        assert_eq!(content.shapes[0].text_body().unwrap().text(), "fallback");
        assert_eq!(report.unknown_elements, 1);
    }

    #[test]
    fn hidden_slides_layouts_and_notes_roots_parse() {
        let xml = format!(r#"<p:sld {NS} show="0"><p:cSld><p:spTree/></p:cSld></p:sld>"#);
        let (content, _) = parse_slide(xml.as_bytes()).unwrap();
        assert!(!content.show);
        assert!(content.shapes.is_empty());
        let xml = format!(
            r#"<p:notes {NS}><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Notes Placeholder"/><p:cNvSpPr/><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>speaker notes</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#
        );
        let (content, _) = parse_slide(xml.as_bytes()).unwrap();
        assert_eq!(content.kind, SlideKind::Notes);
        assert_eq!(
            content.shapes[0].placeholder.as_ref().unwrap().kind,
            PlaceholderKind::Body
        );
        let xml = format!(
            r#"<p:sldLayout {NS} type="title"><p:cSld name="Title Slide"><p:spTree/></p:cSld></p:sldLayout>"#
        );
        assert_eq!(
            parse_slide(xml.as_bytes()).unwrap().0.kind,
            SlideKind::Layout
        );
        let strict = r#"<p:sldMaster xmlns:p="http://purl.oclc.org/ooxml/presentationml/main"><p:cSld><p:spTree/></p:cSld></p:sldMaster>"#;
        assert_eq!(
            parse_slide(strict.as_bytes()).unwrap().0.kind,
            SlideKind::Master
        );
        let wrong = format!(r#"<p:presentation {NS}/>"#);
        assert_eq!(
            parse_slide(wrong.as_bytes()).unwrap_err().msg,
            "root element is not a slide, layout, master or notes element"
        );
        assert!(parse_slide(b"<p:sld").is_err());
    }

    #[test]
    fn coordinates_accept_units() {
        assert_eq!(parse_coordinate(b"914400"), Some(914400));
        assert_eq!(parse_coordinate(b"-5"), Some(-5));
        assert_eq!(parse_coordinate(b"1in"), Some(914400));
        assert_eq!(parse_coordinate(b"2.54cm"), Some(914400));
        assert_eq!(parse_coordinate(b"72pt"), Some(914400));
        assert_eq!(parse_coordinate(b"10mm"), Some(360000));
        assert_eq!(parse_coordinate(b"abc"), None);
        assert_eq!(parse_coordinate(b"3px"), None);
    }
}
