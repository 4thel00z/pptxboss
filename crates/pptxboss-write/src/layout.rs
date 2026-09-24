//! The layout engine. It measures a slide's text with the theme's font
//! metrics, stacks blocks below the title on a gutter grid, sets column
//! children side by side, scales body text down to the theme's minimum
//! when a slide is full, and moves what still does not fit onto
//! continuation slides. Content with a fixed position passes through.

use std::borrow::Cow;

use crate::metrics::{line_count, Face, Font};
use crate::{
    Block, Color, Layout, Paragraph, Rect, Run, SchemeColor, Shape, Slide, SlideSize, Theme,
};

/// Horizontal inset of a text body on each side, in EMU.
const INSET_X: i64 = 91_440;
/// Vertical inset of a text body on each side, in EMU.
const INSET_Y: i64 = 45_720;
/// Hanging indent of a bullet, in EMU.
const BULLET_INDENT: i64 = 228_600;
/// Indent per level, in EMU.
const LEVEL_INDENT: i64 = 457_200;
/// Space between stacked blocks, in EMU.
const GAP: i64 = 228_600;
/// The least height a picture is given before it moves to the next slide.
const PICTURE_MIN: i64 = 914_400;
/// Line spacing of body text, in percent.
const BODY_SPACING: u32 = 90;
/// Steps the body scale takes on the way down to the minimum, in percent.
const SCALE_STEP: u32 = 5;
/// Width of the rule beside a quote, in EMU.
const QUOTE_BAR: i64 = 54_864;
/// Space between the rule and the quote's text, in EMU.
const QUOTE_GAP: i64 = 182_880;
/// Height of the footer band, in EMU.
const FOOTER_HEIGHT: i64 = 365_125;
/// Distance from the bottom of the slide to the top of the footer band, in EMU.
const FOOTER_RISE: i64 = 501_650;

/// The geometry every slide shares, derived from the slide size.
pub(crate) struct Grid {
    pub(crate) title: Rect,
    pub(crate) body: Rect,
    pub(crate) center_title: Rect,
    pub(crate) subtitle: Rect,
    /// The content area of a slide without a title.
    pub(crate) page: Rect,
    /// The footer text at the bottom left.
    pub(crate) footer: Rect,
    /// The slide number at the bottom right.
    pub(crate) slide_number: Rect,
    pub(crate) gutter: i64,
}

impl Grid {
    pub(crate) fn for_size(size: SlideSize) -> Self {
        let margin = size.cx * 55 / 800;
        let width = size.cx - 2 * margin;
        let top = 457_200;
        let bottom = 681_037;
        Self {
            title: Rect::new(margin, 365_125, width, 1_325_563),
            body: Rect::new(margin, 1_825_625, width, size.cy - 1_825_625 - bottom),
            center_title: Rect::new(margin, size.cy / 6, width, 2_387_600),
            subtitle: Rect::new(
                size.cx / 8,
                size.cy / 6 + 2_479_675,
                size.cx * 3 / 4,
                1_655_762,
            ),
            page: Rect::new(margin, top, width, size.cy - top - bottom),
            footer: Rect::new(margin, size.cy - FOOTER_RISE, width * 2 / 3, FOOTER_HEIGHT),
            slide_number: Rect::new(
                margin + width * 2 / 3,
                size.cy - FOOTER_RISE,
                width / 3,
                FOOTER_HEIGHT,
            ),
            gutter: size.cx / 40,
        }
    }

    /// `area` divided into columns by weight with a gutter between neighbors.
    fn split(&self, area: Rect, weights: &[i64]) -> Vec<Rect> {
        let total: i64 = weights.iter().sum::<i64>().max(1);
        let count = weights.len() as i64;
        let usable = (area.cx - self.gutter * (count - 1)).max(count);
        let mut x = area.x;
        weights
            .iter()
            .map(|weight| {
                let cx = usable * weight / total;
                let rect = Rect::new(x, area.y, cx, area.cy);
                x += cx + self.gutter;
                rect
            })
            .collect()
    }
}

/// Space before a body paragraph at its level, in points.
pub(crate) fn space_before(level: u8) -> u32 {
    match level {
        0 => 10,
        _ => 5,
    }
}

