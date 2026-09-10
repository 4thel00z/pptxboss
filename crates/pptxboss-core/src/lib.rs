//! PresentationML (.pptx) reader written from the ECMA-376 specification.
//!
//! The crate is layered. [`zip`] reads the container through a positioned
//! source so only the parts a caller opens are ever read. [`Package`] is the
//! raw Open Packaging Conventions view: every ZIP item, content type and
//! relationship exactly as written, defects included, so a verifier can see
//! them. [`Document`] sits on top and is lenient: it resolves the
//! presentation, its slides and their text, recovering from damaged
//! packages where the structure still allows it and reporting what it
//! skipped.

pub mod crc32;
pub mod document;
pub mod error;
pub mod hash;
pub mod mce;
pub mod model;
pub mod opc;
pub mod package;
pub mod pml;
pub mod presentation;
pub mod slide;
pub mod text;
pub mod xml;
pub mod zip;

pub use document::{Document, DocumentSeed, ImageRef, Slide, SlideRef};
pub use error::{Error, Result};
pub use model::{Content, Paragraph, Run, Shape, SlideContent, TextBody};
pub use package::{Package, PackageDefects, PackageSeed, Part};
pub use presentation::Presentation;
pub use slide::SlideReport;
pub use text::{ExtractReport, TextOptions};
