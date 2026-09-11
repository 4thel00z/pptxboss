//! Chart text: the title, axis titles, series names, categories and
//! values cached in a DrawingML chart part (ECMA-376 Part 1, clause 21.2)
//! or in the 2014 extended chart part (`cx:chartSpace`). Values come back
//! as written; nothing is computed or formatted.

use crate::hash::FastMap;
use crate::mce::children;
use crate::slide::parse_text_body;
use crate::xml::{unescape_attr, Event, Ns, Reader, Start, XmlError};

/// One series of a chart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Series {
    pub name: Option<String>,
    /// Category labels (`c:cat`, or `c:xVal` for scatter and bubble charts), in point order.
    pub categories: Vec<String>,
    /// Values (`c:val`, or `c:yVal`) as written, in point order.
    pub values: Vec<String>,
}

/// What a chart part says in words and numbers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChartData {
    pub title: Option<String>,
    /// The chart type elements found, e.g. `barChart`, `lineChart`; extended charts give their layout ids.
    pub kinds: Vec<String>,
    pub category_axis_title: Option<String>,
    pub value_axis_title: Option<String>,
    pub series: Vec<Series>,
}

impl ChartData {
    /// True when the chart carries no words or numbers at all.
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.series.iter().all(|series| {
                series.name.is_none() && series.categories.is_empty() && series.values.is_empty()
            })
    }

    /// True when every series shares the same category labels.
    pub fn shares_categories(&self) -> bool {
        let Some(first) = self.series.first() else {
            return false;
        };
        !first.categories.is_empty()
            && self
                .series
                .iter()
                .all(|series| series.categories == first.categories)
    }

    /// Row-major cells of the chart as a table: a header of series names
    /// (first cell empty) when any series is named, then one row per
    /// category with each series' value. Series with differing categories
    /// fall back to one row per series: name, then its values.
    pub fn rows(&self) -> Vec<Vec<String>> {
        let mut rows = Vec::new();
        if self.shares_categories() {
            if self.series.iter().any(|series| series.name.is_some()) {
                let mut header = vec![String::new()];
                header.extend(
                    self.series
                        .iter()
                        .map(|series| series.name.clone().unwrap_or_default()),
                );
                rows.push(header);
            }
            for (index, category) in self.series[0].categories.iter().enumerate() {
                let mut row = vec![category.clone()];
                row.extend(
                    self.series
                        .iter()
                        .map(|series| series.values.get(index).cloned().unwrap_or_default()),
                );
                rows.push(row);
            }
            return rows;
        }
        for series in &self.series {
            let mut row = vec![series.name.clone().unwrap_or_default()];
            match series.categories.is_empty() {
                true => row.extend(series.values.iter().cloned()),
                false => row.extend(
                    series
                        .categories
                        .iter()
                        .zip(
                            series
                                .values
                                .iter()
                                .chain(std::iter::repeat(&String::new())),
                        )
                        .map(|(category, value)| format!("{category}: {value}")),
                ),
            }
            rows.push(row);
        }
        rows
    }

    /// The chart as text: the title, then the table rows with cells separated by `separator`.
    pub fn write_text(&self, separator: &str, out: &mut String) {
        let start = out.len();
        if let Some(title) = &self.title {
            out.push_str(title.trim());
        }
        for row in self.rows() {
            if out.len() > start {
                out.push('\n');
            }
            out.push_str(&row.join(separator));
        }
    }
}

/// Parses a chart part of either flavour.
pub fn parse_chart(xml: &[u8]) -> Result<ChartData, XmlError> {
    let mut reader = Reader::new(xml);
    let root = loop {
        match reader.next()? {
            Event::Start(start) => break start,
            Event::Eof => return Ok(ChartData::default()),
            _ => {}
        }
    };
    let mut chart = ChartData::default();
    match root.name.ns {
        Ns::ChartEx => parse_extended(&mut reader, &mut chart)?,
        _ => parse_chart_space(&mut reader, &root, &mut chart)?,
    }
    Ok(chart)
}

/// `c:chartSpace` (or a bare `c:chart`): title, plot area, axes.
fn parse_chart_space<'a>(
    reader: &mut Reader<'a>,
    root: &Start<'a>,
    chart: &mut ChartData,
) -> Result<(), XmlError> {
    if root.name.is(Ns::Chart, b"chart") {
        return parse_c_chart(reader, chart);
    }
    children(
        reader,
        &mut |reader, child| match child.name.is(Ns::Chart, b"chart") {
            true => parse_c_chart(reader, chart),
            false => reader.skip_element(),
        },
    )
}

