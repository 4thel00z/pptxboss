//! A pull tokenizer for the XML subset Office Open XML parts use.
//!
//! Parts are UTF-8 (a byte order mark is skipped), namespace-aware, and
//! carry no DTD. The tokenizer borrows every name and text run from the
//! input, resolves namespace prefixes against a small table of the
//! namespaces the reader knows (Transitional and Strict URIs map to the
//! same [`Ns`]), skips comments and processing instructions, and hands
//! back CDATA as text. Character references, the five predefined
//! entities and the `_xHHHH_` escape convention are decoded by
//! [`unescape_into`], only when a text run contains them.
//!
//! Well-formedness is checked as far as the tokenizer must to make
//! progress: mismatched or unterminated tags are errors. Everything else
//! is lenient by default so a damaged slide still yields its text; the
//! verifier runs [`well_formed`] for the full check.

use std::sync::OnceLock;

use memchr::{memchr, memchr2, memchr3, memmem};

fn xmlns_finder() -> &'static memmem::Finder<'static> {
    static FINDER: OnceLock<memmem::Finder<'static>> = OnceLock::new();
    FINDER.get_or_init(|| memmem::Finder::new(b"xmlns"))
}

/// A namespace the reader recognizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ns {
    /// No namespace (unprefixed attribute, or unprefixed element without a default namespace).
    None,
    /// PresentationML main.
    Pml,
    /// DrawingML main.
    Dml,
    /// Office document relationship references (`r:id`, `r:embed`).
    Rel,
    /// OPC package relationships (`.rels` parts).
    PkgRel,
    /// OPC content types stream.
    ContentTypes,
    /// Markup compatibility and extensibility.
    Mce,
    /// OPC core properties.
    Cp,
    /// Dublin Core elements.
    Dc,
    /// Dublin Core terms.
    Dcterms,
    /// XML Schema instance.
    Xsi,
    /// Extended file properties (`docProps/app.xml`).
    Ep,
    /// Variant types used by extended and custom properties.
    Vt,
    /// DrawingML charts.
    Chart,
    /// DrawingML diagrams.
    Dgm,
    /// DrawingML pictures.
    Pic,
    /// The `xml:` namespace.
    Xml,
    /// Any other namespace, numbered per reader in order of first sight.
    Other(u16),
}

/// Which family of namespace URIs a document uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conformance {
    Transitional,
    Strict,
}

const KNOWN: &[(&[u8], Ns, Conformance)] = &[
    (
        b"http://schemas.openxmlformats.org/presentationml/2006/main",
        Ns::Pml,
        Conformance::Transitional,
    ),
    (
        b"http://purl.oclc.org/ooxml/presentationml/main",
        Ns::Pml,
        Conformance::Strict,
    ),
    (
        b"http://schemas.openxmlformats.org/drawingml/2006/main",
        Ns::Dml,
        Conformance::Transitional,
    ),
    (
        b"http://purl.oclc.org/ooxml/drawingml/main",
        Ns::Dml,
        Conformance::Strict,
    ),
    (
        b"http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        Ns::Rel,
        Conformance::Transitional,
    ),
    (
        b"http://purl.oclc.org/ooxml/officeDocument/relationships",
        Ns::Rel,
        Conformance::Strict,
    ),
    (
        b"http://schemas.openxmlformats.org/package/2006/relationships",
        Ns::PkgRel,
        Conformance::Transitional,
    ),
    (
        b"http://schemas.openxmlformats.org/package/2006/content-types",
        Ns::ContentTypes,
        Conformance::Transitional,
    ),
    (
        b"http://schemas.openxmlformats.org/markup-compatibility/2006",
        Ns::Mce,
        Conformance::Transitional,
    ),
    (
        b"http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
        Ns::Cp,
        Conformance::Transitional,
    ),
    (
        b"http://purl.org/dc/elements/1.1/",
        Ns::Dc,
        Conformance::Transitional,
    ),
    (
        b"http://purl.org/dc/terms/",
        Ns::Dcterms,
        Conformance::Transitional,
    ),
    (
        b"http://www.w3.org/2001/XMLSchema-instance",
        Ns::Xsi,
        Conformance::Transitional,
    ),
    (
        b"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties",
        Ns::Ep,
        Conformance::Transitional,
    ),
    (
        b"http://purl.oclc.org/ooxml/officeDocument/extendedProperties",
        Ns::Ep,
        Conformance::Strict,
    ),
    (
        b"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes",
        Ns::Vt,
        Conformance::Transitional,
    ),
    (
        b"http://purl.oclc.org/ooxml/officeDocument/docPropsVTypes",
        Ns::Vt,
        Conformance::Strict,
    ),
    (
        b"http://schemas.openxmlformats.org/drawingml/2006/chart",
        Ns::Chart,
        Conformance::Transitional,
    ),
    (
        b"http://purl.oclc.org/ooxml/drawingml/chart",
        Ns::Chart,
        Conformance::Strict,
    ),
    (
        b"http://schemas.openxmlformats.org/drawingml/2006/diagram",
        Ns::Dgm,
        Conformance::Transitional,
    ),
    (
        b"http://purl.oclc.org/ooxml/drawingml/diagram",
        Ns::Dgm,
        Conformance::Strict,
    ),
    (
        b"http://schemas.openxmlformats.org/drawingml/2006/picture",
        Ns::Pic,
        Conformance::Transitional,
    ),
    (
        b"http://purl.oclc.org/ooxml/drawingml/picture",
        Ns::Pic,
        Conformance::Strict,
    ),
    (
        b"http://www.w3.org/XML/1998/namespace",
        Ns::Xml,
        Conformance::Transitional,
    ),
];