/// A block as the engine works on it; text and table rows are borrowed
/// until a split makes a tail of them.
#[derive(Clone, Debug)]
enum Item<'a> {
    Text(Cow<'a, [Paragraph]>),
    Picture {
        data: &'a [u8],
        description: Option<&'a str>,
    },
    Table {
        rows: Cow<'a, [Vec<String>]>,
        header: bool,
    },
    Columns(Vec<Item<'a>>),
    Stat {
        value: &'a str,
        label: &'a str,
    },
    Quote {
        text: &'a str,
        attribution: Option<&'a str>,
    },
}

impl<'a> Item<'a> {
    fn from_block(block: &'a Block) -> Item<'a> {
        match block {
            Block::Text(paragraphs) => Item::Text(Cow::Borrowed(paragraphs)),
            Block::Picture { data, description } => Item::Picture {
                data,
                description: description.as_deref(),
            },
            Block::Table { rows, header } => Item::Table {
                rows: Cow::Borrowed(rows),
                header: *header,
            },
            Block::Columns(children) => {
                Item::Columns(children.iter().map(Item::from_block).collect())
            }
            Block::Stat { value, label } => Item::Stat { value, label },
            Block::Quote { text, attribution } => Item::Quote {
                text,
                attribution: attribution.as_deref(),
            },
        }
    }

    fn is_picture(&self) -> bool {
        matches!(self, Item::Picture { .. })
    }
}

/// One element of a planned slide with its final position.
pub(crate) enum Element<'a> {
    /// The body placeholder; None keeps the layout's position. `scale` is
    /// the body scale in percent, 100 when nothing was shrunk.
    Body {
        paragraphs: Vec<Paragraph>,
        rect: Option<Rect>,
        scale: u32,
    },
    /// A text box styled like the body.
    TextBox {
        paragraphs: Vec<Paragraph>,
        rect: Rect,
        scale: u32,
    },
    Picture {
        data: &'a [u8],
        description: Option<&'a str>,
        rect: Rect,
    },
    Table {
        rows: Vec<Vec<String>>,
        header: bool,
        rect: Rect,
        row_heights: Vec<i64>,
        /// Cell text size in points.
        size: u32,
    },
    /// A filled rectangle without text.
    Bar { rect: Rect, color: Color },
    /// Content at a fixed position, written as given.
    Shape(&'a Shape),
}

/// One slide to write.
pub(crate) struct Planned<'a> {
    pub(crate) source: &'a Slide,
    pub(crate) layout: Layout,
    /// The title's size in points when it had to shrink to fit its frame.
    pub(crate) title_size: Option<u32>,
    /// The subtitle's size in points when it had to shrink to fit its frame.
    pub(crate) subtitle_size: Option<u32>,
    pub(crate) elements: Vec<Element<'a>>,
    pub(crate) notes: Option<&'a str>,
}

/// Height in EMU of each item at a scale, and how far it can shrink.
struct Measured {
    natural: i64,
    minimum: i64,
}

struct Engine<'t> {
    theme: &'t Theme,
    grid: &'t Grid,
    body_face: Face,
}

impl<'t> Engine<'t> {
    fn font(&self, run: &Run, level: u8, scale: u32) -> Font {
        let face = run.font.as_deref().map_or(self.body_face, Face::for_font);
        let size = run.size.unwrap_or(self.theme.scale.body_level(level));
        Font {
            face,
            bold: run.bold,
            size: scaled(size, scale),
        }
    }

    fn paragraph_height(&self, paragraph: &Paragraph, width: i64, scale: u32) -> i64 {
        let level = paragraph.level;
        let indent = match paragraph.bullet {
            true => BULLET_INDENT + level as i64 * LEVEL_INDENT,
            false => level as i64 * LEVEL_INDENT,
        };
        let line_width = width - 2 * INSET_X - indent;
        let resolve = |run: &Run| self.font(run, level, scale);
        let lines = line_count(paragraph, line_width, &resolve) as i64;
        let tallest = paragraph
            .runs
            .iter()
            .map(|run| resolve(run).line_height(BODY_SPACING))
            .max()
            .unwrap_or_else(|| resolve(&Run::default()).line_height(BODY_SPACING));
        let before = paragraph
            .space_before
            .unwrap_or_else(|| space_before(level));
        let after = paragraph.space_after.unwrap_or(0);
        lines * tallest + points(scaled(before + after, scale))
    }

    fn text_height(&self, paragraphs: &[Paragraph], width: i64, scale: u32) -> i64 {
        let body: i64 = paragraphs
            .iter()
            .map(|paragraph| self.paragraph_height(paragraph, width, scale))
            .sum();
        body + 2 * INSET_Y
    }

    fn table_rows(&self, rows: &[Vec<String>], header: bool, width: i64, scale: u32) -> Vec<i64> {
        let columns = rows.first().map_or(1, |row| row.len().max(1)) as i64;
        let cell_width = width / columns - 2 * INSET_X;
        let size = scaled(self.theme.scale.table, scale);
        rows.iter()
            .enumerate()
            .map(|(index, cells)| {
                let bold = header && index == 0;
                let font = Font {
                    face: self.body_face,
                    bold,
                    size,
                };
                let lines = cells
                    .iter()
                    .map(|cell| {
                        let paragraph = Paragraph::text(cell.as_str());
                        line_count(&paragraph, cell_width, &|_| font) as i64
                    })
                    .max()
                    .unwrap_or(1);
                lines * font.line_height(100) + 2 * INSET_Y
            })
            .collect()
    }

    /// The figure at display size in the first accent color over its label.
    fn stat_paragraphs(&self, value: &str, label: &str) -> Vec<Paragraph> {
        let scale = &self.theme.scale;
        vec![
            Paragraph::runs(vec![Run::text(value)
                .bold()
                .size(scale.display)
                .color(Color::Scheme(SchemeColor::Accent1))])
            .space_before(0),
            Paragraph::text(label).size(scale.subtitle).space_before(4),
        ]
    }

    /// Italic text at body size with the attribution below at a smaller size.
    fn quote_paragraphs(&self, text: &str, attribution: Option<&str>) -> Vec<Paragraph> {
        let scale = &self.theme.scale;
        let mut paragraphs =
            vec![Paragraph::runs(vec![Run::text(text).italic().size(scale.body)]).space_before(0)];
        paragraphs.extend(attribution.map(|attribution| {
            Paragraph::text(attribution)
                .size(scale.body_level(2))
                .space_before(8)
        }));
        paragraphs
    }

    fn picture_aspect(data: &[u8]) -> (i64, i64) {
        match crate::image::dimensions(data) {
            Some((w, h)) => (w as i64, h as i64),
            None => (4, 3),
        }
    }

