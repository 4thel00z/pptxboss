//! Markdown rendering of a deck: one `##` heading per slide, bullets with
//! their levels, GFM tables, images, chart tables, diagram outlines, and
//! speaker notes and comments as block quotes on request.

use crate::chart::ChartData;
use crate::document::{Document, Slide};
use crate::model::{Bullet, Content, Paragraph, PlaceholderKind, RunKind, Shape, Table, TextBody};
use crate::opc::{Relationships, TargetMode};
use crate::text::ExtractReport;

/// What goes into the Markdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownOptions {
    /// A `## Title` (or `## Slide N`) heading per slide.
    pub headings: bool,
    /// Speaker notes as a `> **Notes:**` block quote after the slide.
    pub notes: bool,
    /// Comments as `> **Comment (Author):**` block quotes after the slide.
    pub comments: bool,
    /// Include slides marked hidden.
    pub hidden_slides: bool,
    /// Include shapes marked hidden.
    pub hidden_shapes: bool,
    /// Include date, footer, header and slide number placeholders.
    pub furniture: bool,
    /// Pictures as `![alt](part)` images.
    pub images: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            headings: true,
            notes: false,
            comments: false,
            hidden_slides: true,
            hidden_shapes: false,
            furniture: false,
            images: true,
        }
    }
}

impl Document {
    /// The whole deck as Markdown, slides separated by a rule, with what was skipped.
    pub fn markdown(&self, options: &MarkdownOptions) -> (String, ExtractReport) {
        self.markdown_at(&self.all_slides(), options)
    }

    /// The slides at `indices` (zero-based) as Markdown in the written
    /// order, separated by a rule, with what was skipped.
    pub fn markdown_at(
        &self,
        indices: &[usize],
        options: &MarkdownOptions,
    ) -> (String, ExtractReport) {
        let results = self.map_slides_at(indices, |slide| match slide {
            Ok(slide) => {
                let mut report = ExtractReport::default();
                let text = slide.markdown(options, &mut report);
                (text, report)
            }
            Err(err) => {
                let mut report = ExtractReport::default();
                report.failed_slides.push((0, err.to_string()));
                (String::new(), report)
            }
        });
        let mut report = ExtractReport::default();
        let mut out = String::new();
        for (&index, (text, slide_report)) in indices.iter().zip(results) {
            report.merge(index, slide_report);
            if text.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push_str("\n\n---\n\n");
            }
            out.push_str(&text);
        }
        if !out.is_empty() {
            out.push('\n');
        }
        (out, report)
    }
}

