//! One fixture per rule: a deck that violates exactly that rule, checked
//! against the minimal deck as the negative case.

use pptxboss_check::{check_bytes, CheckOptions, Report, Severity};
use pptxboss_testkit::{Deck, DeckSlide, ZipBuilder};

fn run(bytes: Vec<u8>) -> Report {
    check_bytes(bytes, &CheckOptions::default()).unwrap()
}

fn codes(report: &Report) -> Vec<&'static str> {
    report.codes()
}

fn deck() -> Deck {
    Deck::new().slide(DeckSlide::titled("Hello").bullet("World").notes("notes"))
}

fn assert_only(report: &Report, code: &str) {
    let found = codes(report);
    assert!(
        found.contains(&code),
        "expected {code}, found {found:?}: {:#?}",
        report.findings
    );
    let others: Vec<_> = found.iter().filter(|c| **c != code).collect();
    assert!(
        others.is_empty(),
        "expected only {code}, also found {others:?}: {:#?}",
        report.findings
    );
}

#[test]
fn the_minimal_deck_is_clean() {
    let report = run(deck().build());
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
    assert!(report.is_clean());
    assert!(report.parts_checked > 5);
}

#[test]
fn zip001_unknown_compression_method() {
    let parts = deck().parts();
    let mut builder = ZipBuilder::new().with_method_code(12);
    for (name, bytes) in &parts {
        builder = builder.stored(name, bytes);
    }
    let report = run(builder.build());
    assert!(codes(&report).contains(&"ZIP001"), "{:?}", codes(&report));
    assert!(codes(&report).contains(&"OPC006"), "{:?}", codes(&report));
}

#[test]
fn zip003_junk_before_the_archive() {
    let parts = deck().parts();
    let mut builder = ZipBuilder::new().with_prefix(b"JUNK");
    for (name, bytes) in &parts {
        builder = builder.deflated(name, bytes);
    }
    assert_only(&run(builder.build()), "ZIP003");
}

#[test]
fn zip004_missing_central_directory() {
    let bytes = deck().build();
    let central = bytes
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap();
    let report = run(bytes[..central].to_vec());
    assert!(codes(&report).contains(&"ZIP004"), "{:?}", codes(&report));
}

#[test]
fn zip005_duplicate_item_names() {
    let parts = deck().parts();
    let mut builder = ZipBuilder::new();
    for (name, bytes) in &parts {
        builder = builder.deflated(name, bytes);
    }
    builder = builder.deflated("ppt/slides/slide1.xml", b"<p:sld/>");
    assert_only(&run(builder.build()), "ZIP005");
}

#[test]
fn zip006_directory_entries() {
    let parts = deck().parts();
    let mut builder = ZipBuilder::new().stored("ppt/", b"");
    for (name, bytes) in &parts {
        builder = builder.deflated(name, bytes);
    }
    assert_only(&run(builder.build()), "ZIP006");
}

#[test]
fn zip007_local_header_disagrees() {
    let mut bytes = deck().build();
    bytes[8] = 99;
    let report = run(bytes);
    assert!(codes(&report).contains(&"ZIP007"), "{:?}", codes(&report));
}

#[test]
fn zip008_crc_mismatch() {
    let mut bytes = deck().build();
    let locals: Vec<usize> = (0..bytes.len() - 4)
        .filter(|&i| bytes[i..i + 4] == [0x50, 0x4b, 0x03, 0x04])
        .collect();
    let centrals: Vec<usize> = (0..bytes.len() - 4)
        .filter(|&i| bytes[i..i + 4] == [0x50, 0x4b, 0x01, 0x02])
        .collect();
    bytes[locals[1] + 14] ^= 0xff;
    bytes[centrals[1] + 16] ^= 0xff;
    let report = run(bytes);
    assert_only(&report, "ZIP008");
    assert_eq!(report.findings[0].part.as_deref(), Some("/_rels/.rels"));
}

#[test]
fn zip010_non_ascii_name_and_zip011_utf8_flag() {
    let parts = deck().parts();
    let mut builder = ZipBuilder::new().with_utf8_flag();
    for (name, bytes) in &parts {
        builder = builder.deflated(name, bytes);
    }
    let report = run(builder.build());
    assert_only(&report, "ZIP011");
    assert_eq!(report.findings[0].severity, Severity::Info);
    let report = run(deck().with_part("ppt/m\u{e9}dia.bin", b"x").build());
    assert!(codes(&report).contains(&"ZIP010"), "{:?}", codes(&report));
}

