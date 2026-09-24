//! Serializes a [`Presentation`] into package parts (ECMA-376 Part 1,
//! clauses 13, 19 and 21; Part 2, clauses 7 and 8).

use crate::fonts::{self, FontInfo};
use crate::layout::{self, Element, Grid, Planned};
use crate::style::{
    bg_xml, body_levels_xml, clr_map_attrs, clr_map_ovr, ppr_xml, run_xml, theme_xml,
    ParagraphMode, NS_A,
};
use crate::xml::{attr, text, DECL};
use crate::{Background, Error, ImageFormat, Layout, Paragraph, Presentation, Rect, Result, Shape};

const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_PML: &str = "application/vnd.openxmlformats-officedocument.presentationml.";
const TABLE_STYLE: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";
/// Cells draw no left, right or top line.
const TABLE_OPEN_SIDES: &str =
    r#"<a:lnL><a:noFill/></a:lnL><a:lnR><a:noFill/></a:lnR><a:lnT><a:noFill/></a:lnT>"#;
/// Header cells end in a thick accent rule.
const TABLE_HEAD_RULE: &str = r#"<a:lnB w="28575" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="accent1"/></a:solidFill><a:prstDash val="solid"/></a:lnB>"#;
/// Body cells end in a faint text-colored rule.
const TABLE_ROW_RULE: &str = r#"<a:lnB w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="tx1"><a:alpha val="30000"/></a:schemeClr></a:solidFill><a:prstDash val="solid"/></a:lnB>"#;

/// One part ready to be zipped.
pub struct PartOut {
    pub name: String,
    pub data: Vec<u8>,
    pub compress: bool,
}

/// An image part to write, numbered in order of registration.
struct Media<'a> {
    name: String,
    format: ImageFormat,
    data: &'a [u8],
}

/// One embedded font ready to write.
struct FontPart {
    info: FontInfo,
    eot: Vec<u8>,
}

struct Ctx<'a> {
    presentation: &'a Presentation,
    /// Slides after the layout engine added its continuation slides.
    slide_count: usize,
    fonts: Vec<FontPart>,
    has_notes: bool,
    media: Vec<Media<'a>>,
    /// Slide pictures seen so far, for error messages.
    pictures: usize,
}

impl<'a> Ctx<'a> {
    /// Registers image bytes as the next media part; returns the relationship target.
    fn add_media(&mut self, data: &'a [u8]) -> Option<String> {
        let format = ImageFormat::sniff(data)?;
        let index = self.media.len() + 1;
        self.media.push(Media {
            name: format!("ppt/media/image{index}.{}", format.extension()),
            format,
            data,
        });
        Some(format!("../media/image{index}.{}", format.extension()))
    }

    /// The `p:bg` for a background, registering its picture when it has one.
    fn background_xml(
        &mut self,
        background: Option<&'a Background>,
        rels: &mut Rels,
    ) -> Result<String> {
        let Some(Background::Picture(data)) = background else {
            return Ok(bg_xml(background, None, self.presentation.theme.inverted));
        };
        let target = self
            .add_media(data)
            .ok_or(Error::UnsupportedBackgroundImage)?;
        let rel = rels.add(&format!("{REL}image"), target);
        Ok(bg_xml(
            background,
            Some(&rel),
            self.presentation.theme.inverted,
        ))
    }
}

