//! The `pptxboss` command line: inspect and extract from `.pptx` files.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pptxboss_core::{Document, TextOptions};

mod info;
mod text;

#[derive(Parser)]
#[command(
    name = "pptxboss",
    version,
    about = "PresentationML (.pptx) toolkit: inspect, extract text and notes"
)]
struct Cli {
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
        /// Print a `--- slide N ---` heading before each slide.
        #[arg(long)]
        headings: bool,
        /// Emit one JSON object per slide instead of text.
        #[arg(long)]
        json: bool,
    },
}

/// A failure with the exit code it maps to: 1 for unreadable input, 2 for bad usage.
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
    let result = match cli.command {
        Command::Info { file, json } => open(&file).and_then(|doc| info::run(&doc, &file, json)),
        Command::Text {
            file,
            notes,
            furniture,
            hidden_shapes,
            skip_hidden,
            headings,
            json,
        } => {
            let options = TextOptions {
                notes,
                furniture,
                hidden_shapes,
                hidden_slides: !skip_hidden,
                ..TextOptions::default()
            };
            open(&file).and_then(|doc| text::run(&doc, &options, headings, json))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            let _ = writeln!(std::io::stderr(), "error: {}", failure.message);
            ExitCode::from(failure.code)
        }
    }
}

fn open(file: &PathBuf) -> Result<Document, Failure> {
    Document::open(file).map_err(|err| Failure {
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
