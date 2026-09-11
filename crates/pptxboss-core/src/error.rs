//! The crate's error type.

use std::fmt;

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong while reading a package.
///
/// Reading is lenient, so most defects never become errors: they are
/// recorded in a report next to the result. An `Error` means the caller
/// received nothing.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// No end of central directory record: the bytes are not a ZIP archive.
    #[error("not a zip archive")]
    NotZip,
    /// An OLE compound file where an ECMA-376 package was required.
    #[error("compound file: a legacy .ppt or an encrypted package, not an ECMA-376 package")]
    CompoundFile,
    /// An OLE compound file with no presentation inside.
    #[error("compound file holds no PowerPoint Document stream")]
    NoPresentationStream,
    /// A password-protected document; the reader does not decrypt it.
    #[error("encrypted: {0}")]
    Encrypted(String),
    /// A structurally broken ZIP record at the given byte offset.
    #[error("zip: {msg} at offset {offset}")]
    Zip { offset: u64, msg: String },
    /// A ZIP feature the reader does not implement.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The compressed data of an entry could not be inflated.
    #[error("inflate: {0}")]
    Inflate(String),
    /// Malformed XML in a part.
    #[error("xml in {part} at byte {offset}: {msg}")]
    Xml {
        part: String,
        offset: usize,
        msg: String,
    },
    /// A part the document model requires is absent.
    #[error("missing part: {0}")]
    MissingPart(String),
    /// A relationship id referenced from a part has no entry in its rels.
    #[error("missing relationship {id} in {part}")]
    MissingRelationship { part: String, id: String },
    /// The package has no presentation part reachable from the package rels.
    #[error("not a presentation package")]
    NotAPresentation,
    #[error("slide {0} not found")]
    SlideNotFound(usize),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub(crate) fn zip(offset: u64, msg: impl fmt::Display) -> Self {
        Error::Zip {
            offset,
            msg: msg.to_string(),
        }
    }
}