/// The URI of a namespace known to the reader, Transitional form.
pub fn transitional_uri(ns: Ns) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|(_, known, conformance)| *known == ns && *conformance == Conformance::Transitional)
        .and_then(|(uri, _, _)| std::str::from_utf8(uri).ok())
}

/// A tokenizer error with the byte offset it was found at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlError {
    pub offset: usize,
    pub msg: &'static str,
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.msg, self.offset)
    }
}

impl std::error::Error for XmlError {}

type XmlResult<T> = std::result::Result<T, XmlError>;

/// A resolved element or attribute name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Name<'a> {
    pub ns: Ns,
    pub prefix: &'a [u8],
    pub local: &'a [u8],
}

impl<'a> Name<'a> {
    /// True for the element `ns:local`.
    #[inline]
    pub fn is(&self, ns: Ns, local: &[u8]) -> bool {
        self.ns == ns && self.local == local
    }

    /// The name as written, `prefix:local` or `local`.
    pub fn qualified(&self) -> String {
        match self.prefix.is_empty() {
            true => String::from_utf8_lossy(self.local).into_owned(),
            false => format!(
                "{}:{}",
                String::from_utf8_lossy(self.prefix),
                String::from_utf8_lossy(self.local)
            ),
        }
    }
}

/// A start tag.
#[derive(Clone, Copy, Debug)]
pub struct Start<'a> {
    pub name: Name<'a>,
    /// Bytes between the element name and the closing `>` or `/>`.
    pub raw_attrs: &'a [u8],
    pub self_closing: bool,
    /// Byte offset of the `<`.
    pub offset: usize,
}

/// One attribute with its raw (still escaped) value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attr<'a> {
    pub name: Name<'a>,
    pub raw_value: &'a [u8],
}

/// One token of the document.
#[derive(Clone, Copy, Debug)]
pub enum Event<'a> {
    Start(Start<'a>),
    /// An end tag, or the synthesized end of a self-closing element.
    End(Name<'a>),
    /// Character data, still escaped unless it came from a CDATA section.
    Text {
        raw: &'a [u8],
        cdata: bool,
    },
    Eof,
}

#[derive(Clone, Copy)]
struct Scope<'a> {
    depth: usize,
    prefix: &'a [u8],
    ns: Ns,
}

/// An element on the open stack: its resolved name and the name bytes as written.
#[derive(Clone, Copy)]
struct Open<'a> {
    name: Name<'a>,
    qname: &'a [u8],
}

