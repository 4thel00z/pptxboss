//! Decks built with pptxboss-write are read back through pptxboss-core and
//! verified with pptxboss-check, never compared against byte dumps.

use pptxboss_check::{check_bytes, CheckOptions};
use pptxboss_core::model::PlaceholderKind;
use pptxboss_core::{Content, Document, TextOptions};
use pptxboss_write::{
    from_markdown, Align, Background, Color, Layout, Metadata, Paragraph, Presentation, Rect, Run,
    SchemeColor, Slide, SlideSize, Theme,
};

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

fn full_deck() -> Presentation {
    Presentation::new()
        .metadata(Metadata {
            title: Some("Round trip".into()),
            creator: "tests".into(),
            subject: None,
            keywords: Some("a, b".into()),
            timestamp: "2026-01-02T03:04:05Z".into(),
        })
        .slide(Slide::title_slide("Deck title", Some("With a subtitle")))
        .slide(
            Slide::titled("Bullets")
                .bullet("First point")
                .sub_bullet("Nested & escaped <ok>", 1)
                .paragraph("A plain paragraph")
                .notes("Speaker notes\nsecond line"),
        )
        .slide(
            Slide::titled("Shapes")
                .text_box(
                    Rect::inches(1.0, 1.5, 4.0, 1.0),
                    vec![
                        Paragraph::text("Free text").bold(),
                        Paragraph::bullet("boxed bullet", 0),
                    ],
                )
                .table(
                    Rect::inches(1.0, 3.0, 6.0, 1.5),
                    vec![
                        vec!["Name".into(), "Value".into()],
                        vec!["Answer".into(), "42".into()],
                    ],
                    true,
                )
                .picture_described(PNG.to_vec(), Rect::inches(8.0, 1.5, 1.0, 1.0), "a red dot"),
        )
        .slide(Slide::new().layout(Layout::Blank).hidden())
}

#[test]
fn a_full_deck_reads_back_and_verifies_clean() {
    let bytes = full_deck().to_bytes().unwrap();
    let report = check_bytes(bytes.clone(), &CheckOptions::default()).unwrap();
    assert!(report.findings.is_empty(), "{:#?}", report.findings);

    let doc = Document::load(bytes).unwrap();
    assert_eq!(doc.slide_count(), 4);
    assert_eq!(
        doc.presentation().slide_size.as_ref().map(|s| (s.cx, s.cy)),
        Some((12_192_000, 6_858_000))
    );
    assert_eq!(
        doc.presentation()
            .slide_size
            .as_ref()
            .and_then(|s| s.kind.clone())
            .as_deref(),
        Some("screen16x9")
    );
    assert_eq!(doc.presentation().notes_master.as_deref(), Some("rId2"));

    let title = doc.slide(0).unwrap();
    assert_eq!(title.title().as_deref(), Some("Deck title"));
    assert_eq!(title.text(), "Deck title\nWith a subtitle");
    assert_eq!(
        title.content.shapes[0].placeholder.as_ref().unwrap().kind,
        PlaceholderKind::CenterTitle
    );
    assert_eq!(
        title.layout_part().unwrap().as_deref(),
        Some("/ppt/slideLayouts/slideLayout1.xml")
    );

    let bullets = doc.slide(1).unwrap();
    assert_eq!(
        bullets.text(),
        "Bullets\nFirst point\nNested & escaped <ok>\nA plain paragraph"
    );
    let body = bullets.content.shapes[1].text_body().unwrap();
    assert_eq!(body.paragraphs[1].level, 1);
    assert_eq!(
        body.paragraphs[2].bullet,
        pptxboss_core::model::Bullet::None
    );
    assert_eq!(
        bullets.notes_text().unwrap().as_deref(),
        Some("Speaker notes\nsecond line")
    );

    let shapes = doc.slide(2).unwrap();
    assert_eq!(
        shapes.text(),
        "Shapes\nFree text\nboxed bullet\nName\tValue\nAnswer\t42"
    );
    assert!(shapes.content.shapes[1].text_box);
    assert_eq!(
        shapes.content.shapes[1].text_body().unwrap().paragraphs[0].runs[0]
            .props
            .bold,
        Some(true)
    );
    assert!(matches!(
        shapes.content.shapes[2].content,
        Content::Table(_)
    ));
    let images = shapes.images().unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].part.as_deref(), Some("/ppt/media/image1.png"));
    assert_eq!(images[0].content_type.as_deref(), Some("image/png"));
    assert_eq!(shapes.image_bytes(&images[0]).unwrap(), PNG);
    assert_eq!(
        shapes.content.shapes[3].description.as_deref(),
        Some("a red dot")
    );

    let blank = doc.slide(3).unwrap();
    assert!(blank.is_hidden());
    assert_eq!(blank.text(), "");
    let (texts, extract) = doc.slide_texts(&TextOptions {
        notes: true,
        ..TextOptions::default()
    });
    assert_eq!(texts[1], "Bullets\nFirst point\nNested & escaped <ok>\nA plain paragraph\nSpeaker notes\nsecond line");
    assert!(extract.is_complete());
}

