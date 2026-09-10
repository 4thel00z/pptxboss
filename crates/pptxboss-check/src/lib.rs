//! A lean verifier for `.pptx` packages against ECMA-376.
//!
//! Each rule is one function with one code, one clause reference and a
//! default severity. Rules run over the raw [`Package`] (which keeps every
//! defect as written) and over the parsed presentation. The set is
//! deliberately structural: container records, part names, content types,
//! relationships, required parts, id ranges and uniqueness, XML
//! well-formedness and namespace consistency. There is no schema
//! validation.
//!
//! Severity policy: a violation of a "shall" in the specification is an
//! `Error`; a "should", an inconsistency PowerPoint itself tolerates, or a
//! policy the specification leaves open is a `Warning`; anything merely
//! notable is `Info`.

use std::fmt;

use pptxboss_core::Package;
use serde::Serialize;

mod container;
mod content_types;
mod core_props;
mod pml;
mod relationships;
mod xml_parts;

pub use container::CONTAINER_RULES;
pub use content_types::CONTENT_TYPE_RULES;
pub use core_props::CORE_PROPERTY_RULES;
pub use pml::PRESENTATION_RULES;
pub use relationships::RELATIONSHIP_RULES;
pub use xml_parts::XML_RULES;

/// How serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        })
    }
}

/// The static description of a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Rule {
    /// Stable identifier such as `REL004`.
    pub code: &'static str,
    pub severity: Severity,
    /// The ECMA-376 clause the rule enforces, e.g. `Part 2 6.5.3.4`.
    pub clause: &'static str,
    /// One sentence saying what the rule checks.
    pub summary: &'static str,
}

/// One violation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub clause: &'static str,
    /// The part the finding is about, when there is one.
    pub part: Option<String>,
    /// Where inside the part: a byte offset, an element, an id.
    pub location: Option<String>,
    pub message: String,
}

impl Finding {
    pub fn new(rule: &Rule, message: impl Into<String>) -> Self {
        Self {
            code: rule.code,
            severity: rule.severity,
            clause: rule.clause,
            part: None,
            location: None,
            message: message.into(),
        }
    }

    pub fn in_part(mut self, part: impl Into<String>) -> Self {
        self.part = Some(part.into());
        self
    }

    pub fn at(mut self, location: impl fmt::Display) -> Self {
        self.location = Some(location.to_string());
        self
    }

    /// Overrides the rule's default severity.
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:<7} {} ", self.severity, self.code)?;
        if let Some(part) = &self.part {
            write!(f, "{part}")?;
            if let Some(location) = &self.location {
                write!(f, " ({location})")?;
            }
            write!(f, ": ")?;
        }
        write!(f, "{} [{}]", self.message, self.clause)
    }
}

/// Knobs for a run.
#[derive(Clone, Debug)]
pub struct CheckOptions {
    /// Stop collecting after this many findings.
    pub max_findings: usize,
    /// Tokenize every XML part strictly for well-formedness (reads every XML part).
    pub xml_well_formed: bool,
    /// Recompute CRC-32 for every part that is read.
    pub verify_crc: bool,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            max_findings: 1000,
            xml_well_formed: true,
            verify_crc: true,
        }
    }
}

/// The result of a run.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// Parts the verifier read.
    pub parts_checked: usize,
    /// True when `max_findings` cut the list short.
    pub truncated: bool,
}

impl Report {
    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Warning)
            .count()
    }

    pub fn is_clean(&self) -> bool {
        self.errors() == 0
    }

    /// Codes present, in first-seen order.
    pub fn codes(&self) -> Vec<&'static str> {
        let mut codes = Vec::new();
        for finding in &self.findings {
            if !codes.contains(&finding.code) {
                codes.push(finding.code);
            }
        }
        codes
    }

    fn push(&mut self, finding: Finding, options: &CheckOptions) {
        if self.findings.len() >= options.max_findings {
            self.truncated = true;
            return;
        }
        self.findings.push(finding);
    }
}

/// Everything a rule can look at.
pub struct Context<'a> {
    pub package: &'a Package,
    pub options: &'a CheckOptions,
}

/// Collects findings; rules append here.
pub struct Sink<'a> {
    report: &'a mut Report,
    options: &'a CheckOptions,
}

impl Sink<'_> {
    pub fn push(&mut self, finding: Finding) {
        self.report.push(finding, self.options);
    }

    pub fn part_read(&mut self) {
        self.report.parts_checked += 1;
    }
}

/// Every rule the verifier knows, in the order they run.
pub fn rules() -> Vec<&'static Rule> {
    CONTAINER_RULES
        .iter()
        .chain(CONTENT_TYPE_RULES.iter())
        .chain(RELATIONSHIP_RULES.iter())
        .chain(XML_RULES.iter())
        .chain(PRESENTATION_RULES.iter())
        .chain(CORE_PROPERTY_RULES.iter())
        .collect()
}

/// Runs every rule over an opened package.
pub fn check(package: &Package, options: &CheckOptions) -> Report {
    let mut report = Report::default();
    let ctx = Context { package, options };
    let mut sink = Sink {
        report: &mut report,
        options,
    };
    container::run(&ctx, &mut sink);
    content_types::run(&ctx, &mut sink);
    relationships::run(&ctx, &mut sink);
    xml_parts::run(&ctx, &mut sink);
    pml::run(&ctx, &mut sink);
    core_props::run(&ctx, &mut sink);
    report.findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.code.cmp(b.code))
            .then_with(|| a.part.cmp(&b.part))
    });
    report
}

/// Opens `path` and runs every rule.
pub fn check_path(
    path: impl AsRef<std::path::Path>,
    options: &CheckOptions,
) -> pptxboss_core::Result<Report> {
    let package = Package::open(path)?;
    Ok(check(&package, options))
}

/// Runs every rule over in-memory bytes.
pub fn check_bytes(bytes: Vec<u8>, options: &CheckOptions) -> pptxboss_core::Result<Report> {
    let package = Package::from_bytes(bytes)?;
    Ok(check(&package, options))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_codes_are_unique_and_well_formed() {
        let all = rules();
        let mut codes: Vec<&str> = all.iter().map(|rule| rule.code).collect();
        codes.sort_unstable();
        let before = codes.len();
        codes.dedup();
        assert_eq!(before, codes.len(), "duplicate rule codes");
        for rule in &all {
            assert!(
                rule.code.len() == 6
                    && rule.code[..3].chars().all(|c| c.is_ascii_uppercase())
                    && rule.code[3..].chars().all(|c| c.is_ascii_digit()),
                "{}",
                rule.code
            );
            assert!(rule.clause.starts_with("Part "), "{}", rule.code);
            assert!(!rule.summary.is_empty());
        }
    }

    #[test]
    fn findings_display_with_part_and_clause() {
        let rule = Rule {
            code: "TST001",
            severity: Severity::Warning,
            clause: "Part 2 6.2.2.2",
            summary: "test",
        };
        let finding = Finding::new(&rule, "something is off")
            .in_part("/ppt/x.xml")
            .at("byte 12");
        assert_eq!(
            finding.to_string(),
            "warning TST001 /ppt/x.xml (byte 12): something is off [Part 2 6.2.2.2]"
        );
        assert_eq!(
            Finding::new(&rule, "plain").to_string(),
            "warning TST001 plain [Part 2 6.2.2.2]"
        );
    }
}
