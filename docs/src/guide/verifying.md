# Verifying a deck

`pptxboss check` runs 72 structural rules from ECMA-376 Parts 1 and 2 over
a package: the ZIP container, part names, content types, relationships,
required parts, id ranges and uniqueness, XML well-formedness, namespace
consistency and core properties. There is no schema validation; the rules
are the constraints the specification states in prose.

## CLI

```sh
pptxboss check deck.pptx
pptxboss check --quiet deck.pptx        # errors only
pptxboss check --json deck.pptx
pptxboss check --no-crc deck.pptx       # skip CRC-32 of XML parts
pptxboss rules                          # every rule
```

Exit code 0 when no errors were found, 1 when errors were found, 2 when the
file could not be opened at all. Warnings alone exit 0.

Each finding prints as `severity CODE part (location): message [clause]`.

## Python

```python
for finding in pptxboss.check("deck.pptx"):
    finding.severity, finding.code, finding.clause, finding.part, finding.location, finding.message
pptxboss.rules()
```

## Rust

```rust
use pptxboss_check::{check_path, CheckOptions};

let report = check_path("deck.pptx", &CheckOptions::default())?;
for finding in &report.findings {
    println!("{finding}");
}
```

## Severity policy

A violation of a "shall" is an error. A "should", an inconsistency
PowerPoint itself tolerates, or a policy the specification leaves open is a
warning. Anything merely notable is info. The severities were calibrated on
790 public test decks and two decks authored by PowerPoint, which verify
clean.

## Limitations

The verifier reads every XML part once. It does not validate against the
XSD schemas, does not check DrawingML value ranges beyond slide size and
ids, and does not inspect chart, diagram or embedded object parts.
