"""The raw package view, defects, the presentation model and part names."""

from pathlib import Path

import pytest

import pptxboss

SLIDE_TYPE = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml"
PRESENTATION_TYPE = "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"


def test_package_lists_parts_and_reads_them(three_slides_pptx: Path) -> None:
    package = pptxboss.Document(three_slides_pptx).package()
    names = [part.name for part in package.parts()]
    assert "/ppt/presentation.xml" in names and "/ppt/slides/slide1.xml" in names
    slide = next(part for part in package.parts() if part.name == "/ppt/slides/slide1.xml")
    assert slide.content_type == SLIDE_TYPE
    assert slide.size > 0 and slide.compressed_size > 0
    assert package.has("/ppt/presentation.xml")
    assert not package.has("/ppt/missing.xml")
    assert package.content_type("/ppt/presentation.xml") == PRESENTATION_TYPE
    xml = package.read("/ppt/slides/slide1.xml")
    assert xml.startswith(b"<?xml") and b"First slide" in xml
    assert package.read("/ppt/slides/slide1.xml", raw=True) == xml
    with pytest.raises(pptxboss.PptxError):
        package.read("/ppt/missing.xml")
    assert repr(package).startswith("Package(")


def test_package_opens_standalone_from_path_or_bytes(three_slides_pptx: Path) -> None:
    from_path = pptxboss.Package(three_slides_pptx)
    from_bytes = pptxboss.Package(data=three_slides_pptx.read_bytes())
    assert [p.name for p in from_path.parts()] == [p.name for p in from_bytes.parts()]
    with pytest.raises(ValueError):
        pptxboss.Package(three_slides_pptx, data=b"x")
    with pytest.raises(ValueError):
        pptxboss.Package()


def test_package_relationships_and_content_types(three_slides_pptx: Path) -> None:
    package = pptxboss.Package(three_slides_pptx)
    office = next(rel for rel in package.rels() if rel.type.endswith("/officeDocument"))
    assert office.id.startswith("rId")
    assert office.external is False
    assert package.resolve("/", office.id) == "/ppt/presentation.xml"
    slides = [rel for rel in package.rels("/ppt/presentation.xml") if rel.type.endswith("/slide")]
    assert len(slides) == 3
    assert package.resolve("/ppt/presentation.xml", slides[0].id) == "/ppt/slides/slide1.xml"
    assert package.resolve("/ppt/presentation.xml", "rId999") is None
    assert package.rels("/ppt/slides/slide1.xml")
    types = package.content_types()
    assert types.overrides["/ppt/presentation.xml"] == PRESENTATION_TYPE
    assert "rels" in types.defaults


def test_package_and_document_defects(three_slides_pptx: Path) -> None:
    doc = pptxboss.Document(three_slides_pptx)
    defects = doc.package().defects
    assert defects.content_types_missing is False
    assert defects.invalid_names == [] and defects.collisions == [] and defects.directories == []
    assert defects.content_types_error is None
    assert doc.defects.located == "relationship"
    assert doc.defects.unresolved_slides == []
    assert doc.defects.slides_recovered_from_rels is False


def test_legacy_deck_has_no_package(legacy_ppt: Path) -> None:
    doc = pptxboss.Document(legacy_ppt)
    assert doc.defects.located == "legacy_stream"
    with pytest.raises(pptxboss.PptxError):
        doc.package()


def test_presentation_model(three_slides_pptx: Path) -> None:
    presentation = pptxboss.Document(three_slides_pptx).presentation()
    assert [slide.rel_id.startswith("rId") for slide in presentation.slides] == [True, True, True]
    assert all(slide.id is None or slide.id >= 256 for slide in presentation.slides)
    assert len(presentation.masters) == 1
    assert presentation.slide_size == (12192000, 6858000)
    assert presentation.slide_size_type is None
    assert presentation.first_slide_num == 1
    assert presentation.rtl is False
    assert presentation.notes_master is None or presentation.notes_master.startswith("rId")


def test_comment_authors_and_slide_parts(features_pptx: Path, three_slides_pptx: Path) -> None:
    features = pptxboss.Document(features_pptx)
    authors = features.comment_authors()
    assert [(author.name, author.initials) for author in authors] == [("Ada Lovelace", "AL")]
    assert authors[0].id
    assert features[0].comments_part() is not None
    assert features[2].comments_part() is None
    deck = pptxboss.Document(three_slides_pptx)
    assert deck[0].notes_part() is None
    assert deck[1].notes_part().startswith("/ppt/notesSlides/")
    assert deck[0].layout_part().startswith("/ppt/slideLayouts/")
