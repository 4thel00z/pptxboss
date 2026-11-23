//! The order in which extraction reads the shapes of a slide.
//!
//! PowerPoint has no structure tree, so there are two orders: the order the
//! slide part lists the shapes in, and the order of their position on the
//! slide.

use std::fmt;
use std::str::FromStr;

use crate::error::Error;
use crate::model::{
    ChildSpace, Content, Placeholder, PlaceholderKind, Shape, SlideContent, Transform,
};

/// The order shapes are read in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReadingOrder {
    /// The order the slide part lists the shapes in, groups expanded in place.
    #[default]
    Content,
    /// Position alone: rows from top to bottom, shapes in a row from left to right.
    Geometric,
}

impl ReadingOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            ReadingOrder::Content => "content",
            ReadingOrder::Geometric => "geometric",
        }
    }
}

impl fmt::Display for ReadingOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReadingOrder {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Error> {
        match value {
            "content" => Ok(ReadingOrder::Content),
            "geometric" => Ok(ReadingOrder::Geometric),
            other => Err(Error::Other(format!(
                "unknown reading order {other:?}: expected 'content' or 'geometric'"
            ))),
        }
    }
}

/// A shape's bounds on the slide in EMU, rotation and flips ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub cx: f64,
    pub cy: f64,
}

impl Frame {
    fn compute_bottom(&self) -> f64 {
        self.y + self.cy
    }
}

impl From<&Transform> for Frame {
    fn from(transform: &Transform) -> Self {
        Frame {
            x: transform.x as f64,
            y: transform.y as f64,
            cx: transform.cx.max(0) as f64,
            cy: transform.cy.max(0) as f64,
        }
    }
}

/// Maps the coordinates inside a group onto the slide: `slide = offset + scale * local`.
#[derive(Clone, Copy, Debug)]
struct Space {
    dx: f64,
    dy: f64,
    sx: f64,
    sy: f64,
}

const SLIDE_SPACE: Space = Space {
    dx: 0.0,
    dy: 0.0,
    sx: 1.0,
    sy: 1.0,
};

impl Space {
    fn place_frame(self, transform: &Transform) -> Frame {
        let local = Frame::from(transform);
        Frame {
            x: self.dx + self.sx * local.x,
            y: self.dy + self.sy * local.y,
            cx: self.sx * local.cx,
            cy: self.sy * local.cy,
        }
    }

    /// The space of a group's children. A group without both a frame and a
    /// child space keeps its children in the enclosing space; a child
    /// extent of zero keeps the enclosing scale.
    fn enter(self, group: Option<&Transform>, children: Option<&ChildSpace>) -> Space {
        let (Some(group), Some(children)) = (group, children) else {
            return self;
        };
        let frame = self.place_frame(group);
        let scale = |outer: f64, inner: i64, fallback: f64| match inner > 0 {
            true => outer / inner as f64,
            false => fallback,
        };
        let sx = scale(frame.cx, children.cx, self.sx);
        let sy = scale(frame.cy, children.cy, self.sy);
        Space {
            dx: frame.x - children.x as f64 * sx,
            dy: frame.y - children.y as f64 * sy,
            sx,
            sy,
        }
    }
}

/// Every shape that is not a group, in content order, with its frame on
/// the slide when its own transform gives one. Groups are flattened:
/// their children are mapped through the group's child space.
pub(crate) fn collect_leaf_frames(shapes: &[Shape]) -> Vec<(&Shape, Option<Frame>)> {
    let mut out = Vec::new();
    let mut stack = vec![(shapes.iter(), SLIDE_SPACE)];
    while let Some((iter, space)) = stack.last_mut() {
        let space = *space;
        let Some(shape) = iter.next() else {
            stack.pop();
            continue;
        };
        match &shape.content {
            Content::Group(children, child_space) => {
                let inner = space.enter(shape.transform.as_ref(), child_space.as_ref());
                stack.push((children.iter(), inner));
            }
            _ => out.push((
                shape,
                shape.transform.as_ref().map(|t| space.place_frame(t)),
            )),
        }
    }
    out
}

