//! Open Packaging Conventions (ECMA-376 Part 2): part names, the content
//! types stream and relationships.
//!
//! Everything here is the raw view. Parsers keep duplicates and record
//! defects instead of discarding them, so the verifier can report what a
//! lenient reader would silently resolve.

use crate::hash::FastMap;
use crate::xml::{unescape_attr, Event, Ns, Reader, XmlError};

/// A problem found while parsing, kept beside the result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Defect {
    pub offset: usize,
    pub msg: &'static str,
}

/// Media type of a Relationships part (Annex E).
pub const RELATIONSHIPS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.relationships+xml";
/// Media type of the Core Properties part (Annex E).
pub const CORE_PROPERTIES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-package.core-properties+xml";
/// Relationship type of the Core Properties part (Annex E).
pub const CORE_PROPERTIES_REL: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
/// Relationship type of a thumbnail (Annex E).
pub const THUMBNAIL_REL: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail";
/// The name of the content types stream in a ZIP package (7.3.7).
pub const CONTENT_TYPES_ITEM: &str = "[Content_Types].xml";
/// The package relationships part (6.5.2.2).
pub const PACKAGE_RELS: &str = "/_rels/.rels";

/// Why a string is not a part name (6.2.2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartNameError {
    Empty,
    NoLeadingSlash,
    EmptySegment,
    TrailingSlash,
    SegmentEndsWithDot,
    ForbiddenCharacter(u8),
    BadPercentEncoding,
    PercentEncodedSlash,
    PercentEncodedUnreserved,
}

impl std::fmt::Display for PartNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartNameError::Empty => f.write_str("empty part name"),
            PartNameError::NoLeadingSlash => f.write_str("part name does not start with '/'"),
            PartNameError::EmptySegment => f.write_str("empty segment"),
            PartNameError::TrailingSlash => f.write_str("trailing '/'"),
            PartNameError::SegmentEndsWithDot => f.write_str("segment ends with '.'"),
            PartNameError::ForbiddenCharacter(byte) => write!(
                f,
                "character {:?} must be percent-encoded",
                char::from(*byte)
            ),
            PartNameError::BadPercentEncoding => f.write_str("'%' not followed by two hex digits"),
            PartNameError::PercentEncodedSlash => f.write_str("percent-encoded slash"),
            PartNameError::PercentEncodedUnreserved => {
                f.write_str("percent-encoded unreserved character")
            }
        }
    }
}

/// Checks `name` against the part name grammar of 6.2.2.2.
pub fn validate_part_name(name: &str) -> Result<(), PartNameError> {
    if name.is_empty() {
        return Err(PartNameError::Empty);
    }
    if !name.starts_with('/') {
        return Err(PartNameError::NoLeadingSlash);
    }
    if name.ends_with('/') {
        return Err(PartNameError::TrailingSlash);
    }
    for segment in name[1..].split('/') {
        if segment.is_empty() {
            return Err(PartNameError::EmptySegment);
        }
        if segment.ends_with('.') {
            return Err(PartNameError::SegmentEndsWithDot);
        }
        let bytes = segment.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let byte = bytes[i];
            if byte == b'%' {
                let hex = bytes
                    .get(i + 1..i + 3)
                    .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
                    .ok_or(PartNameError::BadPercentEncoding)?;
                let value =
                    u8::from_str_radix(std::str::from_utf8(hex).unwrap_or("00"), 16).unwrap_or(0);
                if value == b'/' || value == b'\\' {
                    return Err(PartNameError::PercentEncodedSlash);
                }
                if value.is_ascii_alphanumeric() || matches!(value, b'-' | b'.' | b'_' | b'~') {
                    return Err(PartNameError::PercentEncodedUnreserved);
                }
                i += 3;
                continue;
            }
            if byte >= 0x80
                || byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'-' | b'.'
                        | b'_'
                        | b'~'
                        | b'!'
                        | b'$'
                        | b'&'
                        | b'\''
                        | b'('
                        | b')'
                        | b'*'
                        | b'+'
                        | b','
                        | b';'
                        | b'='
                        | b':'
                        | b'@'
                )
            {
                i += 1;
                continue;
            }
            return Err(PartNameError::ForbiddenCharacter(byte));
        }
    }
    Ok(())
}