/// The pull tokenizer.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    open: Vec<Open<'a>>,
    scopes: Vec<Scope<'a>>,
    /// The innermost binding for each prefix first byte; the common case
    /// resolves with one table lookup and one short compare.
    first_byte: Box<[Option<(&'a [u8], Ns)>; 256]>,
    default_ns: Ns,
    pending_end: Option<Name<'a>>,
    other: Vec<&'a [u8]>,
    saw_transitional: bool,
    saw_strict: bool,
    strict: bool,
    root_closed: bool,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let pos = match data.starts_with(&[0xef, 0xbb, 0xbf]) {
            true => 3,
            false => 0,
        };
        Self {
            data,
            pos,
            open: Vec::with_capacity(16),
            scopes: Vec::with_capacity(8),
            first_byte: Box::new([None; 256]),
            default_ns: Ns::None,
            pending_end: None,
            other: Vec::new(),
            saw_transitional: false,
            saw_strict: false,
            strict: false,
            root_closed: false,
        }
    }

    /// Enables the full well-formedness checks the verifier needs.
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Number of currently open elements.
    #[inline]
    pub fn depth(&self) -> usize {
        self.open.len()
    }

    /// Current byte offset.
    #[inline]
    pub fn offset(&self) -> usize {
        self.pos
    }

    /// True once a Transitional namespace URI has been declared.
    pub fn saw_transitional(&self) -> bool {
        self.saw_transitional
    }

    /// True once a Strict namespace URI has been declared.
    pub fn saw_strict(&self) -> bool {
        self.saw_strict
    }

    /// The URI behind an [`Ns::Other`] index.
    pub fn other_uri(&self, index: u16) -> Option<&'a [u8]> {
        self.other.get(usize::from(index)).copied()
    }

    /// Resolves a prefix against the namespaces in scope. `xml` is always bound.
    #[inline]
    pub fn resolve(&self, prefix: &[u8]) -> Ns {
        let Some(&first) = prefix.first() else {
            return self.default_ns;
        };
        if let Some((bound, ns)) = self.first_byte[usize::from(first)] {
            if bound == prefix {
                return ns;
            }
        }
        self.resolve_slow(prefix)
    }

    fn resolve_slow(&self, prefix: &[u8]) -> Ns {
        if let Some(scope) = self
            .scopes
            .iter()
            .rev()
            .find(|scope| scope.prefix == prefix)
        {
            return scope.ns;
        }
        match prefix {
            b"xml" => Ns::Xml,
            _ => Ns::None,
        }
    }

    fn bind(&mut self, prefix: &'a [u8], ns: Ns) {
        match prefix.first() {
            None => self.default_ns = ns,
            Some(&first) => self.first_byte[usize::from(first)] = Some((prefix, ns)),
        }
    }

    /// Recomputes the fast binding for `prefix` after its scope was popped.
    fn rebind(&mut self, prefix: &[u8]) {
        let remaining = self
            .scopes
            .iter()
            .rev()
            .find(|scope| match prefix.first() {
                None => scope.prefix.is_empty(),
                Some(&first) => scope.prefix.first() == Some(&first),
            })
            .map(|scope| (scope.prefix, scope.ns));
        match prefix.first() {
            None => self.default_ns = remaining.map_or(Ns::None, |(_, ns)| ns),
            Some(&first) => self.first_byte[usize::from(first)] = remaining,
        }
    }

    /// The next token. This is a pull parser, not an iterator: the end of
    /// input is an event, and errors end the stream.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> XmlResult<Event<'a>> {
        if let Some(name) = self.pending_end.take() {
            self.finish_open();
            return Ok(Event::End(name));
        }
        loop {
            if self.pos >= self.data.len() {
                if !self.open.is_empty() {
                    return Err(XmlError {
                        offset: self.pos,
                        msg: "unexpected end of document inside an element",
                    });
                }
                return Ok(Event::Eof);
            }
            if self.data[self.pos] != b'<' {
                if self.open.is_empty() {
                    self.skip_outer_whitespace()?;
                    continue;
                }
                return self.text();
            }
            let tag_start = self.pos;
            let Some(&kind) = self.data.get(tag_start + 1) else {
                return Err(XmlError {
                    offset: tag_start,
                    msg: "unterminated tag",
                });
            };
            match kind {
                b'/' => return self.end_tag(tag_start),
                b'?' => {
                    self.skip_past(tag_start + 2, b"?>", "unterminated processing instruction")?
                }
                b'!' => {
                    if self.data[tag_start + 1..].starts_with(b"!--") {
                        self.skip_past(tag_start + 4, b"-->", "unterminated comment")?;
                        continue;
                    }
                    if self.data[tag_start + 1..].starts_with(b"![CDATA[") {
                        let body_start = tag_start + 9;
                        let end =
                            memmem::find(&self.data[body_start..], b"]]>").ok_or(XmlError {
                                offset: tag_start,
                                msg: "unterminated CDATA section",
                            })?;
                        self.pos = body_start + end + 3;
                        return Ok(Event::Text {
                            raw: &self.data[body_start..body_start + end],
                            cdata: true,
                        });
                    }
                    if self.data[tag_start + 1..].starts_with(b"!DOCTYPE") {
                        return Err(XmlError {
                            offset: tag_start,
                            msg: "DTD declarations are not allowed in package parts",
                        });
                    }
                    return Err(XmlError {
                        offset: tag_start,
                        msg: "unrecognized markup declaration",
                    });
                }
                _ => return self.start_tag(tag_start),
            }
        }
    }

    /// Consumes everything up to and including the end tag matching the
    /// most recent start tag, without resolving names or namespaces.
    pub fn skip_element(&mut self) -> XmlResult<()> {
        let target = self.open.len().saturating_sub(1);
        if self.pending_end.take().is_some() {
            if self.open.is_empty() {
                return Err(XmlError {
                    offset: self.pos,
                    msg: "nothing to skip",
                });
            }
            self.finish_open();
            return Ok(());
        }
        loop {
            let pos = self.pos;
            let Some(rel) = memchr(b'<', &self.data[pos..]) else {
                return Err(XmlError {
                    offset: pos,
                    msg: "unexpected end of document inside an element",
                });
            };
            let tag_start = pos + rel;
            let kind = *self.data.get(tag_start + 1).ok_or(XmlError {
                offset: tag_start,
                msg: "unterminated tag",
            })?;
            match kind {
                b'/' => {
                    let fast = self.open.last().and_then(|open| {
                        let end = tag_start + 2 + open.qname.len();
                        let hit = self.data.get(tag_start + 2..end) == Some(open.qname)
                            && self.data.get(end) == Some(&b'>');
                        hit.then_some(end + 1)
                    });
                    match fast {
                        Some(after) => {
                            self.pos = after;
                            self.finish_open();
                        }
                        None => {
                            let close = memchr(b'>', &self.data[tag_start..]).ok_or(XmlError {
                                offset: tag_start,
                                msg: "unterminated end tag",
                            })?;
                            self.pos = tag_start + close + 1;
                            let qname = trim_ascii(&self.data[tag_start + 2..tag_start + close]);
                            self.close(qname, tag_start)?;
                        }
                    }
                    if self.open.len() == target {
                        return Ok(());
                    }
                }
                b'?' => {
                    self.skip_past(tag_start + 2, b"?>", "unterminated processing instruction")?
                }
                b'!' => {
                    if self.data[tag_start + 1..].starts_with(b"!--") {
                        self.skip_past(tag_start + 4, b"-->", "unterminated comment")?;
                    } else if self.data[tag_start + 1..].starts_with(b"![CDATA[") {
                        self.skip_past(tag_start + 9, b"]]>", "unterminated CDATA section")?;
                    } else {
                        return Err(XmlError {
                            offset: tag_start,
                            msg: "unrecognized markup declaration",
                        });
                    }
                }
                _ => {
                    let (name_end, _, self_closing, _) = self.scan_start(tag_start)?;
                    if !self_closing {
                        let qname = &self.data[tag_start + 1..name_end];
                        let name = Name {
                            ns: Ns::None,
                            prefix: &[],
                            local: qname,
                        };
                        self.open.push(Open { name, qname });
                    }
                }
            }
        }
    }

    /// Reads the text content of the current element up to its end tag,
    /// appending decoded characters to `out`; nested elements contribute
    /// their text too.
    pub fn text_content(&mut self, out: &mut String) -> XmlResult<()> {
        let target = self.open.len().saturating_sub(1);
        loop {
            match self.next()? {
                Event::Text { raw, cdata } => match cdata {
                    true => out.push_str(&String::from_utf8_lossy(raw)),
                    false => unescape_into(raw, out),
                },
                Event::End(_) if self.open.len() == target => return Ok(()),
                Event::Eof => return Ok(()),
                _ => {}
            }
        }
    }

    /// The raw value of attribute `ns:local` on `start`, if present.
    pub fn attr(&self, start: &Start<'a>, ns: Ns, local: &[u8]) -> Option<&'a [u8]> {
        self.attrs(start)
            .find(|attr| attr.name.ns == ns && attr.name.local == local)
            .map(|attr| attr.raw_value)
    }

    /// The attributes of `start`, resolved against the namespaces in scope.
    pub fn attrs(&self, start: &Start<'a>) -> Attrs<'_, 'a> {
        Attrs {
            reader: self,
            raw: start.raw_attrs,
            pos: 0,
        }
    }

    fn text(&mut self) -> XmlResult<Event<'a>> {
        let start = self.pos;
        let end = memchr(b'<', &self.data[start..])
            .map(|rel| start + rel)
            .unwrap_or(self.data.len());
        self.pos = end;
        let raw = &self.data[start..end];
        if self.strict {
            if let Some(offset) = invalid_char_offset(raw) {
                return Err(XmlError {
                    offset: start + offset,
                    msg: "character not allowed in XML",
                });
            }
            if let Some(rel) = memmem::find(raw, b"]]>") {
                return Err(XmlError {
                    offset: start + rel,
                    msg: "']]>' is not allowed in character data",
                });
            }
        }
        Ok(Event::Text { raw, cdata: false })
    }

    fn skip_outer_whitespace(&mut self) -> XmlResult<()> {
        let start = self.pos;
        let end = memchr(b'<', &self.data[start..])
            .map(|rel| start + rel)
            .unwrap_or(self.data.len());
        if !self.data[start..end]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            return Err(XmlError {
                offset: start,
                msg: "text outside the root element",
            });
        }
        self.pos = end;
        Ok(())
    }

    fn skip_past(&mut self, from: usize, needle: &[u8], msg: &'static str) -> XmlResult<()> {
        let rel =
            memmem::find(&self.data[from.min(self.data.len())..], needle).ok_or(XmlError {
                offset: self.pos,
                msg,
            })?;
        self.pos = from + rel + needle.len();
        Ok(())
    }

    /// Scans a start tag beginning at `tag_start`; returns the end of the
    /// name, the end of the attribute region and whether the tag is
    /// self-closing, and leaves `pos` after the tag.
    fn scan_start(&mut self, tag_start: usize) -> XmlResult<(usize, usize, bool, Option<usize>)> {
        let data = self.data;
        let name_start = tag_start + 1;
        let mut i = name_start;
        let mut colon = None;
        while i < data.len() {
            let byte = data[i];
            if NAME_END[usize::from(byte)] {
                break;
            }
            if byte == b':' && colon.is_none() {
                colon = Some(i);
            }
            i += 1;
        }
        if i == name_start {
            return Err(XmlError {
                offset: tag_start,
                msg: "element name expected",
            });
        }
        let name_end = i;
        if !self.strict {
            if let Some(rel) = memchr(b'>', &data[name_end..]) {
                let gt = name_end + rel;
                let between = &data[name_end..gt];
                let quotes = between.iter().filter(|&&b| b == b'"' || b == b'\'').count();
                if quotes % 2 == 0 {
                    self.pos = gt + 1;
                    let self_closing = gt > name_end && data[gt - 1] == b'/';
                    let attrs_end = match self_closing {
                        true => gt - 1,
                        false => gt,
                    };
                    return Ok((name_end, attrs_end, self_closing, colon));
                }
            }
        }
        loop {
            while i < data.len() && matches!(data[i], b' ' | b'\t' | b'\r' | b'\n') {
                i += 1;
            }
            match data.get(i) {
                None => {
                    return Err(XmlError {
                        offset: tag_start,
                        msg: "unterminated start tag",
                    })
                }
                Some(b'>') => {
                    self.pos = i + 1;
                    return Ok((name_end, i, false, colon));
                }
                Some(b'/') => {
                    if data.get(i + 1) != Some(&b'>') {
                        return Err(XmlError {
                            offset: i,
                            msg: "'/' must be followed by '>'",
                        });
                    }
                    self.pos = i + 2;
                    return Ok((name_end, i, true, colon));
                }
                Some(_) => {
                    let eq = memchr3(b'=', b'>', b'/', &data[i..])
                        .map(|rel| i + rel)
                        .ok_or(XmlError {
                            offset: i,
                            msg: "unterminated start tag",
                        })?;
                    if data[eq] != b'=' {
                        if self.strict {
                            return Err(XmlError {
                                offset: i,
                                msg: "attribute without a value",
                            });
                        }
                        i = eq;
                        continue;
                    }
                    let mut v = eq + 1;
                    while v < data.len() && matches!(data[v], b' ' | b'\t' | b'\r' | b'\n') {
                        v += 1;
                    }
                    let quote = *data.get(v).ok_or(XmlError {
                        offset: v,
                        msg: "unterminated start tag",
                    })?;
                    if quote != b'"' && quote != b'\'' {
                        return Err(XmlError {
                            offset: v,
                            msg: "attribute value must be quoted",
                        });
                    }
                    let close =
                        memchr(quote, &data[v + 1..])
                            .map(|rel| v + 1 + rel)
                            .ok_or(XmlError {
                                offset: v,
                                msg: "unterminated attribute value",
                            })?;
                    if self.strict && memchr(b'<', &data[v + 1..close]).is_some() {
                        return Err(XmlError {
                            offset: v,
                            msg: "'<' is not allowed in an attribute value",
                        });
                    }
                    i = close + 1;
                }
            }
        }
    }

    fn start_tag(&mut self, tag_start: usize) -> XmlResult<Event<'a>> {
        let (name_end, attrs_end, self_closing, colon) = self.scan_start(tag_start)?;
        let raw_attrs = &self.data[name_end..attrs_end];
        let qname = &self.data[tag_start + 1..name_end];
        let (prefix, local): (&'a [u8], &'a [u8]) = match colon {
            Some(colon) => (
                &self.data[tag_start + 1..colon],
                &self.data[colon + 1..name_end],
            ),
            None => (&[], qname),
        };
        if self.strict {
            let valid = is_ncname(local) && (prefix.is_empty() || is_ncname(prefix));
            if !valid {
                return Err(XmlError {
                    offset: tag_start,
                    msg: "invalid element name",
                });
            }
            if self.open.is_empty() && self.root_closed {
                return Err(XmlError {
                    offset: tag_start,
                    msg: "more than one root element",
                });
            }
        }
        let depth = self.open.len() + 1;
        if raw_attrs.len() >= 6 && xmlns_finder().find(raw_attrs).is_some() {
            self.declare_namespaces(raw_attrs, depth, tag_start)?;
        }
        let ns = self.resolve(prefix);
        if self.strict && ns == Ns::None && !prefix.is_empty() {
            return Err(XmlError {
                offset: tag_start,
                msg: "undeclared namespace prefix",
            });
        }
        let name = Name { ns, prefix, local };
        self.open.push(Open { name, qname });
        if self_closing {
            self.pending_end = Some(name);
        }
        Ok(Event::Start(Start {
            name,
            raw_attrs,
            self_closing,
            offset: tag_start,
        }))
    }

    fn declare_namespaces(
        &mut self,
        raw_attrs: &'a [u8],
        depth: usize,
        tag_start: usize,
    ) -> XmlResult<()> {
        let mut seen_default = false;
        for attr in (RawAttrs {
            raw: raw_attrs,
            pos: 0,
        }) {
            let (prefix, local) = split_qname(attr.0);
            let is_default = prefix.is_empty() && local == b"xmlns";
            let is_prefixed = prefix == b"xmlns";
            if !is_default && !is_prefixed {
                continue;
            }
            let declared_prefix: &'a [u8] = match is_default {
                true => b"",
                false => local,
            };
            if self.strict {
                if is_default && seen_default {
                    return Err(XmlError {
                        offset: tag_start,
                        msg: "duplicate default namespace declaration",
                    });
                }
                if self
                    .scopes
                    .iter()
                    .any(|scope| scope.depth == depth && scope.prefix == declared_prefix)
                {
                    return Err(XmlError {
                        offset: tag_start,
                        msg: "duplicate namespace declaration",
                    });
                }
            }
            seen_default |= is_default;
            let ns = self.intern(attr.1);
            self.scopes.push(Scope {
                depth,
                prefix: declared_prefix,
                ns,
            });
            self.bind(declared_prefix, ns);
        }
        Ok(())
    }

    fn intern(&mut self, uri: &'a [u8]) -> Ns {
        if uri.is_empty() {
            return Ns::None;
        }
        if let Some((_, ns, conformance)) = KNOWN.iter().find(|(known, _, _)| *known == uri) {
            let class_specific = matches!(
                ns,
                Ns::Pml | Ns::Dml | Ns::Rel | Ns::Ep | Ns::Vt | Ns::Chart | Ns::Dgm | Ns::Pic
            );
            match conformance {
                Conformance::Transitional if class_specific => self.saw_transitional = true,
                Conformance::Strict => self.saw_strict = true,
                _ => {}
            }
            return *ns;
        }
        if let Some(index) = self.other.iter().position(|known| *known == uri) {
            return Ns::Other(index as u16);
        }
        self.other.push(uri);
        Ns::Other((self.other.len() - 1) as u16)
    }

    fn end_tag(&mut self, tag_start: usize) -> XmlResult<Event<'a>> {
        if let Some(open) = self.open.last().copied() {
            let end = tag_start + 2 + open.qname.len();
            let matches_open = self.data.get(tag_start + 2..end) == Some(open.qname)
                && self.data.get(end) == Some(&b'>');
            if matches_open {
                self.pos = end + 1;
                self.finish_open();
                return Ok(Event::End(open.name));
            }
        }
        let close = memchr(b'>', &self.data[tag_start..])
            .map(|rel| tag_start + rel)
            .ok_or(XmlError {
                offset: tag_start,
                msg: "unterminated end tag",
            })?;
        let qname = trim_ascii(&self.data[tag_start + 2..close]);
        self.pos = close + 1;
        let name = self.close(qname, tag_start)?;
        Ok(Event::End(name))
    }

    /// Pops the open element, which must be named `qname` as written.
    fn close(&mut self, qname: &[u8], offset: usize) -> XmlResult<Name<'a>> {
        let Some(open) = self.open.last().copied() else {
            return Err(XmlError {
                offset,
                msg: "end tag without a start tag",
            });
        };
        if open.qname != qname {
            return Err(XmlError {
                offset,
                msg: "end tag does not match the open element",
            });
        }
        self.finish_open();
        Ok(open.name)
    }

    /// Pops the open element and the namespaces it declared.
    #[inline]
    fn finish_open(&mut self) {
        self.open.pop();
        self.pop_scopes();
        self.root_closed |= self.open.is_empty();
    }

    #[inline]
    fn pop_scopes(&mut self) {
        let depth = self.open.len() + 1;
        while self.scopes.last().is_some_and(|scope| scope.depth == depth) {
            let Some(scope) = self.scopes.pop() else {
                return;
            };
            self.rebind(scope.prefix);
        }
    }
}