impl Slide<'_> {
    /// This slide as Markdown, without a trailing newline; empty for a hidden slide left out.
    pub fn markdown(&self, options: &MarkdownOptions, report: &mut ExtractReport) -> String {
        self.merge_parse_report(report);
        if self.is_hidden() && !options.hidden_slides {
            report.hidden_slides_skipped += 1;
            return String::new();
        }
        let rels = self.rels().ok();
        let mut blocks: Vec<String> = Vec::new();
        let title_shape = self
            .content
            .walk()
            .find(|shape| {
                shape.is_title() && shape.text_body().is_some_and(|body| !body.is_empty())
            })
            .map(|shape| shape.id);
        if options.headings {
            let title = title_shape
                .and_then(|_| self.title())
                .map(|title| collapse_whitespace(&title))
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| format!("Slide {}", self.number()));
            blocks.push(format!("## {}", escape_inline(&title)));
        }
        for shape in self.content.walk() {
            if Some(shape.id) == title_shape && options.headings {
                continue;
            }
            if shape.hidden && !options.hidden_shapes {
                continue;
            }
            let furniture = shape
                .placeholder
                .as_ref()
                .is_some_and(|ph| ph.kind.is_furniture());
            if furniture && !options.furniture {
                continue;
            }
            if let Some(block) = self.shape_markdown(shape, rels.as_deref(), options, report) {
                blocks.push(block);
            }
        }
        if options.notes {
            match self.notes() {
                Ok(Some(notes)) if !notes.is_empty() => {
                    let mut quote = String::from("> **Notes:**");
                    for paragraph in notes.paragraphs.iter().filter(|p| !p.is_empty()) {
                        quote.push_str("\n> ");
                        quote.push_str(&escape_inline(&collapse_whitespace(&paragraph.text())));
                    }
                    blocks.push(quote);
                }
                Ok(_) => {}
                Err(err) => report.failed_notes.push((self.index, err.to_string())),
            }
        }
        if options.comments {
            match self.comments() {
                Ok(comments) => {
                    for comment in comments {
                        let who = comment.author.as_deref().unwrap_or("unknown");
                        let label = match comment.reply {
                            true => "Reply",
                            false => "Comment",
                        };
                        let when = comment
                            .date
                            .as_deref()
                            .map(|date| format!(", {date}"))
                            .unwrap_or_default();
                        blocks.push(format!(
                            "> **{label} ({}{when}):** {}",
                            escape_inline(who),
                            escape_inline(&collapse_whitespace(&comment.text))
                        ));
                    }
                }
                Err(err) => report.failed_comments.push((self.index, err.to_string())),
            }
        }
        blocks.join("\n\n")
    }

    fn shape_markdown(
        &self,
        shape: &Shape,
        rels: Option<&Relationships>,
        options: &MarkdownOptions,
        report: &mut ExtractReport,
    ) -> Option<String> {
        let description = shape
            .description
            .as_deref()
            .map(collapse_whitespace)
            .filter(|text| !text.is_empty());
        match &shape.content {
            Content::Text(body) => {
                let in_list = shape.placeholder.as_ref().is_some_and(|ph| {
                    matches!(ph.kind, PlaceholderKind::Body | PlaceholderKind::Object)
                });
                let text = body_markdown(body, in_list, rels);
                (!text.is_empty()).then_some(text)
            }
            Content::Table(table) => table_markdown(table),
            Content::Picture(picture) => {
                if !options.images {
                    return description.map(|text| format!("*{}*", escape_inline(&text)));
                }
                let target = picture
                    .embed
                    .as_deref()
                    .or(picture.link.as_deref())
                    .and_then(|id| rels.and_then(|rels| rels.get(id).map(|rel| (rels, rel))))
                    .map(|(rels, rel)| match rel.mode {
                        TargetMode::External => rel.target.clone(),
                        TargetMode::Internal => rels
                            .resolve(rel)
                            .map(|part| part.trim_start_matches('/').to_string())
                            .unwrap_or_else(|| rel.target.clone()),
                    })
                    .unwrap_or_default();
                let target = match target.is_empty() {
                    false => target,
                    true => self
                        .images()
                        .ok()
                        .and_then(|images| {
                            images
                                .into_iter()
                                .find(|image| image.shape_id == shape.id)
                                .and_then(|image| image.part)
                        })
                        .unwrap_or_default(),
                };
                let alt = description.unwrap_or_else(|| collapse_whitespace(&shape.name));
                Some(format!(
                    "![{}]({})",
                    escape_inline(&alt),
                    target.replace(' ', "%20")
                ))
            }
            Content::Chart(rel_id) => {
                let Some(rel_id) = rel_id else {
                    return description.map(|text| format!("*{}*", escape_inline(&text)));
                };
                match self.chart(rel_id) {
                    Ok(chart) => Some(chart_markdown(&chart)).filter(|text| !text.is_empty()),
                    Err(err) => {
                        report.failed_frames.push((self.index, err.to_string()));
                        None
                    }
                }
            }
            Content::Diagram(rel_id) => {
                let Some(rel_id) = rel_id else {
                    return description.map(|text| format!("*{}*", escape_inline(&text)));
                };
                match self.diagram(rel_id) {
                    Ok(diagram) => {
                        let lines: Vec<String> = diagram
                            .items
                            .iter()
                            .map(|item| {
                                format!(
                                    "{}- {}",
                                    "  ".repeat(usize::from(item.level)),
                                    escape_inline(&collapse_whitespace(&item.text))
                                )
                            })
                            .collect();
                        (!lines.is_empty()).then(|| lines.join("\n"))
                    }
                    Err(err) => {
                        report.failed_frames.push((self.index, err.to_string()));
                        None
                    }
                }
            }
            Content::Ole(ole) => {
                let label = match (&description, &ole.prog_id) {
                    (Some(text), _) => text.clone(),
                    (None, Some(prog_id)) => format!("Embedded object ({prog_id})"),
                    (None, None) => "Embedded object".to_string(),
                };
                Some(format!("*{}*", escape_inline(&label)))
            }
            Content::Group(..) | Content::Connector => None,
            Content::ContentPart(_) | Content::UnknownGraphic(_) => {
                description.map(|text| format!("*{}*", escape_inline(&text)))
            }
        }
    }
}

