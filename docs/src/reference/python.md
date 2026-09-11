# Python reference

The `.pyi` stubs shipped in the package give the exact signatures.

## `Document(path=None, *, data=None, threads=None)`

`threads` caps the workers whole-deck calls use; `None` or 0 means every
core (or `PPTXBOSS_THREADS`), 1 stays on the calling thread. Read back as
`threads`. `format` is `"pptx"` or `"ppt"`; a legacy deck has no
properties, sections, comments or charts, `check()` refuses it, and by
default it stays on the calling thread.

`slide_count`, `path`, `presentation_part`, `slide_size`, `slide_size_type`,
`defects`; `len()`, indexing with negative indexes, iteration; `slide(i)`,
`slides()`, `titles()`, `text(...)`, `text_reporting(...)`, `extract(...)`,
`slide_texts(...)`, `markdown(...)`, `core_properties()`,
`app_properties()`, `sections()`, `comment_authors()`, `presentation()`,
`package()`. Text methods take `notes`, `furniture`, `hidden_shapes`,
`hidden_slides`, `alt_text`, `comments`, `charts`, `diagrams`; `markdown`
takes `headings`, `notes`, `comments`, `hidden_slides`, `hidden_shapes`,
`furniture`, `images`. `extract`, `slide_texts` and `markdown` also take
`indexes`, a list of zero-based slide indexes (negatives count from the
end) read in the written order, which is what the CLI's `--slides` uses.
`text_reporting` returns `(text, warnings)`; `extract` returns
`(text, ExtractReport)`.

## `Slide`

`index`, `number`, `part`, `hidden`, `name`, `title`, `warnings`;
`text(furniture=, hidden_shapes=, alt_text=, charts=, diagrams=)`,
`markdown(...)`, `paragraphs()`, `notes()`, `comments()`, `tables()`,
`shapes()`, `images()`, `image_bytes(image)`, `charts()`, `diagrams()`,
`embedded_objects()`, `object_bytes(object)`, `hyperlink(rel_id)`,
`notes_part()`, `comments_part()`, `layout_part()`.

## `ExtractReport`

What `extract` skipped: `failed_slides`, `failed_notes`,
`failed_comments`, `failed_frames` as `(index, error)` pairs,
`hidden_slides_skipped`, `unknown_graphics`, `unknown_graphic_uris`,
`unknown_elements`; `is_complete` is true when nothing was dropped for a
reason other than the options, and `warnings` gives the same lines as
`text_reporting`.

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

## `Presentation`, `SlideId`, `MasterId`

The parsed presentation part: `slides` and `masters` as `(id, rel_id)`
records, `notes_master` and `handout_master` relationship ids,
`slide_size`, `slide_size_type`, `notes_size`, `first_slide_num`, `rtl`.

## `DocumentDefects`

`located` (`relationship`, `content_type`, `conventional_path` or
`legacy_stream`), `unresolved_slides` as `(position, rel_id)` pairs,
`slides_recovered_from_rels`.

## `CommentAuthor`

`id`, `name`, `initials`.

## `Package`

The raw package, from `Document.package()` or
`Package(path=None, *, data=None)`: `parts()` as `Part` records (`name`,
`content_type`, `size`, `compressed_size`), `has(name)`,
`content_type(name)`, `read(name, raw=False)` (UTF-16 XML comes back as
UTF-8 unless `raw`), `rels(source="/")` as `Relationship` records (`id`,
`type`, `target`, `external`), `resolve(source, rel_id)`,
`content_types()` with `defaults` and `overrides` dicts, and `defects`, a
`PackageDefects` record: `content_types_missing`, `content_types_case`,
`content_types_error`, `content_types_unreadable`, `invalid_names`,
`collisions`, `derivable`, `directories`, `incomplete_pieces`.

## `Comment`

`author`, `initials`, `date`, `text`, `reply`.

## `EmbeddedObject`

`shape_id`, `prog_id`, `rel_id`, `part`, `content_type`, `external`.

## `Shape`

`id`, `name`, `kind`, `hidden`, `placeholder`, `placeholder_index`, `text`,
`description`, `hyperlink`, `frame`, `rotation`, `image_rel`, `rows`,
`table`, `paragraphs`, `children`, `is_title`. `rows` is cell text only;
`table` is the grid with spans and merges; `paragraphs` is the text with
its runs.

## `Table`, `Row`, `Cell`

A table's `column_widths` (EMU) and `rows`; a row's `height` and `cells`;
a cell's `text`, `paragraphs`, `grid_span`, `row_span`, `h_merge`,
`v_merge` and `is_origin`, false for a cell merged into another.

## `Paragraph`, `Run`

A paragraph's `text`, `level`, `bullet` (`inherited`, `none`, `char`,
`auto_number`, `picture`), `bullet_char`, `number_scheme`, `number_start`
and `runs`. A run's `kind` (`text`, `line_break`, `field`), `field`,
`text`, and the formatting as written, None where not set: `bold`,
`italic`, `underline`, `strike`, `size` in hundredths of a point,
`hyperlink` (a relationship id for `Slide.hyperlink`), `lang`, `typeface`.

## `Image`

`shape_id`, `rel_id`, `part`, `content_type`, `external`.

## `check(path=None, *, data=None, max_findings=1000, xml_well_formed=True, verify_crc=True) -> list[Finding]`

## `check_report(...) -> CheckReport`

Same arguments; returns `findings`, `parts_checked`, `truncated` (true
when `max_findings` cut the list short), `errors` and `warnings` counts,
`is_clean` and `codes`.

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
