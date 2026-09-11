//! A minimal PresentationML package builder for tests: enough parts for a
//! conforming deck (content types, package rels, presentation, its
//! properties, one master, one layout, one theme, slides, optional notes),
//! with hooks to leave parts out or corrupt them.

use std::collections::BTreeMap;

use crate::ZipBuilder;

const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// A slide to include in a [`Deck`].
#[derive(Clone, Debug, Default)]
pub struct DeckSlide {
    pub title: Option<String>,
    pub bullets: Vec<String>,
    pub notes: Option<String>,
    pub hidden: bool,
    /// Raw shape XML appended to the shape tree after the generated shapes.
    pub extra_shapes: String,
    /// Extra relationships `(id, type tail, target, external)` for the slide.
    pub extra_rels: Vec<(String, String, String, bool)>,
}

impl DeckSlide {
    pub fn titled(title: &str) -> Self {
        Self {
            title: Some(title.to_string()),
            ..Self::default()
        }
    }

    pub fn bullet(mut self, text: &str) -> Self {
        self.bullets.push(text.to_string());
        self
    }

    pub fn notes(mut self, text: &str) -> Self {
        self.notes = Some(text.to_string());
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    pub fn shapes_xml(mut self, xml: &str) -> Self {
        self.extra_shapes.push_str(xml);
        self
    }

    pub fn rel(mut self, id: &str, type_tail: &str, target: &str, external: bool) -> Self {
        self.extra_rels.push((
            id.to_string(),
            type_tail.to_string(),
            target.to_string(),
            external,
        ));
        self
    }
}

/// Builds a deck; `build()` returns the `.pptx` bytes.
#[derive(Clone, Debug, Default)]
pub struct Deck {
    slides: Vec<DeckSlide>,
    /// Parts to replace or add verbatim (name without leading slash, bytes).
    overrides: BTreeMap<String, Vec<u8>>,
    /// Parts to leave out.
    omitted: Vec<String>,
    /// Extra `Default` content types `(extension, type)`.
    extra_defaults: Vec<(String, String)>,
    /// Media parts `(name, bytes, content type extension)`.
    media: Vec<(String, Vec<u8>)>,
    slide_size: (i64, i64),
    /// Extra relationships of the presentation part `(id, type tail, target)`.
    presentation_rels: Vec<(String, String, String)>,
    /// XML appended inside `p:presentation`, after `p:defaultTextStyle`.
    presentation_extra: String,
}

impl Deck {
    pub fn new() -> Self {
        Self {
            slide_size: (12192000, 6858000),
            ..Self::default()
        }
    }

    pub fn slide(mut self, slide: DeckSlide) -> Self {
        self.slides.push(slide);
        self
    }

    /// Replaces (or adds) a part's bytes; `name` has no leading slash.
    pub fn with_part(mut self, name: &str, bytes: &[u8]) -> Self {
        self.overrides.insert(name.to_string(), bytes.to_vec());
        self
    }

    /// Leaves a generated part out of the package.
    pub fn without_part(mut self, name: &str) -> Self {
        self.omitted.push(name.to_string());
        self
    }

    /// Adds a media part under `ppt/media/`, with a `Default` content type for its extension.
    pub fn media(mut self, file_name: &str, bytes: &[u8], content_type: &str) -> Self {
        let ext = file_name.rsplit('.').next().unwrap_or("bin").to_string();
        if !self.extra_defaults.iter().any(|(known, _)| *known == ext) {
            self.extra_defaults.push((ext, content_type.to_string()));
        }
        self.media
            .push((format!("ppt/media/{file_name}"), bytes.to_vec()));
        self
    }

    pub fn slide_size(mut self, cx: i64, cy: i64) -> Self {
        self.slide_size = (cx, cy);
        self
    }

    /// Adds a relationship from the presentation part; `type_tail` follows
    /// the officeDocument relationship prefix, or is a full URI when it
    /// contains `://`.
    pub fn presentation_rel(mut self, id: &str, type_tail: &str, target: &str) -> Self {
        self.presentation_rels
            .push((id.to_string(), type_tail.to_string(), target.to_string()));
        self
    }