/// Paragraphs as bullets or plain paragraphs. Paragraphs that inherit
/// their bullet from the list style count as bullets inside body
/// placeholders and as plain text elsewhere.
fn body_markdown(body: &TextBody, in_list: bool, rels: Option<&Relationships>) -> String {
    let mut out = String::new();
    let mut previous_was_list = false;
    for paragraph in body.paragraphs.iter().filter(|p| !p.is_empty()) {
        let marker = match &paragraph.bullet {
            Bullet::Char(_) | Bullet::Picture => Some("- "),
            Bullet::AutoNumber { .. } => Some("1. "),
            Bullet::None => None,
            Bullet::Inherited => in_list.then_some("- "),
        };
        let inline = paragraph_markdown(paragraph, rels, marker.is_some(), paragraph.level);
        match marker {
            Some(marker) => {
                if !out.is_empty() {
                    out.push('\n');
                    if !previous_was_list {
                        out.push('\n');
                    }
                }
                out.push_str(&"  ".repeat(usize::from(paragraph.level)));
                out.push_str(marker);
                out.push_str(&inline);
                previous_was_list = true;
            }
            None => {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&inline);
                previous_was_list = false;
            }
        }
    }
    out
}

/// The runs of a paragraph with bold, italic and links; adjacent runs with
/// the same formatting are merged so markers never touch.
fn paragraph_markdown(
    paragraph: &Paragraph,
    rels: Option<&Relationships>,
    in_list: bool,
    level: u8,
) -> String {
    let mut segments: Vec<(bool, bool, Option<String>, String)> = Vec::new();
    for run in &paragraph.runs {
        if run.kind == RunKind::LineBreak {
            segments.push((false, false, None, "\n".to_string()));
            continue;
        }
        if run.text.is_empty() {
            continue;
        }
        let bold = run.props.bold == Some(true);
        let italic = run.props.italic == Some(true);
        let link = run
            .props
            .hyperlink
            .as_deref()
            .and_then(|id| rels.and_then(|rels| rels.get(id)))
            .filter(|rel| rel.mode == TargetMode::External)
            .map(|rel| rel.target.clone());
        match segments.last_mut() {
            Some((b, i, l, text)) if *b == bold && *i == italic && *l == link && text != "\n" => {
                text.push_str(&run.text)
            }
            _ => segments.push((bold, italic, link, run.text.clone())),
        }
    }
    let mut out = String::new();
    for (bold, italic, link, text) in segments {
        if text == "\n" {
            out.push_str("  \n");
            if in_list {
                out.push_str(&"  ".repeat(usize::from(level) + 1));
            }
            continue;
        }
        let leading = text.len() - text.trim_start().len();
        let trailing = text.len() - text.trim_end().len();
        let core = text.trim();
        if core.is_empty() {
            out.push_str(&text);
            continue;
        }
        out.push_str(&text[..leading]);
        let mut piece = escape_inline(core);
        if bold {
            piece = format!("**{piece}**");
        }
        if italic {
            piece = format!("*{piece}*");
        }
        if let Some(url) = link {
            piece = format!("[{piece}]({})", url.replace(' ', "%20"));
        }
        out.push_str(&piece);
        out.push_str(&text[text.len() - trailing..]);
    }
    let line_start = out.trim_start();
    let needs_guard = !in_list
        && (line_start.starts_with(['#', '>', '-', '+'])
            || line_start
                .split_once(['.', ')'])
                .is_some_and(|(digits, _)| {
                    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                }));
    match needs_guard {
        true => format!("\\{}", out.trim_start()),
        false => out,
    }
}

/// A GFM table; the first row is the header. Merged-away cells stay as
/// empty cells so every row keeps the column count.
fn table_markdown(table: &Table) -> Option<String> {
    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| match cell.is_origin() {
                    true => cell_text(&cell.body),
                    false => String::new(),
                })
                .collect()
        })
        .collect();
    gfm_table(&rows)
}