/// The equivalence key of a part name: ASCII letters folded to lower case (6.2.2.3).
pub fn equivalence_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// True when `name` is `other` followed by `/` and more segments (6.2.2.3 "derivable").
pub fn is_derivable(name: &str, other: &str) -> bool {
    let name = equivalence_key(name);
    let mut prefix = equivalence_key(other);
    prefix.push('/');
    name.starts_with(&prefix)
}

/// The Relationships part name for `source` (6.5.2.2, 6.5.2.3); `/` is the package.
pub fn rels_part_name(source: &str) -> String {
    if source == "/" || source.is_empty() {
        return PACKAGE_RELS.to_string();
    }
    let (dir, base) = match source.rfind('/') {
        Some(slash) => (&source[..slash], &source[slash + 1..]),
        None => ("", source),
    };
    format!("{dir}/_rels/{base}.rels")
}

/// The source part of a Relationships part name, or `/` for the package rels.
pub fn source_of_rels(rels: &str) -> Option<String> {
    let stem = rels.strip_suffix(".rels")?;
    let slash = stem.rfind('/')?;
    let (dir, base) = (&stem[..slash], &stem[slash + 1..]);
    let dir = dir.strip_suffix("/_rels")?;
    if base.is_empty() {
        return match dir.is_empty() {
            true => Some("/".to_string()),
            false => None,
        };
    }
    Some(format!("{dir}/{base}"))
}

/// Whether `name` names a Relationships part.
pub fn is_rels_part(name: &str) -> bool {
    source_of_rels(name).is_some()
}

/// The extension of a part name: the text after the last `.` of its last segment (7.2.3.5).
pub fn extension(name: &str) -> Option<&str> {
    let last = name.rsplit('/').next()?;
    let dot = last.rfind('.')?;
    Some(&last[dot + 1..])
}

/// Resolves a relationship `target` against the part it was found in,
/// returning an absolute part name (RFC 3986 5.2 on the path; 6.4).
/// `base` is the source part for a part Relationships part and `/` for
/// the package. Query and fragment are dropped.
pub fn resolve_target(base: &str, target: &str) -> String {
    let target = target.split(['?', '#']).next().unwrap_or("");
    let target = target.replace('\\', "/");
    let merged = match target.starts_with('/') {
        true => target,
        false => {
            let dir_end = base.rfind('/').map_or(0, |slash| slash + 1);
            format!("{}{}", &base[..dir_end], target)
        }
    };
    remove_dot_segments(&merged)
}

