"""Build .pptx decks: ``Presentation``, ``Slide``, ``Paragraph``, ``Run``, ``Theme``, ``Background`` and ``from_markdown``."""

import pptxboss._pptxboss  # registers the extension's write submodule
from pptxboss._pptxboss.write import Background, Paragraph, Presentation, Run, Slide, Theme, from_markdown

__all__ = ["Background", "Paragraph", "Presentation", "Run", "Slide", "Theme", "from_markdown"]
