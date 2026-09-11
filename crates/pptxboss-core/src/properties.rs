//! Package metadata: the Core Properties part (ECMA-376 Part 2, clause 11)
//! and the Extended Properties part (Part 1, clause 22.2). Both are read
//! leniently: unknown elements are skipped, values are trimmed, and an
//! empty element counts as absent.

use crate::mce::children;
use crate::xml::{Event, Ns, Reader, Start, XmlError};

/// `docProps/core.xml`: Dublin Core and OPC properties, as strings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoreProperties {
    pub title: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub keywords: Option<String>,
    pub description: Option<String>,
    pub last_modified_by: Option<String>,
    pub revision: Option<String>,
    /// W3CDTF timestamp as written, e.g. `2024-05-01T10:00:00Z`.
    pub created: Option<String>,
    pub modified: Option<String>,
    pub last_printed: Option<String>,
    pub category: Option<String>,
    pub content_status: Option<String>,
    pub language: Option<String>,
    pub identifier: Option<String>,
    pub version: Option<String>,
}

impl CoreProperties {
    pub fn parse(xml: &[u8]) -> Result<Self, XmlError> {
        let mut reader = Reader::new(xml);
        root(&mut reader, xml)?;
        let mut props = Self::default();
        children(&mut reader, &mut |reader, child| {
            let slot = match (child.name.ns, child.name.local) {
                (Ns::Dc, b"title") => &mut props.title,
                (Ns::Dc, b"subject") => &mut props.subject,
                (Ns::Dc, b"creator") => &mut props.creator,
                (Ns::Dc, b"description") => &mut props.description,
                (Ns::Dc, b"language") => &mut props.language,
                (Ns::Dc, b"identifier") => &mut props.identifier,
                (Ns::Cp, b"keywords") => &mut props.keywords,
                (Ns::Cp, b"lastModifiedBy") => &mut props.last_modified_by,
                (Ns::Cp, b"revision") => &mut props.revision,
                (Ns::Cp, b"lastPrinted") => &mut props.last_printed,
                (Ns::Cp, b"category") => &mut props.category,
                (Ns::Cp, b"contentStatus") => &mut props.content_status,
                (Ns::Cp, b"version") => &mut props.version,
                (Ns::Dcterms, b"created") => &mut props.created,
                (Ns::Dcterms, b"modified") => &mut props.modified,
                _ => return reader.skip_element(),
            };
            *slot = text_of(reader)?;
            Ok(())
        })?;
        Ok(props)
    }

    /// True when no property carries a value.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `docProps/app.xml`: what the writing application recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppProperties {
    pub application: Option<String>,
    pub app_version: Option<String>,
    pub company: Option<String>,
    pub manager: Option<String>,
    pub template: Option<String>,
    pub presentation_format: Option<String>,
    pub slides: Option<u64>,
    pub notes: Option<u64>,
    pub hidden_slides: Option<u64>,
    pub words: Option<u64>,
    pub paragraphs: Option<u64>,
    pub multimedia_clips: Option<u64>,
    /// Editing time in minutes.
    pub total_time: Option<u64>,
    /// `TitlesOfParts`: font names, the theme, then one entry per slide title.
    pub titles_of_parts: Vec<String>,
}

impl AppProperties {
    pub fn parse(xml: &[u8]) -> Result<Self, XmlError> {
        let mut reader = Reader::new(xml);
        root(&mut reader, xml)?;
        let mut props = Self::default();
        children(&mut reader, &mut |reader, child| {
            if child.name.ns != Ns::Ep {
                return reader.skip_element();
            }
            let text_slot = match child.name.local {
                b"Application" => Some(&mut props.application),
                b"AppVersion" => Some(&mut props.app_version),
                b"Company" => Some(&mut props.company),
                b"Manager" => Some(&mut props.manager),
                b"Template" => Some(&mut props.template),
                b"PresentationFormat" => Some(&mut props.presentation_format),
                _ => None,
            };
            if let Some(slot) = text_slot {
                *slot = text_of(reader)?;
                return Ok(());
            }
            let count_slot = match child.name.local {
                b"Slides" => Some(&mut props.slides),
                b"Notes" => Some(&mut props.notes),
                b"HiddenSlides" => Some(&mut props.hidden_slides),
                b"Words" => Some(&mut props.words),
                b"Paragraphs" => Some(&mut props.paragraphs),
                b"MMClips" => Some(&mut props.multimedia_clips),
                b"TotalTime" => Some(&mut props.total_time),
                _ => None,
            };
            if let Some(slot) = count_slot {
                *slot = text_of(reader)?.and_then(|text| text.parse().ok());
                return Ok(());
            }
            if child.name.local == b"TitlesOfParts" {
                return collect_strings(reader, &mut props.titles_of_parts);
            }
            reader.skip_element()
        })?;
        Ok(props)
    }
}

