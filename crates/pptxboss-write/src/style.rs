//! DrawingML for colors, runs and paragraph properties (ECMA-376 Part 1,
//! clauses 20.1 and 21.1).

use crate::xml::{attr, text};
use crate::{Align, Color, Paragraph, Run};

const BULLET_FONT: &str = r#"<a:buFont typeface="Arial" panose="020B0604020202020204" pitchFamily="34" charset="0"/><a:buChar char="&#8226;"/>"#;

/// `a:srgbClr` or `a:schemeClr`.
pub fn color_xml(color: Color) -> String {
    match color {
        Color::Rgb(rgb) => format!(r#"<a:srgbClr val="{}"/>"#, rgb.to_hex()),
        Color::Scheme(slot) => format!(r#"<a:schemeClr val="{}"/>"#, slot.xml()),
    }
}

/// One `a:r`; `link_rel` is the relationship id of the run's hyperlink.
pub fn run_xml(run: &Run, link_rel: Option<&str>) -> String {
    let mut props = String::from(r#"<a:rPr lang="en-US""#);
    if let Some(size) = run.size {
        props.push_str(&format!(r#" sz="{}""#, size * 100));
    }
    if run.bold {
        props.push_str(r#" b="1""#);
    }
    if run.italic {
        props.push_str(r#" i="1""#);
    }
    if run.underline {
        props.push_str(r#" u="sng""#);
    }
    if run.strike {
        props.push_str(r#" strike="sngStrike""#);
    }
    props.push_str(r#" dirty="0""#);
    let mut children = String::new();
    if let Some(color) = run.color {
        children.push_str(&format!("<a:solidFill>{}</a:solidFill>", color_xml(color)));
    }
    if let Some(font) = &run.font {
        children.push_str(&format!(r#"<a:latin typeface="{}"/>"#, attr(font)));
    }
    if let Some(rel) = link_rel {
        children.push_str(&format!(r#"<a:hlinkClick r:id="{rel}"/>"#));
    }
    match children.is_empty() {
        true => props.push_str("/>"),
        false => props.push_str(&format!(">{children}</a:rPr>")),
    }
    format!("<a:r>{props}<a:t>{}</a:t></a:r>", text(&run.text))
}

/// Where a paragraph lives: a placeholder body inherits bullets from the
/// master, a free text box spells them out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParagraphMode {
    Body,
    Box,
}

fn align_attr(align: Align) -> &'static str {
    match align {
        Align::Left => "",
        Align::Center => r#" algn="ctr""#,
        Align::Right => r#" algn="r""#,
        Align::Justify => r#" algn="just""#,
    }
}

fn spacing_xml(paragraph: &Paragraph) -> String {
    let mut xml = String::new();
    if let Some(points) = paragraph.space_before {
        xml.push_str(&format!(
            r#"<a:spcBef><a:spcPts val="{}"/></a:spcBef>"#,
            points * 100
        ));
    }
    if let Some(points) = paragraph.space_after {
        xml.push_str(&format!(
            r#"<a:spcAft><a:spcPts val="{}"/></a:spcAft>"#,
            points * 100
        ));
    }
    xml
}

fn level_attr(level: u8) -> String {
    match level {
        0 => String::new(),
        level => format!(r#" lvl="{level}""#),
    }
}

/// The `a:pPr` of a paragraph, or nothing when every property is inherited.
pub fn ppr_xml(paragraph: &Paragraph, mode: ParagraphMode) -> String {
    let level = paragraph.level as i64;
    let align = align_attr(paragraph.align);
    let spacing = spacing_xml(paragraph);
    let (attrs, bullet) = match (mode, paragraph.bullet) {
        (ParagraphMode::Body, true) => (level_attr(paragraph.level), String::new()),
        (ParagraphMode::Body, false) => (
            format!(r#" marL="{}" lvl="{level}" indent="0""#, level * 457_200),
            "<a:buNone/>".to_string(),
        ),
        (ParagraphMode::Box, true) => (
            format!(
                r#" marL="{}" lvl="{level}" indent="-228600""#,
                228_600 + level * 457_200
            ),
            BULLET_FONT.to_string(),
        ),
        (ParagraphMode::Box, false) => (level_attr(paragraph.level), String::new()),
    };
    if attrs.is_empty() && align.is_empty() && spacing.is_empty() && bullet.is_empty() {
        return String::new();
    }
    let children = format!("{spacing}{bullet}");
    match children.is_empty() {
        true => format!("<a:pPr{attrs}{align}/>"),
        false => format!("<a:pPr{attrs}{align}>{children}</a:pPr>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SchemeColor;

    #[test]
    fn colors() {
        assert_eq!(
            color_xml(Color::rgb(1, 2, 3)),
            r#"<a:srgbClr val="010203"/>"#
        );
        assert_eq!(
            color_xml(Color::Scheme(SchemeColor::Accent1)),
            r#"<a:schemeClr val="accent1"/>"#
        );
    }

    #[test]
    fn run_children_are_ordered() {
        let run = Run::text("x")
            .bold()
            .underline()
            .strike()
            .size(20)
            .color(Color::rgb(0, 0, 0))
            .font("Georgia");
        assert_eq!(
            run_xml(&run, Some("rId7")),
            r#"<a:r><a:rPr lang="en-US" sz="2000" b="1" u="sng" strike="sngStrike" dirty="0"><a:solidFill><a:srgbClr val="000000"/></a:solidFill><a:latin typeface="Georgia"/><a:hlinkClick r:id="rId7"/></a:rPr><a:t>x</a:t></a:r>"#
        );
        assert_eq!(
            run_xml(&Run::text("a<b"), None),
            r#"<a:r><a:rPr lang="en-US" dirty="0"/><a:t>a&lt;b</a:t></a:r>"#
        );
    }

    #[test]
    fn paragraph_properties() {
        assert_eq!(ppr_xml(&Paragraph::bullet("x", 0), ParagraphMode::Body), "");
        assert_eq!(
            ppr_xml(&Paragraph::bullet("x", 2), ParagraphMode::Body),
            r#"<a:pPr lvl="2"/>"#
        );
        assert_eq!(
            ppr_xml(
                &Paragraph::text("x").align(Align::Center).space_before(6),
                ParagraphMode::Body
            ),
            r#"<a:pPr marL="0" lvl="0" indent="0" algn="ctr"><a:spcBef><a:spcPts val="600"/></a:spcBef><a:buNone/></a:pPr>"#
        );
        assert_eq!(
            ppr_xml(&Paragraph::bullet("x", 1), ParagraphMode::Box),
            r#"<a:pPr marL="685800" lvl="1" indent="-228600"><a:buFont typeface="Arial" panose="020B0604020202020204" pitchFamily="34" charset="0"/><a:buChar char="&#8226;"/></a:pPr>"#
        );
        assert_eq!(ppr_xml(&Paragraph::text("x"), ParagraphMode::Box), "");
        assert_eq!(
            ppr_xml(
                &Paragraph::text("x").align(Align::Right),
                ParagraphMode::Box
            ),
            r#"<a:pPr algn="r"/>"#
        );
    }
}
