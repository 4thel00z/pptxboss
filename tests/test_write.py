from pathlib import Path

import pytest

import pptxboss
from pptxboss import write

PNG = bytes.fromhex("89504e470d0a1a0a0000000d49484452000000010000000108020000009077053de0000000c4944415408d763f8cfc0000000301010018dd8db00000000049454e44ae426082")


def build() -> write.Presentation:
    deck = write.Presentation(title="Round trip", creator="tests")
    deck.add(write.Slide("Deck title", subtitle="A subtitle", layout="title"))
    deck.add(write.Slide("Bullets", notes="say hi").bullet("one").bullet("nested", level=1).paragraph("plain"))
    deck.add(
        write.Slide("Shapes")
        .text_box(1.0, 1.5, 4.0, 1.0, ["Free text"], bold=True, size=28)
        .table(1.0, 3.0, 6.0, 1.5, [["Name", "Value"], ["Answer", "42"]])
        .picture(PNG, 8.0, 1.5, 1.0, 1.0, description="a dot")
    )
    deck.add(write.Slide(layout="blank", hidden=True))
    return deck


def test_build_read_back_and_verify(tmp_path: Path) -> None:
    deck = build()
    assert deck.slide_count == 4 and len(deck) == 4
    data = deck.to_bytes()
    assert data == build().to_bytes()
    assert pptxboss.check(data=data) == []
    doc = pptxboss.Document(data=data)
    assert doc.titles() == ["Deck title", "Bullets", "Shapes", None]
    assert doc[0].text() == "Deck title\nA subtitle"
    assert doc[1].text() == "Bullets\none\nnested\nplain"
    assert doc[1].notes() == "say hi"
    assert doc[2].text() == "Shapes\nFree text\nName\tValue\nAnswer\t42"
    assert doc[2].tables() == [[["Name", "Value"], ["Answer", "42"]]]
    images = doc[2].images()
    assert len(images) == 1 and doc[2].image_bytes(images[0]) == PNG
    assert doc[3].hidden
    out = tmp_path / "deck.pptx"
    deck.save(out)
    assert pptxboss.Document(out).slide_count == 4
    assert pptxboss.check(out) == []


def test_markdown_and_errors() -> None:
    deck = write.from_markdown("# Hi\nSub\n\n## Agenda\n- a\n- b\n", size="standard", font="Arial")
    doc = pptxboss.Document(data=deck.to_bytes())
    assert doc.slide_size_type == "screen4x3"
    assert doc.text() == "Hi\nSub\n\nAgenda\na\nb"
    with pytest.raises(ValueError):
        write.Presentation(size="huge")
    with pytest.raises(ValueError):
        write.Slide("x", layout="fancy")
    ragged = write.Presentation().add(write.Slide("t").table(1, 1, 2, 1, [["a", "b"], ["c"]]))
    with pytest.raises(pptxboss.PptxError, match="cells"):
        ragged.to_bytes()
    svg = write.Presentation().add(write.Slide("t").picture(b"<svg/>", 1, 1, 1, 1))
    with pytest.raises(pptxboss.PptxError, match="image"):
        svg.to_bytes()
    assert repr(write.Slide("t")).startswith("write.Slide(")
