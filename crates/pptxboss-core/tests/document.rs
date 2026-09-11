use pptxboss_core::document::Located;
use pptxboss_core::{Content, Document, Error, TextOptions};
use pptxboss_testkit::{Deck, DeckSlide, ZipBuilder};

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

fn deck() -> Deck {
    Deck::new()
        .slide(DeckSlide::titled("First slide").bullet("Alpha").bullet("Beta & Gamma"))
        .slide(DeckSlide::titled("Second slide").bullet("One").notes("Speaker notes here"))
        .slide(DeckSlide::titled("Hidden slide").hidden())
        .slide(
            DeckSlide::default()
                .shapes_xml(r#"<p:pic><p:nvPicPr><p:cNvPr id="4" name="Picture 3"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId3"/></p:blipFill><p:spPr/></p:pic><p:sp><p:nvSpPr><p:cNvPr id="5" name="Link"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="en-US"><a:hlinkClick r:id="rId4"/></a:rPr><a:t>click</a:t></a:r></a:p></p:txBody></p:sp>"#)
                .rel("rId3", "image", "../media/image1.png", false)
                .rel("rId4", "hyperlink", "https://example.com/?q=1&r=2", true),
        )
        .media("image1.png", PNG, "image/png")
}

#[test]
fn slides_titles_text_and_notes_are_read_in_order() {
    let doc = Document::load(deck().build()).unwrap();
    assert_eq!(doc.slide_count(), 4);
    assert_eq!(doc.presentation_part(), "/ppt/presentation.xml");
    assert_eq!(doc.defects().located, Located::Relationship);
    assert!(doc.defects().unresolved_slides.is_empty());
    assert_eq!(doc.slide_refs()[1].part, "/ppt/slides/slide2.xml");
    assert_eq!(doc.slide_refs()[1].id, Some(257));

    let first = doc.slide(0).unwrap();
    assert_eq!(first.number(), 1);
    assert_eq!(first.title().as_deref(), Some("First slide"));
    assert_eq!(first.text(), "First slide\nAlpha\nBeta & Gamma");
    assert!(first.report.is_empty());
    assert_eq!(first.notes_text().unwrap(), None);
    assert_eq!(
        first.layout_part().unwrap().as_deref(),
        Some("/ppt/slideLayouts/slideLayout1.xml")
    );

    let second = doc.slide(1).unwrap();
    assert_eq!(
        second.notes_part().unwrap().as_deref(),
        Some("/ppt/notesSlides/notesSlide2.xml")
    );
    assert_eq!(
        second.notes_text().unwrap().as_deref(),
        Some("Speaker notes here")
    );

    let third = doc.slide(2).unwrap();
    assert!(third.is_hidden());

    assert_eq!(
        doc.text(),
        "First slide\nAlpha\nBeta & Gamma\n\nSecond slide\nOne\n\nHidden slide\n\nclick"
    );
    let (texts, report) = doc.slide_texts(&TextOptions {
        notes: true,
        hidden_slides: false,
        ..TextOptions::default()
    });
    assert_eq!(
        texts,
        [
            "First slide\nAlpha\nBeta & Gamma",
            "Second slide\nOne\nSpeaker notes here",
            "",
            "click"
        ]
    );
    assert_eq!(report.hidden_slides_skipped, 1);
    assert!(report.is_complete());
    assert!(matches!(doc.slide(4), Err(Error::SlideNotFound(4))));
}

#[test]
fn images_and_hyperlinks_resolve_through_slide_relationships() {
    let doc = Document::load(deck().build()).unwrap();
    let slide = doc.slide(3).unwrap();
    let images = slide.images().unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].shape_id, 4);
    assert_eq!(images[0].part.as_deref(), Some("/ppt/media/image1.png"));
    assert_eq!(images[0].content_type.as_deref(), Some("image/png"));
    assert_eq!(slide.image_bytes(&images[0]).unwrap(), PNG);
    let link = slide.content.shapes[1].text_body().unwrap().paragraphs[0].runs[0]
        .props
        .hyperlink
        .clone()
        .unwrap();
    assert_eq!(
        slide.hyperlink_target(&link).unwrap().as_deref(),
        Some("https://example.com/?q=1&r=2")
    );
    assert_eq!(slide.hyperlink_target("rId99").unwrap(), None);
    assert!(matches!(
        &slide.content.shapes[0].content,
        Content::Picture(_)
    ));
}

