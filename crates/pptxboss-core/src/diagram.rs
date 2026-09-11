//! SmartArt text: the data model of a DrawingML diagram (ECMA-376 Part 1,
//! clause 21.4). Points of type `doc` root the tree, untyped points are
//! the nodes, `asst` points are assistants; presentation, parent- and
//! sibling-transition points carry layout only. Untyped connections
//! (`parOf`) give the hierarchy and `srcOrd` the order.

use crate::hash::FastMap;
use crate::mce::children;
use crate::slide::parse_text_body;
use crate::xml::{unescape_attr, Event, Ns, Reader, XmlError};

/// One text-bearing node of a diagram, in reading order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramItem {
    /// Depth below the root: 0 for top-level nodes.
    pub level: u8,
    /// Paragraphs joined by `\n`, trimmed.
    pub text: String,
}

/// The text of a diagram, flattened depth-first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagramData {
    pub items: Vec<DiagramItem>,
}

impl DiagramData {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// One line per node.
    pub fn write_text(&self, out: &mut String) {
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            out.push_str(&item.text);
        }
    }
}

struct Point {
    kind: Kind,
    text: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Doc,
    Node,
    Assistant,
    Layout,
}

/// Parses a diagram data part (`dgm:dataModel`).
pub fn parse_diagram(xml: &[u8]) -> Result<DiagramData, XmlError> {
    let mut reader = Reader::new(xml);
    let found_root = loop {
        match reader.next()? {
            Event::Start(_) => break true,
            Event::Eof => break false,
            _ => {}
        }
    };
    if !found_root {
        return Ok(DiagramData::default());
    }
    let mut points: Vec<(String, Point)> = Vec::new();
    let mut edges: Vec<(String, String, u32)> = Vec::new();
    children(&mut reader, &mut |reader, child| {
        if child.name.is(Ns::Dgm, b"ptLst") {
            return children(reader, &mut |reader, pt| {
                if !pt.name.is(Ns::Dgm, b"pt") {
                    return reader.skip_element();
                }
                let id = reader
                    .attr(&pt, Ns::None, b"modelId")
                    .map(unescape_attr)
                    .unwrap_or_default();
                let kind = match reader.attr(&pt, Ns::None, b"type") {
                    None | Some(b"node") => Kind::Node,
                    Some(b"doc") => Kind::Doc,
                    Some(b"asst") => Kind::Assistant,
                    Some(_) => Kind::Layout,
                };
                let mut text = String::new();
                children(
                    reader,
                    &mut |reader, part| match part.name.is(Ns::Dgm, b"t") {
                        true => {
                            text = parse_text_body(reader)?.text();
                            Ok(())
                        }
                        false => reader.skip_element(),
                    },
                )?;
                points.push((
                    id,
                    Point {
                        kind,
                        text: text.trim().to_string(),
                    },
                ));
                Ok(())
            });
        }
        if child.name.is(Ns::Dgm, b"cxnLst") {
            return children(reader, &mut |reader, cxn| {
                if !cxn.name.is(Ns::Dgm, b"cxn") {
                    return reader.skip_element();
                }
                let hierarchical =
                    matches!(reader.attr(&cxn, Ns::None, b"type"), None | Some(b"parOf"));
                if hierarchical {
                    let source = reader.attr(&cxn, Ns::None, b"srcId").map(unescape_attr);
                    let target = reader.attr(&cxn, Ns::None, b"destId").map(unescape_attr);
                    let order = reader
                        .attr(&cxn, Ns::None, b"srcOrd")
                        .and_then(|raw| std::str::from_utf8(raw).ok()?.trim().parse().ok())
                        .unwrap_or(u32::MAX);
                    if let (Some(source), Some(target)) = (source, target) {
                        edges.push((source, target, order));
                    }
                }
                reader.skip_element()
            });
        }
        reader.skip_element()
    })?;
    Ok(flatten(points, edges))
}