fn parse_c_chart<'a>(reader: &mut Reader<'a>, chart: &mut ChartData) -> Result<(), XmlError> {
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Chart {
            return reader.skip_element();
        }
        match child.name.local {
            b"title" => {
                chart.title = title_text(reader)?;
                Ok(())
            }
            b"plotArea" => parse_plot_area(reader, chart),
            _ => reader.skip_element(),
        }
    })
}

fn parse_plot_area<'a>(reader: &mut Reader<'a>, chart: &mut ChartData) -> Result<(), XmlError> {
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Chart {
            return reader.skip_element();
        }
        let local = String::from_utf8_lossy(child.name.local).into_owned();
        if local.ends_with("Chart") {
            chart.kinds.push(local);
            return children(
                reader,
                &mut |reader, item| match item.name.is(Ns::Chart, b"ser") {
                    true => {
                        let series = parse_series(reader)?;
                        chart.series.push(series);
                        Ok(())
                    }
                    false => reader.skip_element(),
                },
            );
        }
        match child.name.local {
            b"catAx" | b"dateAx" | b"serAx" => {
                let title = axis_title(reader)?;
                if chart.category_axis_title.is_none() {
                    chart.category_axis_title = title;
                }
                Ok(())
            }
            b"valAx" => {
                let title = axis_title(reader)?;
                if chart.value_axis_title.is_none() {
                    chart.value_axis_title = title;
                }
                Ok(())
            }
            _ => reader.skip_element(),
        }
    })
}

fn axis_title<'a>(reader: &mut Reader<'a>) -> Result<Option<String>, XmlError> {
    let mut title = None;
    children(
        reader,
        &mut |reader, child| match child.name.is(Ns::Chart, b"title") {
            true => {
                title = title_text(reader)?;
                Ok(())
            }
            false => reader.skip_element(),
        },
    )?;
    Ok(title)
}

/// The text of a `c:title`: rich text under `c:tx/c:rich`, or a cached string reference.
fn title_text<'a>(reader: &mut Reader<'a>) -> Result<Option<String>, XmlError> {
    let mut text = None;
    children(reader, &mut |reader, child| {
        if !child.name.is(Ns::Chart, b"tx") {
            return reader.skip_element();
        }
        children(reader, &mut |reader, inner| {
            if inner.name.is(Ns::Chart, b"rich") {
                let body = parse_text_body(reader)?.text();
                if !body.trim().is_empty() {
                    text = Some(body);
                }
                return Ok(());
            }
            if inner.name.is(Ns::Chart, b"strRef") {
                let points = string_points(reader)?;
                let joined = points.join(" ");
                if !joined.trim().is_empty() {
                    text = Some(joined);
                }
                return Ok(());
            }
            reader.skip_element()
        })
    })?;
    Ok(text)
}

fn parse_series<'a>(reader: &mut Reader<'a>) -> Result<Series, XmlError> {
    let mut series = Series::default();
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Chart {
            return reader.skip_element();
        }
        match child.name.local {
            b"tx" => {
                let points = data_points(reader)?;
                series.name = Some(points.join(" ")).filter(|name| !name.trim().is_empty());
                Ok(())
            }
            b"cat" | b"xVal" => {
                series.categories = data_points(reader)?;
                Ok(())
            }
            b"val" | b"yVal" => {
                series.values = data_points(reader)?;
                Ok(())
            }
            _ => reader.skip_element(),
        }
    })?;
    Ok(series)
}

/// The cached points of `c:tx`, `c:cat`, `c:val` and friends: a literal
/// `c:v`, or the `c:pt/c:v` children of a string or number reference or
/// literal, in `idx` order.
fn data_points<'a>(reader: &mut Reader<'a>) -> Result<Vec<String>, XmlError> {
    let mut points = Vec::new();
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Chart {
            return reader.skip_element();
        }
        match child.name.local {
            b"v" => {
                let mut text = String::new();
                reader.text_content(&mut text)?;
                points.push(text);
                Ok(())
            }
            b"strRef" | b"numRef" | b"strLit" | b"numLit" | b"multiLvlStrRef" => {
                points.extend(string_points(reader)?);
                Ok(())
            }
            _ => reader.skip_element(),
        }
    })?;
    Ok(points)
}