#[test]
fn map_slides_runs_in_parallel_and_keeps_order() {
    let mut deck = Deck::new();
    for i in 0..40 {
        deck = deck.slide(DeckSlide::titled(&format!("Slide {i}")).bullet("x"));
    }
    let doc = Document::load(deck.build()).unwrap();
    let titles = doc.map_slides(|slide| slide.unwrap().title().unwrap());
    let expected: Vec<String> = (0..40).map(|i| format!("Slide {i}")).collect();
    assert_eq!(titles, expected);
    let seed = doc.seed();
    let from_thread =
        std::thread::spawn(move || Document::from_seed(seed).slide(39).unwrap().title())
            .join()
            .unwrap();
    assert_eq!(from_thread.as_deref(), Some("Slide 39"));
    let capped = Document::load(deck.build()).unwrap().with_threads(1);
    assert_eq!(capped.threads(), 1);
    assert_eq!(Document::from_seed(capped.seed()).threads(), 1);
    assert_eq!(
        capped.map_slides(|slide| slide.unwrap().title().unwrap()),
        expected
    );
    assert_eq!(capped.text(), doc.text());
}

#[test]
fn a_file_opens_with_positioned_reads() {
    let path = std::env::temp_dir().join(format!("pptxboss-doc-{}.pptx", std::process::id()));
    std::fs::write(&path, deck().build()).unwrap();
    let doc = Document::open(&path).unwrap();
    assert_eq!(
        doc.slide(0).unwrap().title().as_deref(),
        Some("First slide")
    );
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn missing_slide_list_is_recovered_from_relationships() {
    let bare = br#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#;
    let doc = Document::load(deck().with_part("ppt/presentation.xml", bare).build()).unwrap();
    assert!(doc.defects().slides_recovered_from_rels);
    assert_eq!(doc.slide_count(), 4);
    assert_eq!(
        doc.slide(0).unwrap().title().as_deref(),
        Some("First slide")
    );
}

#[test]
fn unresolved_slides_are_skipped_and_recorded() {
    let doc = Document::load(deck().without_part("ppt/slides/slide2.xml").build()).unwrap();
    assert_eq!(doc.slide_count(), 3);
    assert_eq!(doc.defects().unresolved_slides.len(), 1);
    assert_eq!(doc.defects().unresolved_slides[0].0, 1);
    assert_eq!(
        doc.slide(1).unwrap().title().as_deref(),
        Some("Hidden slide")
    );
}

#[test]
fn the_presentation_is_found_without_package_relationships() {
    let doc = Document::load(deck().without_part("_rels/.rels").build()).unwrap();
    assert_eq!(doc.defects().located, Located::ContentType);
    assert_eq!(doc.slide_count(), 4);
    let doc = Document::load(
        deck()
            .without_part("_rels/.rels")
            .without_part("[Content_Types].xml")
            .build(),
    )
    .unwrap();
    assert_eq!(doc.defects().located, Located::ConventionalPath);
    assert_eq!(doc.slide(1).unwrap().text(), "Second slide\nOne");
}

#[test]
fn non_presentations_are_refused_with_a_reason() {
    assert!(matches!(
        Document::load(b"plain text".to_vec()),
        Err(Error::NotZip)
    ));
    let mut cfb = vec![0xd0, 0xcf, 0x11, 0xe0];
    cfb.resize(512, 0);
    assert!(matches!(Document::load(cfb), Err(Error::CompoundFile)));
    let zip = pptxboss_testkit::ZipBuilder::new()
        .stored("hello.txt", b"hi")
        .build();
    assert!(matches!(Document::load(zip), Err(Error::NotAPresentation)));
    let broken = deck()
        .with_part("ppt/presentation.xml", b"<p:presentation")
        .build();
    assert!(matches!(Document::load(broken), Err(Error::Xml { .. })));
}

#[test]
fn a_broken_slide_fails_alone_and_is_reported() {
    let doc = Document::load(
        deck()
            .with_part("ppt/slides/slide1.xml", b"<p:sld xmlns:p=\"x\"><p:cSld>")
            .build(),
    )
    .unwrap();
    assert!(matches!(doc.slide(0), Err(Error::Xml { .. })));
    assert_eq!(
        doc.slide(1).unwrap().title().as_deref(),
        Some("Second slide")
    );
    let (texts, report) = doc.slide_texts(&TextOptions::default());
    assert_eq!(texts[0], "");
    assert_eq!(report.failed_slides.len(), 1);
    assert_eq!(report.failed_slides[0].0, 0);
    assert!(!report.is_complete());
    assert!(report.warnings()[0].starts_with("slide 1: unreadable"));
}

fn rebuilt(
    deck: &Deck,
    mut transform: impl FnMut(&str, Vec<u8>, &mut Vec<(String, Vec<u8>)>),
) -> Vec<u8> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for (name, bytes) in deck.parts() {
        transform(&name, bytes, &mut entries);
    }
    let mut builder = ZipBuilder::new();
    for (name, bytes) in &entries {
        builder = builder.deflated(name, bytes);
    }
    builder.build()
}

