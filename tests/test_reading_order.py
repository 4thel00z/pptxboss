import pytest

import pptxboss
from pptxboss import ReadingOrder, write

def build_number_column_deck() -> pptxboss.Document:
    slide = write.Slide(layout="blank")
    for index, y in enumerate([1.57, 3.0, 4.42]):
        slide = slide.text_box(3.19, y, 0.7, 0.5, [f"0{index + 1}"])
    for index, (label, y) in enumerate([("First sentence.", 1.75), ("Second sentence.", 3.16), ("Third sentence.", 4.57)]):
        slide = slide.text_box(4.06, y, 5.5, 0.5, [label])
    deck = write.Presentation(title="Order")
    deck.add(slide)
    return pptxboss.Document(data=deck.to_bytes())


CONTENT = "01\n02\n03\nFirst sentence.\nSecond sentence.\nThird sentence."
GEOMETRIC = "01 First sentence.\n02 Second sentence.\n03 Third sentence."


def test_content_is_the_default() -> None:
    doc = build_number_column_deck()
    assert doc.text() == CONTENT
    assert doc.text(reading_order="content") == CONTENT
    assert doc.text(reading_order=ReadingOrder.CONTENT) == CONTENT
    assert doc[0].text() == CONTENT


def test_geometric_reads_by_position_as_string_or_enum() -> None:
    doc = build_number_column_deck()
    assert doc.text(reading_order="geometric") == GEOMETRIC
    assert doc.text(reading_order=ReadingOrder.GEOMETRIC) == GEOMETRIC
    assert doc[0].text(reading_order=ReadingOrder.GEOMETRIC) == GEOMETRIC
    assert doc.slide_texts(reading_order="geometric") == [GEOMETRIC]
    assert doc.text_reporting(reading_order="geometric")[0] == GEOMETRIC
    assert doc.extract(reading_order="geometric")[0] == GEOMETRIC


def test_markdown_takes_the_order() -> None:
    doc = build_number_column_deck()
    content = doc.markdown(headings=False)
    assert content.startswith("01\n\n02\n\n03\n\nFirst sentence.")
    geometric = doc.markdown(headings=False, reading_order=ReadingOrder.GEOMETRIC)
    assert geometric == "01 First sentence.\n\n02 Second sentence.\n\n03 Third sentence.\n"
    assert doc[0].markdown(headings=False, reading_order="geometric") == geometric.rstrip("\n")


def test_unknown_order_is_a_value_error() -> None:
    doc = build_number_column_deck()
    with pytest.raises(ValueError, match="reading order"):
        doc.text(reading_order="structure-tree")
    with pytest.raises(ValueError, match="reading order"):
        doc[0].markdown(reading_order="nope")


def test_enum_values() -> None:
    assert [order.value for order in ReadingOrder] == ["content", "geometric"]
    assert "ReadingOrder" in pptxboss.__all__
