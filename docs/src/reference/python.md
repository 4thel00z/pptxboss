# Python reference

The `.pyi` stubs shipped in the package give the exact signatures.

## `Document(path=None, *, data=None, threads=None)`

`threads` caps the workers whole-deck calls use; `None` or 0 means every
core (or `PPTXBOSS_THREADS`), 1 stays on the calling thread. Read back as
`threads`. `format` is `"pptx"` or `"ppt"`; a legacy deck has no
properties, sections, comments or charts, `check()` refuses it, and by
default it stays on the calling thread.

`slide_count`, `path`, `presentation_part`, `slide_size`, `slide_size_type`;
`len()`, indexing with negative indexes, iteration; `slide(i)`, `slides()`,
`titles()`, `text(...)`, `text_reporting(...)`, `slide_texts(...)`,
`markdown(...)`, `core_properties()`, `app_properties()`, `sections()`.
Text methods take `notes`, `furniture`, `hidden_shapes`, `hidden_slides`,
`alt_text`, `comments`, `charts`, `diagrams`; `markdown` takes `headings`,
`notes`, `comments`, `hidden_slides`, `hidden_shapes`, `furniture`,
`images`.

## `Slide`

`index`, `number`, `part`, `hidden`, `name`, `title`, `warnings`;
`text(furniture=, hidden_shapes=, alt_text=, charts=, diagrams=)`,
`markdown(...)`, `paragraphs()`, `notes()`, `comments()`, `tables()`,
`shapes()`, `images()`, `image_bytes(image)`, `charts()`, `diagrams()`,
`embedded_objects()`, `object_bytes(object)`, `hyperlink(rel_id)`.

## `Chart`, `ChartSeries`, `Diagram`

A chart's `shape_id`, `title`, `kinds` (e.g. `barChart`), axis titles and
`series`, each with `name`, `categories` and `values` as written. A
diagram's `shape_id` and `items`, `(level, text)` tuples depth-first.

## `CoreProperties`, `AppProperties`

Every field of `docProps/core.xml` as optional text (`title`, `subject`,
`creator`, `keywords`, `description`, `last_modified_by`, `revision`,
`created`, `modified`, `last_printed`, `category`, `content_status`,
`language`, `identifier`, `version`); `docProps/app.xml` as `application`,
`app_version`, `company`, `manager`, `template`, `presentation_format`,
the counts `slides`, `notes`, `hidden_slides`, `words`, `paragraphs`,
`total_time`, and `titles_of_parts`.

## `Section`

`name`, `slides` (zero-based slide indexes).

## `Comment`

`author`, `initials`, `date`, `text`, `reply`.

## `EmbeddedObject`

`shape_id`, `prog_id`, `rel_id`, `part`, `content_type`, `external`.

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
read the archive release the GIL and run on a private `Document` rebuilt
over the same archive, so calls from different threads run in parallel.
Whole-deck calls (`slides()`, `titles()`, `text()`, `slide_texts()`,
`text_reporting()`) use up to the thread count set at construction.