#[test]
fn interleaved_pieces_are_reassembled_into_one_part() {
    let deck = Deck::new().slide(DeckSlide::titled("Pieces").bullet("of eight"));
    let expected = Document::load(deck.build()).unwrap().text();
    let bytes = rebuilt(&deck, |name, bytes, entries| {
        if name != "ppt/slides/slide1.xml" {
            entries.push((name.to_string(), bytes));
            return;
        }
        let (head, rest) = bytes.split_at(50);
        let (middle, tail) = rest.split_at(50);
        entries.push(("ppt/slides/slide1.xml/[0].piece".into(), head.to_vec()));
        entries.push(("ppt/slides/slide1.xml/[1].piece".into(), middle.to_vec()));
        entries.push(("ppt/slides/slide1.xml/[2].LAST.piece".into(), tail.to_vec()));
    });
    let doc = Document::load(bytes).unwrap();
    assert_eq!(doc.text(), expected);
    let part = doc
        .package()
        .parts()
        .iter()
        .find(|part| part.name == "/ppt/slides/slide1.xml")
        .expect("logical part");
    assert_eq!(part.pieces.len(), 3);
    assert!(doc.package().defects().incomplete_pieces.is_empty());
}

#[test]
fn a_utf16_slide_part_reads_like_utf8() {
    let deck = Deck::new().slide(DeckSlide::titled("Wide chars").bullet("ünïcödé"));
    let expected = Document::load(deck.build()).unwrap().text();
    let bytes = rebuilt(&deck, |name, bytes, entries| {
        if name != "ppt/slides/slide1.xml" {
            entries.push((name.to_string(), bytes));
            return;
        }
        let text = String::from_utf8(bytes).unwrap();
        let mut wide = vec![0xfe, 0xff];
        for unit in text.encode_utf16() {
            wide.extend(unit.to_be_bytes());
        }
        entries.push((name.to_string(), wide));
    });
    let doc = Document::load(bytes).unwrap();
    assert_eq!(doc.text(), expected);
    assert!(doc.text().contains("ünïcödé"));
}

const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

