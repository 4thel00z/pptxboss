//! ZIP container rules (ECMA-376 Part 2, clause 7.3 and Annex B) and part
//! naming rules (6.2.2).

use pptxboss_core::zip::{METHOD_DEFLATE, METHOD_STORED};

use crate::{Context, Finding, Rule, Severity, Sink};

pub const ZIP001: Rule = Rule {
    code: "ZIP001",
    severity: Severity::Error,
    clause: "Part 2 B.4 Table B.4",
    summary: "Entries use only the stored (0) or deflate (8) compression methods",
};
pub const ZIP002: Rule = Rule {
    code: "ZIP002",
    severity: Severity::Error,
    clause: "Part 2 B.4 Table B.5",
    summary: "Entries are not encrypted (general purpose bit 0)",
};
pub const ZIP003: Rule = Rule {
    code: "ZIP003",
    severity: Severity::Warning,
    clause: "Part 2 B.2",
    summary: "No bytes precede the first local file header",
};
pub const ZIP004: Rule = Rule {
    code: "ZIP004",
    severity: Severity::Error,
    clause: "Part 2 B.4 Table B.1",
    summary: "The central directory and end record are present and complete",
};
pub const ZIP005: Rule = Rule {
    code: "ZIP005",
    severity: Severity::Error,
    clause: "Part 2 6.2.2.3",
    summary: "No two items share a name",
};
pub const ZIP006: Rule = Rule {
    code: "ZIP006",
    severity: Severity::Warning,
    clause: "Part 2 7.3.2",
    summary: "The archive holds no directory entries; parts are the only items",
};
pub const ZIP007: Rule = Rule {
    code: "ZIP007",
    severity: Severity::Error,
    clause: "Part 2 B.2",
    summary: "Each local file header agrees with its central directory header",
};
pub const ZIP008: Rule = Rule {
    code: "ZIP008",
    severity: Severity::Error,
    clause: "Part 2 B.4 Table B.2",
    summary: "Decompressed part data matches the recorded CRC-32",
};
pub const ZIP009: Rule = Rule {
    code: "ZIP009",
    severity: Severity::Warning,
    clause: "Part 2 B.4 Table B.3",
    summary: "Version needed to extract is at most 4.5",
};
pub const ZIP010: Rule = Rule {
    code: "ZIP010",
    severity: Severity::Warning,
    clause: "Part 2 7.3.3",
    summary: "Item names are ASCII, with non-ASCII characters percent-encoded",
};
pub const ZIP011: Rule = Rule {
    code: "ZIP011",
    severity: Severity::Info,
    clause: "Part 2 B.4 Table B.5",
    summary: "General purpose bit 11 (UTF-8 names) is not set",
};
pub const OPC001: Rule = Rule {
    code: "OPC001",
    severity: Severity::Error,
    clause: "Part 2 6.2.2.2",
    summary: "Part names follow the part name grammar",
};
pub const OPC002: Rule = Rule {
    code: "OPC002",
    severity: Severity::Error,
    clause: "Part 2 6.2.2.3",
    summary: "No two part names are equivalent under ASCII case folding",
};
pub const OPC003: Rule = Rule {
    code: "OPC003",
    severity: Severity::Error,
    clause: "Part 2 6.2.2.3",
    summary: "No part name is derivable from another part name",
};
pub const OPC004: Rule = Rule {
    code: "OPC004",
    severity: Severity::Error,
    clause: "Part 2 7.3.7",
    summary: "The content types stream [Content_Types].xml is present",
};
pub const OPC005: Rule = Rule {
    code: "OPC005",
    severity: Severity::Warning,
    clause: "Part 2 7.3.7",
    summary: "The content types stream item is named exactly [Content_Types].xml",
};
pub const OPC006: Rule = Rule {
    code: "OPC006",
    severity: Severity::Error,
    clause: "Part 2 7.2.3.2.1",
    summary: "The content types stream is well-formed XML",
};
pub const OPC007: Rule = Rule {
    code: "OPC007",
    severity: Severity::Error,
    clause: "Part 2 7.2.5.2",
    summary: "Interleaved items form a complete piece sequence from [0].piece to [n].last.piece",
};

pub static CONTAINER_RULES: [Rule; 18] = [
    ZIP001, ZIP002, ZIP003, ZIP004, ZIP005, ZIP006, ZIP007, ZIP008, ZIP009, ZIP010, ZIP011, OPC001,
    OPC002, OPC003, OPC004, OPC005, OPC006, OPC007,
];