    /// The largest rectangle of the picture's aspect inside `bounds`,
    /// centered horizontally and set at the top.
    fn contain(data: &[u8], bounds: Rect) -> Rect {
        let (w, h) = Self::picture_aspect(data);
        let by_width = bounds.cx * h / w;
        if by_width <= bounds.cy {
            return Rect::new(bounds.x, bounds.y, bounds.cx, by_width);
        }
        let cx = bounds.cy * w / h;
        Rect::new(bounds.x + (bounds.cx - cx) / 2, bounds.y, cx, bounds.cy)
    }

    fn weights(children: &[Item<'_>]) -> Vec<i64> {
        let pictures = children.iter().filter(|child| child.is_picture()).count();
        if children.len() == 2 && pictures == 1 {
            return children
                .iter()
                .map(|child| match child.is_picture() {
                    true => 5,
                    false => 7,
                })
                .collect();
        }
        vec![1; children.len().max(1)]
    }

    fn measure(&self, item: &Item<'_>, width: i64, scale: u32) -> Measured {
        match item {
            Item::Text(paragraphs) => {
                let height = self.text_height(paragraphs, width, scale);
                Measured {
                    natural: height,
                    minimum: height,
                }
            }
            Item::Table { rows, header } => {
                let height = self.table_rows(rows, *header, width, scale).iter().sum();
                Measured {
                    natural: height,
                    minimum: height,
                }
            }
            Item::Picture { data, .. } => {
                let (w, h) = Self::picture_aspect(data);
                let natural = width * h / w;
                Measured {
                    natural,
                    minimum: natural.min(PICTURE_MIN),
                }
            }
            Item::Stat { value, label } => {
                let height = self.text_height(&self.stat_paragraphs(value, label), width, scale);
                Measured {
                    natural: height,
                    minimum: height,
                }
            }
            Item::Quote { text, attribution } => {
                let paragraphs = self.quote_paragraphs(text, *attribution);
                let height = self.text_height(&paragraphs, width - QUOTE_BAR - QUOTE_GAP, scale);
                Measured {
                    natural: height,
                    minimum: height,
                }
            }
            Item::Columns(children) => {
                let widths = self
                    .grid
                    .split(Rect::new(0, 0, width, 0), &Self::weights(children));
                let measured: Vec<Measured> = children
                    .iter()
                    .zip(&widths)
                    .map(|(child, column)| self.measure(child, column.cx, scale))
                    .collect();
                Measured {
                    natural: measured.iter().map(|m| m.natural).max().unwrap_or(0),
                    minimum: measured.iter().map(|m| m.minimum).max().unwrap_or(0),
                }
            }
        }
    }

    fn scale_floor(&self) -> u32 {
        let scale = &self.theme.scale;
        let floor = scale.minimum * 100 / scale.body.max(1);
        floor.clamp(1, 100)
    }

    /// The scales to try, from full size down to the theme's minimum.
    fn scales(&self) -> Vec<u32> {
        let floor = self.scale_floor();
        let mut scales: Vec<u32> = (0..)
            .map(|step| 100 - step * SCALE_STEP)
            .take_while(|scale| *scale > floor)
            .collect();
        scales.push(floor);
        scales
    }

    fn stack_height(measured: &[Measured]) -> i64 {
        let gaps = GAP * (measured.len() as i64 - 1).max(0);
        measured.iter().map(|m| m.minimum).sum::<i64>() + gaps
    }

    /// Places `items` in `area`; returns this slide's elements and the
    /// items that continue on the next slide.
    fn place<'a>(
        &self,
        items: Vec<Item<'a>>,
        area: Rect,
        first_is_body: bool,
    ) -> (Vec<Element<'a>>, u32, Vec<Item<'a>>) {
        let fit = |items: &[Item<'a>]| {
            self.scales().into_iter().find_map(|scale| {
                let measured: Vec<Measured> = items
                    .iter()
                    .map(|item| self.measure(item, area.cx, scale))
                    .collect();
                (Self::stack_height(&measured) <= area.cy).then_some((scale, measured))
            })
        };
        let (scale, measured, items, rest) = match fit(&items) {
            Some((scale, measured)) => (scale, measured, items, Vec::new()),
            None => {
                let floor = self.scale_floor();
                let (kept, rest) = self.split_items(items, area, floor);
                let (scale, measured) = fit(&kept).unwrap_or_else(|| {
                    let measured = kept
                        .iter()
                        .map(|item| self.measure(item, area.cx, floor))
                        .collect();
                    (floor, measured)
                });
                (scale, measured, kept, rest)
            }
        };
        let mut extra = (area.cy - Self::stack_height(&measured)).max(0);
        let mut y = area.y;
        let mut elements = Vec::new();
        for (index, (item, measured)) in items.into_iter().zip(measured).enumerate() {
            let growth = (measured.natural - measured.minimum).clamp(0, extra);
            extra -= growth;
            let height = measured.minimum + growth;
            let rect = Rect::new(area.x, y, area.cx, height);
            let body = first_is_body && index == 0;
            elements.extend(self.elements(item, rect, scale, body));
            y += height + GAP;
        }
        (elements, scale, rest)
    }

