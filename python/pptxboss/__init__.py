"""pptxboss: read, verify and create PowerPoint (.pptx) decks with a clean-room Rust engine."""

from enum import StrEnum

from pptxboss import write
from pptxboss._pptxboss import (
    AppProperties,
    Cell,
    Chart,
    ChartSeries,
    CheckReport,
    Comment,
    CommentAuthor,
    ContentTypes,
    CoreProperties,
    Diagram,
    Document,
    DocumentDefects,
    EmbeddedObject,
    ExtractReport,
    Finding,
    Image,
    MasterId,
    Package,
    PackageDefects,
    Paragraph,
    Part,
    PptxError,
    Presentation,
    Relationship,
    Row,
    Rule,
    Run,
    Section,
    Shape,
    Slide,
    SlideId,
    SlideIter,
    Table,
    __version__,
    check,
    check_report,
    rules,
)


class ReadingOrder(StrEnum):
    """The order a slide's shapes are read in. The text and Markdown methods
    take one as their ``reading_order`` keyword, as this enum or as its string
    value."""

    CONTENT = "content"
    """The order the slide lists its shapes in (the default)."""

    GEOMETRIC = "geometric"
    """Position alone: rows top to bottom, shapes in a row left to right."""


__all__ = [
    "AppProperties",
    "Cell",
    "Chart",
    "ChartSeries",
    "CheckReport",
    "Comment",
    "CommentAuthor",
    "ContentTypes",
    "CoreProperties",
    "Diagram",
    "Document",
    "DocumentDefects",
    "EmbeddedObject",
    "ExtractReport",
    "Finding",
    "Image",
    "MasterId",
    "Package",
    "PackageDefects",
    "Paragraph",
    "Part",
    "PptxError",
    "Presentation",
    "ReadingOrder",
    "Relationship",
    "Row",
    "Rule",
    "Run",
    "Section",
    "Shape",
    "Slide",
    "SlideId",
    "SlideIter",
    "Table",
    "__version__",
    "check",
    "check_report",
    "rules",
    "write",
]
