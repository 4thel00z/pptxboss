//! A line-oriented Markdown subset turned into slides.
//!
//! `#` starts a title slide (the following paragraph is its subtitle),
//! `##` and `###` start a content slide, `---` starts an untitled slide,
//! `-`, `*`, `+` and `1.` lines are bullets whose indentation sets the
//! level, `Notes:` starts the speaker notes of the current slide, and any
//! other text is a paragraph of the body. Inline emphasis markers are
//! stripped.

use crate::{Layout, Paragraph, Presentation, Slide};

/// Builds a presentation from Markdown text.
pub fn from_markdown(markdown: &str) -> Presentation {
    let mut presentation = Presentation::new();
    let mut current: Option<Slide> = None;
    let mut in_notes = false;
    let mut in_code = false;
    let flush = |current: &mut Option<Slide>, presentation: &mut Presentation| {
        if let Some(slide) = current.take() {
            presentation.slides.push(slide);
        }
    };
    for raw in markdown.lines() {
        let line = raw.trim_end();
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            let slide = current.get_or_insert_with(Slide::new);
            slide.body.push(Paragraph::text(line));
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            in_notes = false;
            continue;
        }
        if let Some(title) = trimmed.strip_prefix("# ") {
            flush(&mut current, &mut presentation);
            current = Some(Slide::title_slide(strip_inline(title), None));
            in_notes = false;
            continue;
        }
        if let Some(title) = trimmed
            .strip_prefix("## ")
            .or_else(|| trimmed.strip_prefix("### "))
        {
            flush(&mut current, &mut presentation);
            current = Some(Slide::titled(strip_inline(title)));
            in_notes = false;
            continue;
        }
        if trimmed == "---" || trimmed == "***" {
            flush(&mut current, &mut presentation);
            current = Some(Slide::new().layout(Layout::Blank));
            in_notes = false;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Notes:") {
            in_notes = true;
            let slide = current.get_or_insert_with(Slide::new);
            let rest = rest.trim();
            if !rest.is_empty() {
                append_notes(slide, rest);
            }
            continue;
        }
        let slide = current.get_or_insert_with(Slide::new);
        if in_notes {
            append_notes(slide, trimmed);
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if let Some(item) = bullet_text(trimmed) {
            let level = (indent / 2).min(8) as u8;
            slide
                .body
                .push(Paragraph::bullet(strip_inline(item), level));
            continue;
        }
        if slide.layout == Some(Layout::Title) && slide.subtitle.is_none() && slide.body.is_empty()
        {
            slide.subtitle = Some(strip_inline(trimmed));
            continue;
        }
        slide.body.push(Paragraph::text(strip_inline(trimmed)));
    }
    flush(&mut current, &mut presentation);
    presentation
}

fn append_notes(slide: &mut Slide, line: &str) {
    match &mut slide.notes {
        Some(notes) => {
            notes.push('\n');
            notes.push_str(line);
        }
        None => slide.notes = Some(line.to_string()),
    }
}

fn bullet_text(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest);
        }
    }
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some(rest);
        }
    }
    None
}

/// Removes `**`, `__`, `*`, `_` and backtick emphasis markers.
fn strip_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' | '`' => {}
            '_' if out.chars().last().is_none_or(|c| !c.is_alphanumeric())
                || chars.peek().is_none_or(|c| !c.is_alphanumeric()) => {}
            _ => out.push(ch),
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_bullets_notes_and_breaks() {
        let deck = from_markdown("# Deck Title\nA subtitle line\n\n## First\n- one\n  - nested **bold**\n- two\n\nNotes: remember\nthe second line\n\n---\nfree paragraph\n\n### Third\n1. first\n2) second\n```\ncode here\n```\n");
        assert_eq!(deck.slides.len(), 4);
        let title = &deck.slides[0];
        assert_eq!(title.layout, Some(Layout::Title));
        assert_eq!(title.title.as_deref(), Some("Deck Title"));
        assert_eq!(title.subtitle.as_deref(), Some("A subtitle line"));
        let first = &deck.slides[1];
        assert_eq!(first.title.as_deref(), Some("First"));
        assert_eq!(first.body.len(), 3);
        assert_eq!(first.body[1].plain_text(), "nested bold");
        assert_eq!(first.body[1].level, 1);
        assert!(first.body[0].bullet);
        assert_eq!(first.notes.as_deref(), Some("remember\nthe second line"));
        let untitled = &deck.slides[2];
        assert_eq!(untitled.layout, Some(Layout::Blank));
        assert_eq!(untitled.body[0].plain_text(), "free paragraph");
        assert!(!untitled.body[0].bullet);
        let third = &deck.slides[3];
        assert_eq!(
            third
                .body
                .iter()
                .map(|p| p.plain_text())
                .collect::<Vec<_>>(),
            ["first", "second", "code here"].map(String::from)
        );
        assert!(third.body[0].bullet);
        assert!(!third.body[2].bullet);
    }

    #[test]
    fn inline_markers_are_stripped_but_snake_case_survives() {
        assert_eq!(
            strip_inline("**bold** and *it* and `code`"),
            "bold and it and code"
        );
        assert_eq!(
            strip_inline("snake_case_name stays"),
            "snake_case_name stays"
        );
        assert_eq!(strip_inline("_emphasis_ here"), "emphasis here");
    }
}
