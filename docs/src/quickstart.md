# Quickstart

## CLI

```sh
pptxboss info deck.pptx
pptxboss text --notes deck.pptx
pptxboss check deck.pptx
pptxboss create md out.pptx slides.md
```

## Python

```python
import pptxboss

doc = pptxboss.Document("deck.pptx")
for slide in doc:
    print(slide.number, slide.title)
    print(slide.text())
print(pptxboss.check("deck.pptx"))
```

## Rust

```rust
use pptxboss_core::Document;

let doc = Document::open("deck.pptx")?;
for slide in doc.slides() {
    let slide = slide?;
    println!("{}: {}", slide.number(), slide.text());
}
```
