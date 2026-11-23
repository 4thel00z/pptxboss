use pptxboss_core::{Document, MarkdownOptions, ReadingOrder, TextOptions};
use pptxboss_testkit::{Deck, DeckSlide};

fn make_text_box(id: u32, x: i64, y: i64, cx: i64, cy: i64, text: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Box {id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>"#
    )
}

fn make_bare_box(id: u32, sp_pr: &str, text: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Box {id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{sp_pr}</p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>"#
    )
}

fn make_group(
    id: u32,
    off: (i64, i64),
    ext: (i64, i64),
    ch_off: (i64, i64),
    ch_ext: (i64, i64),
    kids: &str,
) -> String {
    format!(
        r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{id}" name="Group {id}"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/><a:chOff x="{}" y="{}"/><a:chExt cx="{}" cy="{}"/></a:xfrm></p:grpSpPr>{kids}</p:grpSp>"#,
        off.0, off.1, ext.0, ext.1, ch_off.0, ch_off.1, ch_ext.0, ch_ext.1
    )
}

fn make_table(id: u32, x: i64, y: i64, cell: &str) -> String {
    format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="Table {id}"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="1000" cy="500"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblGrid><a:gridCol w="1"/></a:tblGrid><a:tr h="1"><a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>{cell}</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#
    )
}

fn make_picture(id: u32, x: i64, y: i64, alt: &str) -> String {
    format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="Picture {id}" descr="{alt}"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId3"/></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="1000" cy="500"/></a:xfrm></p:spPr></p:pic>"#
    )
}

fn load_deck(slide: DeckSlide) -> Document {
    let deck = Deck::new()
        .slide(slide.rel("rId3", "image", "../media/image1.png", false))
        .media("image1.png", &[0x89, b'P', b'N', b'G'], "image/png");
    Document::load(deck.build()).unwrap()
}

fn read_text(doc: &Document, order: ReadingOrder) -> String {
    doc.text_reporting(&TextOptions {
        reading_order: order,
        alt_text: true,
        ..TextOptions::default()
    })
    .0
}

fn read_markdown(doc: &Document, order: ReadingOrder) -> String {
    doc.markdown(&MarkdownOptions {
        headings: false,
        reading_order: order,
        ..MarkdownOptions::default()
    })
    .0
}

fn make_number_column() -> String {
    let numbers = [("01", 1_440_000), ("02", 2_740_000), ("03", 4_040_000)];
    let sentences = [
        ("First sentence.", 1_600_000),
        ("Second sentence.", 2_890_000),
        ("Third sentence.", 4_180_000),
    ];
    let mut xml = String::new();
    for (i, (label, y)) in numbers.iter().enumerate() {
        xml.push_str(&make_text_box(
            10 + i as u32,
            2_920_000,
            *y,
            600_000,
            500_000,
            label,
        ));
    }
    for (i, (label, y)) in sentences.iter().enumerate() {
        xml.push_str(&make_text_box(
            20 + i as u32,
            3_710_000,
            *y,
            5_000_000,
            500_000,
            label,
        ));
    }
    xml
}

#[test]
fn content_is_the_default_order() {
    assert_eq!(ReadingOrder::default(), ReadingOrder::Content);
    assert_eq!(TextOptions::default().reading_order, ReadingOrder::Content);
    assert_eq!(
        MarkdownOptions::default().reading_order,
        ReadingOrder::Content
    );
}

#[test]
fn number_column_reads_each_number_with_its_sentence() {
    let doc = load_deck(DeckSlide::default().shapes_xml(&make_number_column()));
    assert_eq!(
        read_text(&doc, ReadingOrder::Content),
        "01\n02\n03\nFirst sentence.\nSecond sentence.\nThird sentence."
    );
    assert_eq!(
        read_text(&doc, ReadingOrder::Geometric),
        "01 First sentence.\n02 Second sentence.\n03 Third sentence."
    );
    assert_eq!(
        read_markdown(&doc, ReadingOrder::Content),
        "01\n\n02\n\n03\n\nFirst sentence.\n\nSecond sentence.\n\nThird sentence.\n"
    );
    assert_eq!(
        read_markdown(&doc, ReadingOrder::Geometric),
        "01 First sentence.\n\n02 Second sentence.\n\n03 Third sentence.\n"
    );
}

#[test]
fn slide_and_batch_calls_take_the_order() {
    let doc = load_deck(DeckSlide::default().shapes_xml(&make_number_column()));
    let options = TextOptions {
        reading_order: ReadingOrder::Geometric,
        ..TextOptions::default()
    };
    let (texts, _) = doc.slide_texts_at(&[0], &options);
    assert_eq!(
        texts[0],
        "01 First sentence.\n02 Second sentence.\n03 Third sentence."
    );
    let slide = doc.slide(0).unwrap();
    let mut report = Default::default();
    assert_eq!(slide.text_reporting(&options, &mut report), texts[0]);
    assert_ne!(slide.text(), texts[0]);
}

