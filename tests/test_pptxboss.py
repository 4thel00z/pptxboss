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


def test_check_reports_clause_numbered_findings(three_slides_pptx: Path, tmp_path: Path) -> None:
    assert pptxboss.check(three_slides_pptx) == []
    assert pptxboss.check(data=three_slides_pptx.read_bytes()) == []
    codes = {rule.code for rule in pptxboss.rules()}
    assert "REL004" in codes and "PML003" in codes
    assert all(rule.clause.startswith("Part ") for rule in pptxboss.rules())
    junk = tmp_path / "junk.pptx"
    junk.write_bytes(b"nope")
    with pytest.raises(pptxboss.PptxError):
        pptxboss.check(junk)
    with pytest.raises(ValueError):
        pptxboss.check()


def test_check_finds_a_missing_part(three_slides_pptx: Path, tmp_path: Path) -> None:
    import zipfile

    broken = tmp_path / "broken.pptx"
    with zipfile.ZipFile(three_slides_pptx) as src, zipfile.ZipFile(broken, "w", zipfile.ZIP_DEFLATED) as dst:
        for item in src.infolist():
            if item.filename == "ppt/presProps.xml":
                continue
            dst.writestr(item, src.read(item.filename))
    findings = pptxboss.check(broken)
    codes = [finding.code for finding in findings]
    assert "PML003" in codes and "REL004" in codes
    first = findings[0]
    assert first.severity == "error"
    assert first.clause.startswith("Part ")
    assert "PML003" in str(next(f for f in findings if f.code == "PML003"))
    assert pptxboss.check(broken, max_findings=1) != [] and len(pptxboss.check(broken, max_findings=1)) == 1


def test_threads_cap_gives_the_same_text(three_slides_pptx: Path) -> None:
    every_core = pptxboss.Document(three_slides_pptx)
    one = pptxboss.Document(three_slides_pptx, threads=1)
    assert every_core.threads == 0
    assert one.threads == 1
    assert one.text() == every_core.text()
    assert one.slide_texts() == every_core.slide_texts()
    assert [s.title for s in one.slides()] == [s.title for s in every_core.slides()]


def test_properties_sections_comments_and_objects(features_pptx: Path) -> None:
    doc = pptxboss.Document(features_pptx)
    core = doc.core_properties()
    assert core is not None and isinstance(core, pptxboss.CoreProperties)
    app = doc.app_properties()
    assert app is not None and isinstance(app, pptxboss.AppProperties)
    sections = doc.sections()
    assert [(s.name, s.slides) for s in sections] == [("Opening", [0, 1]), ("Closing", [2])]
    first = doc[0]
    comments = first.comments()
    assert len(comments) == 1
    assert comments[0].author == "Ada Lovelace"
    assert comments[0].initials == "AL"
    assert comments[0].text == "Tighten this point"
    assert comments[0].reply is False
    assert first.text() == "Commented\nA point"
    assert first.text(alt_text=True) == "Commented\nA point\nA blue diagram"
    objects = doc[1].embedded_objects()
    assert len(objects) == 1
    assert objects[0].prog_id == "Excel.Sheet.12"
    assert objects[0].part == "/ppt/embeddings/oleObject1.xlsx"
    assert doc[1].object_bytes(objects[0]) == b"PK-workbook-bytes"
    assert doc[2].comments() == [] and doc[2].embedded_objects() == []
    text = doc.text(comments=True, alt_text=True)
    assert "[comment] Ada Lovelace: Tighten this point" in text
    assert "Budget sheet" in text
    assert "[comment]" not in doc.text()
