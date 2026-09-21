//! Serializes a [`Presentation`] into package parts (ECMA-376 Part 1,
//! clauses 13, 19 and 21; Part 2, clauses 7 and 8).

use crate::xml::{attr, text, DECL};
use crate::{
    Error, ImageFormat, Layout, Paragraph, Presentation, Rect, Result, Run, Shape, Slide, SlideSize,
};

const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
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

struct Ctx<'a> {
    presentation: &'a Presentation,
    has_notes: bool,
    media: Vec<(String, ImageFormat)>,
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
        theme_xml(&presentation.font),
    );

    push_xml(
        &mut parts,
        &mut xml_parts,
        "ppt/slideMasters/slideMaster1.xml",
        &format!("{CT_PML}slideMaster+xml"),
        master_xml(&frames),
    );
    let mut master_rels: Vec<(String, String, String)> = (1..=4)
        .map(|i| {
            (
                format!("rId{i}"),
                format!("{REL}slideLayout"),
                format!("../slideLayouts/slideLayout{i}.xml"),
            )
        })
        .collect();
    master_rels.push((
        "rId5".into(),
        format!("{REL}theme"),
        "../theme/theme1.xml".into(),
    ));
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
        push_xml(
            &mut parts,
            &mut xml_parts,
            &format!("ppt/slideLayouts/slideLayout{index}.xml"),
            &format!("{CT_PML}slideLayout+xml"),
            layout_xml(layout, &frames),
        );
        parts.push(rels_part(
            &format!("ppt/slideLayouts/_rels/slideLayout{index}.xml.rels"),
            &[(
                "rId1".into(),
                format!("{REL}slideMaster"),
                "../slideMasters/slideMaster1.xml".into(),
            )],
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

    let mut media_index = 0;
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let Shape::Picture(picture) = shape {
                let (name, _) = &ctx.media[media_index];
                media_index += 1;
                parts.push(PartOut {
                    name: name.clone(),
                    data: picture.data.clone(),
                    compress: false,
                });
            }
        }
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

fn content_types_xml(xml_parts: &[(String, String)], media: &[(String, ImageFormat)]) -> String {
    let mut xml = format!(
        r#"{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#
    );
    let mut seen: Vec<ImageFormat> = Vec::new();
    for (_, format) in media {
        if seen.contains(format) {
            continue;
        }
        seen.push(*format);
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

fn master_xml(frames: &Frames) -> String {
    let mut xml = format!(
        r#"{DECL}<p:sldMaster xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{}"#,
        group_header()
    );
    xml.push_str(&placeholder_sp(2, "Title Placeholder 1", r#"<p:ph type="title"/>"#, Some(frames.title), r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0" anchor="ctr"><a:normAutofit/></a:bodyPr><a:lstStyle/>"#, &prompt("Click to edit Master title style")));
    xml.push_str(&placeholder_sp(3, "Text Placeholder 2", r#"<p:ph type="body" idx="1"/>"#, Some(frames.body), r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0"><a:normAutofit/></a:bodyPr><a:lstStyle/>"#, &prompt("Click to edit Master text styles")));
    xml.push_str(r#"</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst>"#);
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
    xml
}

fn layout_xml(layout: Layout, frames: &Frames) -> String {
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
    format!(
        r#"{DECL}<p:sldLayout xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}" type="{kind}" preserve="1"><p:cSld name="{name}"><p:spTree>{}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        group_header()
    )
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

fn run_props(run: &Run) -> String {
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
    props.push_str(r#" dirty="0"/>"#);
    props
}

fn runs_xml(paragraph: &Paragraph) -> String {
    if paragraph.runs.is_empty() {
        return r#"<a:endParaRPr lang="en-US"/>"#.to_string();
    }
    paragraph
        .runs
        .iter()
        .map(|run| {
            format!(
                "<a:r>{}<a:t>{}</a:t></a:r>",
                run_props(run),
                text(&run.text)
            )
        })
        .collect()
}

/// Paragraphs for a body placeholder: bullets come from the master style,
/// plain paragraphs switch bullets off.
fn body_paragraphs(paragraphs: &[Paragraph]) -> String {
    let mut xml = String::new();
    for paragraph in paragraphs {
        let ppr = match (paragraph.bullet, paragraph.level) {
            (true, 0) => String::new(),
            (true, level) => format!(r#"<a:pPr lvl="{level}"/>"#),
            (false, level) => format!(
                r#"<a:pPr marL="{}" lvl="{level}" indent="0"><a:buNone/></a:pPr>"#,
                level as i64 * 457_200
            ),
        };
        xml.push_str(&format!("<a:p>{ppr}{}</a:p>", runs_xml(paragraph)));
    }
    if xml.is_empty() {
        xml.push_str(r#"<a:p><a:endParaRPr lang="en-US"/></a:p>"#);
    }
    xml
}

/// Paragraphs for a free text box: bullets are explicit.
fn box_paragraphs(paragraphs: &[Paragraph]) -> String {
    let mut xml = String::new();
    for paragraph in paragraphs {
        let ppr = match paragraph.bullet {
            true => format!(
                r#"<a:pPr marL="{}" lvl="{}" indent="-228600"><a:buFont typeface="Arial" panose="020B0604020202020204" pitchFamily="34" charset="0"/><a:buChar char="&#8226;"/></a:pPr>"#,
                228_600 + paragraph.level as i64 * 457_200,
                paragraph.level
            ),
            false if paragraph.level > 0 => format!(r#"<a:pPr lvl="{}"/>"#, paragraph.level),
            false => String::new(),
        };
        xml.push_str(&format!("<a:p>{ppr}{}</a:p>", runs_xml(paragraph)));
    }
    if xml.is_empty() {
        xml.push_str(r#"<a:p><a:endParaRPr lang="en-US"/></a:p>"#);
    }
    xml
}

/// A relationship of a written part: id, type, target.
type Rel = (String, String, String);

fn slide_xml(
    ctx: &mut Ctx<'_>,
    slide: &Slide,
    n: usize,
    frames: &Frames,
) -> Result<(String, Vec<Rel>)> {
    let layout = slide.effective_layout();
    let mut rels = vec![(
        "rId1".to_string(),
        format!("{REL}slideLayout"),
        format!("../slideLayouts/slideLayout{}.xml", layout.index()),
    )];
    let mut next_rel = 2;
    if slide.notes.is_some() {
        rels.push((
            format!("rId{next_rel}"),
            format!("{REL}notesSlide"),
            format!("../notesSlides/notesSlide{n}.xml"),
        ));
        next_rel += 1;
    }
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
            &body_paragraphs(&slide.body),
        ));
        next_id += 1;
    }
    for shape in &slide.shapes {
        match shape {
            Shape::Text { paragraphs, rect } => {
                shapes.push_str(&format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="{next_id}" name="TextBox {}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" rtlCol="0"><a:spAutoFit/></a:bodyPr><a:lstStyle/>{}</p:txBody></p:sp>"#, next_id - 1, xfrm(*rect), box_paragraphs(paragraphs)));
                next_id += 1;
            }
            Shape::Picture(picture) => {
                let format = ImageFormat::sniff(&picture.data)
                    .ok_or(Error::UnsupportedImage(ctx.media.len() + 1))?;
                let media_name = format!(
                    "ppt/media/image{}.{}",
                    ctx.media.len() + 1,
                    format.extension()
                );
                ctx.media.push((media_name.clone(), format));
                let rel_id = format!("rId{next_rel}");
                next_rel += 1;
                rels.push((
                    rel_id.clone(),
                    format!("{REL}image"),
                    format!("../media/image{}.{}", ctx.media.len(), format.extension()),
                ));
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
        r#"{DECL}<p:sld xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"{show}><p:cSld><p:spTree>{}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        group_header()
    );
    Ok((xml, rels))
}

fn theme_xml(font: &str) -> String {
    let font = attr(font);
    format!(
        r#"{DECL}<a:theme xmlns:a="{NS_A}" name="pptxboss"><a:themeElements><a:clrScheme name="pptxboss"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="pptxboss"><a:majorFont><a:latin typeface="{font}"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="{font}"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="pptxboss"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#
    )
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
