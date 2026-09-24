//! Decks built with pptxboss-write are read back through pptxboss-core and
//! verified with pptxboss-check, never compared against byte dumps.

use pptxboss_check::{check_bytes, CheckOptions};
use pptxboss_core::model::PlaceholderKind;
use pptxboss_core::{Content, Document, TextOptions};
use pptxboss_write::{
    from_markdown, Align, Background, Block, Color, Layout, Metadata, Paragraph, Presentation,
    Rect, Run, SchemeColor, Slide, SlideSize, Theme,
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
                    Run::text(" mail").link("mailto:a@example.com"),
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
    assert_eq!(slide.text(), "Runs\nBold italic link again mail\nCentered");
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
    let mail = rels
        .get(runs[4].props.hyperlink.as_deref().unwrap())
        .unwrap();
    assert_eq!(mail.target, "mailto:a@example.com");
    assert_eq!(mail.mode, pptxboss_core::opc::TargetMode::External);
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
                    .bullet("dark text on white")
                    .notes("notes stay light"),
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
    assert!(master.contains(r#"<a:blip r:embed="rId7"/>"#));
    assert_eq!(
        package
            .resolve("/ppt/slideMasters/slideMaster1.xml", "rId7")
            .unwrap()
            .as_deref(),
        Some("/ppt/media/image1.png")
    );
    assert!(part("/ppt/slideLayouts/slideLayout1.xml").contains(
        r#"<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="101010"/></a:gs><a:gs pos="100000"><a:schemeClr val="accent1"/></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill>"#
    ));
    assert!(part("/ppt/slideLayouts/slideLayout2.xml").contains("<a:masterClrMapping/>"));
    assert!(!part("/ppt/slides/slide1.xml").contains("<p:bg>"));
    let inverted = part("/ppt/slides/slide2.xml");
    assert!(inverted.contains(
        r#"<p:bg><p:bgPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>"#
    ));
    assert!(inverted.contains(r#"<a:overrideClrMapping bg1="dk1" tx1="lt1""#));
    assert!(part("/ppt/slides/slide3.xml").contains(r#"<a:schemeClr val="accent3"/>"#));
    assert!(part("/ppt/notesMasters/notesMaster1.xml")
        .contains(r#"<p:clrMap bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2""#));
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

#[test]
fn placed_content_verifies_clean_and_continues() {
    let mut agenda = Slide::titled("Agenda").notes("first only");
    for i in 0..30 {
        agenda = agenda.bullet(format!("Item {i}: something that takes a line"));
    }
    let mut rows = vec![vec!["Name".to_string(), "Value".to_string()]];
    rows.extend((0..30).map(|i| vec![format!("row {i}"), i.to_string()]));
    let deck = Presentation::new()
        .slide(agenda)
        .slide(
            Slide::titled("Side by side")
                .columns(vec![
                    Block::bullets(["left one", "left two"]),
                    Block::picture_described(PNG.to_vec(), "a red dot"),
                ])
                .block(Block::table(
                    vec![vec!["a".into(), "b".into()], vec!["1".into(), "2".into()]],
                    true,
                )),
        )
        .slide(Slide::titled("Long table").block(Block::table(rows, true)))
        .slide(Slide::new().block(Block::picture(PNG.to_vec())));
    let bytes = deck.to_bytes().unwrap();
    let report = check_bytes(bytes.clone(), &CheckOptions::default()).unwrap();
    assert!(report.findings.is_empty(), "{:#?}", report.findings);

    let doc = Document::load(bytes).unwrap();
    assert!(doc.slide_count() > 4, "{} slides", doc.slide_count());
    let first = doc.slide(0).unwrap();
    assert!(first.text().starts_with("Agenda\nItem 0:"));
    assert_eq!(first.notes_text().unwrap().as_deref(), Some("first only"));
    let second = doc.slide(1).unwrap();
    assert_eq!(second.title().as_deref(), Some("Agenda"));
    assert!(second.notes_text().unwrap().is_none());
    let mut all_text = String::new();
    for i in 0..doc.slide_count() {
        all_text.push_str(&doc.slide(i).unwrap().text());
        all_text.push('\n');
    }
    for i in 0..30 {
        assert!(all_text.contains(&format!("Item {i}:")), "item {i}");
        assert!(all_text.contains(&format!("row {i}\t{i}")), "row {i}");
    }
    assert!(all_text.contains("left one\nleft two\na\tb\n1\t2"));
    assert_eq!(
        all_text.matches("Name\tValue").count(),
        all_text.matches("Long table").count()
    );
    assert!(doc
        .slide(doc.slide_count() - 1)
        .unwrap()
        .content
        .shapes
        .iter()
        .any(|shape| matches!(shape.content, Content::Picture(_))));
}

#[test]
fn embedded_fonts_are_stored_as_eot_parts() {
    let regular = include_bytes!("data/boxy-regular.ttf").to_vec();
    let bold = include_bytes!("data/boxy-bold.ttf").to_vec();
    let theme = Theme::office()
        .font("Boxy")
        .embed_font(regular.clone())
        .embed_font(bold);
    let deck = Presentation::new()
        .theme(theme)
        .slide(Slide::titled("Boxy").bullet("A box").notes("n"));
    let bytes = deck.to_bytes().unwrap();
    let report = check_bytes(bytes.clone(), &CheckOptions::default()).unwrap();
    assert!(report.findings.is_empty(), "{:#?}", report.findings);

    let doc = Document::load(bytes).unwrap();
    let package = doc.package().unwrap();
    let presentation =
        String::from_utf8(package.read_part("/ppt/presentation.xml").unwrap().to_vec()).unwrap();
    let list_start = presentation.find("<p:embeddedFontLst>").unwrap();
    let list_end = presentation.find("</p:embeddedFontLst>").unwrap();
    let list = &presentation[list_start..list_end];
    assert!(list.contains(r#"<p:font typeface="Boxy" panose="020B0503020202020204" pitchFamily="34" charset="0"/>"#), "{list}");
    assert!(
        list.contains(r#"<p:regular r:id="rId8"/><p:bold r:id="rId9"/>"#),
        "{list}"
    );
    assert!(list_end < presentation.find("<p:defaultTextStyle>").unwrap());
    let rels = String::from_utf8(
        package
            .read_part("/ppt/_rels/presentation.xml.rels")
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(rels.contains(r#"Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/font" Target="fonts/font1.fntdata""#), "{rels}");
    assert_eq!(
        package.content_type_of("/ppt/fonts/font1.fntdata"),
        Some("application/x-fontdata")
    );
    let font1 = package.read_part("/ppt/fonts/font1.fntdata").unwrap();
    let le32 = |at: usize| u32::from_le_bytes(font1[at..at + 4].try_into().unwrap());
    assert_eq!(le32(0) as usize, font1.len());
    assert_eq!(le32(8), 0x0002_0002);
    assert_eq!(le32(12), 4, "font data is MicroType Express compressed");
    let body = &font1[font1.len() - le32(4) as usize..];
    assert_eq!(body[0], 3);
    assert!(body.len() < regular.len());

    let restricted = Presentation::new().theme(Theme::office().embed_font(b"<svg/>".to_vec()));
    assert!(matches!(
        restricted.to_bytes(),
        Err(pptxboss_write::Error::UnsupportedFont(_))
    ));
}

#[test]
fn sections_stats_quotes_and_footers_verify_clean_and_read_back() {
    let mut agenda = Slide::titled("Agenda");
    for i in 0..30 {
        agenda = agenda.bullet(format!("Item {i}: something that takes a line"));
    }
    let deck = Presentation::new()
        .theme(Theme::office().footer("Platform review"))
        .slide(Slide::title_slide("Deck", Some("Subtitle")))
        .slide(Slide::section("Part one"))
        .slide(
            Slide::titled("Numbers")
                .columns(vec![
                    Block::stat("86%", "fewer cold starts"),
                    Block::stat("0", "findings"),
                ])
                .block(Block::quote("It just opened.", Some("A reviewer"))),
        )
        .slide(agenda);
    let bytes = deck.to_bytes().unwrap();
    let report = check_bytes(bytes.clone(), &CheckOptions::default()).unwrap();
    assert!(report.findings.is_empty(), "{:#?}", report.findings);

    let doc = Document::load(bytes).unwrap();
    assert!(doc.slide_count() >= 5, "{} slides", doc.slide_count());
    let section = doc.slide(1).unwrap();
    assert_eq!(section.title().as_deref(), Some("Part one"));
    assert_eq!(section.text(), "Part one");
    let numbers = doc.slide(2).unwrap();
    assert_eq!(
        numbers.text(),
        "Numbers\n86%\nfewer cold starts\n0\nfindings\nIt just opened.\nA reviewer"
    );
    for index in 0..doc.slide_count() {
        let slide = doc.slide(index).unwrap();
        let furniture: Vec<String> = slide
            .content
            .shapes
            .iter()
            .filter(|shape| {
                shape
                    .placeholder
                    .as_ref()
                    .is_some_and(|ph| ph.kind.is_furniture())
            })
            .filter_map(|shape| match &shape.content {
                Content::Text(body) => Some(body.text()),
                _ => None,
            })
            .collect();
        match index {
            0 | 1 => assert!(furniture.is_empty(), "slide {index}: {furniture:?}"),
            _ => assert_eq!(
                furniture,
                vec!["Platform review".to_string(), (index + 1).to_string()],
                "slide {index}"
            ),
        }
    }
}

#[test]
fn inverted_slides_without_a_background_keep_their_own_fill() {
    let deck = Presentation::new()
        .slide(Slide::titled("Plain").bullet("x"))
        .slide(Slide::titled("Inverted").inverted().bullet("x"));
    let bytes = deck.to_bytes().unwrap();
    let report = check_bytes(bytes.clone(), &CheckOptions::default()).unwrap();
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
    let doc = Document::load(bytes).unwrap();
    let package = doc.package().unwrap();
    let part = |name: &str| String::from_utf8(package.read_part(name).unwrap().to_vec()).unwrap();
    assert!(!part("/ppt/slides/slide1.xml").contains("<p:bg>"));
    let inverted = part("/ppt/slides/slide2.xml");
    assert!(
        inverted.contains(r#"<p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>"#)
    );
    assert!(inverted.contains(r#"<a:overrideClrMapping bg1="dk1" tx1="lt1""#));
}