/// Depth-first order from the `doc` root; nodes the tree never reaches
/// follow in document order at level 0.
fn flatten(points: Vec<(String, Point)>, edges: Vec<(String, String, u32)>) -> DiagramData {
    let index: FastMap<&str, usize> = points
        .iter()
        .enumerate()
        .map(|(position, (id, _))| (id.as_str(), position))
        .collect();
    let mut children_of: FastMap<usize, Vec<(u32, usize)>> = FastMap::default();
    for (source, target, order) in &edges {
        if let (Some(&from), Some(&to)) = (index.get(source.as_str()), index.get(target.as_str())) {
            children_of.entry(from).or_default().push((*order, to));
        }
    }
    for list in children_of.values_mut() {
        list.sort();
    }
    let mut visited = vec![false; points.len()];
    let mut items = Vec::new();
    let roots: Vec<usize> = points
        .iter()
        .enumerate()
        .filter(|(_, (_, point))| point.kind == Kind::Doc)
        .map(|(position, _)| position)
        .collect();
    for root in roots {
        visited[root] = true;
        let mut stack: Vec<(usize, u8)> = children_of
            .get(&root)
            .map(|list| list.iter().rev().map(|(_, child)| (*child, 0u8)).collect())
            .unwrap_or_default();
        while let Some((position, level)) = stack.pop() {
            if visited[position] {
                continue;
            }
            visited[position] = true;
            let point = &points[position].1;
            let mut next_level = level;
            if point.kind != Kind::Layout {
                if !point.text.is_empty() {
                    items.push(DiagramItem {
                        level,
                        text: point.text.clone(),
                    });
                }
                next_level = level.saturating_add(1);
            }
            if let Some(list) = children_of.get(&position) {
                for (_, child) in list.iter().rev() {
                    stack.push((*child, next_level));
                }
            }
        }
    }
    for (position, (_, point)) in points.iter().enumerate() {
        if visited[position] || point.kind == Kind::Layout || point.text.is_empty() {
            continue;
        }
        items.push(DiagramItem {
            level: 0,
            text: point.text.clone(),
        });
    }
    DiagramData { items }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DGM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
    const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

    fn pt(id: &str, kind: &str, text: &str) -> String {
        let attr = match kind.is_empty() {
            true => String::new(),
            false => format!(r#" type="{kind}""#),
        };
        format!(
            r#"<dgm:pt modelId="{id}"{attr}><dgm:prSet/><dgm:spPr/><dgm:t><a:bodyPr/><a:p><a:r><a:t>{text}</a:t></a:r></a:p></dgm:t></dgm:pt>"#
        )
    }

    #[test]
    fn nodes_follow_the_hierarchy_in_source_order() {
        let xml = format!(
            r#"<dgm:dataModel xmlns:dgm="{DGM}" xmlns:a="{A}"><dgm:ptLst>{}{}{}{}{}{}{}</dgm:ptLst><dgm:cxnLst><dgm:cxn modelId="c1" srcId="doc" destId="b" srcOrd="1" destOrd="0"/><dgm:cxn modelId="c2" srcId="doc" destId="a" srcOrd="0" destOrd="0"/><dgm:cxn modelId="c3" srcId="a" destId="a1" srcOrd="0" destOrd="0"/><dgm:cxn modelId="c4" type="presOf" srcId="a" destId="pres1" srcOrd="0" destOrd="0"/><dgm:cxn modelId="c5" srcId="a" destId="asst" srcOrd="1" destOrd="0"/></dgm:cxnLst></dgm:dataModel>"#,
            pt("doc", "doc", ""),
            pt("a", "", "Alpha"),
            pt("b", "", "Beta"),
            pt("a1", "", "Alpha one"),
            pt("asst", "asst", "Helper"),
            pt("pres1", "pres", "layout only"),
            pt("orphan", "", "Loose end"),
        );
        let diagram = parse_diagram(xml.as_bytes()).unwrap();
        let flat: Vec<(u8, &str)> = diagram
            .items
            .iter()
            .map(|item| (item.level, item.text.as_str()))
            .collect();
        assert_eq!(
            flat,
            [
                (0, "Alpha"),
                (1, "Alpha one"),
                (1, "Helper"),
                (0, "Beta"),
                (0, "Loose end")
            ]
        );
        let mut text = String::new();
        diagram.write_text(&mut text);
        assert_eq!(text, "Alpha\nAlpha one\nHelper\nBeta\nLoose end");
    }

    #[test]
    fn a_model_without_connections_lists_nodes_in_document_order() {
        let xml = format!(
            r#"<dgm:dataModel xmlns:dgm="{DGM}" xmlns:a="{A}"><dgm:ptLst>{}{}{}</dgm:ptLst></dgm:dataModel>"#,
            pt("x", "", "One"),
            pt("t", "parTrans", "skip"),
            pt("y", "", "Two"),
        );
        let diagram = parse_diagram(xml.as_bytes()).unwrap();
        assert_eq!(diagram.items.len(), 2);
        assert_eq!(diagram.items[1].text, "Two");
        assert!(parse_diagram(b"").unwrap().is_empty());
    }
}