/// The shapes of `content` that pass `keep`, in geometric order. A shape
/// without a transform of its own asks `inherited` for its frame.
pub(crate) fn sort_geometric_shapes<'a>(
    content: &'a SlideContent,
    inherited: &dyn Fn(&Shape) -> Option<Frame>,
    keep: &dyn Fn(&Shape) -> bool,
) -> Vec<(&'a Shape, bool)> {
    let entries = collect_leaf_frames(&content.shapes)
        .into_iter()
        .filter(|(shape, _)| keep(shape))
        .map(|(shape, frame)| (shape, frame.or_else(|| inherited(shape))))
        .collect();
    sort_geometric(entries)
}

/// The frames of the placeholders of a layout or master.
pub type PlaceholderFrames = Vec<(Placeholder, Frame)>;

/// The placeholders of a layout or master with the frames they carry,
/// groups mapped like on a slide.
pub(crate) fn collect_placeholder_frames(shapes: &[Shape]) -> PlaceholderFrames {
    collect_leaf_frames(shapes)
        .into_iter()
        .filter_map(|(shape, frame)| Some((shape.placeholder.clone()?, frame?)))
        .collect()
}

fn classify_placeholder(kind: PlaceholderKind) -> &'static str {
    match kind {
        PlaceholderKind::Title | PlaceholderKind::CenterTitle => "title",
        PlaceholderKind::DateTime
        | PlaceholderKind::SlideNumber
        | PlaceholderKind::Footer
        | PlaceholderKind::Header
        | PlaceholderKind::SlideImage => kind.as_str(),
        _ => "body",
    }
}

/// The frame of the placeholder `declared` among `candidates`: same index
/// and type first, then same index when the index is not 0, then the same
/// type, then the same family (titles; body, subtitle and content
/// placeholders). `by_index` is false for a master, whose indexes carry no
/// meaning for the slides below it.
pub(crate) fn match_placeholder(
    candidates: &[(Placeholder, Frame)],
    declared: &Placeholder,
    by_index: bool,
) -> Option<Frame> {
    let find = |test: &dyn Fn(&Placeholder) -> bool| {
        candidates
            .iter()
            .find(|(ph, _)| test(ph))
            .map(|(_, frame)| *frame)
    };
    if by_index {
        let exact = find(&|ph| ph.idx == declared.idx && ph.kind == declared.kind);
        if exact.is_some() {
            return exact;
        }
        let indexed = find(&|ph| declared.idx != 0 && ph.idx == declared.idx);
        if indexed.is_some() {
            return indexed;
        }
    }
    let same_kind = find(&|ph| ph.kind == declared.kind);
    if same_kind.is_some() {
        return same_kind;
    }
    find(&|ph| classify_placeholder(ph.kind) == classify_placeholder(declared.kind))
}

/// Shapes in rows from top to bottom, each row from left to right. A shape
/// joins the current row when it covers at least half of the shorter of
/// itself and the row's shortest shape; the flag is true for the first
/// shape of a row. Shapes without a frame follow in content order, one row
/// each.
pub(crate) fn sort_geometric(shapes: Vec<(&Shape, Option<Frame>)>) -> Vec<(&Shape, bool)> {
    let mut positioned: Vec<(usize, &Shape, Frame)> = Vec::new();
    let mut loose: Vec<&Shape> = Vec::new();
    for (index, (shape, frame)) in shapes.into_iter().enumerate() {
        match frame {
            Some(frame) => positioned.push((index, shape, frame)),
            None => loose.push(shape),
        }
    }
    positioned.sort_by(|a, b| {
        a.2.y
            .total_cmp(&b.2.y)
            .then(a.2.x.total_cmp(&b.2.x))
            .then(a.0.cmp(&b.0))
    });
    let mut rows: Vec<Row> = Vec::new();
    for item in positioned {
        match rows.last_mut() {
            Some(row) if row.accepts(&item.2) => row.add(item),
            _ => rows.push(Row::new(item)),
        }
    }
    let mut out = Vec::new();
    for mut row in rows {
        row.members.sort_by(|a, b| {
            a.2.x
                .total_cmp(&b.2.x)
                .then(a.2.y.total_cmp(&b.2.y))
                .then(a.0.cmp(&b.0))
        });
        out.extend(
            row.members
                .into_iter()
                .enumerate()
                .map(|(position, (_, shape, _))| (shape, position == 0)),
        );
    }
    out.extend(loose.into_iter().map(|shape| (shape, true)));
    out
}

