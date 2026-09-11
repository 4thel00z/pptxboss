from collections.abc import Iterator
from os import PathLike

__version__: str

class PptxError(Exception):
    """Raised for any PowerPoint processing error (bad data, unreadable parts, I/O)."""

class Document:
    """An open presentation. Thread-safe; heavy calls release the GIL."""

    def __init__(
        self, path: str | PathLike[str] | None = None, *, data: bytes | None = None, threads: int | None = None
    ) -> None: ...
    @property
    def slide_count(self) -> int: ...
    @property
    def threads(self) -> int: ...
    @property
    def path(self) -> str | None: ...
    @property
    def presentation_part(self) -> str: ...
    @property
    def slide_size(self) -> tuple[int, int] | None: ...
    @property
    def slide_size_type(self) -> str | None: ...
    def __len__(self) -> int: ...
    def __getitem__(self, index: int) -> Slide: ...
    def __iter__(self) -> SlideIter: ...
    def slide(self, index: int) -> Slide: ...
    def slides(self) -> list[Slide]: ...
    def text(
        self,
        *,
        notes: bool = False,
        furniture: bool = False,
        hidden_shapes: bool = False,
        hidden_slides: bool = True,
        alt_text: bool = False,
        comments: bool = False,
        charts: bool = True,
        diagrams: bool = True,
    ) -> str: ...
    def text_reporting(
        self,
        *,
        notes: bool = False,
        furniture: bool = False,
        hidden_shapes: bool = False,
        hidden_slides: bool = True,
        alt_text: bool = False,
        comments: bool = False,
        charts: bool = True,
        diagrams: bool = True,
    ) -> tuple[str, list[str]]: ...
    def slide_texts(
        self,
        *,
        notes: bool = False,
        furniture: bool = False,
        hidden_shapes: bool = False,
        hidden_slides: bool = True,
        alt_text: bool = False,
        comments: bool = False,
        charts: bool = True,
        diagrams: bool = True,
    ) -> list[str]: ...
    def titles(self) -> list[str | None]: ...
    def markdown(
        self,
        *,
        headings: bool = True,
        notes: bool = False,
        comments: bool = False,
        hidden_slides: bool = True,
        hidden_shapes: bool = False,
        furniture: bool = False,
        images: bool = True,
    ) -> str: ...
    def core_properties(self) -> CoreProperties | None: ...
    def app_properties(self) -> AppProperties | None: ...
    def sections(self) -> list[Section]: ...

class SlideIter(Iterator[Slide]):
    def __iter__(self) -> SlideIter: ...
    def __next__(self) -> Slide: ...

class Slide:
    """One parsed slide."""

    @property
    def index(self) -> int: ...
    @property
    def number(self) -> int: ...
    @property
    def part(self) -> str: ...
    @property
    def hidden(self) -> bool: ...
    @property
    def name(self) -> str | None: ...
    @property
    def title(self) -> str | None: ...
    @property
    def warnings(self) -> list[str]: ...
    def text(
        self,
        *,
        furniture: bool = False,
        hidden_shapes: bool = False,
        alt_text: bool = False,
        charts: bool = True,
        diagrams: bool = True,
    ) -> str: ...
    def markdown(
        self,
        *,
        headings: bool = True,
        notes: bool = False,
        comments: bool = False,
        hidden_shapes: bool = False,
        furniture: bool = False,
        images: bool = True,
    ) -> str: ...
    def paragraphs(self) -> list[str]: ...
    def notes(self) -> str | None: ...
    def comments(self) -> list[Comment]: ...
    def charts(self) -> list[Chart]: ...
    def diagrams(self) -> list[Diagram]: ...
    def embedded_objects(self) -> list[EmbeddedObject]: ...
    def object_bytes(self, object: EmbeddedObject) -> bytes: ...
    def tables(self) -> list[list[list[str]]]: ...
    def shapes(self) -> list[Shape]: ...
    def images(self) -> list[Image]: ...
    def image_bytes(self, image: Image) -> bytes: ...
    def hyperlink(self, rel_id: str) -> str | None: ...

