# Limitations

- Password-protected files (encrypted packages, encrypted `.ppt`) are
  detected and refused, not decrypted.
- Legacy `.ppt` decks: text, titles, notes, hidden flags, slide size and
  pictures are read, and the file is read whole into memory; tables,
  charts, comments and document properties of the binary format are not
  read, and the verifier does not cover it. PowerPoint 95 files are
  refused.
- Interleaved ZIP items (`[n].piece`) are reassembled, but no public test
  deck contains them; the testkit fixture is the only evidence.
- Charts give their cached title, series, categories and values; nothing
  is recomputed from the embedded workbook. Extended charts (`cx:`) inside
  `mc:AlternateContent` fall back to their picture, since their versioned
  namespaces are not claimed as understood. Embedded objects are listed
  with their bytes, not interpreted.
- Text formatting beyond bold, italic, underline, strike, size, language,
  typeface and hyperlinks is not modelled.
- Placeholder inheritance of formatting from layouts and masters is not
  resolved; text is never inherited by design.
- No rendering of slides to images.
- The verifier does not validate against the XSD schemas.
- The writer embeds PNG, JPEG, GIF, BMP and TIFF pictures only, creates no
  charts or diagrams, and does not edit existing decks.
