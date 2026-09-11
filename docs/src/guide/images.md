# Extracting images

Pictures are `p:pic` shapes whose `a:blip` names an image part through a
relationship. Embedded object previews count too.

```python
for image in slide.images():
    image.shape_id, image.rel_id, image.part, image.content_type, image.external
    data = slide.image_bytes(image)        # bytes of the image part
```

```rust
for image in slide.images()? {
    let bytes = slide.image_bytes(&image)?;
}
```

Image parts are read with positioned reads, so a deck's media costs nothing
until asked for. Linked images (`r:link`) resolve to an external target
and have no bytes in the package. Embedded objects (`p:oleObj`) are listed
by `Slide::objects` with their `progId` and part, and `Slide::object_bytes`
reads them. Charts and diagrams are not rendered; `Slide::charts` and
`Slide::diagrams` give their text and data.