fn remove_dot_segments(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for segment in path.split('/').skip(1) {
        match segment {
            "." | "" => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    let mut result = String::with_capacity(path.len());
    for segment in &out {
        result.push('/');
        result.push_str(segment);
    }
    if result.is_empty() {
        result.push('/');
    }
    result
}

/// The content types stream (7.2.3), as declared.
#[derive(Clone, Debug, Default)]
pub struct ContentTypes {
    defaults: Vec<(String, String)>,
    default_index: FastMap<String, usize>,
    overrides: Vec<(String, String)>,
    override_index: FastMap<String, usize>,
    /// Indexes into `defaults()` whose extension repeats an earlier one.
    pub duplicate_defaults: Vec<usize>,
    /// Indexes into `overrides()` whose part name repeats an earlier one.
    pub duplicate_overrides: Vec<usize>,
    pub defects: Vec<Defect>,
    /// True when the root was in a namespace other than the content types namespace.
    pub wrong_namespace: bool,
}

impl ContentTypes {
    /// Parses `[Content_Types].xml`; unknown elements are skipped and recorded.
    pub fn parse(xml: &[u8]) -> Result<Self, XmlError> {
        let mut reader = Reader::new(xml);
        let mut types = ContentTypes::default();
        loop {
            match reader.next()? {
                Event::Start(start) if reader.depth() == 1 => {
                    types.wrong_namespace =
                        start.name.ns != Ns::ContentTypes || start.name.local != b"Types";
                }
                Event::Start(start) if reader.depth() == 2 && start.name.local == b"Default" => {
                    let ext = reader
                        .attr(&start, Ns::None, b"Extension")
                        .map(unescape_attr);
                    let content_type = reader
                        .attr(&start, Ns::None, b"ContentType")
                        .map(unescape_attr);
                    match (ext, content_type) {
                        (Some(ext), Some(content_type)) => types.add_default(ext, content_type),
                        _ => types.defects.push(Defect {
                            offset: start.offset,
                            msg: "Default element without Extension or ContentType",
                        }),
                    }
                    reader.skip_element()?;
                }
                Event::Start(start) if reader.depth() == 2 && start.name.local == b"Override" => {
                    let part_name = reader
                        .attr(&start, Ns::None, b"PartName")
                        .map(unescape_attr);
                    let content_type = reader
                        .attr(&start, Ns::None, b"ContentType")
                        .map(unescape_attr);
                    match (part_name, content_type) {
                        (Some(part_name), Some(content_type)) => {
                            types.add_override(part_name, content_type)
                        }
                        _ => types.defects.push(Defect {
                            offset: start.offset,
                            msg: "Override element without PartName or ContentType",
                        }),
                    }
                    reader.skip_element()?;
                }
                Event::Start(start) => {
                    types.defects.push(Defect {
                        offset: start.offset,
                        msg: "unexpected element in the content types stream",
                    });
                    reader.skip_element()?;
                }
                Event::Eof => return Ok(types),
                _ => {}
            }
        }
    }

    fn add_default(&mut self, extension: String, content_type: String) {
        let key = extension.to_ascii_lowercase();
        let index = self.defaults.len();
        self.defaults.push((extension, content_type));
        if self.default_index.contains_key(&key) {
            self.duplicate_defaults.push(index);
            return;
        }
        self.default_index.insert(key, index);
    }

    fn add_override(&mut self, part_name: String, content_type: String) {
        let key = equivalence_key(&part_name);
        let index = self.overrides.len();
        self.overrides.push((part_name, content_type));
        if self.override_index.contains_key(&key) {
            self.duplicate_overrides.push(index);
            return;
        }
        self.override_index.insert(key, index);
    }

    /// `(Extension, ContentType)` pairs in document order.
    pub fn defaults(&self) -> &[(String, String)] {
        &self.defaults
    }

    /// `(PartName, ContentType)` pairs in document order.
    pub fn overrides(&self) -> &[(String, String)] {
        &self.overrides
    }

    /// The content type of `part_name` (7.2.3.5): a matching Override wins,
    /// then a Default matching the extension; both compared ASCII case-insensitively.
    pub fn content_type_of(&self, part_name: &str) -> Option<&str> {
        if let Some(&index) = self.override_index.get(&equivalence_key(part_name)) {
            return Some(&self.overrides[index].1);
        }
        let ext = extension(part_name)?.to_ascii_lowercase();
        self.default_index
            .get(&ext)
            .map(|&index| self.defaults[index].1.as_str())
    }

    /// The content type a Default declares for `extension`, if any.
    pub fn default_for(&self, extension: &str) -> Option<&str> {
        self.default_index
            .get(&extension.to_ascii_lowercase())
            .map(|&index| self.defaults[index].1.as_str())
    }

    /// The content type an Override declares for exactly `part_name`, if any.
    pub fn override_for(&self, part_name: &str) -> Option<&str> {
        self.override_index
            .get(&equivalence_key(part_name))
            .map(|&index| self.overrides[index].1.as_str())
    }
}

/// `TargetMode` of a relationship (6.5.3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetMode {
    Internal,
    External,
}

/// One `Relationship` element, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    pub rel_type: String,
    pub target: String,
    pub mode: TargetMode,
    /// Byte offset of the element in the Relationships part.
    pub offset: usize,
}

/// A parsed Relationships part (6.5).
#[derive(Clone, Debug, Default)]
pub struct Relationships {
    /// The source part these relationships belong to; `/` for the package.
    pub source: String,
    items: Vec<Relationship>,
    index: FastMap<String, usize>,
    /// Indexes of relationships whose Id repeats an earlier one.
    pub duplicate_ids: Vec<usize>,
    pub defects: Vec<Defect>,
    pub wrong_namespace: bool,
}

impl Relationships {
    /// An empty relationships set for `source`.
    pub fn empty(source: &str) -> Self {
        Self {
            source: source.to_string(),
            ..Self::default()
        }
    }