#[test]
fn opc001_invalid_part_name() {
    let report = run(deck().with_part("ppt/bad name.xml", b"<x/>").build());
    assert!(codes(&report).contains(&"OPC001"), "{:?}", codes(&report));
}

#[test]
fn opc002_equivalent_part_names() {
    let report = run(deck().with_part("PPT/slides/SLIDE1.xml", b"<x/>").build());
    assert!(codes(&report).contains(&"OPC002"), "{:?}", codes(&report));
}

#[test]
fn opc003_derivable_part_names() {
    let report = run(deck().with_part("ppt/slides", b"prefix").build());
    assert!(codes(&report).contains(&"OPC003"), "{:?}", codes(&report));
}

#[test]
fn opc004_missing_content_types() {
    let report = run(deck().without_part("[Content_Types].xml").build());
    assert!(codes(&report).contains(&"OPC004"), "{:?}", codes(&report));
}

#[test]
fn opc006_malformed_content_types() {
    let report = run(deck().with_part("[Content_Types].xml", b"<Types").build());
    assert!(codes(&report).contains(&"OPC006"), "{:?}", codes(&report));
}

fn content_types_with(extra: &str) -> Vec<u8> {
    let parts = deck().parts();
    let (_, types) = parts
        .iter()
        .find(|(name, _)| name == "[Content_Types].xml")
        .unwrap();
    let text = String::from_utf8(types.clone())
        .unwrap()
        .replace("</Types>", &format!("{extra}</Types>"));
    text.into_bytes()
}

#[test]
fn cty001_part_without_content_type() {
    let report = run(deck().with_part("ppt/media/blob.zzz", b"data").build());
    assert!(codes(&report).contains(&"CTY001"), "{:?}", codes(&report));
}

