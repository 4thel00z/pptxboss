//! Serializes a [`Presentation`] into package parts (ECMA-376 Part 1,
//! clauses 13, 19 and 21; Part 2, clauses 7 and 8).

use crate::style::{
    bg_xml, clr_map_attrs, clr_map_ovr, ppr_xml, run_xml, theme_xml, ParagraphMode, NS_A,
};
use crate::xml::{attr, text, DECL};
use crate::{
    Background, Error, ImageFormat, Layout, Paragraph, Presentation, Rect, Result, Shape, Slide,
    SlideSize,
};

const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_PML: &str = "application/vnd.openxmlformats-officedocument.presentationml.";
const TABLE_STYLE: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";

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

struct Ctx<'a> {
    presentation: &'a Presentation,
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
            return Ok(bg_xml(background, None));
        };
        let target = self
            .add_media(data)
            .ok_or(Error::UnsupportedBackgroundImage)?;
        let rel = rels.add(&format!("{REL}image"), target);
        Ok(bg_xml(background, Some(&rel)))
    }
}

/// The geometry the layouts share, derived from the slide size.
struct Frames {
    title: Rect,
    body: Rect,
    center_title: Rect,
    subtitle: Rect,
}

impl Frames {
    fn for_size(size: SlideSize) -> Self {
        let margin = size.cx * 55 / 800;
        let width = size.cx - 2 * margin;
        Self {
            title: Rect::new(margin, 365_125, width, 1_325_563),
            body: Rect::new(margin, 1_825_625, width, size.cy - 1_825_625 - 681_037),
            center_title: Rect::new(margin, size.cy / 6, width, 2_387_600),
            subtitle: Rect::new(
                size.cx / 8,
                size.cy / 6 + 2_479_675,
                size.cx * 3 / 4,
                1_655_762,
            ),
        }
    }
}

pub fn build(presentation: &Presentation) -> Result<Vec<PartOut>> {
    let has_notes = presentation
        .slides
        .iter()
        .any(|slide| slide.notes.is_some());
    let mut ctx = Ctx {
        presentation,
        has_notes,
        media: Vec::new(),
        pictures: 0,
    };
    let frames = Frames::for_size(presentation.size);
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

    let (master, master_rels) = master_xml(&mut ctx, &frames)?;
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
        let (xml, layout_rels) = layout_xml(&mut ctx, layout, &frames)?;
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
            notes_master_xml(),
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

    for (i, slide) in presentation.slides.iter().enumerate() {
        let n = i + 1;
        let (xml, slide_rels) = slide_xml(&mut ctx, slide, n, &frames)?;
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
        if let Some(notes) = &slide.notes {
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
        app_xml(presentation),
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
            data: content_types_xml(&xml_parts, &ctx.media).into_bytes(),
            compress: true,
        },
    );
    Ok(parts)
}