/// The `c:pt/c:v` values under a reference or cache, in `idx` order; for
/// multi-level string caches the innermost level is taken.
fn string_points<'a>(reader: &mut Reader<'a>) -> Result<Vec<String>, XmlError> {
    let mut indexed: Vec<(u32, String)> = Vec::new();
    let mut level = 0usize;
    let mut kept_level = 0usize;
    collect_points(reader, &mut indexed, &mut level, &mut kept_level)?;
    indexed.sort_by_key(|(index, _)| *index);
    Ok(indexed.into_iter().map(|(_, value)| value).collect())
}

fn collect_points<'a>(
    reader: &mut Reader<'a>,
    indexed: &mut Vec<(u32, String)>,
    level: &mut usize,
    kept_level: &mut usize,
) -> Result<(), XmlError> {
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::Chart {
            return reader.skip_element();
        }
        match child.name.local {
            b"pt" => {
                let index = reader
                    .attr(&child, Ns::None, b"idx")
                    .and_then(|raw| std::str::from_utf8(raw).ok()?.trim().parse().ok())
                    .unwrap_or(indexed.len() as u32);
                let mut value = String::new();
                children(
                    reader,
                    &mut |reader, inner| match inner.name.is(Ns::Chart, b"v") {
                        true => reader.text_content(&mut value),
                        false => reader.skip_element(),
                    },
                )?;
                if *level == *kept_level {
                    indexed.push((index, value));
                }
                Ok(())
            }
            b"lvl" => {
                *level += 1;
                if *level > *kept_level {
                    *kept_level = *level;
                    indexed.clear();
                }
                collect_points(reader, indexed, level, kept_level)?;
                *level -= 1;
                Ok(())
            }
            b"f" | b"ptCount" | b"formatCode" | b"extLst" => reader.skip_element(),
            _ => collect_points(reader, indexed, level, kept_level),
        }
    })
}

/// `cx:chartSpace`: the 2014 extended charts (treemap, sunburst, waterfall,
/// histogram, funnel, box and whisker, region maps).
fn parse_extended<'a>(reader: &mut Reader<'a>, chart: &mut ChartData) -> Result<(), XmlError> {
    let mut data: FastMap<String, (Vec<String>, Vec<String>)> = FastMap::default();
    let mut order: Vec<String> = Vec::new();
    children(reader, &mut |reader, child| {
        if child.name.ns != Ns::ChartEx {
            return reader.skip_element();
        }
        match child.name.local {
            b"chartData" => children(reader, &mut |reader, item| {
                if !item.name.is(Ns::ChartEx, b"data") {
                    return reader.skip_element();
                }
                let id = reader
                    .attr(&item, Ns::None, b"id")
                    .map(unescape_attr)
                    .unwrap_or_default();
                let mut categories = Vec::new();
                let mut values = Vec::new();
                children(reader, &mut |reader, dim| {
                    if dim.name.is(Ns::ChartEx, b"strDim") {
                        categories = extended_points(reader)?;
                        return Ok(());
                    }
                    if dim.name.is(Ns::ChartEx, b"numDim") && values.is_empty() {
                        values = extended_points(reader)?;
                        return Ok(());
                    }
                    reader.skip_element()
                })?;
                order.push(id.clone());
                data.insert(id, (categories, values));
                Ok(())
            }),
            b"chart" => children(reader, &mut |reader, item| {
                if item.name.is(Ns::ChartEx, b"title") {
                    chart.title = extended_text(reader)?;
                    return Ok(());
                }
                if !item.name.is(Ns::ChartEx, b"plotArea") {
                    return reader.skip_element();
                }
                children(reader, &mut |reader, region| {
                    if !region.name.is(Ns::ChartEx, b"plotAreaRegion") {
                        return reader.skip_element();
                    }
                    children(reader, &mut |reader, series| {
                        if !series.name.is(Ns::ChartEx, b"series") {
                            return reader.skip_element();
                        }
                        if let Some(layout) = reader.attr(&series, Ns::None, b"layoutId") {
                            chart.kinds.push(unescape_attr(layout));
                        }
                        let mut item = Series::default();
                        let mut data_id = None;
                        children(reader, &mut |reader, part| {
                            if part.name.is(Ns::ChartEx, b"tx") {
                                item.name = extended_text(reader)?;
                                return Ok(());
                            }
                            if part.name.is(Ns::ChartEx, b"dataId") {
                                data_id = reader.attr(&part, Ns::None, b"val").map(unescape_attr);
                            }
                            reader.skip_element()
                        })?;
                        if let Some((categories, values)) =
                            data_id.as_deref().and_then(|id| data.get(id))
                        {
                            item.categories = categories.clone();
                            item.values = values.clone();
                        }
                        chart.series.push(item);
                        Ok(())
                    })
                })
            }),
            _ => reader.skip_element(),
        }
    })?;
    if chart.series.is_empty() {
        for id in order {
            if let Some((categories, values)) = data.remove(&id) {
                chart.series.push(Series {
                    name: None,
                    categories,
                    values,
                });
            }
        }
    }
    Ok(())
}

