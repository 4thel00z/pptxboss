//! `pptxboss text`.

use std::io::Write;

use pptxboss_core::{Document, ExtractReport, TextOptions};
use serde::Serialize;

use crate::{warn_all, Failure};

#[derive(Serialize)]
struct SlideText<'a> {
    number: usize,
    text: &'a str,
}

pub fn run(
    doc: &Document,
    options: &TextOptions,
    headings: bool,
    json: bool,
) -> Result<(), Failure> {
    let (texts, report): (Vec<String>, ExtractReport) = doc.slide_texts(options);
    let mut out = std::io::stdout().lock();
    if json {
        let slides: Vec<SlideText> = texts
            .iter()
            .enumerate()
            .map(|(index, text)| SlideText {
                number: index + 1,
                text,
            })
            .collect();
        serde_json::to_writer(&mut out, &slides).map_err(|err| Failure {
            message: err.to_string(),
            code: 1,
        })?;
        writeln!(out)?;
    } else if headings {
        for (index, text) in texts.iter().enumerate() {
            if index > 0 {
                writeln!(out)?;
            }
            writeln!(out, "--- slide {} ---", index + 1)?;
            if !text.is_empty() {
                writeln!(out, "{text}")?;
            }
        }
    } else {
        let mut first = true;
        for text in texts.iter().filter(|text| !text.is_empty()) {
            if !first {
                writeln!(out)?;
            }
            first = false;
            writeln!(out, "{text}")?;
        }
    }
    drop(out);
    warn_all(&report.warnings());
    Ok(())
}