    /// Appends XML inside the presentation element, e.g. an `p:extLst`.
    pub fn presentation_xml(mut self, xml: &str) -> Self {
        self.presentation_extra.push_str(xml);
        self
    }

    /// The generated parts as `(name, bytes)`, before overrides and omissions.
    pub fn parts(&self) -> Vec<(String, Vec<u8>)> {
        let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
        let has_notes = self.slides.iter().any(|slide| slide.notes.is_some());

        let mut types = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
        );
        for (ext, content_type) in &self.extra_defaults {
            types.push_str(&format!(
                r#"<Default Extension="{ext}" ContentType="{content_type}"/>"#
            ));
        }
        let mut overrides = vec![
            ("/ppt/presentation.xml", "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"),
            ("/ppt/presProps.xml", "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml"),
            ("/ppt/slideMasters/slideMaster1.xml", "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"),
            ("/ppt/slideLayouts/slideLayout1.xml", "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"),
            ("/ppt/theme/theme1.xml", "application/vnd.openxmlformats-officedocument.theme+xml"),
            ("/docProps/core.xml", "application/vnd.openxmlformats-package.core-properties+xml"),
            ("/docProps/app.xml", "application/vnd.openxmlformats-officedocument.extended-properties+xml"),
        ]
        .into_iter()
        .map(|(name, content_type)| (name.to_string(), content_type.to_string()))
        .collect::<Vec<_>>();
        if has_notes {
            overrides.push((
                "/ppt/notesMasters/notesMaster1.xml".into(),
                "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"
                    .into(),
            ));
        }
        for (i, slide) in self.slides.iter().enumerate() {
            overrides.push((
                format!("/ppt/slides/slide{}.xml", i + 1),
                "application/vnd.openxmlformats-officedocument.presentationml.slide+xml".into(),
            ));
            if slide.notes.is_some() {
                overrides.push((
                    format!("/ppt/notesSlides/notesSlide{}.xml", i + 1),
                    "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"
                        .into(),
                ));
            }
        }
        for (name, content_type) in &overrides {
            types.push_str(&format!(
                r#"<Override PartName="{name}" ContentType="{content_type}"/>"#
            ));
        }
        types.push_str("</Types>");
        parts.push(("[Content_Types].xml".into(), types.into_bytes()));