    /// Parses a Relationships part belonging to `source`.
    pub fn parse(source: &str, xml: &[u8]) -> Result<Self, XmlError> {
        let mut reader = Reader::new(xml);
        let mut rels = Relationships::empty(source);
        loop {
            match reader.next()? {
                Event::Start(start) if reader.depth() == 1 => {
                    rels.wrong_namespace =
                        start.name.ns != Ns::PkgRel || start.name.local != b"Relationships";
                }
                Event::Start(start)
                    if reader.depth() == 2 && start.name.local == b"Relationship" =>
                {
                    let mut id = None;
                    let mut rel_type = None;
                    let mut target = None;
                    let mut mode = TargetMode::Internal;
                    for attr in reader.attrs(&start) {
                        if attr.name.ns != Ns::None {
                            continue;
                        }
                        match attr.name.local {
                            b"Id" => id = Some(unescape_attr(attr.raw_value)),
                            b"Type" => rel_type = Some(unescape_attr(attr.raw_value)),
                            b"Target" => target = Some(unescape_attr(attr.raw_value)),
                            b"TargetMode" => match attr.raw_value {
                                b"External" => mode = TargetMode::External,
                                b"Internal" => mode = TargetMode::Internal,
                                _ => rels.defects.push(Defect {
                                    offset: start.offset,
                                    msg: "TargetMode is neither Internal nor External",
                                }),
                            },
                            _ => {}
                        }
                    }
                    match (id, rel_type, target) {
                        (Some(id), Some(rel_type), Some(target)) => rels.add(Relationship {
                            id,
                            rel_type,
                            target,
                            mode,
                            offset: start.offset,
                        }),
                        _ => rels.defects.push(Defect {
                            offset: start.offset,
                            msg: "Relationship without Id, Type or Target",
                        }),
                    }
                    reader.skip_element()?;
                }
                Event::Start(start) => {
                    rels.defects.push(Defect {
                        offset: start.offset,
                        msg: "unexpected element in a Relationships part",
                    });
                    reader.skip_element()?;
                }
                Event::Eof => return Ok(rels),
                _ => {}
            }
        }
    }

    fn add(&mut self, rel: Relationship) {
        let index = self.items.len();
        if self.index.contains_key(&rel.id) {
            self.duplicate_ids.push(index);
            self.items.push(rel);
            return;
        }
        self.index.insert(rel.id.clone(), index);
        self.items.push(rel);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Relationship> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The first relationship with this Id.
    pub fn get(&self, id: &str) -> Option<&Relationship> {
        self.index.get(id).map(|&index| &self.items[index])
    }

    /// Relationships of exactly this type (6.5.3.4: compared as strings).
    pub fn by_type<'s>(&'s self, rel_type: &'s str) -> impl Iterator<Item = &'s Relationship> + 's {
        self.items
            .iter()
            .filter(move |rel| rel.rel_type == rel_type)
    }

    pub fn first_of_type(&self, rel_type: &str) -> Option<&Relationship> {
        self.items.iter().find(|rel| rel.rel_type == rel_type)
    }

    /// The absolute part name an Internal relationship points at.
    pub fn resolve(&self, rel: &Relationship) -> Option<String> {
        match rel.mode {
            TargetMode::External => None,
            TargetMode::Internal => Some(resolve_target(&self.source, &rel.target)),
        }
    }