/// Consumes the root start tag or fails when the part has none.
fn root<'a>(reader: &mut Reader<'a>, xml: &[u8]) -> Result<Start<'a>, XmlError> {
    loop {
        match reader.next()? {
            Event::Start(start) => return Ok(start),
            Event::Eof => {
                return Err(XmlError {
                    offset: xml.len(),
                    msg: "no root element",
                })
            }
            _ => {}
        }
    }
}

/// The trimmed text of the current element, None when blank.
fn text_of(reader: &mut Reader<'_>) -> Result<Option<String>, XmlError> {
    let mut text = String::new();
    reader.text_content(&mut text)?;
    let trimmed = text.trim();
    Ok(match trimmed.is_empty() {
        true => None,
        false => Some(trimmed.to_string()),
    })
}

/// Every `vt:lpstr` (or other leaf) under the current element, in order.
fn collect_strings(reader: &mut Reader<'_>, out: &mut Vec<String>) -> Result<(), XmlError> {
    children(reader, &mut |reader, child| {
        if child.name.is(Ns::Vt, b"vector") || child.name.is(Ns::Vt, b"variant") {
            return collect_strings(reader, out);
        }
        if let Some(text) = text_of(reader)? {
            out.push(text);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_properties_read_dublin_core_and_opc_elements() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<dc:title>Quarterly &amp; review</dc:title><dc:creator>Ada</dc:creator><cp:lastModifiedBy>Bob</cp:lastModifiedBy><cp:revision>7</cp:revision>
<dcterms:created xsi:type="dcterms:W3CDTF">2024-05-01T10:00:00Z</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">2024-06-02T11:30:00Z</dcterms:modified>
<dc:description></dc:description><cp:keywords> tags </cp:keywords><cp:unknown>x</cp:unknown></cp:coreProperties>"#;
        let props = CoreProperties::parse(xml).unwrap();
        assert_eq!(props.title.as_deref(), Some("Quarterly & review"));
        assert_eq!(props.creator.as_deref(), Some("Ada"));
        assert_eq!(props.last_modified_by.as_deref(), Some("Bob"));
        assert_eq!(props.revision.as_deref(), Some("7"));
        assert_eq!(props.created.as_deref(), Some("2024-05-01T10:00:00Z"));
        assert_eq!(props.modified.as_deref(), Some("2024-06-02T11:30:00Z"));
        assert_eq!(props.description, None);
        assert_eq!(props.keywords.as_deref(), Some("tags"));
        assert!(!props.is_empty());
        assert!(CoreProperties::parse(br#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties"/>"#).unwrap().is_empty());
    }

    #[test]
    fn app_properties_read_counts_and_titles_of_parts() {
        let xml = br#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">
<TotalTime>12</TotalTime><Words>345</Words><Application>Microsoft Macintosh PowerPoint</Application><PresentationFormat>Widescreen</PresentationFormat><Paragraphs>40</Paragraphs><Slides>7</Slides><Notes>2</Notes><HiddenSlides>0</HiddenSlides><MMClips>0</MMClips>
<HeadingPairs><vt:vector size="4" baseType="variant"><vt:variant><vt:lpstr>Theme</vt:lpstr></vt:variant><vt:variant><vt:i4>1</vt:i4></vt:variant><vt:variant><vt:lpstr>Slide Titles</vt:lpstr></vt:variant><vt:variant><vt:i4>2</vt:i4></vt:variant></vt:vector></HeadingPairs>
<TitlesOfParts><vt:vector size="3" baseType="lpstr"><vt:lpstr>Office Theme</vt:lpstr><vt:lpstr>Intro</vt:lpstr><vt:lpstr>Numbers</vt:lpstr></vt:vector></TitlesOfParts>
<Company>ACME</Company><AppVersion>16.0000</AppVersion></Properties>"#;
        let props = AppProperties::parse(xml).unwrap();
        assert_eq!(props.total_time, Some(12));
        assert_eq!(props.words, Some(345));
        assert_eq!(props.slides, Some(7));
        assert_eq!(props.notes, Some(2));
        assert_eq!(props.hidden_slides, Some(0));
        assert_eq!(
            props.application.as_deref(),
            Some("Microsoft Macintosh PowerPoint")
        );
        assert_eq!(props.presentation_format.as_deref(), Some("Widescreen"));
        assert_eq!(props.company.as_deref(), Some("ACME"));
        assert_eq!(props.app_version.as_deref(), Some("16.0000"));
        assert_eq!(props.titles_of_parts, ["Office Theme", "Intro", "Numbers"]);
    }
}
