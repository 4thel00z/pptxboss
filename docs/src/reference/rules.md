# Verifier rules

`pptxboss rules` prints the current list with severities and clauses. The
codes are stable. Families:

| Prefix | Covers | Clauses |
|---|---|---|
| `ZIP` | container records, compression methods, flags, header consistency, CRC-32, duplicate items, directories | Part 2 7.3 and Annex B |
| `OPC` | part name grammar, equivalence, derivability, the content types stream | Part 2 6.2.2, 7.2.3, 7.3.7 |
| `CTY` | Default and Override elements, media type syntax, every part typed | Part 2 6.2.3, 7.2.3 |
| `REL` | Relationships parts, ids, targets, reachability | Part 2 6.5 |
| `PKG` | the package relationships: one presentation, core properties, thumbnails | Part 1 13.3.6, 15.2; Part 2 8.2 |
| `XML` | well-formedness, encodings, Strict versus Transitional mixing | Part 2 6.2.5; Part 4 7 |
| `PML` | presentation and slide-family structure, id ranges and uniqueness, required relationships | Part 1 13.3, 19.2, 19.3, 19.7, 20.1.2.2.8 |
| `CPR` | core properties | Part 2 8 |
