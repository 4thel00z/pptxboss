# Limitations

- Encrypted packages and legacy binary `.ppt` files are OLE compound files;
  they are detected and refused, not read.
- UTF-16 encoded XML parts are reported and skipped.
- Interleaved ZIP items (`[n].piece`) are not reassembled.
- Charts, diagrams and embedded objects are recognized and skipped; their
  text is not extracted.
- Text formatting beyond bold, italic, underline, strike, size, language,
  typeface and hyperlinks is not modelled.
- Placeholder inheritance of formatting from layouts and masters is not
  resolved; text is never inherited by design.
- No rendering of slides to images.
- The verifier does not validate against the XSD schemas.
- The writer embeds PNG, JPEG, GIF, BMP and TIFF pictures only, creates no
  charts or diagrams, and does not edit existing decks.
