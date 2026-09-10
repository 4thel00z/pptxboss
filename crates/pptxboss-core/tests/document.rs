use pptxboss_core::document::Located;
use pptxboss_core::{Content, Document, Error, TextOptions};
use pptxboss_testkit::{Deck, DeckSlide};

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
