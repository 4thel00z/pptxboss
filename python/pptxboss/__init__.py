"""pptxboss: read, verify and create PowerPoint (.pptx) decks with a clean-room Rust engine."""

from pptxboss import write
from pptxboss._pptxboss import (
    AppProperties,
    Comment,
    CoreProperties,
    Document,
    EmbeddedObject,
    Finding,
    Image,
    PptxError,
    Rule,
    Section,
    Shape,
    Slide,
    SlideIter,
    __version__,
    check,
    rules,
)

__all__ = [
    "AppProperties",
    "Comment",
    "CoreProperties",
    "Document",
    "EmbeddedObject",
    "Finding",
    "Image",
    "PptxError",
    "Rule",
    "Section",
    "Shape",
    "Slide",
    "SlideIter",
    "__version__",
    "check",
    "rules",
    "write",
]