#[test]
fn comments_of_both_flavours_resolve_their_authors() {
    let legacy_authors = format!(
        r#"<p:cmAuthorLst xmlns:p="{P}"><p:cmAuthor id="0" name="Ada" initials="A" lastIdx="1" clrIdx="0"/></p:cmAuthorLst>"#
    );
    let legacy_comments = format!(
        r#"<p:cmLst xmlns:p="{P}"><p:cm authorId="0" dt="2024-05-01T10:00:00.000" idx="1"><p:pos x="1" y="2"/><p:text>Tighten this</p:text></p:cm></p:cmLst>"#
    );
    let modern_authors = r#"<p188:authorLst xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main"><p188:author id="{G1}" name="Grace" initials="G" userId="g" providerId="AD"/></p188:authorLst>"#;
    let modern_comments = format!(
        r#"<p188:cmLst xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main" xmlns:a="{A}"><p188:cm id="{{C1}}" authorId="{{G1}}" created="2024-06-01T09:00:00.000"><p188:txBody><a:bodyPr/><a:p><a:r><a:t>Looks good</a:t></a:r></a:p></p188:txBody><p188:replyLst><p188:reply id="{{R1}}" authorId="{{G1}}" created="2024-06-01T10:00:00.000"><p188:txBody><a:bodyPr/><a:p><a:r><a:t>Thanks</a:t></a:r></a:p></p188:txBody></p188:reply></p188:replyLst></p188:cm></p188:cmLst>"#
    );
    let deck = Deck::new()
        .slide(DeckSlide::titled("One").bullet("first").rel(
            "rId7",
            "comments",
            "../comments/comment1.xml",
            false,
        ))
        .slide(DeckSlide::titled("Two").rel(
            "rId8",
            "http://schemas.microsoft.com/office/2018/10/relationships/comments",
            "../comments/modernComment_2.xml",
            false,
        ))
        .slide(DeckSlide::titled("Three"))
        .presentation_rel("rId40", "commentAuthors", "commentAuthors.xml")
        .presentation_rel(
            "rId41",
            "http://schemas.microsoft.com/office/2018/10/relationships/authors",
            "authors.xml",
        )
        .with_part("ppt/commentAuthors.xml", legacy_authors.as_bytes())
        .with_part("ppt/authors.xml", modern_authors.as_bytes())
        .with_part("ppt/comments/comment1.xml", legacy_comments.as_bytes())
        .with_part(
            "ppt/comments/modernComment_2.xml",
            modern_comments.as_bytes(),
        );
    let doc = Document::load(deck.build()).unwrap();
    assert_eq!(doc.comment_authors().len(), 2);
    let first = doc.slide(0).unwrap().comments().unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].author.as_deref(), Some("Ada"));
    assert_eq!(first[0].text, "Tighten this");
    let second = doc.slide(1).unwrap().comments().unwrap();
    assert_eq!(second.len(), 2);
    assert_eq!(second[0].author.as_deref(), Some("Grace"));
    assert_eq!(second[0].text, "Looks good");
    assert!(second[1].reply);
    assert_eq!(second[1].text, "Thanks");
    assert!(doc.slide(2).unwrap().comments().unwrap().is_empty());
    let options = TextOptions {
        comments: true,
        ..TextOptions::default()
    };
    let (text, report) = doc.text_reporting(&options);
    assert!(report.is_complete());
    assert!(text.contains("first\n[comment] Ada: Tighten this"));
    assert!(text.contains("[comment] Grace: Looks good\n  [reply] Grace: Thanks"));
    assert!(!doc.text().contains("[comment]"));
}