    /// Splits `items` at the theme's minimum scale: the first item that does
    /// not fit is cut between paragraphs or table rows when it can be,
    /// otherwise it moves whole to the next slide. A first item that cannot
    /// be cut is kept, so a slide never leaves empty.
    fn split_items<'a>(
        &self,
        items: Vec<Item<'a>>,
        area: Rect,
        scale: u32,
    ) -> (Vec<Item<'a>>, Vec<Item<'a>>) {
        let mut kept: Vec<Item<'a>> = Vec::new();
        let mut used = 0i64;
        let mut items = items.into_iter();
        while let Some(item) = items.next() {
            let gap = match kept.is_empty() {
                true => 0,
                false => GAP,
            };
            let available = area.cy - used - gap;
            let measured = self.measure(&item, area.cx, scale);
            if measured.minimum <= available {
                used += measured.minimum + gap;
                kept.push(item);
                continue;
            }
            let (head, tail) = self.cut(item, area.cx, available, scale, kept.is_empty());
            if let Some(head) = head {
                kept.push(head);
            }
            let mut rest: Vec<Item<'a>> = tail.into_iter().collect();
            rest.extend(items);
            return (kept, rest);
        }
        (kept, Vec::new())
    }

    /// The height to fill on this slide so that an item of `total` height
    /// spreads evenly over the slides it needs.
    fn balanced(total: i64, available: i64) -> i64 {
        let available = available.max(1);
        let pages = (total + available - 1) / available;
        match pages {
            0 | 1 => available,
            pages => ((total + pages - 1) / pages).min(available),
        }
    }

    /// Cuts one item to `available` height; `force` keeps at least one
    /// paragraph or row (or the whole item when it cannot be cut) and
    /// balances the cut over the slides the item needs.
    fn cut<'a>(
        &self,
        item: Item<'a>,
        width: i64,
        available: i64,
        scale: u32,
        force: bool,
    ) -> (Option<Item<'a>>, Option<Item<'a>>) {
        match item {
            Item::Text(paragraphs) => {
                let room = available - 2 * INSET_Y;
                let target = match force {
                    true => {
                        let total = self.text_height(&paragraphs, width, scale) - 2 * INSET_Y;
                        Self::balanced(total, room)
                    }
                    false => room,
                };
                let mut height = 0i64;
                let mut count = 0usize;
                for paragraph in paragraphs.iter() {
                    height += self.paragraph_height(paragraph, width, scale);
                    if height > target {
                        break;
                    }
                    count += 1;
                }
                if force {
                    count = count.max(1);
                }
                let (head, tail) = paragraphs.split_at(count.min(paragraphs.len()));
                (
                    (!head.is_empty()).then(|| Item::Text(Cow::Owned(head.to_vec()))),
                    (!tail.is_empty()).then(|| Item::Text(Cow::Owned(tail.to_vec()))),
                )
            }
            Item::Table { rows, header } => {
                let heights = self.table_rows(&rows, header, width, scale);
                let first_body = usize::from(header);
                let header_height: i64 = heights[..first_body.min(heights.len())].iter().sum();
                let body_height: i64 = heights.iter().skip(first_body).sum();
                let target = match force {
                    true => {
                        header_height
                            + Self::balanced(body_height, (available - header_height).max(1))
                    }
                    false => available,
                };
                let mut height = header_height;
                let mut count = first_body;
                for row_height in heights.iter().skip(first_body) {
                    height += row_height;
                    if height > target {
                        break;
                    }
                    count += 1;
                }
                if force {
                    count = count.max(first_body + 1);
                }
                let count = count.min(rows.len());
                if count <= first_body {
                    let whole = Item::Table { rows, header };
                    return match force {
                        true => (Some(whole), None),
                        false => (None, Some(whole)),
                    };
                }
                let head: Vec<Vec<String>> = rows[..count].to_vec();
                let mut tail: Vec<Vec<String>> = rows[..first_body].to_vec();
                tail.extend(rows[count..].iter().cloned());
                let tail_has_body = tail.len() > first_body;
                (
                    Some(Item::Table {
                        rows: Cow::Owned(head),
                        header,
                    }),
                    tail_has_body.then_some(Item::Table {
                        rows: Cow::Owned(tail),
                        header,
                    }),
                )
            }
            other => match force {
                true => (Some(other), None),
                false => (None, Some(other)),
            },
        }
    }

    fn elements<'a>(&self, item: Item<'a>, rect: Rect, scale: u32, body: bool) -> Vec<Element<'a>> {
        match item {
            Item::Text(paragraphs) => {
                let paragraphs = scaled_paragraphs(&paragraphs, scale);
                match body {
                    true => vec![Element::Body {
                        paragraphs,
                        rect: Some(rect),
                        scale,
                    }],
                    false => vec![Element::TextBox {
                        paragraphs,
                        rect,
                        scale,
                    }],
                }
            }
            Item::Picture { data, description } => vec![Element::Picture {
                data,
                description,
                rect: Self::contain(data, rect),
            }],
            Item::Table { rows, header } => {
                let row_heights = self.table_rows(&rows, header, rect.cx, scale);
                let height: i64 = row_heights.iter().sum();
                vec![Element::Table {
                    rows: rows.into_owned(),
                    header,
                    rect: Rect::new(rect.x, rect.y, rect.cx, height),
                    row_heights,
                    size: scaled(self.theme.scale.table, scale),
                }]
            }
            Item::Columns(children) => {
                let columns = self.grid.split(rect, &Self::weights(&children));
                children
                    .into_iter()
                    .zip(columns)
                    .flat_map(|(child, column)| self.elements(child, column, scale, false))
                    .collect()
            }
            Item::Stat { value, label } => vec![Element::TextBox {
                paragraphs: scaled_paragraphs(&self.stat_paragraphs(value, label), scale),
                rect,
                scale,
            }],
            Item::Quote { text, attribution } => {
                let paragraphs =
                    scaled_paragraphs(&self.quote_paragraphs(text, attribution), scale);
                let text_rect = Rect::new(
                    rect.x + QUOTE_BAR + QUOTE_GAP,
                    rect.y,
                    rect.cx - QUOTE_BAR - QUOTE_GAP,
                    rect.cy,
                );
                let height = self.text_height(&paragraphs, text_rect.cx, 100);
                vec![
                    Element::Bar {
                        rect: Rect::new(rect.x, rect.y + INSET_Y, QUOTE_BAR, height - 2 * INSET_Y),
                        color: Color::Scheme(SchemeColor::Accent1),
                    },
                    Element::TextBox {
                        paragraphs,
                        rect: text_rect,
                        scale,
                    },
                ]
            }
        }
    }

    /// The title size that fits its frame, or None when the theme's size does.
    fn title_size(&self, title: &str, layout: Layout) -> Option<u32> {
        let (frame, size) = match layout {
            Layout::Title | Layout::Section => (self.grid.center_title, self.theme.scale.display),
            _ => (self.grid.title, self.theme.scale.title),
        };
        self.fitted_size(title, frame, size, Face::for_font(&self.theme.major_font))
    }

    /// The subtitle size that fits its frame, or None when the theme's size does.
    fn subtitle_size(&self, subtitle: &str) -> Option<u32> {
        self.fitted_size(
            subtitle,
            self.grid.subtitle,
            self.theme.scale.subtitle,
            self.body_face,
        )
    }

    /// The size, stepping down two points at a time from `size` to the
    /// theme's minimum, at which `text` fits `frame`; None when `size` does.
    fn fitted_size(&self, text: &str, frame: Rect, size: u32, face: Face) -> Option<u32> {
        let fits = |size: u32| {
            let font = Font {
                face,
                bold: false,
                size,
            };
            let paragraph = Paragraph::text(text);
            let lines = line_count(&paragraph, frame.cx - 2 * INSET_X, &|_| font) as i64;
            lines * font.line_height(BODY_SPACING) + 2 * INSET_Y <= frame.cy
        };
        if fits(size) {
            return None;
        }
        let minimum = self.theme.scale.minimum.max(1);
        let mut candidate = size;
        while candidate > minimum + 2 {
            candidate -= 2;
            if fits(candidate) {
                return Some(candidate);
            }
        }
        Some(minimum)
    }
}

