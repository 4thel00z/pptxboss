//! A themed deck with formatted runs, a gradient title layout, a table and
//! an inverted slide. Run with `cargo run --example styled -- styled.pptx`.

use pptxboss_write::{
    Align, Background, Color, Layout, Paragraph, Presentation, Rect, Rgb, Run, SchemeColor, Slide,
    Theme,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "styled.pptx".to_string());

    let theme = Theme::tokyo()
        .color(SchemeColor::Accent1, Rgb::hex("#7DCFFF").unwrap())
        .fonts("Georgia", "Calibri")
        .layout_background(
            Layout::Title,
            Background::linear(
                Color::hex("#1A1B26").unwrap(),
                Color::hex("#2F3453").unwrap(),
                45,
            ),
            false,
        );

    let deck = Presentation::new()
        .theme(theme)
        .slide(Slide::title_slide(
            "Platform review",
            Some("Infrastructure, Q3 2026"),
        ))
        .slide(
            Slide::titled("Where the quarter went")
                .body_paragraph(Paragraph::bullet_runs(
                    vec![
                        Run::text("Cold starts ").bold(),
                        Run::text("down 86%").color(Color::Scheme(SchemeColor::Accent4)),
                        Run::text(" after the allocator swap"),
                    ],
                    0,
                ))
                .body_paragraph(Paragraph::bullet_runs(
                    vec![
                        Run::text("Every deck passes "),
                        Run::text("pptxboss check").font("Menlo"),
                        Run::text(" in CI"),
                    ],
                    0,
                ))
                .sub_bullet("72 rules, zero findings on 1,240 decks", 1)
                .body_paragraph(Paragraph::bullet_runs(
                    vec![
                        Run::text("Runbook: "),
                        Run::text("pptxboss.dev/docs").link("https://pptxboss.dev/docs/"),
                    ],
                    0,
                ))
                .notes("Two minutes, then the numbers."),
        )
        .slide(Slide::titled("The numbers").table(
            Rect::inches(0.7, 1.7, 11.9, 2.6),
            vec![
                vec!["Metric".into(), "June".into(), "September".into()],
                vec!["p50 cold start".into(), "840 ms".into(), "118 ms".into()],
                vec!["Decks verified".into(), "310".into(), "1,240".into()],
                vec!["Findings".into(), "14".into(), "0".into()],
            ],
            true,
        ))
        .slide(
            Slide::titled("Next quarter")
                .inverted()
                .background(Background::solid(Color::Scheme(SchemeColor::Accent1)))
                .body_paragraph(
                    Paragraph::runs(vec![Run::text("Templates").bold().size(40)])
                        .align(Align::Left),
                )
                .body_paragraph(Paragraph::text(
                    "Append slides to an existing deck and keep its master.",
                )),
        );

    deck.write_to(&out)?;
    println!("wrote {out}");
    Ok(())
}
