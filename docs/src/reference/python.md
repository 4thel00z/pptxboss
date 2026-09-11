# Python reference

The `.pyi` stubs shipped in the package are the authoritative signatures.

## `Document(path=None, *, data=None, threads=None)`

`threads` caps the workers whole-deck calls use; `None` or 0 means every
core (or `PPTXBOSS_THREADS`), 1 stays on the calling thread. Read back as
`threads`.

`slide_count`, `path`, `presentation_part`, `slide_size`, `slide_size_type`;
`len()`, indexing with negative indexes, iteration; `slide(i)`, `slides()`,
`titles()`, `text(...)`, `text_reporting(...)`, `slide_texts(...)`. Text
methods take `notes`, `furniture`, `hidden_shapes`, `hidden_slides`.

## `Slide`

`index`, `number`, `part`, `hidden`, `name`, `title`, `warnings`;
`text(furniture=, hidden_shapes=)`, `paragraphs()`, `notes()`, `tables()`,
`shapes()`, `images()`, `image_bytes(image)`, `hyperlink(rel_id)`.

## `Shape`

`id`, `name`, `kind`, `hidden`, `placeholder`, `placeholder_index`, `text`,
`description`, `hyperlink`, `frame`, `rotation`, `image_rel`, `rows`,
`children`, `is_title`.

## `Image`

`shape_id`, `rel_id`, `part`, `content_type`, `external`.

## `check(path=None, *, data=None, max_findings=1000, verify_crc=True) -> list[Finding]`

## `Finding`

`code`, `severity`, `clause`, `part`, `location`, `message`.

## `rules() -> list[Rule]`

`code`, `severity`, `clause`, `summary`.

## `PptxError`

Raised for any processing error. `ValueError` for bad arguments,
`IndexError` for slide indexes out of range.

## Threading

Documents and slides are frozen and usable from any thread. Calls that
read the archive release the GIL and run on a private materialization of
the document, so calls from different threads run in parallel. Whole-deck
calls (`slides()`, `titles()`, `text()`, `slide_texts()`,
`text_reporting()`) spread slides over the cap set at construction.