fn rels_part(name: &str, rels: &[(String, String, String)]) -> PartOut {
    let mut xml = format!(r#"{DECL}<Relationships xmlns="{PKG_REL}">"#);
    for (id, rel_type, target) in rels {
        let mode = match target.contains("://") {
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

fn content_types_xml(xml_parts: &[(String, String)], media: &[Media<'_>]) -> String {
    let mut xml = format!(
        r#"{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#
    );
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
    let mut xml = format!(
        r#"{DECL}<p:presentation xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}" saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>"#
    );
    let mut next_rel = 2;
    if ctx.has_notes {
        xml.push_str(&format!(
            r#"<p:notesMasterIdLst><p:notesMasterId r:id="rId{next_rel}"/></p:notesMasterIdLst>"#
        ));
        next_rel += 1;
    }
    if !ctx.presentation.slides.is_empty() {
        xml.push_str("<p:sldIdLst>");
        for i in 0..ctx.presentation.slides.len() {
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
    xml.push_str(&format!(r#"<p:sldSz cx="{}" cy="{}"{kind}/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr>"#, size.cx, size.cy));
    for level in 1..=9 {
        let indent = (level - 1) * 457_200;
        xml.push_str(&format!(r#"<a:lvl{level}pPr marL="{indent}" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1800" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl{level}pPr>"#));
    }
    xml.push_str("</p:defaultTextStyle></p:presentation>");
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
    for i in 0..ctx.presentation.slides.len() {
        rels.push((
            format!("rId{}", next + i),
            format!("{REL}slide"),
            format!("slides/slide{}.xml", i + 1),
        ));
    }
    next += ctx.presentation.slides.len();
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

fn master_xml<'a>(ctx: &mut Ctx<'a>, frames: &Frames) -> Result<(String, Vec<Rel>)> {
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
    xml.push_str(&placeholder_sp(2, "Title Placeholder 1", r#"<p:ph type="title"/>"#, Some(frames.title), r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0" anchor="ctr"><a:normAutofit/></a:bodyPr><a:lstStyle/>"#, &prompt("Click to edit Master title style")));
    xml.push_str(&placeholder_sp(3, "Text Placeholder 2", r#"<p:ph type="body" idx="1"/>"#, Some(frames.body), r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0"><a:normAutofit/></a:bodyPr><a:lstStyle/>"#, &prompt("Click to edit Master text styles")));
    xml.push_str(&format!(
        "</p:spTree></p:cSld><p:clrMap {}/><p:sldLayoutIdLst>",
        clr_map_attrs(theme.inverted)
    ));
    for i in 1..=4u32 {
        xml.push_str(&format!(
            r#"<p:sldLayoutId id="{}" r:id="rId{i}"/>"#,
            2_147_483_648u32 + i
        ));
    }
    xml.push_str(r#"</p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPct val="0"/></a:spcBef><a:buNone/><a:defRPr sz="4400" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/><a:cs typeface="+mj-cs"/></a:defRPr></a:lvl1pPr></p:titleStyle><p:bodyStyle>"#);
    let sizes = [2800u32, 2400, 2000, 1800, 1800, 1800, 1800, 1800, 1800];
    for (level, size) in sizes.iter().enumerate() {
        let level = level + 1;
        let mar_l = 228_600 + (level as i64 - 1) * 457_200;
        xml.push_str(&format!(r#"<a:lvl{level}pPr marL="{mar_l}" indent="-228600" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPts val="{}"/></a:spcBef><a:buFont typeface="Arial" panose="020B0604020202020204" pitchFamily="34" charset="0"/><a:buChar char="&#8226;"/><a:defRPr sz="{size}" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl{level}pPr>"#, if level == 1 { 1000 } else { 500 }));
    }
    xml.push_str(r#"</p:bodyStyle><p:otherStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr>"#);
    for level in 1..=9 {
        let mar_l = (level - 1) * 457_200;
        xml.push_str(&format!(r#"<a:lvl{level}pPr marL="{mar_l}" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1800" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl{level}pPr>"#));
    }
    xml.push_str("</p:otherStyle></p:txStyles></p:sldMaster>");
    Ok((xml, rels.list))
}

fn layout_xml<'a>(
    ctx: &mut Ctx<'a>,
    layout: Layout,
    frames: &Frames,
) -> Result<(String, Vec<Rel>)> {
    let theme = &ctx.presentation.theme;
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
    let ovr = clr_map_ovr(theme.inverted, entry.is_some_and(|entry| entry.inverted));
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
                    Some(frames.center_title),
                    PLAIN_BODY_PR,
                    &prompt("Click to edit Master title style")
                ),
                placeholder_sp(
                    3,
                    "Subtitle 2",
                    r#"<p:ph type="subTitle" idx="1"/>"#,
                    Some(frames.subtitle),
                    r#"<a:bodyPr/><a:lstStyle><a:lvl1pPr marL="0" indent="0" algn="ctr"><a:buNone/><a:defRPr sz="2400"/></a:lvl1pPr></a:lstStyle>"#,
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
    };
    let xml = format!(
        r#"{DECL}<p:sldLayout xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}" type="{kind}" preserve="1"><p:cSld name="{name}">{bg}<p:spTree>{}{shapes}</p:spTree></p:cSld>{ovr}</p:sldLayout>"#,
        group_header()
    );
    Ok((xml, rels.list))
}

fn notes_master_xml() -> String {
    format!(
        r#"{DECL}<p:notesMaster xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{}{}{}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:notesStyle><a:lvl1pPr marL="0" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1200" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl1pPr></p:notesStyle></p:notesMaster>"#,
        group_header(),
        r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="2"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="1371600" y="1143000"/><a:ext cx="4114800" cy="3086100"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln w="12700"><a:solidFill><a:prstClr val="black"/></a:solidFill></a:ln></p:spPr></p:sp>"#,
        placeholder_sp(
            3,
            "Notes Placeholder 2",
            r#"<p:ph type="body" sz="quarter" idx="3"/>"#,
            Some(Rect::new(685_800, 4_400_550, 5_486_400, 3_600_450)),
            PLAIN_BODY_PR,
            &prompt("Click to edit Master text styles")
        )
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

fn paragraphs_xml(paragraphs: &[Paragraph], mode: ParagraphMode, rels: &mut Rels) -> String {
    let mut xml = String::new();
    for paragraph in paragraphs {
        xml.push_str("<a:p>");
        xml.push_str(&ppr_xml(paragraph, mode));
        if paragraph.runs.is_empty() {
            xml.push_str(r#"<a:endParaRPr lang="en-US"/>"#);
        }
        for run in &paragraph.runs {
            let link = run.link.as_deref().map(|url| rels.hyperlink(url));
            xml.push_str(&run_xml(run, link.as_deref()));
        }
        xml.push_str("</a:p>");
    }
    if xml.is_empty() {
        xml.push_str(r#"<a:p><a:endParaRPr lang="en-US"/></a:p>"#);
    }
    xml
}

fn slide_xml<'a>(
    ctx: &mut Ctx<'a>,
    slide: &'a Slide,
    n: usize,
    frames: &Frames,
) -> Result<(String, Vec<Rel>)> {
    let layout = slide.effective_layout();
    let mut rels = Rels::new();
    rels.add(
        &format!("{REL}slideLayout"),
        format!("../slideLayouts/slideLayout{}.xml", layout.index()),
    );
    if slide.notes.is_some() {
        rels.add(
            &format!("{REL}notesSlide"),
            format!("../notesSlides/notesSlide{n}.xml"),
        );
    }
    let bg = ctx.background_xml(slide.background.as_ref(), &mut rels)?;
    let mut shapes = String::new();
    let mut next_id = 2u32;
    if let Some(title) = &slide.title {
        let ph = match layout {
            Layout::Title => r#"<p:ph type="ctrTitle"/>"#,
            _ => r#"<p:ph type="title"/>"#,
        };
        let rect = match layout {
            Layout::Blank => Some(frames.title),
            _ => None,
        };
        shapes.push_str(&placeholder_sp(
            next_id,
            "Title 1",
            ph,
            rect,
            PLAIN_BODY_PR,
            &format!(
                r#"<a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p>"#,
                text(title)
            ),
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
            &format!(
                r#"<a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p>"#,
                text(subtitle)
            ),
        ));
        next_id += 1;
    }
    if !slide.body.is_empty() {
        let rect = match layout {
            Layout::TitleAndContent => None,
            _ => Some(frames.body),
        };
        shapes.push_str(&placeholder_sp(
            next_id,
            "Content Placeholder 2",
            r#"<p:ph idx="1"/>"#,
            rect,
            PLAIN_BODY_PR,
            &paragraphs_xml(&slide.body, ParagraphMode::Body, &mut rels),
        ));
        next_id += 1;
    }
    for shape in &slide.shapes {
        match shape {
            Shape::Text { paragraphs, rect } => {
                shapes.push_str(&format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="{next_id}" name="TextBox {}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" rtlCol="0"><a:spAutoFit/></a:bodyPr><a:lstStyle/>{}</p:txBody></p:sp>"#, next_id - 1, xfrm(*rect), paragraphs_xml(paragraphs, ParagraphMode::Box, &mut rels)));
                next_id += 1;
            }
            Shape::Picture(picture) => {
                ctx.pictures += 1;
                let target = ctx
                    .add_media(&picture.data)
                    .ok_or(Error::UnsupportedImage(ctx.pictures))?;
                let rel_id = rels.add(&format!("{REL}image"), target);
                let descr = picture
                    .description
                    .as_deref()
                    .map(|d| format!(r#" descr="{}""#, attr(d)))
                    .unwrap_or_default();
                shapes.push_str(&format!(r#"<p:pic><p:nvPicPr><p:cNvPr id="{next_id}" name="{}"{descr}/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rel_id}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#, attr(&picture.name), xfrm(picture.rect)));
                next_id += 1;
            }
            Shape::Table(table) => {
                let columns = table.rows.first().map_or(0, Vec::len);
                for (row, cells) in table.rows.iter().enumerate() {
                    if cells.len() != columns {
                        return Err(Error::RaggedTable {
                            row,
                            cells: cells.len(),
                            columns,
                        });
                    }
                }
                let col_w = match columns {
                    0 => table.rect.cx,
                    n => table.rect.cx / n as i64,
                };
                let row_h = match table.rows.len() {
                    0 => table.rect.cy,
                    n => table.rect.cy / n as i64,
                };
                let mut tbl = format!(
                    r#"<a:tbl><a:tblPr firstRow="{}" bandRow="1"><a:tableStyleId>{TABLE_STYLE}</a:tableStyleId></a:tblPr><a:tblGrid>"#,
                    u8::from(table.header)
                );
                for _ in 0..columns {
                    tbl.push_str(&format!(r#"<a:gridCol w="{col_w}"/>"#));
                }
                tbl.push_str("</a:tblGrid>");
                for cells in &table.rows {
                    tbl.push_str(&format!(r#"<a:tr h="{row_h}">"#));
                    for cell in cells {
                        tbl.push_str(&format!(r#"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>"#, text(cell)));
                    }
                    tbl.push_str("</a:tr>");
                }
                tbl.push_str("</a:tbl>");
                shapes.push_str(&format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{next_id}" name="Table {}"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table">{tbl}</a:graphicData></a:graphic></p:graphicFrame>"#, next_id - 1, table.rect.x, table.rect.y, table.rect.cx, table.rect.cy));
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
        clr_map_ovr(ctx.presentation.theme.inverted, slide.inverted)
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

fn app_xml(presentation: &Presentation) -> String {
    let format_name = match presentation.size.type_name() {
        Some("screen16x9") => "Widescreen",
        Some("screen4x3") => "On-screen Show (4:3)",
        _ => "Custom",
    };
    let notes = presentation
        .slides
        .iter()
        .filter(|slide| slide.notes.is_some())
        .count();
    let hidden = presentation
        .slides
        .iter()
        .filter(|slide| slide.hidden)
        .count();
    format!(
        r#"{DECL}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>pptxboss</Application><PresentationFormat>{format_name}</PresentationFormat><Slides>{}</Slides><Notes>{notes}</Notes><HiddenSlides>{hidden}</HiddenSlides><ScaleCrop>false</ScaleCrop><LinksUpToDate>false</LinksUpToDate><SharedDoc>false</SharedDoc><HyperlinksChanged>false</HyperlinksChanged><AppVersion>00.0100</AppVersion></Properties>"#,
        presentation.slides.len()
    )
}
