//! `pptxboss create`.

use std::path::PathBuf;

use clap::Subcommand;
use pptxboss_write::{from_markdown, Presentation, Slide, SlideSize, Theme};

use crate::Failure;

#[derive(Subcommand)]
pub enum Create {
    /// A deck of empty slides.
    Blank {
        out: PathBuf,
        /// Number of blank slides.
        #[arg(long, default_value_t = 1)]
        slides: usize,
        /// Use the 4:3 slide size instead of widescreen.
        #[arg(long)]
        standard: bool,
    },
    /// One slide with a title and bullet points.
    Text {
        out: PathBuf,
        /// The slide title.
        #[arg(long)]
        title: String,
        /// A bullet point; repeat for more.
        #[arg(long = "bullet")]
        bullets: Vec<String>,
        /// Speaker notes for the slide.
        #[arg(long)]
        notes: Option<String>,
        /// Use the 4:3 slide size instead of widescreen.
        #[arg(long)]
        standard: bool,
        /// A TrueType or OpenType font file to store in the deck; repeat for more.
        #[arg(long = "embed-font", value_name = "FILE")]
        embed_fonts: Vec<PathBuf>,
    },
    /// Slides from a Markdown file: `#` starts a title slide, `##` a content slide,
    /// list items become bullets, `Notes:` starts speaker notes, `---` breaks a slide.
    Md {
        out: PathBuf,
        /// The Markdown file, or `-` for standard input.
        input: PathBuf,
        /// Use the 4:3 slide size instead of widescreen.
        #[arg(long)]
        standard: bool,
        /// Theme preset: office, dark, slate, forest, sunset, midnight, mocha,
        /// dracula, nord, tokyo, clay or mono.
        #[arg(long)]
        theme: Option<String>,
        /// Font family for titles and body, applied over the theme.
        #[arg(long)]
        font: Option<String>,
        /// A TrueType or OpenType font file to store in the deck; repeat for more.
        #[arg(long = "embed-font", value_name = "FILE")]
        embed_fonts: Vec<PathBuf>,
    },
}

/// The presentation with each font file read and handed to its theme.
fn with_embedded_fonts(
    mut presentation: Presentation,
    paths: &[PathBuf],
) -> Result<Presentation, Failure> {
    for path in paths {
        let data = std::fs::read(path).map_err(|err| Failure {
            message: format!("{}: {err}", path.display()),
            code: 1,
        })?;
        presentation.theme = std::mem::take(&mut presentation.theme).embed_font(data);
    }
    Ok(presentation)
}

fn size(standard: bool) -> SlideSize {
    match standard {
        true => SlideSize::STANDARD,
        false => SlideSize::WIDESCREEN,
    }
}

fn write(presentation: &Presentation, out: &PathBuf) -> Result<(), Failure> {
    presentation.write_to(out).map_err(|err| Failure {
        message: format!("{}: {err}", out.display()),
        code: 1,
    })
}

pub fn run(command: Create) -> Result<(), Failure> {
    match command {
        Create::Blank {
            out,
            slides,
            standard,
        } => {
            let mut presentation = Presentation::new().size(size(standard));
            for _ in 0..slides {
                presentation.slides.push(Slide::new());
            }
            write(&presentation, &out)
        }
        Create::Text {
            out,
            title,
            bullets,
            notes,
            standard,
            embed_fonts,
        } => {
            let mut slide = Slide::titled(title);
            for bullet in bullets {
                slide = slide.bullet(bullet);
            }
            if let Some(notes) = notes {
                slide = slide.notes(notes);
            }
            let presentation = Presentation::new().size(size(standard)).slide(slide);
            write(&with_embedded_fonts(presentation, &embed_fonts)?, &out)
        }
        Create::Md {
            out,
            input,
            standard,
            theme,
            font,
            embed_fonts,
        } => {
            let markdown = match input.to_str() == Some("-") {
                true => std::io::read_to_string(std::io::stdin())?,
                false => std::fs::read_to_string(&input).map_err(|err| Failure {
                    message: format!("{}: {err}", input.display()),
                    code: 1,
                })?,
            };
            let mut presentation = from_markdown(&markdown).size(size(standard));
            if let Some(name) = theme {
                let theme = Theme::preset(&name).ok_or_else(|| Failure {
                    message: format!(
                        "unknown theme {name:?}; use one of {}",
                        Theme::PRESETS.join(", ")
                    ),
                    code: 2,
                })?;
                presentation = presentation.theme(theme);
            }
            if let Some(font) = font {
                presentation = presentation.font(font);
            }
            write(&with_embedded_fonts(presentation, &embed_fonts)?, &out)
        }
    }
}
