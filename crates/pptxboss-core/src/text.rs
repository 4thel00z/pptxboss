//! Plain-text extraction from slide content, and the report that says
//! what it left out.

use crate::model::{Content, Shape, SlideContent, TextBody};

/// What goes into extracted text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextOptions {
    /// Include date, footer, header and slide number placeholders.
    pub furniture: bool,
    /// Include shapes marked hidden.
    pub hidden_shapes: bool,
    /// Include slides marked hidden (`show="0"`).
    pub hidden_slides: bool,
    /// Include speaker notes after each slide's text.
    pub notes: bool,
    /// Separator between table cells of one row.
    pub cell_separator: String,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            furniture: false,
            hidden_shapes: false,
            hidden_slides: true,
            notes: false,
            cell_separator: "\t".to_string(),
        }
    }
}

/// Appends the text of `content` to `out`: shapes in z-order, one shape
/// per line block, paragraphs separated by newlines, table rows one per
/// line with cells separated by the configured separator. Shapes without
/// text contribute nothing. No text is inherited from layouts or masters.
pub fn write_content_text(content: &SlideContent, options: &TextOptions, out: &mut String) {
    let mut first = true;
    for shape in content.walk() {
        if shape.hidden && !options.hidden_shapes {
            continue;
        }
        if !options.furniture
            && shape
                .placeholder
                .as_ref()
                .is_some_and(|ph| ph.kind.is_furniture())
        {
            continue;
        }
        let start = out.len();
        write_shape_text(shape, options, out);
        if out.len() == start {
            continue;
        }
        if !first {
            out.insert(start, '\n');
        }
        first = false;
    }
}

fn write_shape_text(shape: &Shape, options: &TextOptions, out: &mut String) {
    match &shape.content {
        Content::Text(body) => write_body(body, out),
        Content::Table(table) => {
            let mut first_row = true;
            for row in &table.rows {
                let row_start = out.len();
                let mut first_cell = true;
                for cell in row.cells.iter().filter(|cell| cell.is_origin()) {
                    if !first_cell {
                        out.push_str(&options.cell_separator);
                    }
                    first_cell = false;
                    write_body(&cell.body, out);
                }
                if out.len() == row_start && !first_cell {
                    continue;
                }
                if !first_row {
                    out.insert(row_start, '\n');
                }
                first_row = false;
            }
        }
        _ => {}
    }
}

fn write_body(body: &TextBody, out: &mut String) {
    if body.is_empty() {
        return;
    }
    body.write_text(out);
}

/// What extraction skipped or could not read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractReport {
    /// Slides whose part could not be read or parsed, with the error text.
    pub failed_slides: Vec<(usize, String)>,
    /// Notes parts that could not be read or parsed.
    pub failed_notes: Vec<(usize, String)>,
    /// Hidden slides left out because the options excluded them.
    pub hidden_slides_skipped: u32,
    /// Graphic frames whose content type the reader does not understand.
    pub unknown_graphics: u32,
    /// Elements in unknown namespaces skipped inside shape trees.
    pub unknown_elements: u32,
}

impl ExtractReport {
    /// True when nothing was dropped for a reason other than the options.
    pub fn is_complete(&self) -> bool {
        self.failed_slides.is_empty() && self.failed_notes.is_empty() && self.unknown_graphics == 0
    }

    /// One line per problem, suitable for a warning stream.
    pub fn warnings(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (index, err) in &self.failed_slides {
            lines.push(format!("slide {}: unreadable: {err}", index + 1));
        }
        for (index, err) in &self.failed_notes {
            lines.push(format!("slide {}: notes unreadable: {err}", index + 1));
        }
        if self.unknown_graphics > 0 {
            lines.push(format!(
                "{} graphic frame(s) of unknown type skipped",
                self.unknown_graphics
            ));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn text_shape(
        id: u32,
        text: &str,
        placeholder: Option<PlaceholderKind>,
        hidden: bool,
    ) -> Shape {
        let paragraphs = text
            .split('\n')
            .map(|line| Paragraph {
                level: 0,
                bullet: Bullet::Inherited,
                runs: vec![Run::text(line)],
            })
            .collect();
        Shape {
            id,
            name: format!("Shape {id}"),
            hidden,
            description: None,
            hyperlink: None,
            placeholder: placeholder.map(|kind| Placeholder { kind, idx: 0 }),
            transform: None,
            text_box: false,
            content: Content::Text(TextBody { paragraphs }),
        }
    }

    fn cell(text: &str) -> Cell {
        Cell {
            body: TextBody {
                paragraphs: vec![Paragraph {
                    level: 0,
                    bullet: Bullet::Inherited,
                    runs: vec![Run::text(text)],
                }],
            },
            grid_span: 1,
            row_span: 1,
            h_merge: false,
            v_merge: false,
        }
    }

    #[test]
    fn shapes_are_separated_furniture_and_hidden_are_skipped_and_groups_recurse() {
        let table = Table {
            column_widths: vec![1, 2],
            rows: vec![
                Row {
                    height: 1,
                    cells: vec![cell("a"), cell("b")],
                },
                Row {
                    height: 1,
                    cells: vec![
                        cell("c"),
                        Cell {
                            h_merge: true,
                            ..cell("ignored")
                        },
                    ],
                },
            ],
        };
        let content = SlideContent {
            kind: SlideKind::Slide,
            name: None,
            show: true,
            shapes: vec![
                text_shape(1, "Title", Some(PlaceholderKind::Title), false),
                text_shape(2, "", None, false),
                text_shape(3, "12", Some(PlaceholderKind::SlideNumber), false),
                text_shape(4, "secret", None, true),
                Shape {
                    content: Content::Group(
                        vec![text_shape(6, "in group\nsecond", None, false)],
                        None,
                    ),
                    ..text_shape(5, "", None, false)
                },
                Shape {
                    content: Content::Table(table),
                    ..text_shape(7, "", None, false)
                },
            ],
        };
        let mut out = String::new();
        write_content_text(&content, &TextOptions::default(), &mut out);
        assert_eq!(out, "Title\nin group\nsecond\na\tb\nc");
        let mut out = String::new();
        write_content_text(
            &content,
            &TextOptions {
                furniture: true,
                hidden_shapes: true,
                cell_separator: " | ".into(),
                ..TextOptions::default()
            },
            &mut out,
        );
        assert_eq!(out, "Title\n12\nsecret\nin group\nsecond\na | b\nc");
    }

    #[test]
    fn reports_list_problems() {
        let report = ExtractReport {
            failed_slides: vec![(2, "boom".into())],
            unknown_graphics: 3,
            ..ExtractReport::default()
        };
        assert!(!report.is_complete());
        assert_eq!(
            report.warnings(),
            vec![
                "slide 3: unreadable: boom",
                "3 graphic frame(s) of unknown type skipped"
            ]
        );
        assert!(ExtractReport {
            hidden_slides_skipped: 2,
            unknown_elements: 4,
            ..ExtractReport::default()
        }
        .is_complete());
    }
}
