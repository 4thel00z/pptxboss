"""pptxboss: read PowerPoint (.pptx) decks with a clean-room Rust engine."""

from pptxboss._pptxboss import Document, Image, PptxError, Shape, Slide, SlideIter, __version__

__all__ = ["Document", "Image", "PptxError", "Shape", "Slide", "SlideIter", "__version__"]
