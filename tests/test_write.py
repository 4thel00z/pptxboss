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


def test_run_formatting_round_trips() -> None:
    deck = write.Presentation()
    deck.add(
        write.Slide("Formatted")
        .bullet("emphasis", italic=True)
        .paragraph("loud", bold=True, size=32)
        .text_box(1.0, 1.0, 4.0, 1.0, [write.Paragraph("Claim", bold=True, italic=True, size=28), "plain"])
        .text_box(1.0, 3.0, 4.0, 1.0, ["a", "b"], bullets=True, italic=True)
    )
    doc = pptxboss.Document(data=deck.to_bytes())
    shapes = doc[0].shapes()
    body = shapes[1].paragraphs
    assert body[0].runs[0].italic is True and body[0].runs[0].bold is None
    assert body[1].runs[0].bold is True and body[1].runs[0].size == 3200
    box = shapes[2].paragraphs
    assert box[0].runs[0].bold is True and box[0].runs[0].italic is True and box[0].runs[0].size == 2800
    assert box[1].runs[0].bold is None and box[1].runs[0].italic is None
    assert all(paragraph.runs[0].italic is True for paragraph in shapes[3].paragraphs)
    assert doc[0].text() == "Formatted\nemphasis\nloud\nClaim\nplain\na\nb"
    assert pptxboss.check(data=deck.to_bytes()) == []


def test_metadata_and_custom_size_round_trip() -> None:
    deck = write.Presentation(
        size=(10.0, 5.625),
        title="Review",
        creator="tests",
        subject="Q3",
        keywords="quarterly, review",
        timestamp="2026-01-02T03:04:05Z",
    )
    deck.add(write.Slide("Only"))
    doc = pptxboss.Document(data=deck.to_bytes())
    assert doc.slide_size == (9144000, 5143500)
    assert doc.slide_size_type is None
    core = doc.core_properties()
    assert core is not None
    assert (core.title, core.creator, core.subject, core.keywords) == ("Review", "tests", "Q3", "quarterly, review")
    assert core.created == "2026-01-02T03:04:05Z" and core.modified == "2026-01-02T03:04:05Z"
    standard = write.from_markdown("# Hi\n", size=(10.0, 7.5))
    assert pptxboss.Document(data=standard.to_bytes()).slide_size_type == "screen4x3"
    with pytest.raises(ValueError):
        write.Presentation(size="letter")
    assert pptxboss.check(data=deck.to_bytes()) == []


def test_runs_theme_and_background() -> None:
    theme = write.Theme.preset("dark").layout_background("title", write.Background.linear("#101010", "accent1"), inverted=False)
    deck = write.Presentation(theme=theme)
    deck.add(write.Slide("Dark deck", subtitle="sub", layout="title"))
    deck.add(
        write.Slide("Runs", background=write.Background.solid("#FFFFFF"), inverted=True)
        .bullet(["Revenue ", write.Run("up 12%", bold=True, color="accent1", link="https://example.com")])
        .paragraph("centered", align="center", color="#FF0000")
    )
    data = deck.to_bytes()
    assert pptxboss.check(data=data) == []
    doc = pptxboss.Document(data=data)
    assert doc[1].text() == "Runs\nRevenue up 12%\ncentered"
    runs = doc[1].shapes()[1].paragraphs[0].runs
    assert runs[1].bold is True and runs[1].hyperlink is not None
    paragraph = write.Paragraph(["a", write.Run("b", italic=True)], bold=True)
    assert paragraph.text == "ab"
    assert [run.bold for run in paragraph.runs] == [True, False]
    assert paragraph.runs[1].italic
    assert write.Run("x", color="abcdef").color == "#ABCDEF"
    custom = write.Theme("mine", colors={"accent1": "#123456"}, font="Georgia", background=write.Background.picture(PNG))
    assert custom.colors["accent1"] == "#123456" and custom.major_font == "Georgia"
    assert write.Theme.presets()[:5] == ["office", "dark", "slate", "forest", "sunset"]
    assert "mocha" in write.Theme.presets() and len(write.Theme.presets()) == 12
    assert pptxboss.check(data=write.Presentation(theme=custom).add(write.Slide("x")).to_bytes()) == []
    themed = write.from_markdown("# Hi\n\n## A\n- b\n", theme=write.Theme.preset("sunset"), font="Inter")
    assert pptxboss.check(data=themed.to_bytes()) == []
    with pytest.raises(ValueError, match="color"):
        write.Run("x", color="teal")
    with pytest.raises(ValueError, match="office"):
        write.Theme.preset("nope")
    with pytest.raises(ValueError, match="align"):
        write.Paragraph("x", align="middle")
    with pytest.raises(ValueError, match="slot"):
        write.Theme("x", colors={"neon": "#000000"})
    with pytest.raises(pptxboss.PptxError, match="background"):
        write.Presentation(theme=write.Theme("x", background=write.Background.picture(b"<svg/>"))).to_bytes()
