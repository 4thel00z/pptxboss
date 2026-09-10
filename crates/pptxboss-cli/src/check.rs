//! `pptxboss check`.

use std::io::Write;
use std::path::Path;

use pptxboss_check::{rules, CheckOptions, Report, Severity};
use pptxboss_core::Package;
use serde::Serialize;

use crate::Failure;

#[derive(Serialize)]
struct JsonReport<'a> {
    file: String,
    errors: usize,
    warnings: usize,
    parts_checked: usize,
    truncated: bool,
    findings: &'a [pptxboss_check::Finding],
}

/// Runs the verifier; exit code 0 when clean, 1 when errors were found, 2 when the file could not be opened.
pub fn run(
    file: &Path,
    json: bool,
    quiet: bool,
    max_findings: usize,
    no_crc: bool,
) -> Result<(), Failure> {
    let package = Package::open(file).map_err(|err| Failure {
        message: format!("{}: {err}", file.display()),
        code: 2,
    })?;
    let options = CheckOptions {
        max_findings,
        verify_crc: !no_crc,
        ..CheckOptions::default()
    };
    let report = pptxboss_check::check(&package, &options);
    print_report(file, &report, json, quiet)?;
    match report.is_clean() {
        true => Ok(()),
        false => Err(Failure {
            message: format!(
                "{} error(s), {} warning(s)",
                report.errors(),
                report.warnings()
            ),
            code: 1,
        }),
    }
}

fn print_report(file: &Path, report: &Report, json: bool, quiet: bool) -> Result<(), Failure> {
    let mut out = std::io::stdout().lock();
    if json {
        let value = JsonReport {
            file: file.display().to_string(),
            errors: report.errors(),
            warnings: report.warnings(),
            parts_checked: report.parts_checked,
            truncated: report.truncated,
            findings: &report.findings,
        };
        serde_json::to_writer_pretty(&mut out, &value).map_err(|err| Failure {
            message: err.to_string(),
            code: 1,
        })?;
        writeln!(out)?;
        return Ok(());
    }
    for finding in &report.findings {
        if quiet && finding.severity != Severity::Error {
            continue;
        }
        writeln!(out, "{finding}")?;
    }
    if report.truncated {
        writeln!(out, "... more findings not shown (raise --max-findings)")?;
    }
    let verdict = match report.is_clean() {
        true => "ok",
        false => "not ok",
    };
    writeln!(
        out,
        "{}: {verdict}: {} error(s), {} warning(s), {} part(s) checked",
        file.display(),
        report.errors(),
        report.warnings(),
        report.parts_checked
    )?;
    Ok(())
}

/// Lists every rule the verifier knows.
pub fn list_rules(json: bool) -> Result<(), Failure> {
    let all = rules();
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &all).map_err(|err| Failure {
            message: err.to_string(),
            code: 1,
        })?;
        writeln!(out)?;
        return Ok(());
    }
    for rule in all {
        writeln!(
            out,
            "{:<7} {} [{}] {}",
            rule.severity, rule.code, rule.clause, rule.summary
        )?;
    }
    Ok(())
}