struct Row<'a> {
    top: f64,
    bottom: f64,
    members: Vec<(usize, &'a Shape, Frame)>,
}

impl<'a> Row<'a> {
    fn new(item: (usize, &'a Shape, Frame)) -> Self {
        Row {
            top: item.2.y,
            bottom: item.2.compute_bottom(),
            members: vec![item],
        }
    }

    fn accepts(&self, frame: &Frame) -> bool {
        let overlap = self.bottom.min(frame.compute_bottom()) - self.top.max(frame.y);
        let shorter = (self.bottom - self.top).min(frame.cy);
        overlap >= 0.0 && overlap * 2.0 >= shorter
    }

    fn add(&mut self, item: (usize, &'a Shape, Frame)) {
        if item.2.cy < self.bottom - self.top {
            self.top = item.2.y;
            self.bottom = item.2.compute_bottom();
        }
        self.members.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Placeholder, TextBody};

    fn make_transform(x: i64, y: i64, cx: i64, cy: i64) -> Transform {
        Transform {
            x,
            y,
            cx,
            cy,
            ..Transform::default()
        }
    }

    fn make_shape(id: u32, frame: Option<Transform>) -> Shape {
        Shape {
            id,
            name: String::new(),
            hidden: false,
            description: None,
            hyperlink: None,
            placeholder: None,
            transform: frame,
            text_box: false,
            content: Content::Text(TextBody::default()),
        }
    }

    fn make_group(
        id: u32,
        frame: Option<Transform>,
        space: Option<ChildSpace>,
        kids: Vec<Shape>,
    ) -> Shape {
        Shape {
            content: Content::Group(kids, space),
            ..make_shape(id, frame)
        }
    }

    fn list_ids(shapes: &[Shape]) -> Vec<u32> {
        sort_geometric(collect_leaf_frames(shapes))
            .into_iter()
            .map(|(shape, _)| shape.id)
            .collect()
    }

    #[test]
    fn parses_and_prints_names() {
        assert_eq!(
            "content".parse::<ReadingOrder>().unwrap(),
            ReadingOrder::Content
        );
        assert_eq!(
            "geometric".parse::<ReadingOrder>().unwrap().to_string(),
            "geometric"
        );
        let err = "structure-tree"
            .parse::<ReadingOrder>()
            .unwrap_err()
            .to_string();
        assert!(err.contains("'content' or 'geometric'"), "{err}");
    }

    #[test]
    fn rows_read_left_to_right_then_down() {
        let shapes = vec![
            make_shape(1, Some(make_transform(0, 0, 100, 50))),
            make_shape(2, Some(make_transform(0, 100, 100, 50))),
            make_shape(3, Some(make_transform(200, 110, 100, 50))),
            make_shape(4, Some(make_transform(200, 5, 100, 50))),
        ];
        assert_eq!(list_ids(&shapes), [1, 4, 2, 3]);
    }

    #[test]
    fn a_tall_shape_does_not_merge_the_rows_beside_it() {
        let shapes = vec![
            make_shape(1, Some(make_transform(0, 0, 100, 1000))),
            make_shape(2, Some(make_transform(200, 100, 100, 100))),
            make_shape(3, Some(make_transform(400, 100, 100, 100))),
            make_shape(4, Some(make_transform(200, 400, 100, 100))),
            make_shape(5, Some(make_transform(400, 400, 100, 100))),
        ];
        assert_eq!(list_ids(&shapes), [1, 2, 3, 4, 5]);
        let rows: Vec<bool> = sort_geometric(collect_leaf_frames(&shapes))
            .into_iter()
            .map(|(_, starts)| starts)
            .collect();
        assert_eq!(rows, [true, false, false, true, false]);
    }

    #[test]
    fn shapes_without_a_frame_follow_in_content_order() {
        let shapes = vec![
            make_shape(1, None),
            make_shape(2, Some(make_transform(0, 500, 10, 10))),
            make_shape(3, None),
            make_shape(4, Some(make_transform(0, 0, 10, 10))),
        ];
        let order = sort_geometric(collect_leaf_frames(&shapes));
        let got: Vec<(u32, bool)> = order.iter().map(|(s, starts)| (s.id, *starts)).collect();
        assert_eq!(got, [(4, true), (2, true), (1, true), (3, true)]);
    }

    #[test]
    fn group_children_map_through_the_child_space() {
        let kids = vec![
            make_shape(10, Some(make_transform(1000, 1000, 100, 100))),
            make_shape(11, Some(make_transform(1000, 3000, 100, 100))),
        ];
        let shapes = vec![
            make_shape(1, Some(make_transform(0, 1500, 10, 10))),
            make_group(
                2,
                Some(make_transform(0, 1000, 500, 500)),
                Some(ChildSpace {
                    x: 1000,
                    y: 1000,
                    cx: 1000,
                    cy: 4000,
                }),
                kids,
            ),
        ];
        let frames = collect_leaf_frames(&shapes);
        let child = frames.iter().find(|(s, _)| s.id == 11).unwrap().1.unwrap();
        assert_eq!(child.y, 1000.0 + 2000.0 * 0.125);
        assert_eq!(child.cx, 50.0);
        assert_eq!(list_ids(&shapes), [10, 11, 1]);
    }

    #[test]
    fn group_math_survives_zero_and_huge_values() {
        let kids = vec![make_shape(
            10,
            Some(make_transform(i64::MAX, i64::MIN, i64::MAX, -5)),
        )];
        let shapes = vec![make_group(
            2,
            Some(make_transform(i64::MAX, i64::MAX, i64::MAX, 0)),
            Some(ChildSpace {
                x: i64::MIN,
                y: 0,
                cx: 0,
                cy: 0,
            }),
            kids,
        )];
        assert_eq!(list_ids(&shapes), [10]);
        let nested = vec![make_group(
            1,
            None,
            Some(ChildSpace::default()),
            vec![make_shape(2, None)],
        )];
        assert_eq!(list_ids(&nested), [2]);
    }

    #[test]
    fn placeholders_match_by_index_then_type() {
        let ph = |kind, idx| Placeholder { kind, idx };
        let frame = |y| Frame {
            x: 0.0,
            y,
            cx: 1.0,
            cy: 1.0,
        };
        let candidates = vec![
            (ph(PlaceholderKind::Title, 0), frame(1.0)),
            (ph(PlaceholderKind::Body, 1), frame(2.0)),
            (ph(PlaceholderKind::Body, 2), frame(3.0)),
        ];
        let find = |kind, idx, by_index| {
            match_placeholder(&candidates, &ph(kind, idx), by_index).map(|f| f.y)
        };
        assert_eq!(find(PlaceholderKind::Body, 2, true), Some(3.0));
        assert_eq!(find(PlaceholderKind::Object, 2, true), Some(3.0));
        assert_eq!(find(PlaceholderKind::CenterTitle, 0, true), Some(1.0));
        assert_eq!(find(PlaceholderKind::Subtitle, 7, true), Some(2.0));
        assert_eq!(find(PlaceholderKind::Object, 2, false), Some(2.0));
        assert_eq!(find(PlaceholderKind::SlideNumber, 4, true), None);
    }
}
