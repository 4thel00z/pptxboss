//! `pptxboss markdown`.

use std::io::Write;

use pptxboss_core::{Document, MarkdownOptions};

use crate::{warn_all, Failure};

/// Prints the slides at `indices` (zero-based, in the written order) as Markdown.
pub fn run(doc: &Document, indices: &[usize], options: &MarkdownOptions) -> Result<(), Failure> {
    let (markdown, report) = doc.markdown_at(indices, options);
    let mut out = std::io::stdout().lock();
    out.write_all(markdown.as_bytes())?;
    drop(out);
    warn_all(&report.warnings());
    Ok(())
}