#[test]
fn rows_with_a_multi_line_shape_keep_it_on_its_own_line() {
    let two_lines = r#"<p:sp><p:nvSpPr><p:cNvPr id="3" name="Two"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="2000" y="0"/><a:ext cx="1000" cy="500"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:t>top</a:t></a:r></a:p><a:p><a:r><a:t>bottom</a:t></a:r></a:p></p:txBody></p:sp>"#;
    let doc = load_deck(DeckSlide::default().shapes_xml(&format!(
        "{}{two_lines}",
        make_text_box(2, 0, 0, 1000, 500, "left")
    )));
    assert_eq!(
        read_text(&doc, ReadingOrder::Geometric),
        "left\ntop\nbottom"
    );
}

#[test]
fn group_children_are_placed_through_the_group_transform() {
    let inner = format!(
        "{}{}",
        make_text_box(11, 1000, 1000, 100, 100, "inner top"),
        make_text_box(12, 1000, 3000, 100, 100, "inner bottom")
    );
    let xml = format!(
        "{}{}{}",
        make_text_box(2, 0, 1_100_000, 100, 100, "between"),
        make_group(
            3,
            (0, 1_000_000),
            (500, 500_000),
            (1000, 1000),
            (1000, 4000),
            &inner
        ),
        make_text_box(4, 0, 2_000_000, 100, 100, "after")
    );
    let doc = load_deck(DeckSlide::default().shapes_xml(&xml));
    assert_eq!(
        read_text(&doc, ReadingOrder::Content),
        "between\ninner top\ninner bottom\nafter"
    );
    assert_eq!(
        read_text(&doc, ReadingOrder::Geometric),
        "inner top\nbetween\ninner bottom\nafter"
    );
}

#[test]
fn nested_groups_compose_their_transforms() {
    let leaf = make_text_box(21, 0, 0, 10, 10, "deep");
    let inner = make_group(20, (0, 0), (10, 10), (0, 0), (10, 10), &leaf);
    let outer = make_group(10, (0, 5_000_000), (10, 10), (0, 0), (10, 10), &inner);
    let xml = format!("{outer}{}", make_text_box(2, 0, 100, 10, 10, "high"));
    let doc = load_deck(DeckSlide::default().shapes_xml(&xml));
    assert_eq!(read_text(&doc, ReadingOrder::Content), "deep\nhigh");
    assert_eq!(read_text(&doc, ReadingOrder::Geometric), "high\ndeep");
}

#[test]
fn placeholders_take_their_position_from_layout_then_master() {
    let low = make_text_box(30, 0, 9_000_000, 100, 100, "Footer note");
    let high = make_text_box(31, 0, 10_000, 100, 100, "Kicker");
    let doc = load_deck(
        DeckSlide::titled("Heading")
            .bullet("Body line")
            .shapes_xml(&format!("{low}{high}")),
    );
    assert_eq!(
        read_text(&doc, ReadingOrder::Content),
        "Heading\nBody line\nFooter note\nKicker"
    );
    assert_eq!(
        read_text(&doc, ReadingOrder::Geometric),
        "Kicker\nHeading\nBody line\nFooter note"
    );
}

#[test]
fn tables_and_pictures_sort_by_their_frame() {
    let xml = format!(
        "{}{}{}",
        make_text_box(2, 0, 3_000_000, 1000, 500, "Caption"),
        make_table(3, 0, 1_000_000, "Cell"),
        make_picture(4, 0, 2_000_000, "Photo"),
    );
    let doc = load_deck(DeckSlide::default().shapes_xml(&xml));
    assert_eq!(
        read_text(&doc, ReadingOrder::Content),
        "Caption\nCell\nPhoto"
    );
    assert_eq!(
        read_text(&doc, ReadingOrder::Geometric),
        "Cell\nPhoto\nCaption"
    );
    assert_eq!(
        read_markdown(&doc, ReadingOrder::Geometric),
        "| Cell |\n|---|\n\n![Photo](ppt/media/image1.png)\n\nCaption\n"
    );
}

#[test]
fn shapes_without_a_position_follow_in_content_order() {
    let xml = format!(
        "{}{}{}{}{}",
        make_bare_box(2, "", "no spPr frame"),
        make_text_box(3, 0, 5000, 100, 100, "low"),
        make_bare_box(4, "<a:xfrm/>", "empty xfrm"),
        make_text_box(5, 0, 0, 100, 100, "high"),
        make_bare_box(6, "", "last"),
    );
    let doc = load_deck(DeckSlide::default().shapes_xml(&xml));
    assert_eq!(
        read_text(&doc, ReadingOrder::Content),
        "no spPr frame\nlow\nempty xfrm\nhigh\nlast"
    );
    assert_eq!(
        read_text(&doc, ReadingOrder::Geometric),
        "high\nlow\nno spPr frame\nempty xfrm\nlast"
    );
}