pub fn run(ctx: &Context<'_>, sink: &mut Sink<'_>) {
    let package = ctx.package;
    let archive = package.archive();
    let layout = archive.layout();
    if layout.reconstructed {
        sink.push(Finding::new(
            &ZIP004,
            "no central directory; entries were recovered by scanning local file headers",
        ));
    } else if layout.truncated_central {
        sink.push(Finding::new(
            &ZIP004,
            format!(
                "central directory declares {} entries but only {} were readable",
                layout.declared_entries,
                archive.entries().len()
            ),
        ));
    }
    if layout.offset_shift != 0 {
        sink.push(Finding::new(
            &ZIP003,
            format!(
                "{} byte(s) precede the archive; every offset had to be shifted",
                layout.offset_shift
            ),
        ));
    }
    for entry in archive.entries() {
        let name = format!("/{}", entry.name);
        if entry.method != METHOD_STORED && entry.method != METHOD_DEFLATE {
            sink.push(
                Finding::new(&ZIP001, format!("compression method {}", entry.method))
                    .in_part(&name),
            );
        }
        if entry.is_encrypted() {
            sink.push(Finding::new(&ZIP002, "entry is encrypted").in_part(&name));
        }
        if entry.version_needed > 45 {
            sink.push(
                Finding::new(
                    &ZIP009,
                    format!(
                        "version needed to extract is {}.{}",
                        entry.version_needed / 10,
                        entry.version_needed % 10
                    ),
                )
                .in_part(&name),
            );
        }
        if !entry.raw_name.is_ascii() {
            sink.push(Finding::new(&ZIP010, "item name contains non-ASCII bytes").in_part(&name));
        }
        if entry.names_are_utf8() {
            sink.push(Finding::new(&ZIP011, "UTF-8 name flag is set").in_part(&name));
        }
        if layout.reconstructed {
            continue;
        }
        match archive.local_header(entry) {
            Ok(local) => {
                let mut mismatches = Vec::new();
                if local.method != entry.method {
                    mismatches.push("compression method");
                }
                if local.flags & !0x0008 != entry.flags & !0x0008 {
                    mismatches.push("general purpose flags");
                }
                if local.raw_name != entry.raw_name {
                    mismatches.push("file name");
                }
                if !entry.has_data_descriptor() {
                    if u64::from(local.crc32) != u64::from(entry.crc32) {
                        mismatches.push("crc-32");
                    }
                    if u64::from(local.compressed_size) != entry.compressed_size
                        && local.compressed_size != 0xffff_ffff
                    {
                        mismatches.push("compressed size");
                    }
                    if u64::from(local.uncompressed_size) != entry.uncompressed_size
                        && local.uncompressed_size != 0xffff_ffff
                    {
                        mismatches.push("uncompressed size");
                    }
                }
                if !mismatches.is_empty() {
                    sink.push(
                        Finding::new(
                            &ZIP007,
                            format!(
                                "local header differs from the central directory in: {}",
                                mismatches.join(", ")
                            ),
                        )
                        .in_part(&name),
                    );
                }
            }
            Err(err) => sink.push(
                Finding::new(&ZIP007, format!("local header unreadable: {err}")).in_part(&name),
            ),
        }
    }
    for &index in archive.duplicates() {
        if let Some(entry) = archive.get(index) {
            sink.push(
                Finding::new(&ZIP005, "item name repeats an earlier item's name")
                    .in_part(format!("/{}", entry.name)),
            );
        }
    }
    let defects = package.defects();
    for directory in &defects.directories {
        sink.push(
            Finding::new(&ZIP006, "directory entry in a package").in_part(format!("/{directory}")),
        );
    }
    for (name, reason) in &defects.invalid_names {
        sink.push(Finding::new(&OPC001, reason.to_string()).in_part(name));
    }
    for (kept, dropped) in &defects.collisions {
        sink.push(Finding::new(&OPC002, format!("equivalent to {kept}")).in_part(dropped));
    }
    for (derived, base) in &defects.derivable {
        sink.push(Finding::new(&OPC003, format!("derivable from {base}")).in_part(derived));
    }
    for logical in &defects.incomplete_pieces {
        sink.push(
            Finding::new(
                &OPC007,
                "piece sequence has gaps or no single .last piece; not a part",
            )
            .in_part(logical),
        );
    }
    if defects.content_types_missing {
        sink.push(Finding::new(&OPC004, "[Content_Types].xml is missing"));
    }
    if let Some(actual) = &defects.content_types_case {
        sink.push(Finding::new(
            &OPC005,
            format!("content types stream is named {actual}"),
        ));
    }
    if let Some(err) = &defects.content_types_error {
        sink.push(Finding::new(&OPC006, err.to_string()).in_part("/[Content_Types].xml"));
    }
    if let Some(err) = &defects.content_types_unreadable {
        sink.push(
            Finding::new(
                &OPC006,
                format!("content types stream could not be read: {err}"),
            )
            .in_part("/[Content_Types].xml"),
        );
    }
    if ctx.options.verify_crc {
        let mut data = Vec::new();
        for part in package.parts() {
            let is_xml = package
                .content_type_of(&part.name)
                .is_some_and(|ct| ct.ends_with("+xml") || ct.ends_with("/xml"))
                || part.name.ends_with(".rels");
            if !is_xml {
                continue;
            }
            sink.part_read();
            let entries: Vec<usize> = match part.pieces.is_empty() {
                true => vec![part.entry],
                false => part.pieces.clone(),
            };
            for index in entries {
                let Some(entry) = archive.get(index) else {
                    continue;
                };
                if let Err(err) = archive.read(entry, &mut data) {
                    sink.push(
                        Finding::new(&ZIP008, format!("part could not be read: {err}"))
                            .in_part(&part.name),
                    );
                    continue;
                }
                if !pptxboss_core::zip::Archive::crc_matches(entry, &data) {
                    sink.push(
                        Finding::new(
                            &ZIP008,
                            format!(
                                "decompressed data of {} does not match the recorded CRC-32",
                                entry.name
                            ),
                        )
                        .in_part(&part.name),
                    );
                }
            }
        }
    }
}