#[test]
fn sections_map_slide_ids_to_indexes() {
    let sections = r#"<p:extLst><p:ext uri="{521415D9-36F7-43E2-AB2F-B90AF26B5E84}"><p14:sectionLst xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main"><p14:section name="Intro" id="{A}"><p14:sldIdLst><p14:sldId id="256"/></p14:sldIdLst></p14:section><p14:section name="Body &amp; more" id="{B}"><p14:sldIdLst><p14:sldId id="257"/><p14:sldId id="999"/><p14:sldId id="258"/></p14:sldIdLst></p14:section><p14:section name="Empty" id="{C}"><p14:sldIdLst/></p14:section></p14:sectionLst></p:ext></p:extLst>"#;
    let deck = Deck::new()
        .slide(DeckSlide::titled("1"))
        .slide(DeckSlide::titled("2"))
        .slide(DeckSlide::titled("3"))
        .presentation_xml(sections);
    let doc = Document::load(deck.build()).unwrap();
    let sections = doc.sections();
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].name, "Intro");
    assert_eq!(sections[0].slides, [0]);
    assert_eq!(sections[1].name, "Body & more");
    assert_eq!(sections[1].slides, [1, 2]);
    assert!(sections[2].slides.is_empty());
    assert!(
        Document::load(Deck::new().slide(DeckSlide::titled("x")).build())
            .unwrap()
            .sections()
            .is_empty()
    );
}

#[test]
fn core_and_app_properties_are_read_when_present() {
    let doc = Document::load(Deck::new().slide(DeckSlide::titled("x")).build()).unwrap();
    assert!(doc.core_properties().unwrap().is_some());
    assert!(doc.app_properties().unwrap().is_some());
    let core = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>Custom title</dc:title><dc:creator>Ada</dc:creator><dcterms:modified xsi:type="dcterms:W3CDTF">2024-01-02T03:04:05Z</dcterms:modified></cp:coreProperties>"#;
    let custom = Document::load(
        Deck::new()
            .slide(DeckSlide::titled("x"))
            .with_part("docProps/core.xml", core.as_bytes())
            .build(),
    )
    .unwrap();
    let props = custom.core_properties().unwrap().unwrap();
    assert_eq!(props.title.as_deref(), Some("Custom title"));
    assert_eq!(props.creator.as_deref(), Some("Ada"));
    assert_eq!(props.modified.as_deref(), Some("2024-01-02T03:04:05Z"));
    let without = Document::load(
        Deck::new()
            .slide(DeckSlide::titled("x"))
            .without_part("docProps/core.xml")
            .without_part("docProps/app.xml")
            .build(),
    )
    .unwrap();
    assert!(without.core_properties().unwrap().is_none());
    assert!(without.app_properties().unwrap().is_none());
}

#[test]
fn embedded_objects_and_alt_text_are_exposed() {
    let shapes = format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="5" name="Object 4" descr="Budget sheet"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></p:xfrm><a:graphic><a:graphicData uri="{P}/ole"><p:oleObj name="Worksheet" r:id="rId9" imgW="100" imgH="100" progId="Excel.Sheet.12"><p:embed/></p:oleObj></a:graphicData></a:graphic></p:graphicFrame><p:pic><p:nvPicPr><p:cNvPr id="6" name="Picture 5" descr="A red square"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId3"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#
    );
    let deck = Deck::new()
        .slide(
            DeckSlide::titled("Objects")
                .shapes_xml(&shapes)
                .rel("rId9", "package", "../embeddings/book.xlsx", false)
                .rel("rId3", "image", "../media/image1.png", false),
        )
        .media("image1.png", PNG, "image/png")
        .with_part("ppt/embeddings/book.xlsx", b"PK-workbook-bytes");
    let doc = Document::load(deck.build()).unwrap();
    let slide = doc.slide(0).unwrap();
    let objects = slide.objects().unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].prog_id.as_deref(), Some("Excel.Sheet.12"));
    assert_eq!(
        objects[0].part.as_deref(),
        Some("/ppt/embeddings/book.xlsx")
    );
    assert_eq!(
        slide.object_bytes(&objects[0]).unwrap(),
        b"PK-workbook-bytes"
    );
    assert_eq!(slide.text(), "Objects");
    let options = TextOptions {
        alt_text: true,
        ..TextOptions::default()
    };
    let mut report = Default::default();
    let text = slide.text_reporting(&options, &mut report);
    assert_eq!(text, "Objects\nBudget sheet\nA red square");
    let _ = R;
}