/// Iterator over the attributes of a start tag, resolved against the
/// reader's namespaces in scope.
pub struct Attrs<'r, 'a> {
    reader: &'r Reader<'a>,
    raw: &'a [u8],
    pos: usize,
}

impl<'r, 'a> Iterator for Attrs<'r, 'a> {
    type Item = Attr<'a>;

    fn next(&mut self) -> Option<Attr<'a>> {
        let (qname, raw_value, next) = next_raw_attr(self.raw, self.pos)?;
        self.pos = next;
        let (prefix, local) = split_qname(qname);
        let ns = match prefix.is_empty() {
            true => Ns::None,
            false => self.reader.resolve(prefix),
        };
        Some(Attr {
            name: Name { ns, prefix, local },
            raw_value,
        })
    }
}

struct RawAttrs<'a> {
    raw: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for RawAttrs<'a> {
    type Item = (&'a [u8], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let (qname, value, next) = next_raw_attr(self.raw, self.pos)?;
        self.pos = next;
        Some((qname, value))
    }
}

/// Scans one `name="value"` pair starting at `pos`; returns (name, raw value, position after).
fn next_raw_attr(raw: &[u8], mut pos: usize) -> Option<(&[u8], &[u8], usize)> {
    while pos < raw.len() && matches!(raw[pos], b' ' | b'\t' | b'\r' | b'\n') {
        pos += 1;
    }
    if pos >= raw.len() {
        return None;
    }
    let eq = memchr(b'=', &raw[pos..]).map(|rel| pos + rel)?;
    let name = trim_ascii(&raw[pos..eq]);
    let mut v = eq + 1;
    while v < raw.len() && matches!(raw[v], b' ' | b'\t' | b'\r' | b'\n') {
        v += 1;
    }
    let quote = *raw.get(v)?;
    let close = memchr(quote, &raw[v + 1..]).map(|rel| v + 1 + rel)?;
    Some((name, &raw[v + 1..close], close + 1))
}

