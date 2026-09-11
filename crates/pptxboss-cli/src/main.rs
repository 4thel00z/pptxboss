//! The `pptxboss` command line: inspect and extract from `.pptx` files.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pptxboss_core::{Document, MarkdownOptions, TextOptions};

mod check;
mod create;
mod info;
mod markdown;
mod skill;
mod text;

#[derive(Parser)]
#[command(
    name = "pptxboss",
    version,
    about = "PresentationML (.pptx) toolkit: inspect, extract text and notes, verify against ECMA-376, create decks"
)]
struct Cli {
    /// Cap the worker threads used to parse slides (default: every core).
    #[arg(long, global = true, value_name = "N")]
    threads: Option<usize>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Summarize a deck: slide count, size, and one line per slide.
    Info {
        file: PathBuf,
        /// Emit JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Extract the text of every slide.
    Text {
        file: PathBuf,
        /// Append each slide's speaker notes after its text.
        #[arg(long)]
        notes: bool,
        /// Include date, footer, header and slide number placeholders.
        #[arg(long)]
        furniture: bool,
        /// Include shapes marked hidden.
        #[arg(long)]
        hidden_shapes: bool,
        /// Leave out slides marked hidden.
        #[arg(long)]
        skip_hidden: bool,
        /// Include the alternative text of pictures and other shapes without text.
        #[arg(long)]
        alt_text: bool,
        /// Append each slide's comments after its text and notes.
        #[arg(long)]
        comments: bool,
        /// Leave out chart titles, series and values.
        #[arg(long)]
        no_charts: bool,
        /// Leave out the text of diagrams (SmartArt).
        #[arg(long)]
        no_diagrams: bool,
        /// Print a `--- slide N ---` heading before each slide.
        #[arg(long)]
        headings: bool,
        /// Emit one JSON object per slide instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Render the deck as Markdown: a heading per slide, bullets, tables, images, charts and diagrams.
    Markdown {
        file: PathBuf,
        /// Speaker notes as a block quote after each slide.
        #[arg(long)]
        notes: bool,
        /// Comments as block quotes after each slide.
        #[arg(long)]
        comments: bool,
        /// Leave out slides marked hidden.
        #[arg(long)]
        skip_hidden: bool,
        /// Include shapes marked hidden.
        #[arg(long)]
        hidden_shapes: bool,
        /// Include date, footer, header and slide number placeholders.
        #[arg(long)]
        furniture: bool,
        /// No `## Title` heading per slide.
        #[arg(long)]
        no_headings: bool,
        /// Leave images out.
        #[arg(long)]
        no_images: bool,
    },
    /// Verify a deck against ECMA-376: container, package, relationships and PresentationML structure.
    ///
    /// Exit code 0 when no errors were found, 1 when errors were found, 2 when the file could not be opened.
    Check {
        file: PathBuf,
        /// Emit the findings as JSON.
        #[arg(long)]
        json: bool,
        /// Show only errors.
        #[arg(long, short)]
        quiet: bool,
        /// Stop after this many findings.
        #[arg(long, default_value_t = 1000)]
        max_findings: usize,
        /// Skip CRC-32 verification of XML parts.
        #[arg(long)]
        no_crc: bool,
    },
    /// List the verifier's rules with their codes, severities and clauses.
    Rules {
        #[arg(long)]
        json: bool,
    },
    /// Create a deck: blank slides, one slide from arguments, or slides from Markdown.
    #[command(subcommand)]
    Create(create::Create),
    /// Print or install the bundled agent skill.
    #[command(subcommand)]
    Skill(skill::Skill),
}

/// A failure with the exit code it maps to: 1 for unreadable input or verifier errors, 2 when `check` cannot open its file.
pub struct Failure {
    pub message: String,
    pub code: u8,
}

impl From<pptxboss_core::Error> for Failure {
    fn from(err: pptxboss_core::Error) -> Self {
        Failure {
            message: err.to_string(),
            code: 1,
        }
    }
}

impl From<std::io::Error> for Failure {
    fn from(err: std::io::Error) -> Self {
        Failure {
            message: err.to_string(),
            code: 1,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let threads = cli.threads.unwrap_or(0);
    let result = match cli.command {
        Command::Info { file, json } => {
            open(&file, threads).and_then(|doc| info::run(&doc, &file, json))
        }
        Command::Text {
            file,
            notes,
            furniture,
            hidden_shapes,
            skip_hidden,
            alt_text,
            comments,
            no_charts,
            no_diagrams,
            headings,
            json,
        } => {
            let options = TextOptions {
                notes,
                furniture,
                hidden_shapes,
                hidden_slides: !skip_hidden,
                alt_text,
                comments,
                charts: !no_charts,
                diagrams: !no_diagrams,
                ..TextOptions::default()
            };
            open(&file, threads).and_then(|doc| text::run(&doc, &options, headings, json))
        }
        Command::Markdown {
            file,
            notes,
            comments,
            skip_hidden,
            hidden_shapes,
            furniture,
            no_headings,
            no_images,
        } => {
            let options = MarkdownOptions {
                headings: !no_headings,
                notes,
                comments,
                hidden_slides: !skip_hidden,
                hidden_shapes,
                furniture,
                images: !no_images,
            };
            open(&file, threads).and_then(|doc| markdown::run(&doc, &options))
        }
        Command::Check {
            file,
            json,
            quiet,
            max_findings,
            no_crc,
        } => check::run(&file, json, quiet, max_findings, no_crc),
        Command::Rules { json } => check::list_rules(json),
        Command::Create(command) => create::run(command),
        Command::Skill(command) => skill::run(command),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            if !failure.message.is_empty() {
                let _ = writeln!(std::io::stderr(), "error: {}", failure.message);
            }
            ExitCode::from(failure.code)
        }
    }
}

fn open(file: &PathBuf, threads: usize) -> Result<Document, Failure> {
    Document::open(file)
        .map(|doc| doc.with_threads(threads))
        .map_err(|err| Failure {
            message: format!("{}: {err}", file.display()),
            code: 1,
        })
}

/// Prints report warnings to stderr, one per line.
pub fn warn_all(lines: &[String]) {
    let mut stderr = std::io::stderr().lock();
    for line in lines {
        let _ = writeln!(stderr, "warning: {line}");
    }
}
