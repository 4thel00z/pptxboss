"""Build .pptx decks: ``Presentation``, ``Slide`` and ``from_markdown``."""

import pptxboss._pptxboss  # registers the extension's write submodule
from pptxboss._pptxboss.write import Paragraph, Presentation, Slide, from_markdown

__all__ = ["Paragraph", "Presentation", "Slide", "from_markdown"]