pub fn build(presentation: &Presentation) -> Result<Vec<PartOut>> {
    let grid = Grid::for_size(presentation.size);
    let planned: Vec<Planned<'_>> = presentation
        .slides
        .iter()
        .flat_map(|slide| layout::plan(slide, &grid, &presentation.theme))
        .collect();
    let has_notes = planned.iter().any(|slide| slide.notes.is_some());
    let fonts = presentation
        .theme
        .embedded_fonts
        .iter()
        .map(|data| {
            let info = fonts::info(data)?;
            let eot = fonts::eot(data, &info);
            Ok(FontPart { info, eot })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut ctx = Ctx {
        presentation,
        slide_count: planned.len(),
        fonts,
        has_notes,
        media: Vec::new(),
        pictures: 0,
    };
    let mut parts: Vec<PartOut> = Vec::new();
    let mut xml_parts: Vec<(String, String)> = Vec::new();

    let push_xml = |parts: &mut Vec<PartOut>,
                    xml_parts: &mut Vec<(String, String)>,
                    name: &str,
                    content_type: &str,
                    body: String| {
        xml_parts.push((format!("/{name}"), content_type.to_string()));
        parts.push(PartOut {
            name: name.to_string(),
            data: body.into_bytes(),
            compress: true,
        });
    };

    push_xml(
        &mut parts,
        &mut xml_parts,
        "ppt/presentation.xml",
        &format!("{CT_PML}presentation.main+xml"),
        presentation_xml(&ctx),
    );
    parts.push(rels_part(
        "ppt/_rels/presentation.xml.rels",
        &presentation_rels(&ctx),
    ));
    push_xml(
        &mut parts,
        &mut xml_parts,
        "ppt/presProps.xml",
        &format!("{CT_PML}presProps+xml"),
        format!(r#"{DECL}<p:presentationPr xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"/>"#),
    );
    push_xml(
        &mut parts,
        &mut xml_parts,
        "ppt/viewProps.xml",
        &format!("{CT_PML}viewProps+xml"),
        format!(
            r#"{DECL}<p:viewPr xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:normalViewPr><p:restoredLeft sz="15620"/><p:restoredTop sz="94660"/></p:normalViewPr><p:gridSpacing cx="76200" cy="76200"/></p:viewPr>"#
        ),
    );
    push_xml(
        &mut parts,
        &mut xml_parts,
        "ppt/tableStyles.xml",
        &format!("{CT_PML}tableStyles+xml"),
        format!(r#"{DECL}<a:tblStyleLst xmlns:a="{NS_A}" def="{TABLE_STYLE}"/>"#),
    );
    push_xml(
        &mut parts,
        &mut xml_parts,
        "ppt/theme/theme1.xml",
        "application/vnd.openxmlformats-officedocument.theme+xml",
        theme_xml(&presentation.theme),
    );

    let (master, master_rels) = master_xml(&mut ctx, &grid)?;
    push_xml(
        &mut parts,
        &mut xml_parts,
        "ppt/slideMasters/slideMaster1.xml",
        &format!("{CT_PML}slideMaster+xml"),
        master,
    );
    parts.push(rels_part(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        &master_rels,
    ));
    for layout in [
        Layout::Title,
        Layout::TitleAndContent,
        Layout::TitleOnly,
        Layout::Blank,
    ] {
        let index = layout.index();
        let (xml, layout_rels) = layout_xml(&mut ctx, layout, &grid)?;
        push_xml(
            &mut parts,
            &mut xml_parts,
            &format!("ppt/slideLayouts/slideLayout{index}.xml"),
            &format!("{CT_PML}slideLayout+xml"),
            xml,
        );
        parts.push(rels_part(
            &format!("ppt/slideLayouts/_rels/slideLayout{index}.xml.rels"),
            &layout_rels,
        ));
    }
    if has_notes {
        push_xml(
            &mut parts,
            &mut xml_parts,
            "ppt/notesMasters/notesMaster1.xml",
            &format!("{CT_PML}notesMaster+xml"),
            notes_master_xml(presentation.theme.inverted),
        );
        parts.push(rels_part(
            "ppt/notesMasters/_rels/notesMaster1.xml.rels",
            &[(
                "rId1".into(),
                format!("{REL}theme"),
                "../theme/theme1.xml".into(),
            )],
        ));
    }

    for (i, slide) in planned.iter().enumerate() {
        let n = i + 1;
        let (xml, slide_rels) = slide_xml(&mut ctx, slide, n, &grid)?;
        push_xml(
            &mut parts,
            &mut xml_parts,
            &format!("ppt/slides/slide{n}.xml"),
            &format!("{CT_PML}slide+xml"),
            xml,
        );
        parts.push(rels_part(
            &format!("ppt/slides/_rels/slide{n}.xml.rels"),
            &slide_rels,
        ));
        if let Some(notes) = slide.notes {
            push_xml(
                &mut parts,
                &mut xml_parts,
                &format!("ppt/notesSlides/notesSlide{n}.xml"),
                &format!("{CT_PML}notesSlide+xml"),
                notes_xml(notes),
            );
            parts.push(rels_part(
                &format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                &[
                    (
                        "rId1".into(),
                        format!("{REL}notesMaster"),
                        "../notesMasters/notesMaster1.xml".into(),
                    ),
                    (
                        "rId2".into(),
                        format!("{REL}slide"),
                        format!("../slides/slide{n}.xml"),
                    ),
                ],
            ));
        }
    }

    for media in &ctx.media {
        parts.push(PartOut {
            name: media.name.clone(),
            data: media.data.to_vec(),
            compress: false,
        });
    }
    for (i, font) in ctx.fonts.iter().enumerate() {
        parts.push(PartOut {
            name: format!("ppt/fonts/font{}.fntdata", i + 1),
            data: font.eot.clone(),
            compress: true,
        });
    }

    push_xml(
        &mut parts,
        &mut xml_parts,
        "docProps/core.xml",
        "application/vnd.openxmlformats-package.core-properties+xml",
        core_xml(presentation),
    );
    push_xml(
        &mut parts,
        &mut xml_parts,
        "docProps/app.xml",
        "application/vnd.openxmlformats-officedocument.extended-properties+xml",
        app_xml(presentation, &planned),
    );

    parts.insert(0, rels_part("_rels/.rels", &[
        ("rId1".into(), format!("{REL}officeDocument"), "ppt/presentation.xml".into()),
        ("rId2".into(), "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties".into(), "docProps/core.xml".into()),
        ("rId3".into(), format!("{REL}extended-properties"), "docProps/app.xml".into()),
    ]));
    parts.insert(
        0,
        PartOut {
            name: "[Content_Types].xml".into(),
            data: content_types_xml(&xml_parts, &ctx.media, !ctx.fonts.is_empty()).into_bytes(),
            compress: true,
        },
    );
    Ok(parts)
}

fn rels_part(name: &str, rels: &[(String, String, String)]) -> PartOut {
    let mut xml = format!(r#"{DECL}<Relationships xmlns="{PKG_REL}">"#);
    for (id, rel_type, target) in rels {
        let external = target.contains("://") || rel_type.ends_with("/hyperlink");
        let mode = match external {
            true => r#" TargetMode="External""#,
            false => "",
        };
        xml.push_str(&format!(
            r#"<Relationship Id="{id}" Type="{rel_type}" Target="{}"{mode}/>"#,
            attr(target)
        ));
    }
    xml.push_str("</Relationships>");
    PartOut {
        name: name.to_string(),
        data: xml.into_bytes(),
        compress: true,
    }
}

fn content_types_xml(xml_parts: &[(String, String)], media: &[Media<'_>], fonts: bool) -> String {
    let mut xml = format!(
        r#"{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#
    );
    if fonts {
        xml.push_str(r#"<Default Extension="fntdata" ContentType="application/x-fontdata"/>"#);
    }
    let mut seen: Vec<ImageFormat> = Vec::new();
    for media in media {
        let format = media.format;
        if seen.contains(&format) {
            continue;
        }
        seen.push(format);
        xml.push_str(&format!(
            r#"<Default Extension="{}" ContentType="{}"/>"#,
            format.extension(),
            format.content_type()
        ));
    }
    for (name, content_type) in xml_parts {
        xml.push_str(&format!(
            r#"<Override PartName="{}" ContentType="{content_type}"/>"#,
            attr(name)
        ));
    }
    xml.push_str("</Types>");
    xml
}

fn presentation_xml(ctx: &Ctx<'_>) -> String {
    let size = ctx.presentation.size;
    let embedded = match ctx.fonts.is_empty() {
        true => "",
        false => r#" embedTrueTypeFonts="1""#,
    };
    let mut xml = format!(
        r#"{DECL}<p:presentation xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"{embedded} saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>"#
    );
    let mut next_rel = 2;
    if ctx.has_notes {
        xml.push_str(&format!(
            r#"<p:notesMasterIdLst><p:notesMasterId r:id="rId{next_rel}"/></p:notesMasterIdLst>"#
        ));
        next_rel += 1;
    }
    if ctx.slide_count > 0 {
        xml.push_str("<p:sldIdLst>");
        for i in 0..ctx.slide_count {
            xml.push_str(&format!(
                r#"<p:sldId id="{}" r:id="rId{}"/>"#,
                256 + i,
                next_rel + i
            ));
        }
        xml.push_str("</p:sldIdLst>");
    }
    let kind = size
        .type_name()
        .map(|kind| format!(r#" type="{kind}""#))
        .unwrap_or_default();
    xml.push_str(&format!(
        r#"<p:sldSz cx="{}" cy="{}"{kind}/><p:notesSz cx="6858000" cy="9144000"/>{}<p:defaultTextStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr>"#,
        size.cx,
        size.cy,
        embedded_fonts_xml(ctx)
    ));
    for level in 1..=9 {
        let indent = (level - 1) * 457_200;
        xml.push_str(&format!(r#"<a:lvl{level}pPr marL="{indent}" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1800" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl{level}pPr>"#));
    }
    xml.push_str("</p:defaultTextStyle></p:presentation>");
    xml
}

/// The relationship id of the `i`th font part: fonts follow the slides
/// and the four fixed relationships.
fn font_rel_id(ctx: &Ctx<'_>, i: usize) -> String {
    let notes = usize::from(ctx.has_notes);
    format!("rId{}", 2 + notes + ctx.slide_count + 4 + i)
}

/// `p:embeddedFontLst`: one entry per family, its files in the regular,
/// bold, italic and bold italic slots by the style each font declares.
fn embedded_fonts_xml(ctx: &Ctx<'_>) -> String {
    if ctx.fonts.is_empty() {
        return String::new();
    }
    let mut families: Vec<&str> = Vec::new();
    for font in &ctx.fonts {
        if !families.contains(&font.info.family.as_str()) {
            families.push(&font.info.family);
        }
    }
    let mut xml = String::from("<p:embeddedFontLst>");
    for family in families {
        let members: Vec<(usize, &FontPart)> = ctx
            .fonts
            .iter()
            .enumerate()
            .filter(|(_, font)| font.info.family == family)
            .collect();
        let first = members[0].1;
        xml.push_str(&format!(
            r#"<p:embeddedFont><p:font typeface="{}" panose="{}" pitchFamily="{}" charset="0"/>"#,
            attr(family),
            first.info.panose_hex(),
            first.info.pitch_family()
        ));
        for (element, bold, italic) in [
            ("regular", false, false),
            ("bold", true, false),
            ("italic", false, true),
            ("boldItalic", true, true),
        ] {
            let slot = members
                .iter()
                .rev()
                .find(|(_, font)| font.info.bold == bold && font.info.italic == italic);
            if let Some((i, _)) = slot {
                xml.push_str(&format!(
                    r#"<p:{element} r:id="{}"/>"#,
                    font_rel_id(ctx, *i)
                ));
            }
        }
        xml.push_str("</p:embeddedFont>");
    }
    xml.push_str("</p:embeddedFontLst>");
    xml
}

fn presentation_rels(ctx: &Ctx<'_>) -> Vec<(String, String, String)> {
    let mut rels = vec![(
        "rId1".to_string(),
        format!("{REL}slideMaster"),
        "slideMasters/slideMaster1.xml".to_string(),
    )];
    let mut next = 2;
    if ctx.has_notes {
        rels.push((
            format!("rId{next}"),
            format!("{REL}notesMaster"),
            "notesMasters/notesMaster1.xml".into(),
        ));
        next += 1;
    }
    for i in 0..ctx.slide_count {
        rels.push((
            format!("rId{}", next + i),
            format!("{REL}slide"),
            format!("slides/slide{}.xml", i + 1),
        ));
    }
    next += ctx.slide_count;
    rels.push((
        format!("rId{next}"),
        format!("{REL}presProps"),
        "presProps.xml".into(),
    ));
    rels.push((
        format!("rId{}", next + 1),
        format!("{REL}viewProps"),
        "viewProps.xml".into(),
    ));
    rels.push((
        format!("rId{}", next + 2),
        format!("{REL}theme"),
        "theme/theme1.xml".into(),
    ));
    rels.push((
        format!("rId{}", next + 3),
        format!("{REL}tableStyles"),
        "tableStyles.xml".into(),
    ));
    for i in 0..ctx.fonts.len() {
        rels.push((
            font_rel_id(ctx, i),
            format!("{REL}font"),
            format!("fonts/font{}.fntdata", i + 1),
        ));
    }
    rels
}

fn xfrm(rect: Rect) -> String {
    format!(
        r#"<a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        rect.x, rect.y, rect.cx, rect.cy
    )
}

fn group_header() -> String {
    r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#.to_string()
}

const PLAIN_BODY_PR: &str = "<a:bodyPr/><a:lstStyle/>";

/// A placeholder shape; `body_pr` is the `bodyPr` and `lstStyle` markup, `body` the paragraphs.
fn placeholder_sp(
    id: u32,
    name: &str,
    ph: &str,
    rect: Option<Rect>,
    body_pr: &str,
    body: &str,
) -> String {
    let sp_pr = match rect {
        Some(rect) => format!(
            "<p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>",
            xfrm(rect)
        ),
        None => "<p:spPr/>".to_string(),
    };
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr>{ph}</p:nvPr></p:nvSpPr>{sp_pr}<p:txBody>{body_pr}{body}</p:txBody></p:sp>"#,
        attr(name)
    )
}

fn prompt(text_value: &str) -> String {
    format!(
        r#"<a:p><a:r><a:rPr lang="en-US"/><a:t>{}</a:t></a:r><a:endParaRPr lang="en-US"/></a:p>"#,
        text(text_value)
    )
}

fn master_xml<'a>(ctx: &mut Ctx<'a>, grid: &Grid) -> Result<(String, Vec<Rel>)> {
    let theme = &ctx.presentation.theme;
    let mut rels = Rels::new();
    for i in 1..=4 {
        rels.add(
            &format!("{REL}slideLayout"),
            format!("../slideLayouts/slideLayout{i}.xml"),
        );
    }
    rels.add(&format!("{REL}theme"), "../theme/theme1.xml".to_string());
    let bg = ctx.background_xml(theme.background.as_ref(), &mut rels)?;
    let mut xml = format!(
        r#"{DECL}<p:sldMaster xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld>{bg}<p:spTree>{}"#,
        group_header()
    );
    xml.push_str(&placeholder_sp(2, "Title Placeholder 1", r#"<p:ph type="title"/>"#, Some(grid.title), r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0" anchor="ctr"><a:normAutofit/></a:bodyPr><a:lstStyle/>"#, &prompt("Click to edit Master title style")));
    xml.push_str(&placeholder_sp(3, "Text Placeholder 2", r#"<p:ph type="body" idx="1"/>"#, Some(grid.body), r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0"><a:normAutofit/></a:bodyPr><a:lstStyle/>"#, &prompt("Click to edit Master text styles")));
    xml.push_str(&format!(
        "</p:spTree></p:cSld><p:clrMap {}/><p:sldLayoutIdLst>",
        clr_map_attrs(false)
    ));
    for i in 1..=4u32 {
        xml.push_str(&format!(
            r#"<p:sldLayoutId id="{}" r:id="rId{i}"/>"#,
            2_147_483_648u32 + i
        ));
    }
    xml.push_str(&format!(r#"</p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPct val="0"/></a:spcBef><a:buNone/><a:defRPr sz="{}" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/><a:cs typeface="+mj-cs"/></a:defRPr></a:lvl1pPr></p:titleStyle><p:bodyStyle>{}"#, theme.scale.title * 100, body_levels_xml(theme, 100)));
    xml.push_str(r#"</p:bodyStyle><p:otherStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr>"#);
    for level in 1..=9 {
        let mar_l = (level - 1) * 457_200;
        xml.push_str(&format!(r#"<a:lvl{level}pPr marL="{mar_l}" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1800" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl{level}pPr>"#));
    }
    xml.push_str("</p:otherStyle></p:txStyles></p:sldMaster>");
    Ok((xml, rels.list))
}

fn layout_xml<'a>(ctx: &mut Ctx<'a>, layout: Layout, grid: &Grid) -> Result<(String, Vec<Rel>)> {
    let theme = &ctx.presentation.theme;
    let display = theme.scale.display * 100;
    let subtitle = theme.scale.subtitle * 100;
    let mut rels = Rels::new();
    rels.add(
        &format!("{REL}slideMaster"),
        "../slideMasters/slideMaster1.xml".to_string(),
    );
    let entry = theme.layout_background_for(layout);
    let bg = match entry {
        Some(entry) => ctx.background_xml(Some(&entry.background), &mut rels)?,
        None => String::new(),
    };
    let ovr = clr_map_ovr(entry.is_some_and(|entry| entry.inverted));
    let (kind, name, shapes) = match layout {
        Layout::Title => (
            "title",
            "Title Slide",
            format!(
                "{}{}",
                placeholder_sp(
                    2,
                    "Title 1",
                    r#"<p:ph type="ctrTitle"/>"#,
                    Some(grid.center_title),
                    &format!(
                        r#"<a:bodyPr anchor="b"/><a:lstStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="{display}"><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:defRPr></a:lvl1pPr></a:lstStyle>"#
                    ),
                    &prompt("Click to edit Master title style")
                ),
                placeholder_sp(
                    3,
                    "Subtitle 2",
                    r#"<p:ph type="subTitle" idx="1"/>"#,
                    Some(grid.subtitle),
                    &format!(
                        r#"<a:bodyPr/><a:lstStyle><a:lvl1pPr marL="0" indent="0" algn="ctr"><a:buNone/><a:defRPr sz="{subtitle}"/></a:lvl1pPr></a:lstStyle>"#
                    ),
                    &prompt("Click to edit Master subtitle style")
                )
            ),
        ),
        Layout::TitleAndContent => (
            "obj",
            "Title and Content",
            format!(
                "{}{}",
                placeholder_sp(
                    2,
                    "Title 1",
                    r#"<p:ph type="title"/>"#,
                    None,
                    PLAIN_BODY_PR,
                    &prompt("Click to edit Master title style")
                ),
                placeholder_sp(
                    3,
                    "Content Placeholder 2",
                    r#"<p:ph idx="1"/>"#,
                    None,
                    PLAIN_BODY_PR,
                    &prompt("Click to edit Master text styles")
                )
            ),
        ),
        Layout::TitleOnly => (
            "titleOnly",
            "Title Only",
            placeholder_sp(
                2,
                "Title 1",
                r#"<p:ph type="title"/>"#,
                None,
                PLAIN_BODY_PR,
                &prompt("Click to edit Master title style"),
            ),
        ),
        Layout::Blank => ("blank", "Blank", String::new()),
        #[allow(unreachable_patterns)]
        _ => ("blank", "Blank", String::new()),
    };
    let xml = format!(
        r#"{DECL}<p:sldLayout xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}" type="{kind}" preserve="1"><p:cSld name="{name}">{bg}<p:spTree>{}{shapes}</p:spTree></p:cSld>{ovr}</p:sldLayout>"#,
        group_header()
    );
    Ok((xml, rels.list))
}

/// The notes master keeps a light page under every theme: an inverted
/// theme holds its light colors in the dark slots, so the map swaps back.
fn notes_master_xml(theme_inverted: bool) -> String {
    format!(
        r#"{DECL}<p:notesMaster xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{}{}{}</p:spTree></p:cSld><p:clrMap {}/><p:notesStyle><a:lvl1pPr marL="0" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1200" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl1pPr></p:notesStyle></p:notesMaster>"#,
        group_header(),
        r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="2"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="1371600" y="1143000"/><a:ext cx="4114800" cy="3086100"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln w="12700"><a:solidFill><a:prstClr val="black"/></a:solidFill></a:ln></p:spPr></p:sp>"#,
        placeholder_sp(
            3,
            "Notes Placeholder 2",
            r#"<p:ph type="body" sz="quarter" idx="3"/>"#,
            Some(Rect::new(685_800, 4_400_550, 5_486_400, 3_600_450)),
            PLAIN_BODY_PR,
            &prompt("Click to edit Master text styles")
        ),
        clr_map_attrs(theme_inverted)
    )
}

fn notes_xml(notes: &str) -> String {
    let mut body = String::new();
    for line in notes.lines() {
        body.push_str(&format!(
            r#"<a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p>"#,
            text(line)
        ));
    }
    if body.is_empty() {
        body.push_str("<a:p/>");
    }
    format!(
        r#"{DECL}<p:notes xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:spTree>{}<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#,
        group_header()
    )
}

/// A relationship of a written part: id, type, target.
type Rel = (String, String, String);

/// Relationships of one part, ids handed out in order.
struct Rels {
    list: Vec<Rel>,
    next: usize,
}

impl Rels {
    fn new() -> Self {
        Self {
            list: Vec::new(),
            next: 1,
        }
    }

    fn add(&mut self, rel_type: &str, target: String) -> String {
        let id = format!("rId{}", self.next);
        self.next += 1;
        self.list.push((id.clone(), rel_type.to_string(), target));
        id
    }

    /// An external hyperlink relationship; one per distinct URL.
    fn hyperlink(&mut self, url: &str) -> String {
        let rel_type = format!("{REL}hyperlink");
        if let Some((id, _, _)) = self
            .list
            .iter()
            .find(|(_, kind, target)| *kind == rel_type && target == url)
        {
            return id.clone();
        }
        self.add(&rel_type, url.to_string())
    }
}

fn paragraphs_xml(
    paragraphs: &[Paragraph],
    mode: ParagraphMode,
    rels: &mut Rels,
    swapped: bool,
) -> String {
    let mut xml = String::new();
    for paragraph in paragraphs {
        xml.push_str("<a:p>");
        xml.push_str(&ppr_xml(paragraph, mode));
        if paragraph.runs.is_empty() {
            xml.push_str(r#"<a:endParaRPr lang="en-US"/>"#);
        }
        for run in &paragraph.runs {
            let link = run.link.as_deref().map(|url| rels.hyperlink(url));
            xml.push_str(&run_xml(run, link.as_deref(), swapped));
        }
        xml.push_str("</a:p>");
    }
    if xml.is_empty() {
        xml.push_str(r#"<a:p><a:endParaRPr lang="en-US"/></a:p>"#);
    }
    xml
}

/// A run of plain text at an optional explicit size in points.
fn plain_paragraph(value: &str, size: Option<u32>) -> String {
    let sz = size
        .map(|points| format!(r#" sz="{}""#, points * 100))
        .unwrap_or_default();
    format!(
        r#"<a:p><a:r><a:rPr lang="en-US"{sz} dirty="0"/><a:t>{}</a:t></a:r></a:p>"#,
        text(value)
    )
}

/// A `lstStyle` carrying the body levels at `scale`, or an empty one at 100.
fn body_list_style(ctx: &Ctx<'_>, scale: u32, always: bool) -> String {
    if scale == 100 && !always {
        return "<a:lstStyle/>".to_string();
    }
    format!(
        "<a:lstStyle>{}</a:lstStyle>",
        body_levels_xml(&ctx.presentation.theme, scale)
    )
}

fn picture_xml<'a>(
    ctx: &mut Ctx<'a>,
    rels: &mut Rels,
    id: u32,
    data: &'a [u8],
    name: &str,
    description: Option<&str>,
    rect: Rect,
) -> Result<String> {
    ctx.pictures += 1;
    let target = ctx
        .add_media(data)
        .ok_or(Error::UnsupportedImage(ctx.pictures))?;
    let rel_id = rels.add(&format!("{REL}image"), target);
    let descr = description
        .map(|d| format!(r#" descr="{}""#, attr(d)))
        .unwrap_or_default();
    Ok(format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{}"{descr}/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rel_id}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
        attr(name),
        xfrm(rect)
    ))
}

fn table_xml(
    id: u32,
    rows: &[Vec<String>],
    header: bool,
    rect: Rect,
    row_heights: &[i64],
    size: u32,
) -> Result<String> {
    let columns = rows.first().map_or(0, Vec::len);
    for (row, cells) in rows.iter().enumerate() {
        if cells.len() != columns {
            return Err(Error::RaggedTable {
                row,
                cells: cells.len(),
                columns,
            });
        }
    }
    let col_w = match columns {
        0 => rect.cx,
        n => rect.cx / n as i64,
    };
    let mut tbl = format!(
        r#"<a:tbl><a:tblPr firstRow="{}" bandRow="0"/><a:tblGrid>"#,
        u8::from(header)
    );
    for _ in 0..columns {
        tbl.push_str(&format!(r#"<a:gridCol w="{col_w}"/>"#));
    }
    tbl.push_str("</a:tblGrid>");
    let sz = size * 100;
    for (row, cells) in rows.iter().enumerate() {
        let heading = header && row == 0;
        let (bold, rule) = match heading {
            true => (r#" b="1""#, TABLE_HEAD_RULE),
            false => ("", TABLE_ROW_RULE),
        };
        tbl.push_str(&format!(r#"<a:tr h="{}">"#, row_heights[row]));
        for cell in cells {
            tbl.push_str(&format!(r#"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="{sz}"{bold} dirty="0"/><a:t>{}</a:t></a:r></a:p></a:txBody><a:tcPr anchor="ctr">{TABLE_OPEN_SIDES}{rule}<a:noFill/></a:tcPr></a:tc>"#, text(cell)));
        }
        tbl.push_str("</a:tr>");
    }
    tbl.push_str("</a:tbl>");
    Ok(format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="Table {}"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">{tbl}</a:graphicData></a:graphic></p:graphicFrame>"#,
        id - 1,
        rect.x,
        rect.y,
        rect.cx,
        rect.cy
    ))
}

/// Equal row heights for a table at a fixed position.
fn even_rows(rows: usize, height: i64) -> Vec<i64> {
    match rows {
        0 => Vec::new(),
        n => vec![height / n as i64; n],
    }
}

fn slide_xml<'a>(
    ctx: &mut Ctx<'a>,
    planned: &Planned<'a>,
    n: usize,
    grid: &Grid,
) -> Result<(String, Vec<Rel>)> {
    let slide = planned.source;
    let layout = planned.layout;
    let mut rels = Rels::new();
    rels.add(
        &format!("{REL}slideLayout"),
        format!("../slideLayouts/slideLayout{}.xml", layout.index()),
    );
    if planned.notes.is_some() {
        rels.add(
            &format!("{REL}notesSlide"),
            format!("../notesSlides/notesSlide{n}.xml"),
        );
    }
    let bg = match (slide.background.as_ref(), slide.inverted) {
        (Some(background), _) => ctx.background_xml(Some(background), &mut rels)?,
        (None, true) => ctx.background_xml(None, &mut rels)?,
        (None, false) => String::new(),
    };
    let swapped = ctx.presentation.theme.inverted;
    let mut shapes = String::new();
    let mut next_id = 2u32;
    if let Some(title) = &slide.title {
        let ph = match layout {
            Layout::Title => r#"<p:ph type="ctrTitle"/>"#,
            _ => r#"<p:ph type="title"/>"#,
        };
        let rect = match layout {
            Layout::Blank => Some(grid.title),
            _ => None,
        };
        shapes.push_str(&placeholder_sp(
            next_id,
            "Title 1",
            ph,
            rect,
            PLAIN_BODY_PR,
            &plain_paragraph(title, planned.title_size),
        ));
        next_id += 1;
    }
    if let Some(subtitle) = &slide.subtitle {
        shapes.push_str(&placeholder_sp(
            next_id,
            "Subtitle 2",
            r#"<p:ph type="subTitle" idx="1"/>"#,
            None,
            PLAIN_BODY_PR,
            &plain_paragraph(subtitle, None),
        ));
        next_id += 1;
    }
    for element in &planned.elements {
        match element {
            Element::Body {
                paragraphs,
                rect,
                scale,
            } => {
                let autofit = match rect {
                    Some(_) => "<a:bodyPr><a:noAutofit/></a:bodyPr>",
                    None => "<a:bodyPr/>",
                };
                let body_pr = format!("{autofit}{}", body_list_style(ctx, *scale, false));
                shapes.push_str(&placeholder_sp(
                    next_id,
                    "Content Placeholder 2",
                    r#"<p:ph idx="1"/>"#,
                    *rect,
                    &body_pr,
                    &paragraphs_xml(paragraphs, ParagraphMode::Body, &mut rels, swapped),
                ));
                next_id += 1;
            }
            Element::TextBox {
                paragraphs,
                rect,
                scale,
            } => {
                shapes.push_str(&format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="{next_id}" name="TextBox {}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" rtlCol="0" anchor="t"/>{}{}</p:txBody></p:sp>"#, next_id - 1, xfrm(*rect), body_list_style(ctx, *scale, true), paragraphs_xml(paragraphs, ParagraphMode::Body, &mut rels, swapped)));
                next_id += 1;
            }
            Element::Picture {
                data,
                description,
                rect,
            } => {
                let name = format!("Picture {}", next_id - 1);
                shapes.push_str(&picture_xml(
                    ctx,
                    &mut rels,
                    next_id,
                    data,
                    &name,
                    *description,
                    *rect,
                )?);
                next_id += 1;
            }
            Element::Table {
                rows,
                header,
                rect,
                row_heights,
                size,
            } => {
                shapes.push_str(&table_xml(
                    next_id,
                    rows,
                    *header,
                    *rect,
                    row_heights,
                    *size,
                )?);
                next_id += 1;
            }
            Element::Shape(Shape::Text { paragraphs, rect }) => {
                shapes.push_str(&format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="{next_id}" name="TextBox {}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" rtlCol="0"><a:spAutoFit/></a:bodyPr><a:lstStyle/>{}</p:txBody></p:sp>"#, next_id - 1, xfrm(*rect), paragraphs_xml(paragraphs, ParagraphMode::Box, &mut rels, swapped)));
                next_id += 1;
            }
            Element::Shape(Shape::Picture(picture)) => {
                shapes.push_str(&picture_xml(
                    ctx,
                    &mut rels,
                    next_id,
                    &picture.data,
                    &picture.name,
                    picture.description.as_deref(),
                    picture.rect,
                )?);
                next_id += 1;
            }
            Element::Shape(Shape::Table(table)) => {
                shapes.push_str(&table_xml(
                    next_id,
                    &table.rows,
                    table.header,
                    table.rect,
                    &even_rows(table.rows.len(), table.rect.cy),
                    ctx.presentation.theme.scale.table,
                )?);
                next_id += 1;
            }
        }
    }
    let show = match slide.hidden {
        true => r#" show="0""#,
        false => "",
    };
    let xml = format!(
        r#"{DECL}<p:sld xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"{show}><p:cSld>{bg}<p:spTree>{}{shapes}</p:spTree></p:cSld>{}</p:sld>"#,
        group_header(),
        clr_map_ovr(slide.inverted)
    );
    Ok((xml, rels.list))
}

fn core_xml(presentation: &Presentation) -> String {
    let meta = &presentation.metadata;
    let mut xml = format!(
        r#"{DECL}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#
    );
    if let Some(title) = &meta.title {
        xml.push_str(&format!("<dc:title>{}</dc:title>", text(title)));
    }
    if let Some(subject) = &meta.subject {
        xml.push_str(&format!("<dc:subject>{}</dc:subject>", text(subject)));
    }
    xml.push_str(&format!("<dc:creator>{}</dc:creator>", text(&meta.creator)));
    if let Some(keywords) = &meta.keywords {
        xml.push_str(&format!("<cp:keywords>{}</cp:keywords>", text(keywords)));
    }
    xml.push_str(&format!(r#"<cp:lastModifiedBy>{}</cp:lastModifiedBy><cp:revision>1</cp:revision><dcterms:created xsi:type="dcterms:W3CDTF">{}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">{}</dcterms:modified></cp:coreProperties>"#, text(&meta.creator), text(&meta.timestamp), text(&meta.timestamp)));
    xml
}

fn app_xml(presentation: &Presentation, slides: &[Planned<'_>]) -> String {
    let format_name = match presentation.size.type_name() {
        Some("screen16x9") => "Widescreen",
        Some("screen4x3") => "On-screen Show (4:3)",
        _ => "Custom",
    };
    let notes = slides.iter().filter(|slide| slide.notes.is_some()).count();
    let hidden = slides.iter().filter(|slide| slide.source.hidden).count();
    format!(
        r#"{DECL}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>pptxboss</Application><PresentationFormat>{format_name}</PresentationFormat><Slides>{}</Slides><Notes>{notes}</Notes><HiddenSlides>{hidden}</HiddenSlides><ScaleCrop>false</ScaleCrop><LinksUpToDate>false</LinksUpToDate><SharedDoc>false</SharedDoc><HyperlinksChanged>false</HyperlinksChanged><AppVersion>00.0100</AppVersion></Properties>"#,
        slides.len()
    )
}