fn cell_text(body: &TextBody) -> String {
    body.paragraphs
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| escape_cell(&collapse_whitespace(&p.text())))
        .collect::<Vec<_>>()
        .join("<br>")
}

fn gfm_table(rows: &[Vec<String>]) -> Option<String> {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 || rows.iter().all(|row| row.iter().all(String::is_empty)) {
        return None;
    }
    let line = |row: &[String]| {
        let mut cells: Vec<&str> = row.iter().map(String::as_str).collect();
        cells.resize(width, "");
        format!("| {} |", cells.join(" | "))
    };
    let mut out = line(&rows[0]);
    out.push('\n');
    out.push_str(&format!("|{}", "---|".repeat(width)));
    for row in &rows[1..] {
        out.push('\n');
        out.push_str(&line(row));
    }
    Some(out)
}

/// `**Chart: title**` and the chart's rows as a table.
fn chart_markdown(chart: &ChartData) -> String {
    let mut blocks = Vec::new();
    match &chart.title {
        Some(title) => blocks.push(format!(
            "**Chart: {}**",
            escape_inline(&collapse_whitespace(title))
        )),
        None if !chart.series.is_empty() => blocks.push("**Chart**".to_string()),
        None => {}
    }
    let mut rows: Vec<Vec<String>> = chart
        .rows()
        .into_iter()
        .map(|row| row.iter().map(|cell| escape_cell(cell)).collect())
        .collect();
    let has_header = chart.shares_categories() && chart.series.iter().any(|s| s.name.is_some());
    if !rows.is_empty() && !has_header {
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        rows.insert(0, vec![String::new(); width]);
    }
    if let Some(table) = gfm_table(&rows) {
        blocks.push(table);
    }
    blocks.join("\n\n")
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Escapes the punctuation Markdown would otherwise interpret inside text.
fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '>') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn escape_cell(text: &str) -> String {
    escape_inline(text).replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Run, RunProps};

    fn run(text: &str, bold: bool, italic: bool) -> Run {
        Run {
            kind: RunKind::Text,
            text: text.to_string(),
            props: RunProps {
                bold: Some(bold),
                italic: Some(italic),
                ..RunProps::default()
            },
        }
    }

    #[test]
    fn runs_merge_and_wrap_in_markers() {
        let paragraph = Paragraph {
            level: 0,
            bullet: Bullet::None,
            runs: vec![
                run("Plain ", false, false),
                run("bo", true, false),
                run("ld", true, false),
                run(" and *stars*", false, true),
            ],
        };
        assert_eq!(
            paragraph_markdown(&paragraph, None, false, 0),
            "Plain **bold** *and \\*stars\\**"
        );
    }

    #[test]
    fn leading_markdown_syntax_is_guarded() {
        let paragraph = Paragraph {
            level: 0,
            bullet: Bullet::None,
            runs: vec![run("- not a bullet", false, false)],
        };
        assert_eq!(
            paragraph_markdown(&paragraph, None, false, 0),
            "\\- not a bullet"
        );
        let numbered = Paragraph {
            level: 0,
            bullet: Bullet::None,
            runs: vec![run("2024. A year", false, false)],
        };
        assert_eq!(
            paragraph_markdown(&numbered, None, false, 0),
            "\\2024. A year"
        );
    }

    #[test]
    fn tables_and_charts_become_gfm_tables() {
        let rows = vec![
            vec!["a|b".to_string(), "c".to_string()],
            vec!["1".to_string()],
        ];
        assert_eq!(
            gfm_table(&rows).unwrap(),
            "| a|b | c |\n|---|---|\n| 1 |  |"
        );
        let chart = ChartData {
            title: Some("Sales".into()),
            series: vec![crate::chart::Series {
                name: None,
                categories: vec!["N".into(), "S".into()],
                values: vec!["1".into(), "2".into()],
            }],
            ..ChartData::default()
        };
        assert_eq!(
            chart_markdown(&chart),
            "**Chart: Sales**\n\n|  |  |\n|---|---|\n| N | 1 |\n| S | 2 |"
        );
    }
}
