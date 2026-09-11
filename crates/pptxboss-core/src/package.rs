//! The raw Open Packaging Conventions view of a ZIP package (ECMA-376
//! Part 2, clause 7): every item mapped to a part name, the content types
//! stream, and relationships parsed on demand.
//!
//! Nothing here interprets PresentationML. Defects such as invalid part
//! names, case-insensitive name collisions or a missing content types
//! stream are recorded, not repaired, so the verifier can report them
//! while [`crate::Document`] reads around them.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use crate::encoding::utf16_xml_to_utf8;
use crate::error::{Error, Result};
use crate::hash::FastMap;
use crate::opc::{
    equivalence_key, rels_part_name, validate_part_name, ContentTypes, PartNameError,
    Relationships, CONTENT_TYPES_ITEM,
};
use crate::xml::XmlError;
use crate::zip::{Archive, Entry, Source};

/// A ZIP item that maps to a part.
#[derive(Clone, Debug)]
pub struct Part {
    /// The part name: `/` followed by the item name (7.3.5).
    pub name: String,
    /// Index of the backing entry in the archive; the first piece of an interleaved part.
    pub entry: usize,
    /// Every piece of an interleaved part (7.2.4) in piece order; empty otherwise.
    pub pieces: Vec<usize>,
}

/// What the package layer found wrong with the container's item names.
#[derive(Clone, Debug, Default)]
pub struct PackageDefects {
    /// The content types stream item is absent.
    pub content_types_missing: bool,
    /// The content types stream item exists under a different case than `[Content_Types].xml`.
    pub content_types_case: Option<String>,
    /// The content types stream could not be parsed.
    pub content_types_error: Option<XmlError>,
    /// The content types stream could not be decompressed or read.
    pub content_types_unreadable: Option<String>,
    /// Parts whose names violate the grammar of 6.2.2.2, with the reason.
    pub invalid_names: Vec<(String, PartNameError)>,
    /// Pairs of part names that are equivalent under ASCII case folding (6.2.2.3); the second loses.
    pub collisions: Vec<(String, String)>,
    /// Pairs `(derived, base)` where one part name is derivable from another (6.2.2.3).
    pub derivable: Vec<(String, String)>,
    /// Entries that are directories; a package has no directories.
    pub directories: Vec<String>,
    /// Logical items whose `[n].piece` sequence is incomplete (7.2.5.2); not mapped to parts.
    pub incomplete_pieces: Vec<String>,
}

struct Shared {
    archive: Archive,
    parts: Vec<Part>,
    index: FastMap<String, usize>,
    /// The archive entries of `[Content_Types].xml` (several when
    /// interleaved), parsed on first use: reading a deck through its
    /// relationships never needs it.
    content_types_entries: Vec<usize>,
    content_types: std::sync::OnceLock<ParsedContentTypes>,
    /// Defects found while indexing; the name-grammar and derivability
    /// checks are added on first request, since only the verifier reads them.
    base_defects: PackageDefects,
    defects: std::sync::OnceLock<PackageDefects>,
}

/// A logical item's display name and its pieces as `(number, is last, entry)`.
type PieceSequence = (String, Vec<(u64, bool, usize)>);

/// The content types stream with what went wrong while reading it.
#[derive(Default)]
struct ParsedContentTypes {
    types: ContentTypes,
    error: Option<XmlError>,
    unreadable: Option<String>,
}

impl ParsedContentTypes {
    fn read(archive: &Archive, entries: &[usize]) -> Self {
        if entries.is_empty() {
            return Self::default();
        }
        match read_entries(archive, entries) {
            Err(err) => Self {
                unreadable: Some(err.to_string()),
                ..Self::default()
            },
            Ok(bytes) => match ContentTypes::parse(&bytes) {
                Ok(types) => Self {
                    types,
                    ..Self::default()
                },
                Err(err) => Self {
                    error: Some(err),
                    ..Self::default()
                },
            },
        }
    }
}

/// A thread-safe handle from which a [`Package`] with fresh caches is made.
#[derive(Clone)]
pub struct PackageSeed(Arc<Shared>);

/// The raw package: parts, content types, relationships.
pub struct Package {
    shared: Arc<Shared>,
    rels: RefCell<FastMap<String, Rc<Relationships>>>,
    data: RefCell<FastMap<usize, Rc<Vec<u8>>>>,
}