#[test]
fn output_is_deterministic() {
    assert_eq!(
        full_deck().to_bytes().unwrap(),
        full_deck().to_bytes().unwrap()
    );
}

#[test]
fn an_empty_and_a_standard_size_deck_verify_clean() {
    let empty = Presentation::new().to_bytes().unwrap();
    let report = check_bytes(empty.clone(), &CheckOptions::default()).unwrap();
    assert!(report.is_clean(), "{:#?}", report.findings);
    assert_eq!(report.codes(), ["PML022"]);
    assert_eq!(Document::load(empty).unwrap().slide_count(), 0);
    let standard = Presentation::new()
        .size(SlideSize::STANDARD)
        .slide(Slide::titled("x").bullet("y"))
        .to_bytes()
        .unwrap();
    assert!(check_bytes(standard.clone(), &CheckOptions::default())
        .unwrap()
        .findings
        .is_empty());
    assert_eq!(
        Document::load(standard)
            .unwrap()
            .presentation()
            .slide_size
            .as_ref()
            .and_then(|s| s.kind.clone())
            .as_deref(),
        Some("screen4x3")
    );
}

#[test]
fn bad_input_is_an_error() {
    let ragged = Presentation::new().slide(Slide::titled("t").table(
        Rect::inches(1.0, 1.0, 2.0, 1.0),
        vec![vec!["a".into(), "b".into()], vec!["c".into()]],
        false,
    ));
    assert!(matches!(
        ragged.to_bytes(),
        Err(pptxboss_write::Error::RaggedTable {
            row: 1,
            cells: 1,
            columns: 2
        })
    ));
    let svg = Presentation::new()
        .slide(Slide::titled("t").picture(b"<svg/>".to_vec(), Rect::inches(1.0, 1.0, 1.0, 1.0)));
    assert!(matches!(
        svg.to_bytes(),
        Err(pptxboss_write::Error::UnsupportedImage(1))
    ));
}

#[test]
fn markdown_decks_verify_clean_and_read_back() {
    let deck = from_markdown("# Hello\nA subtitle\n\n## Agenda\n- one\n- two\n\nNotes: say hi\n\n## Table talk\nJust a paragraph.\n");
    let bytes = deck.to_bytes().unwrap();
    assert!(check_bytes(bytes.clone(), &CheckOptions::default())
        .unwrap()
        .findings
        .is_empty());
    let doc = Document::load(bytes).unwrap();
    assert_eq!(
        doc.text(),
        "Hello\nA subtitle\n\nAgenda\none\ntwo\n\nTable talk\nJust a paragraph."
    );
    assert_eq!(
        doc.slide(1).unwrap().notes_text().unwrap().as_deref(),
        Some("say hi")
    );
}

