# Markdown to slides

```sh
pptxboss create md out.pptx slides.md
cat slides.md | pptxboss create md out.pptx -
```

The converter is line-oriented:

| Markdown | Result |
|---|---|
| `# Title` | a title slide; the next paragraph becomes its subtitle |
| `## Title`, `### Title` | a new content slide |
| `---`, `***` | a new untitled slide |
| `- item`, `* item`, `+ item`, `1. item`, `1) item` | a bullet; two spaces of indentation per level |
| `Notes: text` | speaker notes for the current slide until a blank line |
| fenced code | plain paragraphs |
| any other line | a body paragraph without a bullet |

Inline `**bold**`, `*italic*`, `_emphasis_` and backtick markers are
stripped; underscores inside words stay.

```rust
let deck = pptxboss_write::from_markdown(&markdown).font("Arial");
deck.write_to("out.pptx")?;
```