impl Package {
    /// Opens a package file with positioned reads.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let archive = match Archive::open_path(path) {
            Err(Error::NotZip) => {
                return Err(classify_not_zip(&std::fs::read(path).unwrap_or_default()))
            }
            other => other?,
        };
        Self::from_archive(archive)
    }

    /// Opens a package held in memory.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]) {
            return Err(Error::CompoundFile);
        }
        Self::from_archive(Archive::from_bytes(bytes)?)
    }

    /// Opens a package over any positioned source.
    pub fn from_source(source: Arc<dyn Source>) -> Result<Self> {
        Self::from_archive(Archive::open(source)?)
    }

    pub fn from_archive(archive: Archive) -> Result<Self> {
        let mut parts = Vec::with_capacity(archive.entries().len());
        let mut index: FastMap<String, usize> =
            FastMap::with_capacity_and_hasher(archive.entries().len(), Default::default());
        let mut defects = PackageDefects::default();
        let mut content_types_entries = Vec::new();
        let mut pieces: FastMap<String, PieceSequence> = FastMap::default();
        for (i, entry) in archive.entries().iter().enumerate() {
            if entry.is_directory() {
                defects.directories.push(entry.name.clone());
                continue;
            }
            if let Some((prefix_len, number, last)) = piece_suffix(&entry.name) {
                let logical = format!("/{}", &entry.name[..prefix_len]);
                pieces
                    .entry(equivalence_key(&logical))
                    .or_insert_with(|| (logical, Vec::new()))
                    .1
                    .push((number, last, i));
                continue;
            }
            if entry.name.eq_ignore_ascii_case(CONTENT_TYPES_ITEM) {
                if entry.name != CONTENT_TYPES_ITEM {
                    defects.content_types_case = Some(entry.name.clone());
                }
                if content_types_entries.is_empty() {
                    content_types_entries.push(i);
                }
                continue;
            }
            let name = format!("/{}", entry.name);
            let key = equivalence_key(&name);
            if let Some(&existing) = index.get(&key) {
                let existing: &Part = &parts[existing];
                if existing.name != name {
                    defects.collisions.push((existing.name.clone(), name));
                }
                continue;
            }
            index.insert(key, parts.len());
            parts.push(Part {
                name,
                entry: i,
                pieces: Vec::new(),
            });
        }
        let mut sequences: Vec<(String, PieceSequence)> = pieces.into_iter().collect();
        sequences.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, (logical, mut items)) in sequences {
            items.sort_by_key(|(number, _, _)| *number);
            let complete = items
                .iter()
                .enumerate()
                .all(|(position, (number, last, _))| {
                    *number == position as u64 && *last == (position + 1 == items.len())
                });
            if !complete {
                defects.incomplete_pieces.push(logical);
                continue;
            }
            let entries: Vec<usize> = items.iter().map(|(_, _, entry)| *entry).collect();
            if logical[1..].eq_ignore_ascii_case(CONTENT_TYPES_ITEM) {
                if content_types_entries.is_empty() {
                    content_types_entries = entries;
                }
                continue;
            }
            if let Some(&existing) = index.get(&key) {
                defects
                    .collisions
                    .push((parts[existing].name.clone(), logical));
                continue;
            }
            index.insert(key, parts.len());
            parts.push(Part {
                name: logical,
                entry: entries[0],
                pieces: entries,
            });
        }

        defects.content_types_missing = content_types_entries.is_empty();
        let shared = Arc::new(Shared {
            archive,
            parts,
            index,
            content_types_entries,
            content_types: std::sync::OnceLock::new(),
            base_defects: defects,
            defects: std::sync::OnceLock::new(),
        });
        Ok(Self::with_shared(shared))
    }

    fn with_shared(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            rels: RefCell::new(FastMap::default()),
            data: RefCell::new(FastMap::default()),
        }
    }

    /// A handle that can cross threads; see [`Package::from_seed`].
    pub fn seed(&self) -> PackageSeed {
        PackageSeed(Arc::clone(&self.shared))
    }

    /// A package sharing the archive and parsed directory of `seed`, with its own caches.
    pub fn from_seed(seed: PackageSeed) -> Self {
        Self::with_shared(seed.0)
    }

    pub fn archive(&self) -> &Archive {
        &self.shared.archive
    }

    /// Every part in archive order.
    pub fn parts(&self) -> &[Part] {
        &self.shared.parts
    }

    fn parsed_content_types(&self) -> &ParsedContentTypes {
        self.shared.content_types.get_or_init(|| {
            ParsedContentTypes::read(&self.shared.archive, &self.shared.content_types_entries)
        })
    }

    pub fn content_types(&self) -> &ContentTypes {
        &self.parsed_content_types().types
    }

    pub fn defects(&self) -> &PackageDefects {
        self.shared.defects.get_or_init(|| {
            let mut defects = self.shared.base_defects.clone();
            let parsed = self.parsed_content_types();
            defects.content_types_error = parsed.error.clone();
            defects.content_types_unreadable = parsed.unreadable.clone();
            for part in &self.shared.parts {
                if let Err(reason) = validate_part_name(&part.name) {
                    defects.invalid_names.push((part.name.clone(), reason));
                }
            }
            find_derivable(&self.shared.parts, &mut defects);
            defects
        })
    }

    /// The index of the part named `name`, compared ASCII case-insensitively.
    pub fn part_index(&self, name: &str) -> Option<usize> {
        self.shared.index.get(&equivalence_key(name)).copied()
    }

    pub fn has_part(&self, name: &str) -> bool {
        self.part_index(name).is_some()
    }

    /// The part as written, found case-insensitively.
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.part_index(name).map(|i| &self.shared.parts[i])
    }

    /// The archive entry backing `name`.
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.part(name)
            .and_then(|part| self.shared.archive.get(part.entry))
    }

    /// The declared content type of `name` (7.2.3.5).
    pub fn content_type_of(&self, name: &str) -> Option<&str> {
        self.content_types().content_type_of(name)
    }

    /// The decompressed bytes of a part, cached for the life of this package.
    pub fn read_part(&self, name: &str) -> Result<Rc<Vec<u8>>> {
        let index = self
            .part_index(name)
            .ok_or_else(|| Error::MissingPart(name.to_string()))?;
        if let Some(data) = self.data.borrow().get(&index) {
            return Ok(Rc::clone(data));
        }
        let part = &self.shared.parts[index];
        let mut bytes = Vec::new();
        self.read_part_entries(part, &mut bytes)?;
        let data = Rc::new(bytes);
        self.data.borrow_mut().insert(index, Rc::clone(&data));
        Ok(data)
    }

    /// The decompressed bytes of a part into `out`, bypassing the cache.
    /// Interleaved pieces are concatenated; a UTF-16 XML part comes back as UTF-8.
    pub fn read_part_into(&self, name: &str, out: &mut Vec<u8>) -> Result<()> {
        let index = self
            .part_index(name)
            .ok_or_else(|| Error::MissingPart(name.to_string()))?;
        self.read_part_entries(&self.shared.parts[index], out)
    }

    /// The decompressed bytes of a part exactly as stored (pieces
    /// concatenated), without the UTF-16 transcoding of [`Package::read_part`].
    pub fn read_part_bytes(&self, name: &str, out: &mut Vec<u8>) -> Result<()> {
        let index = self
            .part_index(name)
            .ok_or_else(|| Error::MissingPart(name.to_string()))?;
        self.read_stored(&self.shared.parts[index], out)
    }

    fn read_stored(&self, part: &Part, out: &mut Vec<u8>) -> Result<()> {
        match part.pieces.is_empty() {
            true => {
                let entry = &self.shared.archive.entries()[part.entry];
                self.shared.archive.read(entry, out)
            }
            false => {
                *out = read_entries(&self.shared.archive, &part.pieces)?;
                Ok(())
            }
        }
    }

    fn read_part_entries(&self, part: &Part, out: &mut Vec<u8>) -> Result<()> {
        self.read_stored(part, out)?;
        if let Some(utf8) = utf16_xml_to_utf8(out) {
            *out = utf8;
        }
        Ok(())
    }

    /// The relationships of `source` (`/` for the package). A missing
    /// Relationships part yields an empty set; a malformed one is an error.
    pub fn rels(&self, source: &str) -> Result<Rc<Relationships>> {
        let key = equivalence_key(source);
        if let Some(rels) = self.rels.borrow().get(&key) {
            return Ok(Rc::clone(rels));
        }
        let rels_name = rels_part_name(source);
        let rels = match self.part_index(&rels_name) {
            None => Relationships::empty(source),
            Some(_) => {
                let data = self.read_part(&rels_name)?;
                Relationships::parse(source, &data).map_err(|err| Error::Xml {
                    part: rels_name.clone(),
                    offset: err.offset,
                    msg: err.msg.to_string(),
                })?
            }
        };
        let rels = Rc::new(rels);
        self.rels.borrow_mut().insert(key, Rc::clone(&rels));
        Ok(rels)
    }

    pub fn package_rels(&self) -> Result<Rc<Relationships>> {
        self.rels("/")
    }

    /// The part name relationship `id` of `source` points at, when internal.
    pub fn resolve(&self, source: &str, id: &str) -> Result<Option<String>> {
        Ok(self.rels(source)?.target_of(id))
    }
}

