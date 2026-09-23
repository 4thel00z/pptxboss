//! DrawingML for colors, fills, backgrounds, color maps, runs, paragraph
//! properties and the theme part (ECMA-376 Part 1, clauses 20.1 and 21.1).

use crate::layout::space_before;
use crate::xml::{attr, text, DECL};
use crate::{Align, Background, Color, Paragraph, Run, SchemeColor, Theme};

pub const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const FMT_SCHEME: &str = r#"<a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>"#;

const BULLET_FONT: &str = r#"<a:buClr><a:schemeClr val="accent1"/></a:buClr><a:buFont typeface="Arial" panose="020B0604020202020204" pitchFamily="34" charset="0"/><a:buChar char="&#8226;"/>"#;

/// The nine levels of the master's body style at `scale` percent: indents,
/// line spacing, space before, accent bullets and sizes from the theme's
/// type scale. The master carries it at 100; a text box the layout engine
/// places, or a body it shrinks, carries its own copy.
pub fn body_levels_xml(theme: &Theme, scale: u32) -> String {
    (1..=9u8)
        .map(|level| {
            let index = level - 1;
            let mar_l = 228_600 + index as i64 * 457_200;
            let size = (theme.scale.body_level(index) * scale + 50) / 100;
            let before = (space_before(index) * scale + 50) / 100;
            format!(
                r#"<a:lvl{level}pPr marL="{mar_l}" indent="-228600" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPts val="{}"/></a:spcBef>{BULLET_FONT}<a:defRPr sz="{}" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl{level}pPr>"#,
                before * 100,
                size.max(1) * 100
            )
        })
        .collect()
}

