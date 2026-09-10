from pathlib import Path
from concurrent.futures import ThreadPoolExecutor

import pytest

import pptxboss


def test_version_is_exported() -> None:
    assert pptxboss.__version__.count(".") == 2


def test_open_and_count(three_slides_pptx: Path) -> None:
    doc = pptxboss.Document(three_slides_pptx)
    assert doc.slide_count == 3
    assert len(doc) == 3
    assert doc.path == str(three_slides_pptx)
    assert doc.presentation_part == "/ppt/presentation.xml"
    assert doc.slide_size == (12192000, 6858000)
    assert doc.slide_size_type is None
    assert repr(doc).startswith("Document(")


def test_open_from_bytes(three_slides_pptx: Path) -> None:
    doc = pptxboss.Document(data=three_slides_pptx.read_bytes())
    assert doc.path is None
    assert doc.slide_count == 3
    with pytest.raises(ValueError):
        pptxboss.Document(three_slides_pptx, data=b"x")
    with pytest.raises(ValueError):
        pptxboss.Document()


def test_slides_titles_text_and_notes(three_slides_pptx: Path) -> None:
    doc = pptxboss.Document(three_slides_pptx)
    first = doc.slide(0)
    assert first.number == 1
    assert first.index == 0
    assert first.part == "/ppt/slides/slide1.xml"
    assert first.title == "First slide"
    assert first.text() == "First slide\nAlpha\nBeta & Gamma"
    assert first.paragraphs() == ["First slide", "Alpha", "Beta & Gamma"]
    assert first.notes() is None
    assert first.warnings == []
    assert not first.hidden
    second = doc[1]
    assert second.notes() == "Speaker notes for slide two"
    third = doc[-1]
    assert third.hidden
    assert third.title == "Hidden slide"
    assert doc.titles() == ["First slide", "Second slide", "Hidden slide"]
    assert doc.text() == "First slide\nAlpha\nBeta & Gamma\n\nSecond slide\nOne\nTwo\n\nHidden slide\nNot shown"
    assert doc.slide_texts(notes=True, hidden_slides=False) == ["First slide\nAlpha\nBeta & Gamma", "Second slide\nOne\nTwo\nSpeaker notes for slide two", ""]
    text, warnings = doc.text_reporting()
    assert text == doc.text()
    assert warnings == []
    assert [slide.number for slide in doc] == [1, 2, 3]
    assert [slide.title for slide in doc.slides()] == ["First slide", "Second slide", "Hidden slide"]
    with pytest.raises(IndexError):
        doc.slide(3)
    with pytest.raises(IndexError):
        doc[-4]


def test_shapes_tables_images_and_hyperlinks(shapes_pptx: Path) -> None:
    doc = pptxboss.Document(shapes_pptx)
    slide = doc.slide(0)
    shapes = slide.shapes()
    kinds = [shape.kind for shape in shapes]
    assert kinds == ["text", "picture", "table", "group", "text"]
    title = shapes[0]
    assert title.is_title
    assert title.placeholder == "title"
    assert title.text == "Shapes"
    picture = shapes[1]
    assert picture.image_rel == "rId3"
    assert picture.description == "a red dot"
    assert picture.frame == (1000000, 2000000, 914400, 914400)
    table = shapes[2]
    assert table.rows == [["Name", "Value"], ["Answer", "42"]]
    assert slide.tables() == [[["Name", "Value"], ["Answer", "42"]]]
    group = shapes[3]
    assert [child.name for child in group.children] == ["TextBox 6"]
    assert group.children[0].text == "grouped link"
    assert slide.text() == "Shapes\nName\tValue\nAnswer\t42\ngrouped link"
    assert slide.text(furniture=True) == "Shapes\nName\tValue\nAnswer\t42\ngrouped link\n1"
    images = slide.images()
    assert len(images) == 1
    assert images[0].part == "/ppt/media/image1.png"
    assert images[0].content_type == "image/png"
    assert slide.image_bytes(images[0])[:4] == b"\x89PNG"
    assert slide.hyperlink("rId4") == "https://example.com/"
    assert slide.hyperlink("rId99") is None
    assert repr(shapes[1]).startswith("Shape(id=4")


def test_errors_are_pptx_errors(tmp_path: Path) -> None:
    junk = tmp_path / "junk.pptx"
    junk.write_bytes(b"not a package")
    with pytest.raises(pptxboss.PptxError, match="not a zip archive"):
        pptxboss.Document(junk)
    with pytest.raises(pptxboss.PptxError):
        pptxboss.Document(tmp_path / "missing.pptx")
    assert issubclass(pptxboss.PptxError, Exception)


def test_documents_are_usable_from_threads(three_slides_pptx: Path) -> None:
    doc = pptxboss.Document(three_slides_pptx)
    with ThreadPoolExecutor(max_workers=4) as pool:
        titles = list(pool.map(lambda index: doc.slide(index).title, [0, 1, 2] * 4))
    assert titles == ["First slide", "Second slide", "Hidden slide"] * 4