#[test]
fn unreadable_coordinates_read_as_zero_without_failing() {
    let xml = format!(
        "{}{}{}",
        make_bare_box(
            2,
            r#"<a:xfrm><a:off x="oops" y="500"/><a:ext cx="100" cy="100"/></a:xfrm>"#,
            "bad x"
        ),
        make_bare_box(3, r#"<a:xfrm><a:off x="0" y="0"/></a:xfrm>"#, "no extent"),
        make_text_box(4, 0, 9000, 100, 100, "last"),
    );
    let doc = load_deck(DeckSlide::default().shapes_xml(&xml));
    assert_eq!(
        read_text(&doc, ReadingOrder::Geometric),
        "no extent\nbad x\nlast"
    );
}

#[test]
fn negative_and_extreme_extents_do_not_panic() {
    let xml = format!(
        "{}{}{}",
        make_text_box(2, i64::MAX / 2, i64::MIN / 2, i64::MAX, -5, "huge"),
        make_group(
            3,
            (i64::MAX, i64::MAX),
            (0, 0),
            (i64::MIN, 0),
            (0, 0),
            &make_text_box(4, 1, 1, 1, 1, "kid")
        ),
        make_text_box(5, 0, 0, 0, 0, "point"),
    );
    let doc = load_deck(DeckSlide::default().shapes_xml(&xml));
    let geometric = read_text(&doc, ReadingOrder::Geometric);
    for word in ["huge", "kid", "point"] {
        assert!(geometric.contains(word), "{geometric}");
    }
}

#[test]
fn empty_slides_and_decks_read_as_empty() {
    let doc = load_deck(DeckSlide::default());
    assert_eq!(read_text(&doc, ReadingOrder::Geometric), "");
    assert_eq!(
        read_markdown(&doc, ReadingOrder::Geometric),
        read_markdown(&doc, ReadingOrder::Content)
    );
}

#[test]
fn notes_stay_after_the_shapes_in_both_orders() {
    let doc = load_deck(
        DeckSlide::default()
            .shapes_xml(&make_number_column())
            .notes("Say it"),
    );
    let options = |order| TextOptions {
        reading_order: order,
        notes: true,
        ..TextOptions::default()
    };
    let geometric = doc.text_reporting(&options(ReadingOrder::Geometric)).0;
    assert!(
        geometric.ends_with("03 Third sentence.\nSay it"),
        "{geometric}"
    );
    let markdown = doc
        .markdown(&MarkdownOptions {
            notes: true,
            reading_order: ReadingOrder::Geometric,
            ..MarkdownOptions::default()
        })
        .0;
    assert!(markdown.ends_with("> **Notes:**\n> Say it\n"), "{markdown}");
}

#[test]
fn hidden_shapes_do_not_split_a_row() {
    let hidden = r#"<p:sp><p:nvSpPr><p:cNvPr id="9" name="Ghost" hidden="1"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="1500" y="0"/><a:ext cx="100" cy="9000"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:t>ghost</a:t></a:r></a:p></p:txBody></p:sp>"#;
    let xml = format!(
        "{}{hidden}{}",
        make_text_box(2, 0, 0, 1000, 500, "a"),
        make_text_box(3, 2000, 0, 1000, 500, "b")
    );
    let doc = load_deck(DeckSlide::default().shapes_xml(&xml));
    assert_eq!(read_text(&doc, ReadingOrder::Geometric), "a b");
}

#[test]
fn reading_order_parses_and_prints() {
    assert_eq!(
        "geometric".parse::<ReadingOrder>().unwrap(),
        ReadingOrder::Geometric
    );
    assert_eq!(ReadingOrder::Content.to_string(), "content");
    assert!("structure-tree".parse::<ReadingOrder>().is_err());
}

#[test]
fn charts_sort_by_their_frame() {
    let chart_xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Revenue</a:t></a:r></a:p></c:rich></c:tx></c:title><c:plotArea><c:barChart><c:ser><c:idx val="0"/><c:tx><c:v>2024</c:v></c:tx><c:cat><c:strLit><c:pt idx="0"><c:v>Q1</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
    let frame = r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="7" name="Chart 6"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="2000000"/><a:ext cx="1000" cy="500"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rId5"/></a:graphicData></a:graphic></p:graphicFrame>"#;
    let xml = format!(
        "{}{frame}{}",
        make_text_box(2, 0, 3_000_000, 1000, 500, "Below"),
        make_text_box(3, 0, 1_000_000, 1000, 500, "Above"),
    );
    let deck = Deck::new()
        .slide(DeckSlide::default().shapes_xml(&xml).rel(
            "rId5",
            "chart",
            "../charts/chart1.xml",
            false,
        ))
        .with_part("ppt/charts/chart1.xml", chart_xml.as_bytes());
    let doc = Document::load(deck.build()).unwrap();
    assert_eq!(
        read_text(&doc, ReadingOrder::Content),
        "Below\nRevenue\n\t2024\nQ1\t10\nAbove"
    );
    assert_eq!(
        read_text(&doc, ReadingOrder::Geometric),
        "Above\nRevenue\n\t2024\nQ1\t10\nBelow"
    );
}
