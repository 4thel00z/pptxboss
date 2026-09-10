"""pptxboss: read and verify PowerPoint (.pptx) decks with a clean-room Rust engine."""

from pptxboss._pptxboss import Document, Finding, Image, PptxError, Rule, Shape, Slide, SlideIter, __version__, check, rules

__all__ = ["Document", "Finding", "Image", "PptxError", "Rule", "Shape", "Slide", "SlideIter", "__version__", "check", "rules"]
