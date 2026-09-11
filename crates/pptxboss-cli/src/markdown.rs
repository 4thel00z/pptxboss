//! `pptxboss markdown`.

use std::io::Write;

use pptxboss_core::{Document, MarkdownOptions};

use crate::{warn_all, Failure};

pub fn run(doc: &Document, options: &MarkdownOptions) -> Result<(), Failure> {
    let (markdown, report) = doc.markdown(options);
    let mut out = std::io::stdout().lock();
    out.write_all(markdown.as_bytes())?;
    drop(out);
    warn_all(&report.warnings());
    Ok(())
}
