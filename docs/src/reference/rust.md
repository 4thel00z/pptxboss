# Rust reference

| Crate | Start here |
|---|---|
| `pptxboss-core` | `Document::open`, `Document::slide`, `Slide::text`, `Document::map_slides`; `Package` for the raw view; `zip::Archive`, `xml::Reader`, `opc`, `model` |
| `pptxboss-check` | `check`, `check_path`, `check_bytes`, `rules`, `Finding`, `Severity`, `CheckOptions` |
| `pptxboss-write` | `Presentation`, `Slide`, `Paragraph`, `Rect`, `SlideSize`, `Layout`, `from_markdown` |
| `pptxboss-cli` | the binary |

`Document` is single-threaded; `Document::seed()` gives a `Send + Sync`
handle from which any thread rebuilds a `Document` over the same archive
with private caches. `Package` works the same way.

Every error type is a `thiserror` enum per crate: `pptxboss_core::Error`,
`pptxboss_write::Error`.