/// `a:srgbClr` or `a:schemeClr`; `swapped` is set when the theme is
/// inverted, so a scheme slot points at the token that holds its value.
pub fn color_xml(color: Color, swapped: bool) -> String {
    match color {
        Color::Rgb(rgb) => format!(r#"<a:srgbClr val="{}"/>"#, rgb.to_hex()),
        Color::Scheme(slot) if swapped => {
            format!(r#"<a:schemeClr val="{}"/>"#, slot.swapped().xml())
        }
        Color::Scheme(slot) => format!(r#"<a:schemeClr val="{}"/>"#, slot.xml()),
    }
}

/// One `a:r`; `link_rel` is the relationship id of the run's hyperlink.
pub fn run_xml(run: &Run, link_rel: Option<&str>, swapped: bool) -> String {
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
        children.push_str(&format!(
            "<a:solidFill>{}</a:solidFill>",
            color_xml(color, swapped)
        ));
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

/// The fill element of a background; `blip_rel` is the image relationship of a picture.
pub fn fill_xml(background: &Background, blip_rel: Option<&str>, swapped: bool) -> String {
    match background {
        Background::Solid(color) => {
            format!("<a:solidFill>{}</a:solidFill>", color_xml(*color, swapped))
        }
        Background::Gradient { stops, angle } => {
            let stops: String = stops
                .iter()
                .map(|stop| {
                    format!(
                        r#"<a:gs pos="{}">{}</a:gs>"#,
                        stop.position.min(100) as u32 * 1000,
                        color_xml(stop.color, swapped)
                    )
                })
                .collect();
            format!(
                r#"<a:gradFill rotWithShape="1"><a:gsLst>{stops}</a:gsLst><a:lin ang="{}" scaled="0"/></a:gradFill>"#,
                *angle as u32 * 60_000
            )
        }
        Background::Picture(_) => format!(
            r#"<a:blipFill dpi="0" rotWithShape="1"><a:blip r:embed="{}"/><a:srcRect/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"#,
            blip_rel.unwrap_or_default()
        ),
    }
}

/// The `p:bg` of a master, layout or slide; None is the theme background.
pub fn bg_xml(background: Option<&Background>, blip_rel: Option<&str>, swapped: bool) -> String {
    match background {
        None => {
            r#"<p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>"#.to_string()
        }
        Some(background) => format!(
            "<p:bg><p:bgPr>{}<a:effectLst/></p:bgPr></p:bg>",
            fill_xml(background, blip_rel, swapped)
        ),
    }
}

/// The `p:clrMap` attributes: the standard mapping, or light and dark
/// swapped for an inverted layout or slide.
pub fn clr_map_attrs(inverted: bool) -> &'static str {
    match inverted {
        false => {
            r#"bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink""#
        }
        true => {
            r#"bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink""#
        }
    }
}

/// The color map override of a layout or slide: the master mapping, or
/// light and dark swapped when `inverted` is set.
pub fn clr_map_ovr(inverted: bool) -> String {
    match inverted {
        false => "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>".to_string(),
        true => format!(
            "<p:clrMapOvr><a:overrideClrMapping {}/></p:clrMapOvr>",
            clr_map_attrs(true)
        ),
    }
}

/// `ppt/theme/theme1.xml`. An inverted theme writes its dark colors into
/// the light slots and the reverse, so every renderer paints the
/// background dark and the text light without relying on the color map.
pub fn theme_xml(theme: &Theme) -> String {
    let name = attr(&theme.name);
    let slots: String = SchemeColor::ALL
        .into_iter()
        .map(|slot| {
            let token = slot.xml();
            let source = match theme.inverted {
                true => slot.swapped(),
                false => slot,
            };
            format!(
                r#"<a:{token}><a:srgbClr val="{}"/></a:{token}>"#,
                theme.colors.get(source).to_hex()
            )
        })
        .collect();
    let major = attr(&theme.major_font);
    let minor = attr(&theme.minor_font);
    format!(
        r#"{DECL}<a:theme xmlns:a="{NS_A}" name="{name}"><a:themeElements><a:clrScheme name="{name}">{slots}</a:clrScheme><a:fontScheme name="{name}"><a:majorFont><a:latin typeface="{major}"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="{minor}"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="{name}">{FMT_SCHEME}</a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GradientStop;

    #[test]
    fn colors() {
        assert_eq!(
            color_xml(Color::rgb(1, 2, 3), false),
            r#"<a:srgbClr val="010203"/>"#
        );
        assert_eq!(
            color_xml(Color::Scheme(SchemeColor::Accent1), true),
            r#"<a:schemeClr val="accent1"/>"#
        );
        assert_eq!(
            color_xml(Color::Scheme(SchemeColor::Dark1), false),
            r#"<a:schemeClr val="dk1"/>"#
        );
        assert_eq!(
            color_xml(Color::Scheme(SchemeColor::Dark1), true),
            r#"<a:schemeClr val="lt1"/>"#
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
            run_xml(&run, Some("rId7"), false),
            r#"<a:r><a:rPr lang="en-US" sz="2000" b="1" u="sng" strike="sngStrike" dirty="0"><a:solidFill><a:srgbClr val="000000"/></a:solidFill><a:latin typeface="Georgia"/><a:hlinkClick r:id="rId7"/></a:rPr><a:t>x</a:t></a:r>"#
        );
        assert_eq!(
            run_xml(&Run::text("a<b"), None, false),
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
            r#"<a:pPr marL="685800" lvl="1" indent="-228600"><a:buClr><a:schemeClr val="accent1"/></a:buClr><a:buFont typeface="Arial" panose="020B0604020202020204" pitchFamily="34" charset="0"/><a:buChar char="&#8226;"/></a:pPr>"#
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

    #[test]
    fn fills_backgrounds_and_overrides() {
        let gradient = Background::gradient(
            vec![
                GradientStop {
                    position: 0,
                    color: Color::rgb(0, 0, 0),
                },
                GradientStop {
                    position: 50,
                    color: Color::Scheme(SchemeColor::Accent1),
                },
            ],
            90,
        );
        assert_eq!(
            fill_xml(&gradient, None, false),
            r#"<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="000000"/></a:gs><a:gs pos="50000"><a:schemeClr val="accent1"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill>"#
        );
        assert_eq!(
            fill_xml(&Background::picture(vec![]), Some("rId9"), false),
            r#"<a:blipFill dpi="0" rotWithShape="1"><a:blip r:embed="rId9"/><a:srcRect/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"#
        );
        assert_eq!(
            bg_xml(None, None, false),
            r#"<p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>"#
        );
        assert_eq!(
            bg_xml(Some(&Background::solid(Color::rgb(1, 2, 3))), None, false),
            r#"<p:bg><p:bgPr><a:solidFill><a:srgbClr val="010203"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>"#
        );
        assert_eq!(
            clr_map_ovr(false),
            "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>"
        );
        assert!(clr_map_ovr(true)
            .starts_with(r#"<p:clrMapOvr><a:overrideClrMapping bg1="dk1" tx1="lt1""#));
    }

    #[test]
    fn body_levels_follow_the_type_scale() {
        let full = body_levels_xml(&Theme::office(), 100);
        assert!(full.starts_with(r#"<a:lvl1pPr marL="228600" indent="-228600""#));
        assert!(full.contains(r#"<a:spcBef><a:spcPts val="1000"/></a:spcBef><a:buClr><a:schemeClr val="accent1"/></a:buClr>"#));
        assert!(full.contains(r#"<a:defRPr sz="2800" kern="1200">"#));
        assert!(full.contains(r#"<a:lvl2pPr marL="685800" indent="-228600""#));
        assert!(full.contains(r#"<a:spcPts val="500"/></a:spcBef>"#));
        assert!(full.contains(r#"<a:defRPr sz="1800" kern="1200">"#));
        assert_eq!(full.matches("<a:lvl").count(), 9);
        let half = body_levels_xml(&Theme::office(), 50);
        assert!(half.contains(r#"<a:defRPr sz="1400" kern="1200">"#));
        assert!(half.contains(r#"<a:spcPts val="500"/></a:spcBef>"#));
        assert!(half.contains(r#"<a:defRPr sz="900" kern="1200">"#));
    }

    #[test]
    fn theme_part_and_color_map() {
        let xml = theme_xml(&Theme::dark());
        assert!(xml.contains(
            r#"<a:clrScheme name="dark"><a:dk1><a:srgbClr val="F5F5F5"/></a:dk1><a:lt1><a:srgbClr val="1E1E1E"/></a:lt1><a:dk2><a:srgbClr val="D0D0D0"/></a:dk2><a:lt2><a:srgbClr val="2D2D2D"/></a:lt2><a:accent1><a:srgbClr val="4FC3F7"/>"#
        ));
        assert!(theme_xml(&Theme::office()).contains(
            r#"<a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1>"#
        ));
        assert!(xml.contains(r#"<a:majorFont><a:latin typeface="Calibri"/>"#));
        assert!(clr_map_attrs(false).starts_with(r#"bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2""#));
        assert!(clr_map_attrs(true).starts_with(r#"bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2""#));
    }
}