/// `cx:tx`: `cx:txData/cx:v` or rich text.
fn extended_text<'a>(reader: &mut Reader<'a>) -> Result<Option<String>, XmlError> {
    let mut text = None;
    children(reader, &mut |reader, child| {
        if child.name.is(Ns::ChartEx, b"tx") {
            return children(reader, &mut |reader, inner| {
                if inner.name.is(Ns::ChartEx, b"txData") {
                    return children(reader, &mut |reader, leaf| match leaf
                        .name
                        .is(Ns::ChartEx, b"v")
                    {
                        true => {
                            let mut value = String::new();
                            reader.text_content(&mut value)?;
                            text = Some(value).filter(|value| !value.trim().is_empty());
                            Ok(())
                        }
                        false => reader.skip_element(),
                    });
                }
                if inner.name.is(Ns::ChartEx, b"rich") {
                    let body = parse_text_body(reader)?.text();
                    text = Some(body).filter(|body| !body.trim().is_empty());
                    return Ok(());
                }
                reader.skip_element()
            });
        }
        if child.name.is(Ns::ChartEx, b"txData") {
            return children(
                reader,
                &mut |reader, leaf| match leaf.name.is(Ns::ChartEx, b"v") {
                    true => {
                        let mut value = String::new();
                        reader.text_content(&mut value)?;
                        text = Some(value).filter(|value| !value.trim().is_empty());
                        Ok(())
                    }
                    false => reader.skip_element(),
                },
            );
        }
        if child.name.is(Ns::ChartEx, b"rich") {
            let body = parse_text_body(reader)?.text();
            text = Some(body).filter(|body| !body.trim().is_empty());
            return Ok(());
        }
        reader.skip_element()
    })?;
    Ok(text)
}