#[test]
fn files_are_written_to_disk() {
    let path = std::env::temp_dir().join(format!("pptxboss-write-{}.pptx", std::process::id()));
    Presentation::new()
        .slide(Slide::titled("Disk"))
        .write_to(&path)
        .unwrap();
    assert_eq!(
        Document::open(&path)
            .unwrap()
            .slide(0)
            .unwrap()
            .title()
            .as_deref(),
        Some("Disk")
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn styled_runs_read_back() {
    let url = "https://example.com/?a=1&b=2";
    let deck = Presentation::new().slide(
        Slide::titled("Runs")
            .body_paragraph(
                Paragraph::runs(vec![
                    Run::text("Bold ").bold().underline().size(20),
                    Run::text("italic ").italic().strike().font("Georgia"),
                    Run::text("link").link(url),
                    Run::text(" again").link(url),
                ])
                .space_after(12),
            )
            .text_box(
                Rect::inches(1.0, 4.0, 4.0, 1.0),
                vec![Paragraph::text("Centered")
                    .align(Align::Center)
                    .color(Color::Scheme(SchemeColor::Accent2))],
            ),
    );
    let bytes = deck.to_bytes().unwrap();
    let report = check_bytes(bytes.clone(), &CheckOptions::default()).unwrap();
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
    let doc = Document::load(bytes).unwrap();
    let slide = doc.slide(0).unwrap();
    assert_eq!(slide.text(), "Runs\nBold italic link again\nCentered");
    let runs = &slide.content.shapes[1].text_body().unwrap().paragraphs[0].runs;
    assert_eq!(runs[0].props.bold, Some(true));
    assert_eq!(runs[0].props.underline, Some(true));
    assert_eq!(runs[0].props.size, Some(2000));
    assert_eq!(runs[1].props.italic, Some(true));
    assert_eq!(runs[1].props.strike, Some(true));
    assert_eq!(runs[1].props.typeface.as_deref(), Some("Georgia"));
    let link_id = runs[2].props.hyperlink.clone().unwrap();
    assert_eq!(runs[3].props.hyperlink.as_deref(), Some(link_id.as_str()));
    let rels = slide.rels().unwrap();
    assert_eq!(rels.get(&link_id).unwrap().target, url);
    let package = doc.package().unwrap();
    let xml = String::from_utf8(
        package
            .read_part("/ppt/slides/slide1.xml")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(xml.contains(r#"algn="ctr""#));
    assert!(xml.contains(r#"<a:schemeClr val="accent2"/>"#));
    assert!(xml.contains(r#"<a:spcAft><a:spcPts val="1200"/></a:spcAft>"#));
}

#[test]
fn themes_and_backgrounds_verify_clean() {
    let theme = Theme::dark()
        .background(Background::picture(PNG.to_vec()))
        .layout_background(
            Layout::Title,
            Background::linear(
                Color::rgb(0x10, 0x10, 0x10),
                Color::Scheme(SchemeColor::Accent1),
                90,
            ),
            false,
        );
    let build = || {
        Presentation::new()
            .theme(theme.clone())
            .slide(Slide::title_slide(
                "Dark deck",
                Some("gradient title layout"),
            ))
            .slide(
                Slide::titled("Inverted")
                    .background(Background::solid(Color::rgb(0xFF, 0xFF, 0xFF)))
                    .inverted()
                    .bullet("dark text on white"),
            )
            .slide(
                Slide::titled("Picture")
                    .picture(PNG.to_vec(), Rect::inches(1.0, 1.0, 1.0, 1.0))
                    .body_paragraph(
                        Paragraph::text("accent").color(Color::Scheme(SchemeColor::Accent3)),
                    ),
            )
    };
    let bytes = build().to_bytes().unwrap();
    assert_eq!(bytes, build().to_bytes().unwrap());
    let report = check_bytes(bytes.clone(), &CheckOptions::default()).unwrap();
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
    let doc = Document::load(bytes).unwrap();
    let package = doc.package().unwrap();
    let part = |name: &str| String::from_utf8(package.read_part(name).unwrap().to_vec()).unwrap();
    let theme_part = part("/ppt/theme/theme1.xml");
    assert!(theme_part.contains(r#"name="dark""#));
    assert!(theme_part.contains(
        r#"<a:dk1><a:srgbClr val="F5F5F5"/></a:dk1><a:lt1><a:srgbClr val="1E1E1E"/></a:lt1>"#
    ));
    let master = part("/ppt/slideMasters/slideMaster1.xml");
    assert!(master.contains(r#"<p:clrMap bg1="lt1" tx1="dk1""#));
    assert!(master.contains(r#"<a:blip r:embed="rId6"/>"#));
    assert_eq!(
        package
            .resolve("/ppt/slideMasters/slideMaster1.xml", "rId6")
            .unwrap()
            .as_deref(),
        Some("/ppt/media/image1.png")
    );
    assert!(part("/ppt/slideLayouts/slideLayout1.xml").contains(
        r#"<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="101010"/></a:gs><a:gs pos="100000"><a:schemeClr val="accent1"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill>"#
    ));
    assert!(part("/ppt/slideLayouts/slideLayout2.xml").contains("<a:masterClrMapping/>"));
    let inverted = part("/ppt/slides/slide2.xml");
    assert!(inverted.contains(
        r#"<p:bg><p:bgPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>"#
    ));
    assert!(inverted.contains(r#"<a:overrideClrMapping bg1="dk1" tx1="lt1""#));
    assert!(part("/ppt/slides/slide3.xml").contains(r#"<a:schemeClr val="accent3"/>"#));
    let images = doc.slide(2).unwrap().images().unwrap();
    assert_eq!(images[0].part.as_deref(), Some("/ppt/media/image2.png"));
    assert!(doc.slide(0).unwrap().text().starts_with("Dark deck"));
}

#[test]
fn every_preset_verifies_clean() {
    for name in Theme::PRESETS {
        let bytes = Presentation::new()
            .theme(Theme::preset(name).unwrap())
            .slide(Slide::title_slide(name, Some("preset")))
            .slide(Slide::titled("Body").bullet("one").sub_bullet("two", 1))
            .to_bytes()
            .unwrap();
        let report = check_bytes(bytes, &CheckOptions::default()).unwrap();
        assert!(report.findings.is_empty(), "{name}: {:#?}", report.findings);
    }
}

#[test]
fn bad_background_is_an_error() {
    let deck = Presentation::new()
        .theme(Theme::new("x").background(Background::picture(b"<svg/>".to_vec())));
    assert!(matches!(
        deck.to_bytes(),
        Err(pptxboss_write::Error::UnsupportedBackgroundImage)
    ));
}