/// Bytes that end an element name inside a start tag.
static NAME_END: [bool; 256] = {
    let mut table = [false; 256];
    table[b' ' as usize] = true;
    table[b'\t' as usize] = true;
    table[b'\r' as usize] = true;
    table[b'\n' as usize] = true;
    table[b'/' as usize] = true;
    table[b'>' as usize] = true;
    table
};

#[inline]
fn split_qname(qname: &[u8]) -> (&[u8], &[u8]) {
    match memchr(b':', qname) {
        Some(colon) => (&qname[..colon], &qname[colon + 1..]),
        None => (&[], qname),
    }
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    &bytes[start..end.max(start)]
}

/// Whether `name` is an XML NCName (ASCII rules; non-ASCII bytes are accepted).
pub fn is_ncname(name: &[u8]) -> bool {
    let Some(&first) = name.first() else {
        return false;
    };
    let start_ok = first.is_ascii_alphabetic() || first == b'_' || first >= 0x80;
    start_ok
        && name.iter().all(|&byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.') || byte >= 0x80
        })
}

/// Offset of the first byte that XML 1.0 forbids in character data
/// (controls other than tab, newline and carriage return), if any.
pub fn invalid_char_offset(raw: &[u8]) -> Option<usize> {
    raw.iter()
        .position(|&byte| byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r'))
}

/// Decodes character references, the five predefined entities and
/// `_xHHHH_` escapes from `raw`, appending to `out`. Unknown entities and
/// malformed references are kept literally; invalid UTF-8 is replaced.
pub fn unescape_into(raw: &[u8], out: &mut String) {
    let needs_work = memchr2(b'&', b'_', raw).is_some();
    if !needs_work {
        push_lossy(raw, out);
        return;
    }
    let mut rest = raw;
    while let Some(rel) = memchr2(b'&', b'_', rest) {
        push_lossy(&rest[..rel], out);
        rest = &rest[rel..];
        match rest[0] {
            b'&' => {
                let Some(semi) = memchr(b';', &rest[..rest.len().min(12)]) else {
                    out.push('&');
                    rest = &rest[1..];
                    continue;
                };
                let entity = &rest[1..semi];
                match decode_entity(entity) {
                    Some(ch) => {
                        out.push(ch);
                        rest = &rest[semi + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &rest[1..];
                    }
                }
            }
            _ => match decode_x_escape(rest) {
                Some(ch) => {
                    out.push(ch);
                    rest = &rest[7..];
                }
                None => {
                    out.push('_');
                    rest = &rest[1..];
                }
            },
        }
    }
    push_lossy(rest, out);
}

/// Decodes only XML entities and character references, leaving `_xHHHH_`
/// sequences untouched (for attribute values such as part names).
pub fn unescape_attr(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    if memchr(b'&', raw).is_none() {
        push_lossy(raw, &mut out);
        return out;
    }
    let mut rest = raw;
    while let Some(rel) = memchr(b'&', rest) {
        push_lossy(&rest[..rel], &mut out);
        rest = &rest[rel..];
        let decoded = memchr(b';', &rest[..rest.len().min(12)])
            .and_then(|semi| decode_entity(&rest[1..semi]).map(|ch| (ch, semi)));
        match decoded {
            Some((ch, semi)) => {
                out.push(ch);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    push_lossy(rest, &mut out);
    out
}

fn decode_entity(entity: &[u8]) -> Option<char> {
    match entity {
        b"lt" => Some('<'),
        b"gt" => Some('>'),
        b"amp" => Some('&'),
        b"quot" => Some('"'),
        b"apos" => Some('\''),
        _ => {
            let digits = entity.strip_prefix(b"#")?;
            let code = match digits.strip_prefix(b"x") {
                Some(hex) => u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?,
                None => std::str::from_utf8(digits).ok()?.parse::<u32>().ok()?,
            };
            char::from_u32(code)
        }
    }
}

fn decode_x_escape(rest: &[u8]) -> Option<char> {
    if rest.len() < 7 || rest[1] != b'x' || rest[6] != b'_' {
        return None;
    }
    let hex = std::str::from_utf8(&rest[2..6]).ok()?;
    let code = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(code)
}

#[inline]
fn push_lossy(bytes: &[u8], out: &mut String) {
    match std::str::from_utf8(bytes) {
        Ok(text) => out.push_str(text),
        Err(_) => out.push_str(&String::from_utf8_lossy(bytes)),
    }
}

/// Runs the strict tokenizer over a whole part, returning the first error.
pub fn well_formed(data: &[u8]) -> XmlResult<()> {
    let mut reader = Reader::new(data).strict();
    let mut saw_root = false;
    loop {
        match reader.next()? {
            Event::Start(_) => saw_root = true,
            Event::Eof => break,
            _ => {}
        }
    }
    match saw_root {
        true => Ok(()),
        false => Err(XmlError {
            offset: data.len(),
            msg: "no root element",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLIDE: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="Title 1"/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="en-US"><a:hlinkClick r:id="rId2"/></a:rPr><a:t>Hello &amp; welcome</a:t></a:r><a:br/><a:r><a:t><![CDATA[a < b]]></a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;

    fn collect(data: &[u8]) -> Vec<String> {
        let mut reader = Reader::new(data);
        let mut out = Vec::new();
        loop {
            match reader.next().unwrap() {
                Event::Start(start) => out.push(format!(
                    "<{:?}:{}{}",
                    start.name.ns,
                    String::from_utf8_lossy(start.name.local),
                    if start.self_closing { "/" } else { "" }
                )),
                Event::End(name) => out.push(format!("</{}", String::from_utf8_lossy(name.local))),
                Event::Text { raw, cdata } => {
                    let mut text = String::new();
                    match cdata {
                        true => text.push_str(std::str::from_utf8(raw).unwrap()),
                        false => unescape_into(raw, &mut text),
                    }
                    out.push(format!("'{text}'"));
                }
                Event::Eof => return out,
            }
        }
    }

    #[test]
    fn resolves_prefixes_and_emits_ends_for_self_closing_tags() {
        let events = collect(SLIDE);
        assert_eq!(events[0], "<Pml:sld");
        assert!(events.contains(&"<Pml:cNvPr/".to_string()));
        assert!(events.contains(&"<Dml:bodyPr/".to_string()));
        assert!(events.contains(&"'Hello & welcome'".to_string()));
        assert!(events.contains(&"'a < b'".to_string()));
        let ends = events
            .iter()
            .filter(|event| event.starts_with("</"))
            .count();
        let starts = events
            .iter()
            .filter(|event| event.starts_with('<') && !event.starts_with("</"))
            .count();
        assert_eq!(starts, ends);
        assert_eq!(events.last().unwrap(), "</sld");
    }

    #[test]
    fn attributes_resolve_namespaces_and_default_namespace_does_not_apply_to_them() {
        let mut reader = Reader::new(SLIDE);
        loop {
            if let Event::Start(start) = reader.next().unwrap() {
                if start.name.is(Ns::Dml, b"hlinkClick") {
                    assert_eq!(reader.attr(&start, Ns::Rel, b"id"), Some(&b"rId2"[..]));
                    assert_eq!(reader.attr(&start, Ns::None, b"id"), None);
                    break;
                }
            }
        }
        let data = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#;
        let mut reader = Reader::new(data);
        let Event::Start(types) = reader.next().unwrap() else {
            panic!()
        };
        assert!(types.name.is(Ns::ContentTypes, b"Types"));
        let Event::Start(default) = reader.next().unwrap() else {
            panic!()
        };
        assert!(default.name.is(Ns::ContentTypes, b"Default"));
        let attrs: Vec<_> = reader.attrs(&default).collect();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].name.ns, Ns::None);
        assert_eq!(attrs[0].name.local, b"Extension");
        assert_eq!(attrs[1].raw_value, b"application/xml");
    }

    #[test]
    fn strict_namespace_uris_map_to_the_same_ids() {
        let data = br#"<p:sld xmlns:p="http://purl.oclc.org/ooxml/presentationml/main" xmlns:a="http://purl.oclc.org/ooxml/drawingml/main"><a:t>x</a:t></p:sld>"#;
        let mut reader = Reader::new(data);
        let Event::Start(sld) = reader.next().unwrap() else {
            panic!()
        };
        assert_eq!(sld.name.ns, Ns::Pml);
        let Event::Start(t) = reader.next().unwrap() else {
            panic!()
        };
        assert_eq!(t.name.ns, Ns::Dml);
        assert!(reader.saw_strict());
        assert!(!reader.saw_transitional());
    }

    #[test]
    fn unknown_namespaces_are_numbered_and_scoped() {
        let data =
            br#"<r xmlns="urn:a"><x xmlns="urn:b"><y/></x><z xmlns:q="urn:b"><q:w/></z></r>"#;
        let mut reader = Reader::new(data);
        let mut seen = Vec::new();
        loop {
            match reader.next().unwrap() {
                Event::Start(start) => seen.push((
                    String::from_utf8_lossy(start.name.local).into_owned(),
                    start.name.ns,
                )),
                Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!(seen[0], ("r".into(), Ns::Other(0)));
        assert_eq!(seen[1], ("x".into(), Ns::Other(1)));
        assert_eq!(seen[2], ("y".into(), Ns::Other(1)));
        assert_eq!(seen[3], ("z".into(), Ns::Other(0)));
        assert_eq!(seen[4], ("w".into(), Ns::Other(1)));
        assert_eq!(reader.other_uri(1), Some(&b"urn:b"[..]));
    }

    #[test]
    fn skip_element_consumes_the_subtree_and_keeps_depth_consistent() {
        let data = br#"<a><b x="1>2"><c/><!-- </b> --><d><![CDATA[</b>]]></d></b><e>after</e></a>"#;
        let mut reader = Reader::new(data);
        assert!(matches!(reader.next().unwrap(), Event::Start(_)));
        let Event::Start(b) = reader.next().unwrap() else {
            panic!()
        };
        assert_eq!(b.name.local, b"b");
        reader.skip_element().unwrap();
        assert_eq!(reader.depth(), 1);
        let Event::Start(e) = reader.next().unwrap() else {
            panic!()
        };
        assert_eq!(e.name.local, b"e");
        let mut text = String::new();
        reader.text_content(&mut text).unwrap();
        assert_eq!(text, "after");
        assert!(matches!(reader.next().unwrap(), Event::End(_)));
        assert!(matches!(reader.next().unwrap(), Event::Eof));
    }

    #[test]
    fn skipping_a_self_closing_element_works() {
        let data = b"<a><b/><c>x</c></a>";
        let mut reader = Reader::new(data);
        reader.next().unwrap();
        reader.next().unwrap();
        reader.skip_element().unwrap();
        let Event::Start(c) = reader.next().unwrap() else {
            panic!()
        };
        assert_eq!(c.name.local, b"c");
    }

    #[test]
    fn mismatched_and_unterminated_tags_are_errors() {
        assert!(Reader::new(b"<a></b>").next().is_ok());
        let mut reader = Reader::new(b"<a></b>");
        reader.next().unwrap();
        assert_eq!(
            reader.next().unwrap_err().msg,
            "end tag does not match the open element"
        );
        let mut reader = Reader::new(b"<a><b>");
        reader.next().unwrap();
        reader.next().unwrap();
        assert_eq!(
            reader.next().unwrap_err().msg,
            "unexpected end of document inside an element"
        );
        let mut reader = Reader::new(b"<a x='1'");
        assert_eq!(reader.next().unwrap_err().msg, "unterminated start tag");
        assert_eq!(
            Reader::new(b"<!DOCTYPE x><x/>").next().unwrap_err().msg,
            "DTD declarations are not allowed in package parts"
        );
    }

    #[test]
    fn a_byte_order_mark_and_declaration_are_skipped() {
        let mut data = vec![0xef, 0xbb, 0xbf];
        data.extend_from_slice(b"<?xml version=\"1.0\"?>\n<x/>");
        let mut reader = Reader::new(&data);
        let Event::Start(x) = reader.next().unwrap() else {
            panic!()
        };
        assert_eq!(x.name.local, b"x");
    }

    #[test]
    fn unescape_handles_entities_references_and_x_escapes() {
        let mut out = String::new();
        unescape_into(
            b"a &lt;b&gt; &amp; &quot;c&apos; &#65;&#x42; _x0009_tab &unknown; &amp _x00ZZ_ end",
            &mut out,
        );
        assert_eq!(out, "a <b> & \"c' AB \ttab &unknown; &amp _x00ZZ_ end");
        let mut out = String::new();
        unescape_into(b"plain text_with_underscores", &mut out);
        assert_eq!(out, "plain text_with_underscores");
        assert_eq!(
            unescape_attr(b"/ppt/a_x0041_.xml &amp; b"),
            "/ppt/a_x0041_.xml & b"
        );
        let mut out = String::new();
        unescape_into(&[b'o', b'k', 0xff, b'!'], &mut out);
        assert_eq!(out, "ok\u{fffd}!");
    }

    #[test]
    fn well_formed_reports_violations_the_lenient_reader_tolerates() {
        assert!(well_formed(b"<a><b x=\"1\"/>text</a>").is_ok());
        assert_eq!(
            well_formed(b"<a>bad\x01char</a>").unwrap_err().msg,
            "character not allowed in XML"
        );
        assert_eq!(
            well_formed(b"<a x=\"<\"/>").unwrap_err().msg,
            "'<' is not allowed in an attribute value"
        );
        assert_eq!(
            well_formed(b"<p:a/>").unwrap_err().msg,
            "undeclared namespace prefix"
        );
        assert_eq!(
            well_formed(b"<1a/>").unwrap_err().msg,
            "invalid element name"
        );
        assert_eq!(well_formed(b"").unwrap_err().msg, "no root element");
        assert_eq!(
            well_formed(b"<a/><b/>").unwrap_err().msg,
            "more than one root element"
        );
        assert!(Reader::new(b"<p:a/>").next().is_ok());
        let mut lenient = Reader::new(b"<a/><b/>");
        lenient.next().unwrap();
        lenient.next().unwrap();
        assert!(matches!(lenient.next().unwrap(), Event::Start(_)));
    }

    #[test]
    fn text_outside_root_is_rejected_but_whitespace_is_fine() {
        let mut reader = Reader::new(b"\n<a/>\n");
        assert!(matches!(reader.next().unwrap(), Event::Start(_)));
        assert!(matches!(reader.next().unwrap(), Event::End(_)));
        assert!(matches!(reader.next().unwrap(), Event::Eof));
        let mut reader = Reader::new(b"junk<a/>");
        assert_eq!(
            reader.next().unwrap_err().msg,
            "text outside the root element"
        );
    }

    #[test]
    fn transitional_uri_lookup() {
        assert_eq!(
            transitional_uri(Ns::Pml),
            Some("http://schemas.openxmlformats.org/presentationml/2006/main")
        );
        assert_eq!(transitional_uri(Ns::Other(3)), None);
    }
}