/// The concatenated decompressed bytes of `entries`, in order.
fn read_entries(archive: &Archive, entries: &[usize]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut piece = Vec::new();
    for &entry in entries {
        archive.read(&archive.entries()[entry], &mut piece)?;
        out.extend_from_slice(&piece);
    }
    Ok(out)
}

/// `(prefix length, piece number, is last)` when `name` ends with a piece
/// suffix `/[n].piece` or `/[n].last.piece` (7.2.5.2), compared ASCII
/// case-insensitively; piece numbers carry no leading zeros.
fn piece_suffix(name: &str) -> Option<(usize, u64, bool)> {
    let lower = name.to_ascii_lowercase();
    let body = lower.strip_suffix(".piece")?;
    let (body, last) = match body.strip_suffix(".last") {
        Some(body) => (body, true),
        None => (body, false),
    };
    let body = body.strip_suffix(']')?;
    let open = body.rfind("/[")?;
    let digits = &body[open + 2..];
    let valid = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'));
    if !valid || open == 0 {
        return None;
    }
    Some((open, digits.parse().ok()?, last))
}

fn find_derivable(parts: &[Part], defects: &mut PackageDefects) {
    let mut keys: Vec<(String, usize)> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| (equivalence_key(&part.name), i))
        .collect();
    keys.sort();
    for window in keys.windows(2) {
        let (base, base_index) = &window[0];
        let (next, next_index) = &window[1];
        if next.len() > base.len() && next.starts_with(base) && next.as_bytes()[base.len()] == b'/'
        {
            defects.derivable.push((
                parts[*next_index].name.clone(),
                parts[*base_index].name.clone(),
            ));
        }
    }
}

