//! Writes the checked-in test decks under `tests/fixtures/`.
//!
//! `cargo run -p pptxboss-testkit --example gen_fixtures`

use std::path::Path;

use pptxboss_testkit::{Deck, DeckSlide};

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xdd, 0x8d, 0xb0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let fixtures: Vec<(&str, Deck)> = vec![
        ("minimal.pptx", Deck::new().slide(DeckSlide::titled("Hello, world"))),
        (
            "three-slides.pptx",
            Deck::new()
                .slide(DeckSlide::titled("First slide").bullet("Alpha").bullet("Beta & Gamma"))
                .slide(DeckSlide::titled("Second slide").bullet("One").bullet("Two").notes("Speaker notes for slide two"))
                .slide(DeckSlide::titled("Hidden slide").bullet("Not shown").hidden()),
        ),
        (
            "shapes.pptx",
            Deck::new()
                .slide(
                    DeckSlide::titled("Shapes")
                        .shapes_xml(concat!(
                            r#"<p:pic><p:nvPicPr><p:cNvPr id="4" name="Picture 3" descr="a red dot"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId3"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="1000000" y="2000000"/><a:ext cx="914400" cy="914400"/></a:xfrm></p:spPr></p:pic>"#,
                            r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="5" name="Table 4"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="3500000"/><a:ext cx="6000000" cy="1000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1" bandRow="1"/><a:tblGrid><a:gridCol w="3000000"/><a:gridCol w="3000000"/></a:tblGrid><a:tr h="500000"><a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Name</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc><a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Value</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc></a:tr><a:tr h="500000"><a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Answer</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc><a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>42</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
                            r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="6" name="Group 5"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="7000000" y="2000000"/><a:ext cx="2000000" cy="1000000"/><a:chOff x="0" y="0"/><a:chExt cx="2000000" cy="1000000"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="7" name="TextBox 6"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="en-US"><a:hlinkClick r:id="rId4"/></a:rPr><a:t>grouped link</a:t></a:r></a:p></p:txBody></p:sp></p:grpSp>"#,
                            r#"<p:sp><p:nvSpPr><p:cNvPr id="8" name="Slide Number Placeholder 7"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldNum" sz="quarter" idx="12"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:fld id="{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}" type="slidenum"><a:rPr lang="en-US"/><a:t>1</a:t></a:fld></a:p></p:txBody></p:sp>"#
                        ))
                        .rel("rId3", "image", "../media/image1.png", false)
                        .rel("rId4", "hyperlink", "https://example.com/", true),
                )
                .media("image1.png", PNG, "image/png"),
        ),
    ];
    for (name, deck) in fixtures {
        let path = dir.join(name);
        std::fs::write(&path, deck.build()).expect("fixture written");
        println!("wrote {}", path.display());
    }
}