class Shape:
    """One node of a slide's shape tree."""

    id: int
    name: str
    kind: str
    hidden: bool
    placeholder: str | None
    placeholder_index: int | None
    text: str | None
    description: str | None
    hyperlink: str | None
    frame: tuple[int, int, int, int] | None
    rotation: int
    image_rel: str | None
    rows: list[list[str]] | None
    children: list[Shape]
    @property
    def is_title(self) -> bool: ...

class ChartSeries:
    """One series of a chart: name, category labels and values as written."""

    name: str | None
    categories: list[str]
    values: list[str]

class Chart:
    """A chart on a slide: its cached words and numbers."""

    shape_id: int
    title: str | None
    kinds: list[str]
    category_axis_title: str | None
    value_axis_title: str | None
    series: list[ChartSeries]

class Diagram:
    """A diagram (SmartArt) on a slide: (level, text) per node, depth-first."""

    shape_id: int
    items: list[tuple[int, str]]

class CoreProperties:
    """The Core Properties part (docProps/core.xml), every field optional text."""

    title: str | None
    subject: str | None
    creator: str | None
    keywords: str | None
    description: str | None
    last_modified_by: str | None
    revision: str | None
    created: str | None
    modified: str | None
    last_printed: str | None
    category: str | None
    content_status: str | None
    language: str | None
    identifier: str | None
    version: str | None

class AppProperties:
    """The Extended Properties part (docProps/app.xml)."""

    application: str | None
    app_version: str | None
    company: str | None
    manager: str | None
    template: str | None
    presentation_format: str | None
    slides: int | None
    notes: int | None
    hidden_slides: int | None
    words: int | None
    paragraphs: int | None
    total_time: int | None
    titles_of_parts: list[str]

class Section:
    """A section of the slide list; `slides` holds zero-based slide indexes."""

    name: str
    slides: list[int]

class Comment:
    """A comment on a slide; replies follow their parent with `reply` set."""

    author: str | None
    initials: str | None
    date: str | None
    text: str
    reply: bool

class EmbeddedObject:
    """An embedded object (p:oleObj) on a slide."""

    shape_id: int
    prog_id: str | None
    rel_id: str | None
    part: str | None
    content_type: str | None
    external: str | None

class Image:
    """An image referenced from a slide."""

    shape_id: int
    rel_id: str
    part: str | None
    content_type: str | None
    external: str | None

class Finding:
    """One verifier finding."""

    code: str
    severity: str
    clause: str
    part: str | None
    location: str | None
    message: str

class Rule:
    """One verifier rule."""

    code: str
    severity: str
    clause: str
    summary: str

def check(path: str | PathLike[str] | None = None, *, data: bytes | None = None, max_findings: int = 1000, verify_crc: bool = True) -> list[Finding]:
    """Verifies a deck against ECMA-376 and returns its findings, most severe first."""

def rules() -> list[Rule]:
    """Every rule the verifier knows."""

class write:
    """The `pptxboss.write` submodule: build decks."""

    class Slide:
        """One slide under construction; coordinates are inches."""

        def __init__(self, title: str | None = None, *, subtitle: str | None = None, layout: str | None = None, notes: str | None = None, hidden: bool = False) -> None: ...
        def bullet(self, text: str, level: int = 0) -> write.Slide: ...
        def paragraph(self, text: str) -> write.Slide: ...
        def text_box(self, x: float, y: float, w: float, h: float, lines: list[str], *, bullets: bool = False, bold: bool = False, size: int | None = None) -> write.Slide: ...
        def table(self, x: float, y: float, w: float, h: float, rows: list[list[str]], *, header: bool = True) -> write.Slide: ...
        def picture(self, data: bytes, x: float, y: float, w: float, h: float, *, description: str | None = None) -> write.Slide: ...
        @property
        def title(self) -> str | None: ...
        @property
        def notes(self) -> str | None: ...

    class Presentation:
        """A deck under construction."""

        def __init__(self, *, size: str = "widescreen", font: str | None = None, title: str | None = None, creator: str | None = None) -> None: ...
        def add(self, slide: write.Slide) -> write.Presentation: ...
        @property
        def slide_count(self) -> int: ...
        def __len__(self) -> int: ...
        def to_bytes(self) -> bytes: ...
        def save(self, path: str | PathLike[str]) -> None: ...

    @staticmethod
    def from_markdown(markdown: str, *, size: str = "widescreen", font: str | None = None) -> write.Presentation: ...