/// The `cx:pt` values of the innermost `cx:lvl` of a dimension, in `idx` order.
fn extended_points<'a>(reader: &mut Reader<'a>) -> Result<Vec<String>, XmlError> {
    let mut levels: Vec<Vec<(u32, String)>> = Vec::new();
    children(reader, &mut |reader, child| {
        if !child.name.is(Ns::ChartEx, b"lvl") {
            return reader.skip_element();
        }
        let mut points = Vec::new();
        children(reader, &mut |reader, pt| {
            if !pt.name.is(Ns::ChartEx, b"pt") {
                return reader.skip_element();
            }
            let index = reader
                .attr(&pt, Ns::None, b"idx")
                .and_then(|raw| std::str::from_utf8(raw).ok()?.trim().parse().ok())
                .unwrap_or(points.len() as u32);
            let mut value = String::new();
            reader.text_content(&mut value)?;
            points.push((index, value));
            Ok(())
        })?;
        points.sort_by_key(|(index, _)| *index);
        levels.push(points);
        Ok(())
    })?;
    Ok(levels
        .into_iter()
        .last()
        .unwrap_or_default()
        .into_iter()
        .map(|(_, value)| value)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
    const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

    #[test]
    fn bar_chart_title_series_categories_and_values() {
        let xml = format!(
            r#"<c:chartSpace xmlns:c="{C}" xmlns:a="{A}"><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Sales by </a:t></a:r><a:r><a:t>region</a:t></a:r></a:p></c:rich></c:tx></c:title><c:plotArea><c:barChart><c:ser><c:idx val="0"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>2023</c:v></c:pt></c:strCache></c:strRef></c:tx><c:cat><c:strRef><c:strCache><c:pt idx="1"><c:v>South</c:v></c:pt><c:pt idx="0"><c:v>North</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:numCache><c:formatCode>General</c:formatCode><c:pt idx="0"><c:v>4.3</c:v></c:pt><c:pt idx="1"><c:v>2.5</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser><c:ser><c:idx val="1"/><c:tx><c:v>2024</c:v></c:tx><c:cat><c:strLit><c:pt idx="0"><c:v>North</c:v></c:pt><c:pt idx="1"><c:v>South</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:pt idx="0"><c:v>5</c:v></c:pt><c:pt idx="1"><c:v>3</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart><c:catAx><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Region</a:t></a:r></a:p></c:rich></c:tx></c:title></c:catAx><c:valAx><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Millions</a:t></a:r></a:p></c:rich></c:tx></c:title></c:valAx></c:plotArea></c:chart></c:chartSpace>"#
        );
        let chart = parse_chart(xml.as_bytes()).unwrap();
        assert_eq!(chart.title.as_deref(), Some("Sales by region"));
        assert_eq!(chart.kinds, ["barChart"]);
        assert_eq!(chart.category_axis_title.as_deref(), Some("Region"));
        assert_eq!(chart.value_axis_title.as_deref(), Some("Millions"));
        assert_eq!(chart.series.len(), 2);
        assert_eq!(chart.series[0].name.as_deref(), Some("2023"));
        assert_eq!(chart.series[0].categories, ["North", "South"]);
        assert_eq!(chart.series[0].values, ["4.3", "2.5"]);
        assert_eq!(chart.series[1].name.as_deref(), Some("2024"));
        assert!(chart.shares_categories());
        let mut text = String::new();
        chart.write_text("\t", &mut text);
        assert_eq!(
            text,
            "Sales by region\n\t2023\t2024\nNorth\t4.3\t5\nSouth\t2.5\t3"
        );
    }

    #[test]
    fn scatter_series_with_different_points_list_per_series() {
        let xml = format!(
            r#"<c:chartSpace xmlns:c="{C}"><c:chart><c:plotArea><c:scatterChart><c:ser><c:tx><c:v>Run A</c:v></c:tx><c:xVal><c:numLit><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:xVal><c:yVal><c:numLit><c:pt idx="0"><c:v>10</c:v></c:pt><c:pt idx="1"><c:v>20</c:v></c:pt></c:numLit></c:yVal></c:ser><c:ser><c:tx><c:v>Run B</c:v></c:tx><c:xVal><c:numLit><c:pt idx="0"><c:v>3</c:v></c:pt></c:numLit></c:xVal><c:yVal><c:numLit><c:pt idx="0"><c:v>30</c:v></c:pt></c:numLit></c:yVal></c:ser></c:scatterChart></c:plotArea></c:chart></c:chartSpace>"#
        );
        let chart = parse_chart(xml.as_bytes()).unwrap();
        assert!(chart.title.is_none());
        assert!(!chart.shares_categories());
        let mut text = String::new();
        chart.write_text("\t", &mut text);
        assert_eq!(text, "Run A\t1: 10\t2: 20\nRun B\t3: 30");
    }

    #[test]
    fn extended_charts_read_titles_series_and_dimensions() {
        let xml = r#"<cx:chartSpace xmlns:cx="http://schemas.microsoft.com/office/drawing/2014/chartex" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><cx:chartData><cx:data id="0"><cx:strDim type="cat"><cx:f>Sheet1!$A$2:$A$4</cx:f><cx:lvl ptCount="3"><cx:pt idx="0">Leaf</cx:pt><cx:pt idx="1">Stem</cx:pt><cx:pt idx="2">Root</cx:pt></cx:lvl></cx:strDim><cx:numDim type="size"><cx:f>Sheet1!$B$2:$B$4</cx:f><cx:lvl ptCount="3" formatCode="General"><cx:pt idx="0">5</cx:pt><cx:pt idx="1">3</cx:pt><cx:pt idx="2">2</cx:pt></cx:lvl></cx:numDim></cx:data></cx:chartData><cx:chart><cx:title pos="t" align="ctr" overlay="0"><cx:tx><cx:txData><cx:v>Plant parts</cx:v></cx:txData></cx:tx></cx:title><cx:plotArea><cx:plotAreaRegion><cx:series layoutId="treemap" uniqueId="{1}"><cx:tx><cx:txData><cx:v>Mass</cx:v></cx:txData></cx:tx><cx:dataId val="0"/></cx:series></cx:plotAreaRegion></cx:plotArea></cx:chart></cx:chartSpace>"#;
        let chart = parse_chart(xml.as_bytes()).unwrap();
        assert_eq!(chart.title.as_deref(), Some("Plant parts"));
        assert_eq!(chart.kinds, ["treemap"]);
        assert_eq!(chart.series.len(), 1);
        assert_eq!(chart.series[0].name.as_deref(), Some("Mass"));
        assert_eq!(chart.series[0].categories, ["Leaf", "Stem", "Root"]);
        assert_eq!(chart.series[0].values, ["5", "3", "2"]);
    }

    #[test]
    fn an_empty_chart_part_is_empty_not_an_error() {
        let chart = parse_chart(format!(r#"<c:chartSpace xmlns:c="{C}"/>"#).as_bytes()).unwrap();
        assert!(chart.is_empty());
        assert!(chart.rows().is_empty());
    }
}