fn classify_not_zip(head: &[u8]) -> Error {
    match head.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]) {
        true => Error::CompoundFile,
        false => Error::NotZip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pptxboss_testkit::ZipBuilder;

    const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/></Types>"#;
    const ROOT_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#;
    const PRES_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#;

    fn package() -> Package {
        let bytes = ZipBuilder::new()
            .deflated("[Content_Types].xml", TYPES.as_bytes())
            .deflated("_rels/.rels", ROOT_RELS.as_bytes())
            .deflated("ppt/presentation.xml", b"<p/>")
            .deflated("ppt/_rels/presentation.xml.rels", PRES_RELS.as_bytes())
            .deflated("ppt/slides/slide1.xml", b"<s/>")
            .stored("ppt/media/image1.png", b"PNG")
            .build();
        Package::from_bytes(bytes).unwrap()
    }

    #[test]
    fn parts_are_named_with_a_leading_slash_and_found_case_insensitively() {
        let package = package();
        let names: Vec<&str> = package
            .parts()
            .iter()
            .map(|part| part.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "/_rels/.rels",
                "/ppt/presentation.xml",
                "/ppt/_rels/presentation.xml.rels",
                "/ppt/slides/slide1.xml",
                "/ppt/media/image1.png"
            ]
        );
        assert!(package.has_part("/PPT/Slides/SLIDE1.xml"));
        assert!(!package.has_part("/[Content_Types].xml"));
        assert_eq!(package.content_type_of("/ppt/presentation.xml"), Some("application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"));
        assert_eq!(package.content_type_of("/ppt/media/image1.png"), None);
        assert!(package.defects().invalid_names.is_empty());
        assert!(!package.defects().content_types_missing);
    }

    #[test]
    fn parts_are_read_and_cached_and_relationships_resolve() {
        let package = package();
        let first = package.read_part("/ppt/slides/slide1.xml").unwrap();
        let second = package.read_part("/ppt/slides/slide1.xml").unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(first.as_slice(), b"<s/>");
        assert!(matches!(
            package.read_part("/nope.xml"),
            Err(Error::MissingPart(_))
        ));
        let root = package.package_rels().unwrap();
        assert_eq!(
            root.target_of("rId1").as_deref(),
            Some("/ppt/presentation.xml")
        );
        assert_eq!(
            package
                .resolve("/ppt/presentation.xml", "rId2")
                .unwrap()
                .as_deref(),
            Some("/ppt/slides/slide1.xml")
        );
        let none = package.rels("/ppt/slides/slide1.xml").unwrap();
        assert!(none.is_empty());
        assert_eq!(none.source, "/ppt/slides/slide1.xml");
        let mut out = Vec::new();
        package
            .read_part_into("/ppt/media/image1.png", &mut out)
            .unwrap();
        assert_eq!(out, b"PNG");
    }

    #[test]
    fn a_seed_makes_an_equivalent_package_with_fresh_caches() {
        let package = package();
        let seed = package.seed();
        let other = std::thread::spawn(move || {
            let package = Package::from_seed(seed);
            package.read_part("/ppt/presentation.xml").unwrap().to_vec()
        })
        .join()
        .unwrap();
        assert_eq!(other, b"<p/>");
    }

    #[test]
    fn container_defects_are_recorded_not_repaired() {
        let bytes = ZipBuilder::new()
            .deflated("[content_types].XML", TYPES.as_bytes())
            .stored("ppt/", b"")
            .stored("ppt/slides/slide1.xml", b"<a/>")
            .stored("PPT/SLIDES/slide1.xml", b"<b/>")
            .stored("ppt/slides", b"prefix")
            .stored("bad name.xml", b"")
            .build();
        let package = Package::from_bytes(bytes).unwrap();
        let defects = package.defects();
        assert_eq!(
            defects.content_types_case.as_deref(),
            Some("[content_types].XML")
        );
        assert_eq!(defects.directories, vec!["ppt/"]);
        assert_eq!(
            defects.collisions,
            vec![(
                "/ppt/slides/slide1.xml".to_string(),
                "/PPT/SLIDES/slide1.xml".to_string()
            )]
        );
        assert_eq!(
            defects.derivable,
            vec![(
                "/ppt/slides/slide1.xml".to_string(),
                "/ppt/slides".to_string()
            )]
        );
        assert_eq!(defects.invalid_names.len(), 1);
        assert_eq!(defects.invalid_names[0].0, "/bad name.xml");
        assert_eq!(
            package
                .read_part("/ppt/slides/slide1.xml")
                .unwrap()
                .as_slice(),
            b"<a/>"
        );
        assert_eq!(
            package.content_type_of("/ppt/slides/slide1.xml"),
            Some("application/xml")
        );
    }

    #[test]
    fn missing_or_broken_content_types_do_not_prevent_opening() {
        let missing =
            Package::from_bytes(ZipBuilder::new().stored("a.xml", b"<a/>").build()).unwrap();
        assert!(missing.defects().content_types_missing);
        assert_eq!(missing.content_type_of("/a.xml"), None);
        let broken = Package::from_bytes(
            ZipBuilder::new()
                .stored("[Content_Types].xml", b"<Types")
                .stored("a.xml", b"<a/>")
                .build(),
        )
        .unwrap();
        assert!(broken.defects().content_types_error.is_some());
    }

    #[test]
    fn compound_files_and_non_archives_are_classified() {
        let mut cfb = vec![0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
        cfb.resize(512, 0);
        assert!(matches!(Package::from_bytes(cfb), Err(Error::CompoundFile)));
        assert!(matches!(
            Package::from_bytes(b"hello".to_vec()),
            Err(Error::NotZip)
        ));
        let path = std::env::temp_dir().join(format!("pptxboss-cfb-{}.pptx", std::process::id()));
        let mut cfb = vec![0xd0, 0xcf, 0x11, 0xe0];
        cfb.resize(64, 0);
        std::fs::write(&path, &cfb).unwrap();
        assert!(matches!(Package::open(&path), Err(Error::CompoundFile)));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn malformed_rels_are_an_error_but_only_when_asked_for() {
        let bytes = ZipBuilder::new()
            .stored("[Content_Types].xml", TYPES.as_bytes())
            .stored("_rels/.rels", b"<Relationships")
            .stored("ppt/presentation.xml", b"<p/>")
            .build();
        let package = Package::from_bytes(bytes).unwrap();
        assert_eq!(
            package
                .read_part("/ppt/presentation.xml")
                .unwrap()
                .as_slice(),
            b"<p/>"
        );
        assert!(matches!(package.package_rels(), Err(Error::Xml { .. })));
    }
}