        parts.push(("_rels/.rels".into(), rels(&[
            ("rId1", &format!("{REL}officeDocument"), "ppt/presentation.xml", false),
            ("rId2", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties", "docProps/core.xml", false),
            ("rId3", &format!("{REL}extended-properties"), "docProps/app.xml", false),
        ])));

        let mut pres = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentation xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>"#
        );
        let mut pres_rels: Vec<(String, String, String, bool)> = vec![(
            "rId1".into(),
            format!("{REL}slideMaster"),
            "slideMasters/slideMaster1.xml".into(),
            false,
        )];
        let mut next_rel = 2;
        if has_notes {
            pres.push_str(&format!(r#"<p:notesMasterIdLst><p:notesMasterId r:id="rId{next_rel}"/></p:notesMasterIdLst>"#));
            pres_rels.push((
                format!("rId{next_rel}"),
                format!("{REL}notesMaster"),
                "notesMasters/notesMaster1.xml".into(),
                false,
            ));
            next_rel += 1;
        }
        if !self.slides.is_empty() {
            pres.push_str("<p:sldIdLst>");
            for i in 0..self.slides.len() {
                pres.push_str(&format!(
                    r#"<p:sldId id="{}" r:id="rId{next_rel}"/>"#,
                    256 + i
                ));
                pres_rels.push((
                    format!("rId{next_rel}"),
                    format!("{REL}slide"),
                    format!("slides/slide{}.xml", i + 1),
                    false,
                ));
                next_rel += 1;
            }
            pres.push_str("</p:sldIdLst>");
        }
        pres_rels.push((
            format!("rId{next_rel}"),
            format!("{REL}presProps"),
            "presProps.xml".into(),
            false,
        ));
        next_rel += 1;
        for (id, tail, target) in &self.presentation_rels {
            let rel_type = match tail.contains("://") {
                true => tail.clone(),
                false => format!("{REL}{tail}"),
            };
            pres_rels.push((id.clone(), rel_type, target.clone(), false));
        }
        pres_rels.push((
            format!("rId{next_rel}"),
            format!("{REL}theme"),
            "theme/theme1.xml".into(),
            false,
        ));
        pres.push_str(&format!(r#"<p:sldSz cx="{}" cy="{}"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr></p:defaultTextStyle>{}</p:presentation>"#, self.slide_size.0, self.slide_size.1, self.presentation_extra));
        parts.push(("ppt/presentation.xml".into(), pres.into_bytes()));
        let pres_rel_refs: Vec<(&str, &str, &str, bool)> = pres_rels
            .iter()
            .map(|(id, ty, target, ext)| (id.as_str(), ty.as_str(), target.as_str(), *ext))
            .collect();
        parts.push((
            "ppt/_rels/presentation.xml.rels".into(),
            rels(&pres_rel_refs),
        ));
        parts.push(("ppt/presProps.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentationPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"/>"#).into_bytes()));

        parts.push(("ppt/slideMasters/slideMaster1.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="2" name="Title Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="838200" y="365125"/><a:ext cx="10515600" cy="1325563"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Click to edit Master title style</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz="4400"/></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr marL="228600" indent="-228600"><a:buChar char="&#8226;"/><a:defRPr sz="2800"/></a:lvl1pPr></p:bodyStyle><p:otherStyle><a:defPPr/></p:otherStyle></p:txStyles></p:sldMaster>"#).into_bytes()));
        parts.push((
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
            rels(&[
                (
                    "rId1",
                    &format!("{REL}slideLayout"),
                    "../slideLayouts/slideLayout1.xml",
                    false,
                ),
                ("rId2", &format!("{REL}theme"), "../theme/theme1.xml", false),
            ]),
        ));

        parts.push(("ppt/slideLayouts/slideLayout1.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sldLayout xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" type="obj" preserve="1"><p:cSld name="Title and Content"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>Click to edit Master title style</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Content Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="838200" y="1825625"/><a:ext cx="10515600" cy="4351338"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr lvl="0"/><a:r><a:rPr lang="en-US"/><a:t>Click to edit Master text styles</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#).into_bytes()));
        parts.push((
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
            rels(&[(
                "rId1",
                &format!("{REL}slideMaster"),
                "../slideMasters/slideMaster1.xml",
                false,
            )]),
        ));

        parts.push(("ppt/theme/theme1.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="{A}" name="Office Theme"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#).into_bytes()));

        if has_notes {
            parts.push(("ppt/notesMasters/notesMaster1.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:notesMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:notesMaster>"#).into_bytes()));
            parts.push((
                "ppt/notesMasters/_rels/notesMaster1.xml.rels".into(),
                rels(&[("rId1", &format!("{REL}theme"), "../theme/theme1.xml", false)]),
            ));
        }

        for (i, slide) in self.slides.iter().enumerate() {
            let n = i + 1;
            let mut shapes = String::new();
            let mut next_id = 2;
            if let Some(title) = &slide.title {
                shapes.push_str(&format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="{next_id}" name="Title {next_id}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p></p:txBody></p:sp>"#, escape(title)));
                next_id += 1;
            }
            if !slide.bullets.is_empty() {
                let mut paragraphs = String::new();
                for bullet in &slide.bullets {
                    paragraphs.push_str(&format!(
                        r#"<a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p>"#,
                        escape(bullet)
                    ));
                }
                shapes.push_str(&format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="{next_id}" name="Content Placeholder {next_id}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"#));
            }
            shapes.push_str(&slide.extra_shapes);
            let show = match slide.hidden {
                true => r#" show="0""#,
                false => "",
            };
            parts.push((format!("ppt/slides/slide{n}.xml"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"{show}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#).into_bytes()));
            let mut slide_rels: Vec<(String, String, String, bool)> = vec![(
                "rId1".into(),
                format!("{REL}slideLayout"),
                "../slideLayouts/slideLayout1.xml".into(),
                false,
            )];
            if slide.notes.is_some() {
                slide_rels.push((
                    "rId2".into(),
                    format!("{REL}notesSlide"),
                    format!("../notesSlides/notesSlide{n}.xml"),
                    false,
                ));
            }
            for (id, tail, target, external) in &slide.extra_rels {
                let rel_type = match tail.contains("://") {
                    true => tail.clone(),
                    false => format!("{REL}{tail}"),
                };
                slide_rels.push((id.clone(), rel_type, target.clone(), *external));
            }
            let refs: Vec<(&str, &str, &str, bool)> = slide_rels
                .iter()
                .map(|(id, ty, target, ext)| (id.as_str(), ty.as_str(), target.as_str(), *ext))
                .collect();
            parts.push((format!("ppt/slides/_rels/slide{n}.xml.rels"), rels(&refs)));
            if let Some(notes) = &slide.notes {
                parts.push((format!("ppt/notesSlides/notesSlide{n}.xml"), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:notes xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" dirty="0"/><a:t>{}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#, escape(notes)).into_bytes()));
                parts.push((
                    format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                    rels(&[
                        (
                            "rId1",
                            &format!("{REL}notesMaster"),
                            "../notesMasters/notesMaster1.xml",
                            false,
                        ),
                        (
                            "rId2",
                            &format!("{REL}slide"),
                            &format!("../slides/slide{n}.xml"),
                            false,
                        ),
                    ]),
                ));
            }
        }

        parts.push(("docProps/core.xml".into(), br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>Test Deck</dc:title><dc:creator>pptxboss testkit</dc:creator><cp:lastModifiedBy>pptxboss testkit</cp:lastModifiedBy><cp:revision>1</cp:revision><dcterms:created xsi:type="dcterms:W3CDTF">2026-01-02T03:04:05Z</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">2026-01-02T03:04:05Z</dcterms:modified></cp:coreProperties>"#.to_vec()));
        parts.push(("docProps/app.xml".into(), format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>pptxboss testkit</Application><PresentationFormat>Widescreen</PresentationFormat><Slides>{}</Slides><Notes>{}</Notes></Properties>"#, self.slides.len(), self.slides.iter().filter(|slide| slide.notes.is_some()).count()).into_bytes()));

        for (name, bytes) in &self.media {
            parts.push((name.clone(), bytes.clone()));
        }
        parts
    }

    /// The package bytes.
    pub fn build(&self) -> Vec<u8> {
        let mut builder = ZipBuilder::new();
        let mut seen = Vec::new();
        for (name, bytes) in self.parts() {
            if self.omitted.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            let data = self.overrides.get(&name).cloned().unwrap_or(bytes);
            builder = builder.deflated(&name, &data);
        }
        for (name, bytes) in &self.overrides {
            if seen.contains(name) || self.omitted.contains(name) {
                continue;
            }
            builder = builder.deflated(name, bytes);
        }
        builder.build()
    }
}

fn rels(entries: &[(&str, &str, &str, bool)]) -> Vec<u8> {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{PKG_REL}">"#
    );
    for (id, rel_type, target, external) in entries {
        let mode = match external {
            true => r#" TargetMode="External""#,
            false => "",
        };
        xml.push_str(&format!(
            r#"<Relationship Id="{id}" Type="{rel_type}" Target="{}"{mode}/>"#,
            escape(target)
        ));
    }
    xml.push_str("</Relationships>");
    xml.into_bytes()
}

/// Escapes text for XML character data and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}