#[test]
fn cty002_duplicate_default() {
    let types = content_types_with(r#"<Default Extension="XML" ContentType="text/xml"/>"#);
    assert_only(
        &run(deck().with_part("[Content_Types].xml", &types).build()),
        "CTY002",
    );
}

#[test]
fn cty003_duplicate_override() {
    let types = content_types_with(
        r#"<Override PartName="/PPT/presentation.xml" ContentType="application/xml"/>"#,
    );
    assert_only(
        &run(deck().with_part("[Content_Types].xml", &types).build()),
        "CTY003",
    );
}

#[test]
fn cty004_override_for_missing_part() {
    let types = content_types_with(
        r#"<Override PartName="/ppt/ghost.xml" ContentType="application/xml"/>"#,
    );
    assert_only(
        &run(deck().with_part("[Content_Types].xml", &types).build()),
        "CTY004",
    );
}

#[test]
fn cty005_unexpected_element() {
    let types = content_types_with(r#"<Bogus/>"#);
    assert_only(
        &run(deck().with_part("[Content_Types].xml", &types).build()),
        "CTY005",
    );
}

#[test]
fn cty006_bad_media_type() {
    let types = content_types_with(r#"<Default Extension="zzz" ContentType="not a media type"/>"#);
    assert_only(
        &run(deck().with_part("[Content_Types].xml", &types).build()),
        "CTY006",
    );
}

#[test]
fn cty008_extension_with_dot() {
    let types =
        content_types_with(r#"<Default Extension="tar.gz" ContentType="application/gzip"/>"#);
    assert_only(
        &run(deck().with_part("[Content_Types].xml", &types).build()),
        "CTY008",
    );
}

fn slide_rels(extra: &str) -> Vec<u8> {
    let parts = deck().parts();
    let (_, rels) = parts
        .iter()
        .find(|(name, _)| name == "ppt/slides/_rels/slide1.xml.rels")
        .unwrap();
    String::from_utf8(rels.clone())
        .unwrap()
        .replace("</Relationships>", &format!("{extra}</Relationships>"))
        .into_bytes()
}

#[test]
fn rel001_malformed_rels() {
    let report = run(deck()
        .with_part("ppt/slides/_rels/slide1.xml.rels", b"<Relationships")
        .build());
    assert!(codes(&report).contains(&"REL001"), "{:?}", codes(&report));
}

#[test]
fn rel002_duplicate_relationship_id() {
    let rels = slide_rels(
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/>"#,
    );
    assert_only(
        &run(deck()
            .with_part("ppt/slides/_rels/slide1.xml.rels", &rels)
            .build()),
        "REL002",
    );
}

#[test]
fn rel003_relationship_without_target() {
    let rels = slide_rels(
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"/>"#,
    );
    assert_only(
        &run(deck()
            .with_part("ppt/slides/_rels/slide1.xml.rels", &rels)
            .build()),
        "REL003",
    );
}

#[test]
fn rel004_dangling_internal_target() {
    let rels = slide_rels(
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/missing.png"/>"#,
    );
    assert_only(
        &run(deck()
            .with_part("ppt/slides/_rels/slide1.xml.rels", &rels)
            .build()),
        "REL004",
    );
}

#[test]
fn rel005_target_is_a_rels_part() {
    let rels = slide_rels(
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="_rels/slide1.xml.rels"/>"#,
    );
    assert_only(
        &run(deck()
            .with_part("ppt/slides/_rels/slide1.xml.rels", &rels)
            .build()),
        "REL005",
    );
}

#[test]
fn rel006_id_is_not_an_xml_name() {
    let rels = slide_rels(
        r#"<Relationship Id="9 id" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/>"#,
    );
    assert_only(
        &run(deck()
            .with_part("ppt/slides/_rels/slide1.xml.rels", &rels)
            .build()),
        "REL006",
    );
}

#[test]
fn rel007_orphan_rels_part() {
    let report = run(deck().with_part("ppt/slides/_rels/slide9.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#).build());
    assert_only(&report, "REL007");
}

#[test]
fn rel008_unreachable_part() {
    let types = content_types_with(
        r#"<Override PartName="/ppt/extra.xml" ContentType="application/xml"/>"#,
    );
    let report = run(deck()
        .with_part("[Content_Types].xml", &types)
        .with_part("ppt/extra.xml", b"<x/>")
        .build());
    assert_only(&report, "REL008");
}

#[test]
fn rel009_internal_target_with_scheme() {
    let rels = slide_rels(
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com"/>"#,
    );
    assert_only(
        &run(deck()
            .with_part("ppt/slides/_rels/slide1.xml.rels", &rels)
            .build()),
        "REL009",
    );
}

#[test]
fn rel010_empty_type() {
    let rels = slide_rels(
        r#"<Relationship Id="rId9" Type="" Target="https://example.com" TargetMode="External"/>"#,
    );
    assert_only(
        &run(deck()
            .with_part("ppt/slides/_rels/slide1.xml.rels", &rels)
            .build()),
        "REL010",
    );
}

#[test]
fn pkg001_no_office_document_relationship() {
    let report = run(deck().with_part("_rels/.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#).build());
    assert!(codes(&report).contains(&"PKG001"), "{:?}", codes(&report));
}

#[test]
fn pkg002_presentation_with_wrong_content_type() {
    let parts = deck().parts();
    let (_, types) = parts
        .iter()
        .find(|(name, _)| name == "[Content_Types].xml")
        .unwrap();
    let text = String::from_utf8(types.clone()).unwrap().replace(
        "presentationml.presentation.main+xml",
        "presentationml.slide+xml",
    );
    let report = run(deck()
        .with_part("[Content_Types].xml", text.as_bytes())
        .build());
    assert!(codes(&report).contains(&"PKG002"), "{:?}", codes(&report));
}

#[test]
fn pkg003_two_core_properties_relationships() {
    let parts = deck().parts();
    let (_, rels) = parts
        .iter()
        .find(|(name, _)| name == "_rels/.rels")
        .unwrap();
    let text = String::from_utf8(rels.clone()).unwrap().replace("</Relationships>", r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#);
    assert_only(
        &run(deck().with_part("_rels/.rels", text.as_bytes()).build()),
        "PKG003",
    );
}

#[test]
fn pkg004_two_thumbnails() {
    let parts = deck().parts();
    let (_, rels) = parts
        .iter()
        .find(|(name, _)| name == "_rels/.rels")
        .unwrap();
    let text = String::from_utf8(rels.clone()).unwrap().replace("</Relationships>", r#"<Relationship Id="rId8" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail" Target="docProps/core.xml"/><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail" Target="docProps/app.xml"/></Relationships>"#);
    assert_only(
        &run(deck().with_part("_rels/.rels", text.as_bytes()).build()),
        "PKG004",
    );
}

#[test]
fn xml001_malformed_slide() {
    let report = run(deck().with_part("ppt/slides/slide1.xml", b"<p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld></p:sld>").build());
    assert!(codes(&report).contains(&"XML001"), "{:?}", codes(&report));
}

#[test]
fn xml001_dtd_and_control_character() {
    let report = run(deck().with_part("ppt/presProps.xml", b"<!DOCTYPE x><p:presentationPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>").build());
    assert!(codes(&report).contains(&"XML001"), "{:?}", codes(&report));
    let report = run(deck().with_part("ppt/presProps.xml", b"<p:presentationPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">\x01</p:presentationPr>").build());
    assert!(codes(&report).contains(&"XML001"), "{:?}", codes(&report));
}

#[test]
fn xml002_foreign_encoding_declaration() {
    let report = run(deck().with_part("ppt/presProps.xml", b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><p:presentationPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>").build());
    assert_only(&report, "XML002");
}

#[test]
fn xml003_mixed_conformance_classes() {
    let strict = br#"<p:presentationPr xmlns:p="http://purl.oclc.org/ooxml/presentationml/main"/>"#;
    assert_only(
        &run(deck().with_part("ppt/presProps.xml", strict).build()),
        "XML003",
    );
}

fn presentation_xml(replace: &str, with: &str) -> Vec<u8> {
    let parts = deck().parts();
    let (_, pres) = parts
        .iter()
        .find(|(name, _)| name == "ppt/presentation.xml")
        .unwrap();
    let text = String::from_utf8(pres.clone()).unwrap();
    assert!(text.contains(replace), "{replace} not in presentation.xml");
    text.replace(replace, with).into_bytes()
}

#[test]
fn pml002_no_master() {
    let pres = presentation_xml(
        r#"<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>"#,
        "",
    );
    let report = run(deck().with_part("ppt/presentation.xml", &pres).build());
    assert!(codes(&report).contains(&"PML002"), "{:?}", codes(&report));
}

#[test]
fn pml003_no_pres_props() {
    let report = run(deck().without_part("ppt/presProps.xml").build());
    assert!(codes(&report).contains(&"PML003"), "{:?}", codes(&report));
    assert!(codes(&report).contains(&"REL004"), "{:?}", codes(&report));
}

#[test]
fn pml004_slide_id_out_of_range() {
    let pres = presentation_xml(r#"<p:sldId id="256""#, r#"<p:sldId id="7""#);
    assert_only(
        &run(deck().with_part("ppt/presentation.xml", &pres).build()),
        "PML004",
    );
}

#[test]
fn pml005_duplicate_slide_ids() {
    let two = Deck::new()
        .slide(DeckSlide::titled("a"))
        .slide(DeckSlide::titled("b"));
    let parts = two.parts();
    let (_, pres) = parts
        .iter()
        .find(|(name, _)| name == "ppt/presentation.xml")
        .unwrap();
    let text = String::from_utf8(pres.clone())
        .unwrap()
        .replace(r#"<p:sldId id="257""#, r#"<p:sldId id="256""#);
    assert_only(
        &run(two
            .with_part("ppt/presentation.xml", text.as_bytes())
            .build()),
        "PML005",
    );
}

#[test]
fn pml006_slide_relationship_does_not_resolve() {
    let parts = deck().parts();
    let (_, pres) = parts
        .iter()
        .find(|(name, _)| name == "ppt/presentation.xml")
        .unwrap();
    let text = String::from_utf8(pres.clone()).unwrap();
    let start = text.find(r#"<p:sldId id="256" r:id=""#).unwrap();
    let end = start + text[start..].find("/>").unwrap() + 2;
    let text = format!(
        "{}{}{}",
        &text[..start],
        r#"<p:sldId id="256" r:id="rId77"/>"#,
        &text[end..]
    );
    let report = run(deck()
        .with_part("ppt/presentation.xml", text.as_bytes())
        .build());
    assert!(codes(&report).contains(&"PML006"), "{:?}", codes(&report));
}

#[test]
fn pml008_master_id_too_small() {
    let pres = presentation_xml(
        r#"<p:sldMasterId id="2147483648""#,
        r#"<p:sldMasterId id="5""#,
    );
    assert_only(
        &run(deck().with_part("ppt/presentation.xml", &pres).build()),
        "PML008",
    );
}

#[test]
fn pml010_slide_size_out_of_range() {
    let report = run(deck().slide_size(100, 6858000).build());
    assert_only(&report, "PML010");
}

#[test]
fn pml011_slide_size_type_disagrees_with_ratio() {
    let pres = presentation_xml(
        r#"<p:sldSz cx="12192000" cy="6858000"/>"#,
        r#"<p:sldSz cx="12192000" cy="6858000" type="screen4x3"/>"#,
    );
    let report = run(deck().with_part("ppt/presentation.xml", &pres).build());
    assert_only(&report, "PML011");
    assert_eq!(report.findings[0].severity, Severity::Warning);
}

#[test]
fn pml012_missing_notes_size() {
    let pres = presentation_xml(r#"<p:notesSz cx="6858000" cy="9144000"/>"#, "");
    assert_only(
        &run(deck().with_part("ppt/presentation.xml", &pres).build()),
        "PML012",
    );
}

#[test]
fn pml013_notes_master_relationship_without_list() {
    let pres = presentation_xml(
        r#"<p:notesMasterIdLst><p:notesMasterId r:id="rId2"/></p:notesMasterIdLst>"#,
        "",
    );
    let report = run(deck().with_part("ppt/presentation.xml", &pres).build());
    assert_only(&report, "PML013");
}

#[test]
fn pml014_wrong_root_element() {
    let report = run(deck()
        .with_part(
            "ppt/presProps.xml",
            br#"<p:viewPr xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#,
        )
        .build());
    assert_only(&report, "PML014");
}

#[test]
fn pml015_slide_without_layout_relationship() {
    let report = run(deck().with_part("ppt/slides/_rels/slide1.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide1.xml"/></Relationships>"#).build());
    assert!(codes(&report).contains(&"PML015"), "{:?}", codes(&report));
}

#[test]
fn pml016_layout_without_master_relationship() {
    let report = run(deck().with_part("ppt/slideLayouts/_rels/slideLayout1.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#).build());
    assert!(codes(&report).contains(&"PML016"), "{:?}", codes(&report));
}

#[test]
fn pml017_master_layout_id_does_not_resolve() {
    let parts = deck().parts();
    let (_, master) = parts
        .iter()
        .find(|(name, _)| name == "ppt/slideMasters/slideMaster1.xml")
        .unwrap();
    let text = String::from_utf8(master.clone()).unwrap().replace(r#"<p:sldLayoutId id="2147483649" r:id="rId1"/>"#, r#"<p:sldLayoutId id="2147483649" r:id="rId1"/><p:sldLayoutId id="2147483650" r:id="rId9"/>"#);
    let report = run(deck()
        .with_part("ppt/slideMasters/slideMaster1.xml", text.as_bytes())
        .build());
    assert!(codes(&report).contains(&"PML017"), "{:?}", codes(&report));
    assert!(codes(&report).contains(&"PML020"), "{:?}", codes(&report));
}

#[test]
fn pml018_notes_slide_without_master_relationship() {
    let report = run(deck().with_part("ppt/notesSlides/_rels/notesSlide1.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="../slides/slide1.xml"/></Relationships>"#).build());
    assert!(codes(&report).contains(&"PML018"), "{:?}", codes(&report));
}

#[test]
fn pml019_duplicate_shape_ids() {
    let extra = r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Dup"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/></p:sp>"#;
    let report = run(Deck::new()
        .slide(DeckSlide::titled("t").shapes_xml(extra))
        .build());
    assert_only(&report, "PML019");
}

#[test]
fn pml020_unresolved_r_id() {
    let extra = r#"<p:pic><p:nvPicPr><p:cNvPr id="9" name="Pic"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId42"/></p:blipFill><p:spPr/></p:pic>"#;
    let report = run(Deck::new()
        .slide(DeckSlide::titled("t").shapes_xml(extra))
        .build());
    assert_only(&report, "PML020");
}

#[test]
fn pml021_master_target_with_wrong_content_type() {
    let parts = deck().parts();
    let (_, types) = parts
        .iter()
        .find(|(name, _)| name == "[Content_Types].xml")
        .unwrap();
    let text = String::from_utf8(types.clone()).unwrap().replace(
        "presentationml.slideMaster+xml",
        "presentationml.slideLayout+xml",
    );
    let report = run(deck()
        .with_part("[Content_Types].xml", text.as_bytes())
        .build());
    assert!(codes(&report).contains(&"PML021"), "{:?}", codes(&report));
}

#[test]
fn pml022_no_slides_is_informational() {
    let report = run(Deck::new().build());
    assert_only(&report, "PML022");
    assert_eq!(report.findings[0].severity, Severity::Info);
    assert!(report.is_clean());
}

#[test]
fn pml023_slide_relates_to_a_forbidden_part_kind() {
    let rels = slide_rels(
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps" Target="../presProps.xml"/>"#,
    );
    assert_only(
        &run(deck()
            .with_part("ppt/slides/_rels/slide1.xml.rels", &rels)
            .build()),
        "PML023",
    );
}

fn core_props(body: &str) -> Vec<u8> {
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">{body}</cp:coreProperties>"#).into_bytes()
}

#[test]
fn cpr001_wrong_root() {
    let report = run(deck()
        .with_part("docProps/core.xml", br#"<x xmlns="urn:x"/>"#)
        .build());
    assert_only(&report, "CPR001");
}

#[test]
fn cpr002_created_without_xsi_type() {
    let report = run(deck()
        .with_part(
            "docProps/core.xml",
            &core_props("<dcterms:created>2026-01-01</dcterms:created>"),
        )
        .build());
    assert_only(&report, "CPR002");
}

#[test]
fn cpr003_repeated_element() {
    let report = run(deck()
        .with_part(
            "docProps/core.xml",
            &core_props("<dc:title>a</dc:title><dc:title>b</dc:title>"),
        )
        .build());
    assert_only(&report, "CPR003");
}

#[test]
fn cpr004_unknown_element() {
    let report = run(deck()
        .with_part("docProps/core.xml", &core_props("<cp:mood>fine</cp:mood>"))
        .build());
    assert_only(&report, "CPR004");
}

#[test]
fn max_findings_truncates() {
    let mut deck = Deck::new();
    for i in 0..5 {
        deck = deck.with_part(&format!("ppt/orphan{i}.xml"), b"<x/>");
    }
    let report = check_bytes(
        deck.build(),
        &CheckOptions {
            max_findings: 2,
            ..CheckOptions::default()
        },
    )
    .unwrap();
    assert_eq!(report.findings.len(), 2);
    assert!(report.truncated);
}

fn utf16le_with_bom(bytes: &[u8]) -> Vec<u8> {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let mut out = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        out.extend(unit.to_le_bytes());
    }
    out
}

/// The deck rebuilt entry by entry, with `slide1.xml` replaced by `pieces`.
fn deck_with_slide_pieces(pieces: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = ZipBuilder::new();
    for (name, bytes) in deck().parts() {
        if name != "ppt/slides/slide1.xml" {
            builder = builder.deflated(&name, &bytes);
            continue;
        }
        for (piece, data) in pieces {
            builder = builder.deflated(piece, data);
        }
    }
    builder.build()
}

#[test]
fn opc007_incomplete_piece_sequence() {
    let slide = deck()
        .parts()
        .into_iter()
        .find(|(name, _)| name == "ppt/slides/slide1.xml")
        .unwrap()
        .1;
    let complete = deck_with_slide_pieces(&[
        ("ppt/slides/slide1.xml/[0].piece", &slide[..40]),
        ("ppt/slides/slide1.xml/[1].last.piece", &slide[40..]),
    ]);
    assert!(!codes(&run(complete)).contains(&"OPC007"));
    let gap = deck_with_slide_pieces(&[
        ("ppt/slides/slide1.xml/[0].piece", &slide[..40]),
        ("ppt/slides/slide1.xml/[2].last.piece", &slide[40..]),
    ]);
    let report = run(gap);
    assert!(codes(&report).contains(&"OPC007"), "{:?}", codes(&report));
}

#[test]
fn xml004_utf16_part_is_noted_and_still_checked() {
    let mut builder = ZipBuilder::new();
    for (name, bytes) in deck().parts() {
        let data = match name == "ppt/slides/slide1.xml" {
            true => utf16le_with_bom(&bytes),
            false => bytes,
        };
        builder = builder.deflated(&name, &data);
    }
    let report = run(builder.build());
    let finding = report
        .findings
        .iter()
        .find(|finding| finding.code == "XML004")
        .expect("XML004 reported");
    assert_eq!(finding.severity, Severity::Info);
    assert!(!codes(&report).contains(&"XML001"));
    assert!(report.is_clean(), "{:?}", codes(&report));
}
