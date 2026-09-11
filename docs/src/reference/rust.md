# Rust reference

| Crate | Start here |
|---|---|
| `pptxboss-core` | `Document::open`, `Document::slide`, `Slide::text`, `Document::map_slides`, `Document::markdown`, `Document::core_properties`, `Document::sections`, `Slide::comments`, `Slide::charts`, `Slide::diagrams`, `Slide::objects`; `Package` for the raw view; `zip::Archive`, `inflate`, `xml::Reader`, `opc`, `model`, `chart`, `diagram`, `markdown`; `cfb::Compound` and `ppt::LegacyDeck` behind `.ppt` files (`Document::legacy`) |
| `pptxboss-check` | `check`, `check_path`, `check_bytes`, `rules`, `Finding`, `Severity`, `CheckOptions` |
| `pptxboss-write` | `Presentation`, `Slide`, `Paragraph`, `Rect`, `SlideSize`, `Layout`, `from_markdown` |
| `pptxboss-cli` | the binary |

`Document` is single-threaded; `Document::seed()` gives a `Send + Sync`
handle from which any thread rebuilds a `Document` over the same archive
with private caches. `Package` works the same way. `Document::map_slides`
spreads slides over every core unless `Document::with_threads` (or
`set_threads`, or the `PPTXBOSS_THREADS` variable) caps the workers; the
seed carries the cap. A legacy `.ppt` deck stays on the calling thread
unless a cap is set explicitly, because its slides are too cheap to
spread.

Every error type is a `thiserror` enum per crate: `pptxboss_core::Error`,
`pptxboss_write::Error`.
