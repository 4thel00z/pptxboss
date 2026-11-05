//! A deck laid out by the engine: a full agenda that shrinks, a list that
//! continues on a second slide, text beside a picture, a long table that
//! carries its header over, stacked blocks, a section divider, a row of
//! key numbers with a pull quote, and a footer band on every content
//! slide. Run with `cargo run --example placed -- placed.pptx [picture.png]`.

use pptxboss_write::{Block, Paragraph, Presentation, Slide, Theme};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "placed.pptx".to_string());
    let picture = args.next().map(std::fs::read).transpose()?;

    let mut agenda = Slide::titled("Agenda").notes("Fourteen items shrink to fit.");
    for topic in [
        "Where the quarter went",
        "Cold starts and the allocator swap",
        "Verification in CI",
        "The numbers",
        "What broke",
        "What we fixed",
        "Open incidents",
        "Hiring",
        "Budget",
        "Roadmap",
        "Templates",
        "Markdown depth",
        "Questions",
        "Thanks",
    ] {
        agenda = agenda.bullet(topic);
    }

    let mut list = Slide::titled("Every rule we check");
    for i in 1..=36 {
        list = list.bullet(format!(
            "Rule {i}: a finding with a sentence long enough to wrap onto a second line at body size"
        ));
    }

    let mut rows = vec![vec![
        "Deck".to_string(),
        "Slides".to_string(),
        "Findings".to_string(),
    ]];
    rows.extend((1..=28).map(|i| {
        vec![
            format!("deck-{i:02}.pptx"),
            (i * 3).to_string(),
            (i % 4).to_string(),
        ]
    }));

    let beside: Block = match &picture {
        Some(data) => {
            Block::picture_described(data.clone(), "the picture passed on the command line")
        }
        None => Block::bullets([
            "Pass a picture path as the second argument",
            "and it appears in this column",
        ]),
    };

    let deck = Presentation::new()
        .theme(Theme::nord().footer("Layout without coordinates"))
        .slide(Slide::title_slide(
            "Layout without coordinates",
            Some("blocks, columns, fitting and continuation"),
        ))
        .slide(Slide::section("Fitting"))
        .slide(agenda)
        .slide(list)
        .slide(
            Slide::titled("Text beside a picture")
                .columns(vec![
                    Block::bullets([
                        "The text column takes seven twelfths",
                        "The picture keeps its aspect ratio",
                        "Both start at the same height",
                    ]),
                    beside,
                ])
                .notes("Columns split seven to five when one side is a picture."),
        )
        .slide(Slide::titled("Decks verified this week").block(Block::table(rows, true)))
        .slide(Slide::section("The numbers"))
        .slide(
            Slide::titled("What changed")
                .columns(vec![
                    Block::stat("86%", "fewer cold starts"),
                    Block::stat("1,240", "decks verified"),
                    Block::stat("0", "findings"),
                ])
                .block(Block::quote(
                    "The deck opened in the right font on a machine that had never seen it.",
                    Some("A reviewer, September 2026"),
                )),
        )
        .slide(
            Slide::titled("Stacked blocks")
                .paragraph("A paragraph, then a table, then whatever room is left.")
                .block(Block::table(
                    vec![
                        vec!["Metric".into(), "June".into(), "September".into()],
                        vec!["p50 cold start".into(), "840 ms".into(), "118 ms".into()],
                        vec!["Findings".into(), "14".into(), "0".into()],
                    ],
                    true,
                ))
                .block(Block::text(vec![
                    Paragraph::text("Three columns of plain text below the table.")
                ]))
                .columns(vec![
                    Block::bullets(["one", "two"]),
                    Block::bullets(["three", "four"]),
                    Block::bullets(["five", "six"]),
                ]),
        )
        .slide(Slide::titled(
            "A title long enough that it cannot stay at forty-four points and still fit inside the frame the master gives it",
        ).bullet("The title shrank; this bullet did not."));
    deck.write_to(&out)?;
    Ok(())
}