fn scaled(points: u32, scale: u32) -> u32 {
    ((points * scale + 50) / 100).max(1)
}

fn points(value: u32) -> i64 {
    value as i64 * crate::EMU_PER_POINT
}

/// Paragraphs with explicit run sizes and spacing scaled; inherited sizes
/// are scaled by the list style the writer emits.
fn scaled_paragraphs(paragraphs: &[Paragraph], scale: u32) -> Vec<Paragraph> {
    if scale == 100 {
        return paragraphs.to_vec();
    }
    paragraphs
        .iter()
        .map(|paragraph| {
            let mut out = paragraph.clone();
            out.space_before = out.space_before.map(|value| scaled(value, scale));
            out.space_after = out.space_after.map(|value| scaled(value, scale));
            for run in &mut out.runs {
                run.size = run.size.map(|value| scaled(value, scale));
            }
            out
        })
        .collect()
}

/// The slides to write for one source slide: the slide itself, then any
/// continuation slides its overflow needs.
pub(crate) fn plan<'a>(slide: &'a Slide, grid: &Grid, theme: &Theme) -> Vec<Planned<'a>> {
    let engine = Engine {
        theme,
        grid,
        body_face: Face::for_font(&theme.minor_font),
    };
    let mut items: Vec<Item<'a>> = Vec::new();
    if !slide.body.is_empty() {
        items.push(Item::Text(Cow::Borrowed(&slide.body)));
    }
    items.extend(slide.blocks.iter().map(Item::from_block));
    let shapes = || slide.shapes.iter().map(Element::Shape);
    let subtitle_size = slide
        .subtitle
        .as_deref()
        .and_then(|subtitle| engine.subtitle_size(subtitle));
    if items.is_empty() {
        let layout = slide.layout_for(false);
        return vec![Planned {
            source: slide,
            layout,
            title_size: slide
                .title
                .as_deref()
                .and_then(|title| engine.title_size(title, layout)),
            subtitle_size,
            elements: shapes().collect(),
            notes: slide.notes.as_deref(),
        }];
    }
    let area = match slide.title {
        Some(_) => grid.body,
        None => grid.page,
    };
    let mut planned = Vec::new();
    let mut pending = items;
    let mut first = true;
    while !pending.is_empty() {
        let first_is_body = matches!(pending.first(), Some(Item::Text(_)));
        let only_body = first_is_body && pending.len() == 1;
        let (mut elements, scale, rest) = engine.place(pending, area, first_is_body);
        if only_body && scale == 100 && slide.title.is_some() && rest.is_empty() {
            if let Some(Element::Body { rect, .. }) = elements.first_mut() {
                *rect = None;
            }
        }
        if first {
            elements.extend(shapes());
        }
        let layout = slide.layout_for(first_is_body);
        planned.push(Planned {
            source: slide,
            layout,
            title_size: slide
                .title
                .as_deref()
                .and_then(|title| engine.title_size(title, layout)),
            subtitle_size,
            elements,
            notes: slide.notes.as_deref().filter(|_| first),
        });
        pending = rest;
        first = false;
    }
    planned
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widescreen() -> Grid {
        Grid::for_size(SlideSize::WIDESCREEN)
    }

    fn rect_of(element: &Element<'_>) -> Rect {
        match element {
            Element::Body { rect, .. } => rect.expect("explicit rect"),
            Element::TextBox { rect, .. } => *rect,
            Element::Picture { rect, .. } => *rect,
            Element::Table { rect, .. } => *rect,
            Element::Bar { rect, .. } => *rect,
            Element::Shape(_) => panic!("shape"),
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn inside(inner: Rect, outer: Rect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.cx <= outer.x + outer.cx
            && inner.y + inner.cy <= outer.y + outer.cy
    }

    fn overlap(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.cx && b.x < a.x + a.cx && a.y < b.y + b.cy && b.y < a.y + a.cy
    }

    #[test]
    fn grid_splits_by_weight_with_gutters() {
        let grid = widescreen();
        let area = Rect::new(0, 0, 10_000_000, 100);
        let halves = grid.split(area, &[1, 1]);
        assert_eq!(halves[0].x, 0);
        assert_eq!(halves[0].cx, (10_000_000 - grid.gutter) / 2);
        assert_eq!(halves[1].x, halves[0].cx + grid.gutter);
        let sevens = grid.split(area, &[7, 5]);
        assert_eq!(sevens[0].cx, (10_000_000 - grid.gutter) * 7 / 12);
        assert_eq!(grid.page.y, 457_200);
        assert!(grid.page.cy > grid.body.cy);
        let crowded = grid.split(Rect::new(0, 0, 10_000, 100), &[1; 60]);
        assert_eq!(crowded.len(), 60);
        assert!(crowded.iter().all(|column| column.cx >= 1));
        assert!(grid.footer.y >= grid.body.y + grid.body.cy);
        assert!(grid.footer.y >= grid.page.y + grid.page.cy);
        assert_eq!(grid.slide_number.x, grid.footer.x + grid.footer.cx);
        assert_eq!(
            grid.slide_number.x + grid.slide_number.cx,
            grid.body.x + grid.body.cx
        );
    }

    #[test]
    fn stats_and_quotes_become_boxes_that_move_whole() {
        let grid = widescreen();
        let theme = Theme::office();
        let slide = Slide::titled("Numbers")
            .columns(vec![
                Block::stat("86%", "fewer cold starts"),
                Block::stat("1,240", "decks verified"),
                Block::stat("0", "findings"),
            ])
            .block(Block::quote(
                "The deck opened in the right font on a machine that had never seen it.",
                Some("A reviewer"),
            ));
        let planned = plan(&slide, &grid, &theme);
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].layout, Layout::TitleOnly);
        let elements = &planned[0].elements;
        assert_eq!(elements.len(), 5);
        for element in &elements[..3] {
            match element {
                Element::TextBox { paragraphs, .. } => {
                    assert_eq!(paragraphs.len(), 2);
                    assert_eq!(paragraphs[0].runs[0].size, Some(theme.scale.display));
                    assert_eq!(
                        paragraphs[0].runs[0].color,
                        Some(Color::Scheme(SchemeColor::Accent1))
                    );
                }
                _ => panic!("stat box expected"),
            }
        }
        let rects: Vec<Rect> = elements.iter().map(rect_of).collect();
        assert_eq!(rects[0].y, rects[2].y);
        assert_eq!(rects[0].x, grid.body.x);
        let (bar, quote) = (rects[3], rects[4]);
        assert!(matches!(
            elements[3],
            Element::Bar {
                color: Color::Scheme(SchemeColor::Accent1),
                ..
            }
        ));
        assert_eq!(bar.x, grid.body.x);
        assert_eq!(bar.cx, QUOTE_BAR);
        assert_eq!(quote.x, bar.x + QUOTE_BAR + QUOTE_GAP);
        assert!(bar.y >= quote.y && bar.y + bar.cy <= quote.y + quote.cy);
        match &elements[4] {
            Element::TextBox { paragraphs, .. } => {
                assert!(paragraphs[0].runs[0].italic);
                assert_eq!(paragraphs[1].plain_text(), "A reviewer");
            }
            _ => panic!("quote box expected"),
        }
        for (i, a) in rects.iter().enumerate() {
            assert!(inside(*a, grid.body), "{a:?}");
            for b in &rects[i + 1..] {
                assert!(!overlap(*a, *b), "{a:?} overlaps {b:?}");
            }
        }

        let mut crowded = Slide::titled("Crowded");
        for i in 0..40 {
            crowded = crowded.bullet(format!("Filler line {i} to push the quote off the slide"));
        }
        crowded = crowded.block(Block::quote("Kept whole", None));
        let planned = plan(&crowded, &grid, &theme);
        let quotes = planned
            .iter()
            .flat_map(|p| p.elements.iter())
            .filter(|element| matches!(element, Element::Bar { .. }))
            .count();
        assert_eq!(quotes, 1);
    }

    #[test]
    fn section_slides_and_long_subtitles_fit_their_frames() {
        let grid = widescreen();
        let theme = Theme::office();
        let divider = Slide::section("Part two");
        let section = plan(&divider, &grid, &theme);
        assert_eq!(section.len(), 1);
        assert_eq!(section[0].layout, Layout::Section);
        assert!(section[0].title_size.is_none());
        assert!(section[0].elements.is_empty());
        assert!(!Layout::Section.has_footer() && Layout::TitleOnly.has_footer());

        let long = "far beyond the frame ".repeat(20);
        let wordy = Slide::title_slide("Deck", Some(&long));
        let planned = plan(&wordy, &grid, &theme);
        let size = planned[0].subtitle_size.expect("shrunk subtitle");
        assert!((18..24).contains(&size), "{size}");
        let short = Slide::title_slide("Deck", Some("Short"));
        assert!(plan(&short, &grid, &theme)[0].subtitle_size.is_none());
    }

    #[test]
    fn standard_size_slides_keep_every_block_inside() {
        let grid = Grid::for_size(SlideSize::STANDARD);
        let theme = Theme::office();
        let mut rows = vec![vec!["Deck".to_string(), "Slides".to_string()]];
        rows.extend((0..20).map(|i| vec![format!("deck-{i}"), i.to_string()]));
        let slide = Slide::titled("Standard")
            .paragraph("A paragraph above a table, a picture column and a row of numbers.")
            .block(Block::table(rows, true))
            .columns(vec![
                Block::bullets(["left one", "left two", "left three"]),
                Block::picture(png(1200, 800)),
            ])
            .columns(vec![
                Block::stat("4:3", "slide size"),
                Block::stat("9,144,000", "EMU wide"),
            ]);
        let planned = plan(&slide, &grid, &theme);
        assert!(planned.len() >= 2, "{} slides", planned.len());
        for p in &planned {
            let rects: Vec<Rect> = p
                .elements
                .iter()
                .filter(|element| !matches!(element, Element::Shape(_)))
                .map(rect_of)
                .collect();
            assert!(!rects.is_empty());
            for (i, a) in rects.iter().enumerate() {
                assert!(inside(*a, grid.body), "{a:?} outside {:?}", grid.body);
                assert!(a.y + a.cy <= grid.footer.y, "{a:?} reaches the footer");
                for b in &rects[i + 1..] {
                    assert!(!overlap(*a, *b), "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn layouts_are_inferred_from_content() {
        let grid = widescreen();
        let theme = Theme::office();
        let layout = |slide: Slide| plan(&slide, &grid, &theme)[0].layout;
        assert_eq!(
            layout(Slide::titled("t").bullet("b")),
            Layout::TitleAndContent
        );
        assert_eq!(layout(Slide::titled("t")), Layout::TitleOnly);
        let mut subtitled = Slide::titled("t");
        subtitled.subtitle = Some("s".to_string());
        assert_eq!(layout(subtitled), Layout::Title);
        assert_eq!(layout(Slide::new()), Layout::Blank);
        assert_eq!(layout(Slide::title_slide("t", None)), Layout::Title);
        assert_eq!(
            layout(Slide::new().layout(Layout::Blank).bullet("x")),
            Layout::Blank
        );
        assert_eq!(
            layout(Slide::titled("t").block(Block::picture(png(1, 1)))),
            Layout::TitleOnly
        );
        assert_eq!(
            layout(Slide::titled("t").block(Block::bullets(["a"]))),
            Layout::TitleAndContent
        );
    }

    #[test]
    fn a_fitting_body_keeps_the_layout_position() {
        let slide = Slide::titled("t").bullet("one").bullet("two");
        let planned = plan(&slide, &widescreen(), &Theme::office());
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].layout, Layout::TitleAndContent);
        assert!(planned[0].title_size.is_none());
        assert!(matches!(
            planned[0].elements.as_slice(),
            [Element::Body {
                rect: None,
                scale: 100,
                ..
            }]
        ));
    }

    #[test]
    fn a_full_body_shrinks_then_continues() {
        let mut slide = Slide::titled("Agenda");
        for i in 0..9 {
            slide = slide.bullet(format!("Point number {i} with a few more words on it"));
        }
        let planned = plan(&slide, &widescreen(), &Theme::office());
        assert_eq!(planned.len(), 1);
        match &planned[0].elements[0] {
            Element::Body { scale, rect, .. } => {
                assert!(*scale < 100 && *scale >= 50, "scale {scale}");
                assert!(rect.is_some());
            }
            _ => panic!("body expected"),
        }
        for i in 9..40 {
            slide = slide.bullet(format!("Point number {i} with a few more words on it"));
        }
        let noted = slide.clone().notes("n");
        let planned = plan(&noted, &widescreen(), &Theme::office());
        assert!(planned.len() >= 3, "{} slides", planned.len());
        let scales: Vec<u32> = planned
            .iter()
            .map(|p| match &p.elements[0] {
                Element::Body { scale, .. } => *scale,
                _ => 0,
            })
            .collect();
        assert!(
            scales.iter().max().unwrap() - scales.iter().min().unwrap() <= SCALE_STEP,
            "{scales:?}"
        );
        assert_eq!(planned[0].notes, Some("n"));
        assert!(planned[1].notes.is_none());
        let counts: Vec<usize> = planned
            .iter()
            .map(|p| match &p.elements[0] {
                Element::Body { paragraphs, .. } => paragraphs.len(),
                _ => 0,
            })
            .collect();
        assert_eq!(counts.iter().sum::<usize>(), 40);
        let (most, least) = (*counts.iter().max().unwrap(), *counts.iter().min().unwrap());
        assert!(most - least <= 1, "unbalanced split {counts:?}");
        for p in &planned {
            assert_eq!(p.layout, Layout::TitleAndContent);
            match &p.elements[0] {
                Element::Body {
                    rect: Some(rect), ..
                } => {
                    assert!(inside(*rect, widescreen().body))
                }
                Element::Body {
                    rect: None, scale, ..
                } => assert_eq!(*scale, 100),
                _ => panic!("body expected"),
            }
        }
    }

    #[test]
    fn columns_share_the_width_and_pictures_keep_their_aspect() {
        let grid = widescreen();
        let picture = png(1600, 400);
        let slide = Slide::titled("t").columns(vec![
            Block::bullets(["left one", "left two"]),
            Block::picture(picture.clone()),
        ]);
        let planned = plan(&slide, &grid, &Theme::office());
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].layout, Layout::TitleOnly);
        let elements = &planned[0].elements;
        assert_eq!(elements.len(), 2);
        let text = rect_of(&elements[0]);
        let pic = rect_of(&elements[1]);
        assert!(matches!(elements[0], Element::TextBox { .. }));
        assert!(inside(text, grid.body) && inside(pic, grid.body));
        assert!(!overlap(text, pic));
        assert!(text.cx > pic.cx);
        assert_eq!(pic.cy, pic.cx * 400 / 1600);
        assert_eq!(text.y, pic.y);
    }

    #[test]
    fn stacked_blocks_do_not_overlap_and_tables_size_rows() {
        let grid = widescreen();
        let rows: Vec<Vec<String>> = (0..4)
            .map(|r| vec![format!("r{r}"), "x".repeat(3)])
            .collect();
        let slide = Slide::titled("t")
            .bullet("intro")
            .block(Block::table(rows.clone(), true))
            .block(Block::picture(png(100, 100)));
        let planned = plan(&slide, &grid, &Theme::office());
        assert_eq!(planned.len(), 1);
        let rects: Vec<Rect> = planned[0].elements.iter().map(rect_of).collect();
        assert_eq!(rects.len(), 3);
        for (i, a) in rects.iter().enumerate() {
            assert!(inside(*a, grid.body), "{a:?}");
            for b in &rects[i + 1..] {
                assert!(!overlap(*a, *b), "{a:?} overlaps {b:?}");
            }
        }
        match &planned[0].elements[1] {
            Element::Table {
                row_heights, rect, ..
            } => {
                assert_eq!(row_heights.len(), 4);
                assert_eq!(row_heights.iter().sum::<i64>(), rect.cy);
            }
            _ => panic!("table expected"),
        }
        assert!(matches!(planned[0].elements[0], Element::Body { .. }));
    }

    #[test]
    fn long_tables_continue_with_their_header() {
        let mut rows = vec![vec!["Name".to_string(), "Value".to_string()]];
        rows.extend((0..40).map(|i| vec![format!("row {i}"), i.to_string()]));
        let slide = Slide::titled("t").block(Block::table(rows, true));
        let planned = plan(&slide, &widescreen(), &Theme::office());
        assert!(planned.len() >= 2);
        let mut counts = Vec::new();
        for p in &planned {
            match &p.elements[0] {
                Element::Table { rows, .. } => {
                    assert_eq!(rows[0][0], "Name");
                    counts.push(rows.len() - 1);
                }
                _ => panic!("table expected"),
            }
            assert_eq!(p.layout, Layout::TitleOnly);
        }
        assert_eq!(counts.iter().sum::<usize>(), 40);
        assert!(
            counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 1,
            "{counts:?}"
        );
    }

    #[test]
    fn oversized_pictures_and_titles_are_handled() {
        let grid = widescreen();
        let slide = Slide::new().block(Block::picture(png(100, 1000)));
        let planned = plan(&slide, &grid, &Theme::office());
        assert_eq!(planned.len(), 1);
        let rect = rect_of(&planned[0].elements[0]);
        assert!(inside(rect, grid.page));
        assert_eq!(rect.cy, grid.page.cy);
        assert_eq!(planned[0].layout, Layout::Blank);

        let long = "A very long title that certainly needs more than the two lines its frame allows for at the default size";
        let slide = Slide::titled(long);
        let planned = plan(&slide, &grid, &Theme::office());
        let size = planned[0].title_size.expect("shrunk title");
        assert!((18..44).contains(&size));
        assert!(plan(&Slide::titled("Short"), &grid, &Theme::office())[0]
            .title_size
            .is_none());
    }

    #[test]
    fn fixed_shapes_stay_on_the_first_slide() {
        let slide = Slide::titled("t")
            .text_box(Rect::new(0, 0, 1, 1), vec![Paragraph::text("x")])
            .bullet("b");
        let planned = plan(&slide, &widescreen(), &Theme::office());
        assert_eq!(planned[0].elements.len(), 2);
        assert!(matches!(planned[0].elements[1], Element::Shape(_)));
    }

    #[test]
    fn explicit_sizes_scale_with_the_body() {
        let paragraphs = vec![Paragraph::text("x").size(20).space_before(10)];
        let half = scaled_paragraphs(&paragraphs, 50);
        assert_eq!(half[0].runs[0].size, Some(10));
        assert_eq!(half[0].space_before, Some(5));
        assert_eq!(scaled_paragraphs(&paragraphs, 100), paragraphs);
        assert_eq!(scaled(28, 75), 21);
        assert_eq!(Engine::balanced(250, 100), 84);
        assert_eq!(Engine::balanced(90, 100), 100);
        assert_eq!(Engine::balanced(200, 100), 100);
    }
}
