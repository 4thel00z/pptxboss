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
    def format(self) -> str: ...
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
    def extract(
        self,
        *,
        indexes: list[int] | None = None,
        notes: bool = False,
        furniture: bool = False,
        hidden_shapes: bool = False,
        hidden_slides: bool = True,
        alt_text: bool = False,
        comments: bool = False,
        charts: bool = True,
        diagrams: bool = True,
    ) -> tuple[str, ExtractReport]:
        """The text plus a structured report of what was skipped; `indexes` picks slides (zero-based, negatives from the end) in the written order."""
    def slide_texts(
        self,
        *,
        indexes: list[int] | None = None,
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
        indexes: list[int] | None = None,
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
    def comment_authors(self) -> list[CommentAuthor]: ...
    def package(self) -> Package:
        """The raw package view; raises PptxError for a legacy .ppt deck."""
    @property
    def defects(self) -> DocumentDefects: ...
    def presentation(self) -> Presentation: ...

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
    def notes_part(self) -> str | None: ...
    def comments_part(self) -> str | None: ...
    def layout_part(self) -> str | None: ...

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
    table: Table | None
    paragraphs: list[Paragraph] | None
    children: list[Shape]
    @property
    def is_title(self) -> bool: ...

class Table:
    """A table's cell grid with spans and merges; widths and heights are EMU."""

    column_widths: list[int]
    rows: list[Row]

class Row:
    height: int
    cells: list[Cell]

class Cell:
    """A merged-away cell has h_merge or v_merge set and is_origin False."""

    text: str
    paragraphs: list[Paragraph]
    grid_span: int
    row_span: int
    h_merge: bool
    v_merge: bool
    is_origin: bool

class Paragraph:
    """One paragraph of a text shape or cell, with its runs."""

    text: str
    level: int
    bullet: str
    bullet_char: str | None
    number_scheme: str | None
    number_start: int | None
    runs: list[Run]

class Run:
    """One run with its formatting as written; None means not set. `size` is hundredths of a point."""

    kind: str
    field: str | None
    text: str
    bold: bool | None
    italic: bool | None
    underline: bool | None
    strike: bool | None
    size: int | None
    hyperlink: str | None
    lang: str | None
    typeface: str | None

class ExtractReport:
    """What text extraction skipped or could not read."""

    failed_slides: list[tuple[int, str]]
    failed_notes: list[tuple[int, str]]
    failed_comments: list[tuple[int, str]]
    failed_frames: list[tuple[int, str]]
    hidden_slides_skipped: int
    unknown_graphics: int
    unknown_graphic_uris: list[str]
    unknown_elements: int
    @property
    def is_complete(self) -> bool: ...
    @property
    def warnings(self) -> list[str]: ...

class DocumentDefects:
    """What the document layer worked around to find the slides."""

    located: str
    unresolved_slides: list[tuple[int, str]]
    slides_recovered_from_rels: bool

class Presentation:
    """The parsed presentation part; masters are `r:id` values."""

    slides: list[SlideId]
    masters: list[MasterId]
    notes_master: str | None
    handout_master: str | None
    slide_size: tuple[int, int] | None
    slide_size_type: str | None
    notes_size: tuple[int, int] | None
    first_slide_num: int
    rtl: bool

class SlideId:
    id: int | None
    rel_id: str

class MasterId:
    id: int | None
    rel_id: str

class CommentAuthor:
    id: str
    name: str
    initials: str | None

class Package:
    """The raw package: parts, content types and relationships as written."""

    def __init__(self, path: str | PathLike[str] | None = None, *, data: bytes | None = None) -> None: ...
    def parts(self) -> list[Part]: ...
    def has(self, name: str) -> bool: ...
    def content_type(self, name: str) -> str | None: ...
    def read(self, name: str, *, raw: bool = False) -> bytes:
        """A part's bytes; UTF-16 XML comes back as UTF-8 unless raw=True."""
    def rels(self, source: str = "/") -> list[Relationship]: ...
    def resolve(self, source: str, rel_id: str) -> str | None: ...
    def content_types(self) -> ContentTypes: ...
    @property
    def defects(self) -> PackageDefects: ...

class Part:
    name: str
    content_type: str | None
    size: int
    compressed_size: int

class Relationship:
    id: str
    type: str
    target: str
    external: bool

class ContentTypes:
    defaults: dict[str, str]
    overrides: dict[str, str]

class PackageDefects:
    content_types_missing: bool
    content_types_case: str | None
    content_types_error: str | None
    content_types_unreadable: str | None
    invalid_names: list[tuple[str, str]]
    collisions: list[tuple[str, str]]
    derivable: list[tuple[str, str]]
    directories: list[str]
    incomplete_pieces: list[str]

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

class CheckReport:
    """The verifier's result with how much was checked."""

    findings: list[Finding]
    parts_checked: int
    truncated: bool
    errors: int
    warnings: int
    is_clean: bool
    codes: list[str]

def check(path: str | PathLike[str] | None = None, *, data: bytes | None = None, max_findings: int = 1000, xml_well_formed: bool = True, verify_crc: bool = True) -> list[Finding]:
    """Verifies a deck against ECMA-376 and returns its findings, most severe first."""

def check_report(path: str | PathLike[str] | None = None, *, data: bytes | None = None, max_findings: int = 1000, xml_well_formed: bool = True, verify_crc: bool = True) -> CheckReport:
    """Verifies a deck and returns the findings with parts_checked and truncated."""

def rules() -> list[Rule]:
    """Every rule the verifier knows."""

class write:
    """The `pptxboss.write` submodule: build decks."""

    class Paragraph:
        """One formatted paragraph for text_box; size is in points."""

        def __init__(self, text: str, *, level: int = 0, bullet: bool = False, bold: bool = False, italic: bool = False, size: int | None = None) -> None: ...
        @property
        def text(self) -> str: ...
        @property
        def level(self) -> int: ...
        @property
        def bullet(self) -> bool: ...
        @property
        def bold(self) -> bool: ...
        @property
        def italic(self) -> bool: ...
        @property
        def size(self) -> int | None: ...

    class Slide:
        """One slide under construction; coordinates are inches."""

        def __init__(self, title: str | None = None, *, subtitle: str | None = None, layout: str | None = None, notes: str | None = None, hidden: bool = False) -> None: ...
        def bullet(self, text: str, level: int = 0, *, bold: bool = False, italic: bool = False, size: int | None = None) -> write.Slide: ...
        def paragraph(self, text: str, *, bold: bool = False, italic: bool = False, size: int | None = None) -> write.Slide: ...
        def text_box(self, x: float, y: float, w: float, h: float, lines: list[str | write.Paragraph], *, bullets: bool = False, bold: bool = False, italic: bool = False, size: int | None = None) -> write.Slide: ...
        def table(self, x: float, y: float, w: float, h: float, rows: list[list[str]], *, header: bool = True) -> write.Slide: ...
        def picture(self, data: bytes, x: float, y: float, w: float, h: float, *, description: str | None = None) -> write.Slide: ...
        @property
        def title(self) -> str | None: ...
        @property
        def notes(self) -> str | None: ...

    class Presentation:
        """A deck under construction."""

        def __init__(
            self,
            *,
            size: str | tuple[float, float] = "widescreen",
            font: str | None = None,
            title: str | None = None,
            creator: str | None = None,
            subject: str | None = None,
            keywords: str | None = None,
            timestamp: str | None = None,
        ) -> None:
            """size is widescreen, standard or (width, height) in inches; timestamp is W3C-DTF for created and modified."""
        def add(self, slide: write.Slide) -> write.Presentation: ...
        @property
        def slide_count(self) -> int: ...
        def __len__(self) -> int: ...
        def to_bytes(self) -> bytes: ...
        def save(self, path: str | PathLike[str]) -> None: ...

    @staticmethod
    def from_markdown(markdown: str, *, size: str | tuple[float, float] = "widescreen", font: str | None = None) -> write.Presentation: ...