    /// The absolute part name behind relationship `id`, if internal.
    pub fn target_of(&self, id: &str) -> Option<String> {
        self.get(id).and_then(|rel| self.resolve(rel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_name_grammar() {
        assert_eq!(validate_part_name("/ppt/slides/slide1.xml"), Ok(()));
        assert_eq!(validate_part_name("/a/%20b/c.xml"), Ok(()));
        assert_eq!(validate_part_name("/ppt/média.xml"), Ok(()));
        assert_eq!(validate_part_name(""), Err(PartNameError::Empty));
        assert_eq!(
            validate_part_name("ppt/x.xml"),
            Err(PartNameError::NoLeadingSlash)
        );
        assert_eq!(
            validate_part_name("/ppt/"),
            Err(PartNameError::TrailingSlash)
        );
        assert_eq!(
            validate_part_name("/ppt//x.xml"),
            Err(PartNameError::EmptySegment)
        );
        assert_eq!(
            validate_part_name("/ppt/./x.xml"),
            Err(PartNameError::SegmentEndsWithDot)
        );
        assert_eq!(
            validate_part_name("/ppt/../x.xml"),
            Err(PartNameError::SegmentEndsWithDot)
        );
        assert_eq!(
            validate_part_name("/ppt/x."),
            Err(PartNameError::SegmentEndsWithDot)
        );
        assert_eq!(
            validate_part_name("/a b.xml"),
            Err(PartNameError::ForbiddenCharacter(b' '))
        );
        assert_eq!(
            validate_part_name("/a%2Fb.xml"),
            Err(PartNameError::PercentEncodedSlash)
        );
        assert_eq!(
            validate_part_name("/%41.xml"),
            Err(PartNameError::PercentEncodedUnreserved)
        );
        assert_eq!(
            validate_part_name("/%XY.xml"),
            Err(PartNameError::BadPercentEncoding)
        );
        assert_eq!(
            validate_part_name("/[Content_Types].xml"),
            Err(PartNameError::ForbiddenCharacter(b'['))
        );
    }

    #[test]
    fn equivalence_and_derivability_are_segment_aware() {
        assert_eq!(
            equivalence_key("/PPT/Slides/Slide1.XML"),
            "/ppt/slides/slide1.xml"
        );
        assert!(is_derivable("/a/b", "/a"));
        assert!(is_derivable("/A/b", "/a"));
        assert!(!is_derivable("/ab", "/a"));
        assert!(!is_derivable("/a", "/a"));
    }

    #[test]
    fn rels_names_round_trip() {
        assert_eq!(rels_part_name("/"), "/_rels/.rels");
        assert_eq!(
            rels_part_name("/ppt/presentation.xml"),
            "/ppt/_rels/presentation.xml.rels"
        );
        assert_eq!(
            rels_part_name("/ppt/slides/slide1.xml"),
            "/ppt/slides/_rels/slide1.xml.rels"
        );
        assert_eq!(source_of_rels("/_rels/.rels").as_deref(), Some("/"));
        assert_eq!(
            source_of_rels("/ppt/slides/_rels/slide1.xml.rels").as_deref(),
            Some("/ppt/slides/slide1.xml")
        );
        assert_eq!(source_of_rels("/ppt/slides/slide1.xml"), None);
        assert_eq!(source_of_rels("/ppt/_rels/.rels"), None);
        assert!(is_rels_part("/_rels/.rels"));
        assert!(!is_rels_part("/ppt/slides/slide1.xml"));
    }

    #[test]
    fn extensions_come_from_the_last_segment_only() {
        assert_eq!(extension("/ppt/slides/slide1.xml"), Some("xml"));
        assert_eq!(extension("/ppt.d/media/image"), None);
        assert_eq!(extension("/ppt/media/image1.PNG"), Some("PNG"));
        assert_eq!(extension("/_rels/.rels"), Some("rels"));
    }

    #[test]
    fn targets_resolve_against_the_source_part() {
        assert_eq!(
            resolve_target("/ppt/slides/slide1.xml", "../slideLayouts/slideLayout1.xml"),
            "/ppt/slideLayouts/slideLayout1.xml"
        );
        assert_eq!(
            resolve_target("/", "ppt/presentation.xml"),
            "/ppt/presentation.xml"
        );
        assert_eq!(
            resolve_target("/", "/ppt/presentation.xml"),
            "/ppt/presentation.xml"
        );
        assert_eq!(resolve_target("/a/b/foo.xml", "bar.xml"), "/a/b/bar.xml");
        assert_eq!(resolve_target("/a/b/foo.xml", "./bar.xml"), "/a/b/bar.xml");
        assert_eq!(resolve_target("/a/b/foo.xml", "/b/bar.xml"), "/b/bar.xml");
        assert_eq!(resolve_target("/", "../bar.xml"), "/bar.xml");
        assert_eq!(
            resolve_target("/ppt/slides/slide1.xml", "../media/image1.png?x=1#frag"),
            "/ppt/media/image1.png"
        );
        assert_eq!(
            resolve_target("/ppt/slides/slide1.xml", "..\\media\\image1.png"),
            "/ppt/media/image1.png"
        );
        assert_eq!(
            resolve_target("/ppt/slides/slide1.xml", "../../../../x.xml"),
            "/x.xml"
        );
    }

    const TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="PNG" ContentType="image/png"/>
<Default Extension="xml" ContentType="text/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
<Override PartName="/PPT/slides/slide1.xml" ContentType="dup"/>
<Bogus/>
</Types>"#;

    #[test]
    fn content_types_match_overrides_then_defaults_case_insensitively() {
        let types = ContentTypes::parse(TYPES).unwrap();
        assert!(!types.wrong_namespace);
        assert_eq!(types.content_type_of("/ppt/presentation.xml"), Some("application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"));
        assert_eq!(
            types.content_type_of("/ppt/Slides/Slide1.xml"),
            Some("application/vnd.openxmlformats-officedocument.presentationml.slide+xml")
        );
        assert_eq!(
            types.content_type_of("/ppt/slides/slide2.xml"),
            Some("application/xml")
        );
        assert_eq!(
            types.content_type_of("/ppt/media/image1.png"),
            Some("image/png")
        );
        assert_eq!(types.content_type_of("/ppt/media/image1"), None);
        assert_eq!(
            types.content_type_of("/_rels/.rels"),
            Some("application/vnd.openxmlformats-package.relationships+xml")
        );
        assert_eq!(types.defaults().len(), 4);
        assert_eq!(types.duplicate_defaults, vec![3]);
        assert_eq!(types.overrides().len(), 3);
        assert_eq!(types.duplicate_overrides, vec![2]);
        assert_eq!(types.defects.len(), 1);
        assert_eq!(
            types.defects[0].msg,
            "unexpected element in the content types stream"
        );
        assert_eq!(
            types.default_for("Rels"),
            Some("application/vnd.openxmlformats-package.relationships+xml")
        );
        assert_eq!(
            types.override_for("/ppt/slides/slide1.xml"),
            Some("application/vnd.openxmlformats-officedocument.presentationml.slide+xml")
        );
    }

    #[test]
    fn content_types_in_the_wrong_namespace_are_flagged() {
        let types = ContentTypes::parse(
            b"<Types><Default Extension=\"xml\" ContentType=\"application/xml\"/></Types>",
        )
        .unwrap();
        assert!(types.wrong_namespace);
        assert_eq!(types.content_type_of("/a.xml"), Some("application/xml"));
        assert!(ContentTypes::parse(b"<Types").is_err());
    }

    const RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/?a=1&amp;b=2" TargetMode="External"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image2.png"/>
<Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="x" TargetMode="Sideways"/>
<Relationship Id="rId6" Target="nothing"/>
</Relationships>"#;

    #[test]
    fn relationships_keep_duplicates_and_resolve_internal_targets() {
        let rels = Relationships::parse("/ppt/slides/slide1.xml", RELS).unwrap();
        assert!(!rels.wrong_namespace);
        assert_eq!(rels.len(), 5);
        assert_eq!(rels.duplicate_ids, vec![3]);
        assert_eq!(rels.defects.len(), 2);
        assert_eq!(
            rels.target_of("rId1").as_deref(),
            Some("/ppt/slideLayouts/slideLayout1.xml")
        );
        let link = rels.get("rId2").unwrap();
        assert_eq!(link.mode, TargetMode::External);
        assert_eq!(link.target, "https://example.com/?a=1&b=2");
        assert_eq!(rels.resolve(link), None);
        assert_eq!(
            rels.target_of("rId3").as_deref(),
            Some("/ppt/media/image1.png")
        );
        assert_eq!(
            rels.by_type(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
            )
            .count(),
            3
        );
        assert!(rels.first_of_type("urn:none").is_none());
        assert_eq!(rels.get("rId6"), None);
        let package = Relationships::parse("/", b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"a\" Type=\"t\" Target=\"ppt/presentation.xml\"/></Relationships>").unwrap();
        assert_eq!(
            package.target_of("a").as_deref(),
            Some("/ppt/presentation.xml")
        );
    }
}
