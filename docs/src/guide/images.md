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
and have no bytes in the package. Charts, diagrams and embedded objects are
reported by kind and relationship id, not rendered.
