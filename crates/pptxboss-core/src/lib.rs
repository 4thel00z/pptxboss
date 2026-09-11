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

pub mod chart;
pub mod comments;
pub mod crc32;
pub mod diagram;
pub mod document;
pub mod encoding;
pub mod error;
pub mod hash;
pub mod inflate;
pub mod markdown;
pub mod mce;
pub mod model;
pub mod opc;
pub mod package;
pub mod pml;
pub mod presentation;
pub mod properties;
pub mod slide;
pub mod text;
pub mod xml;
pub mod zip;

pub use chart::{ChartData, Series};
pub use comments::{Comment, CommentAuthor};
pub use diagram::{DiagramData, DiagramItem};
pub use document::{Document, DocumentSeed, ImageRef, ObjectRef, Slide, SlideRef, SlideSection};
pub use error::{Error, Result};
pub use markdown::MarkdownOptions;
pub use model::{Content, Paragraph, Run, Shape, SlideContent, TextBody};
pub use package::{Package, PackageDefects, PackageSeed, Part};
pub use presentation::{Presentation, Section};
pub use properties::{AppProperties, CoreProperties};
pub use slide::SlideReport;
pub use text::{ExtractReport, TextOptions};
