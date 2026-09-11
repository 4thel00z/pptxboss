use std::path::PathBuf;
use std::process::Command;

use pptxboss_testkit::{Deck, DeckSlide};

fn fixture(name: &str, deck: &Deck) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("pptxboss-cli-{}-{name}.pptx", std::process::id()));
    std::fs::write(&path, deck.build()).unwrap();
    path
}

fn run(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_pptxboss"))
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn deck() -> Deck {
    Deck::new()
        .slide(
            DeckSlide::titled("Welcome")
                .bullet("Point one")
                .bullet("Point two"),
        )
        .slide(DeckSlide::titled("Second").notes("remember this").hidden())
        .slide(DeckSlide::default())
}

#[test]
fn text_prints_slides_separated_by_blank_lines() {
    let path = fixture("text", &deck());
    let (code, stdout, stderr) = run(&["text", path.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "Welcome\nPoint one\nPoint two\n\nSecond\n");
    assert!(stderr.is_empty());
    let (_, stdout, _) = run(&[
        "text",
        "--notes",
        "--headings",
        "--skip-hidden",
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        stdout,
        "--- slide 1 ---\nWelcome\nPoint one\nPoint two\n\n--- slide 2 ---\n\n--- slide 3 ---\n"
    );
    let (_, stdout, _) = run(&["text", "--notes", path.to_str().unwrap()]);
    assert_eq!(
        stdout,
        "Welcome\nPoint one\nPoint two\n\nSecond\nremember this\n"
    );
    let (_, stdout, _) = run(&["text", "--json", path.to_str().unwrap()]);
    assert_eq!(
        stdout.trim(),
        r#"[{"number":1,"text":"Welcome\nPoint one\nPoint two"},{"number":2,"text":"Second"},{"number":3,"text":""}]"#
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn info_summarizes_the_deck() {
    let path = fixture("info", &deck());
    let (code, stdout, _) = run(&["info", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(stdout.contains("slides:       3"));
    assert!(stdout.contains("13.33 x 7.50 in"));
    assert!(stdout.contains("   1  Welcome"));
    assert!(stdout.contains("   2  Second [hidden, notes]"));
    assert!(stdout.contains("   3  (no title)"));
    let (_, stdout, _) = run(&["info", "--json", path.to_str().unwrap()]);
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["slides"], 3);
    assert_eq!(value["slide_list"][1]["hidden"], true);
    assert_eq!(value["slide_list"][1]["has_notes"], true);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unreadable_input_exits_with_code_one() {
    let path = std::env::temp_dir().join(format!("pptxboss-cli-{}-junk.pptx", std::process::id()));
    std::fs::write(&path, b"not a package").unwrap();
    let (code, stdout, stderr) = run(&["text", path.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("not a zip archive"));
    let (code, _, stderr) = run(&["info", "/nonexistent/file.pptx"]);
    assert_eq!(code, 1);
    assert!(stderr.starts_with("error: "));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_broken_slide_is_a_warning_not_a_failure() {
    let path = fixture(
        "broken",
        &deck().with_part("ppt/slides/slide1.xml", b"<p:sld"),
    );
    let (code, stdout, stderr) = run(&["text", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Second\n");
    assert!(stderr.starts_with("warning: slide 1: unreadable"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn check_reports_findings_and_exit_codes() {
    let path = fixture("check-clean", &deck());
    let (code, stdout, stderr) = run(&["check", path.to_str().unwrap()]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains(": ok: 0 error(s), 0 warning(s)"));
    let broken = fixture("check-broken", &deck().without_part("ppt/presProps.xml"));
    let (code, stdout, stderr) = run(&["check", broken.to_str().unwrap()]);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(stdout.contains("error   PML003"), "{stdout}");
    assert!(stdout.contains("error   REL004"), "{stdout}");
    assert!(stdout.contains(": not ok:"));
    let (code, stdout, _) = run(&["check", "--json", broken.to_str().unwrap()]);
    assert_eq!(code, 1);
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(value["errors"].as_u64().unwrap() >= 2);
    assert_eq!(value["findings"][0]["severity"], "error");
    let junk = std::env::temp_dir().join(format!(
        "pptxboss-cli-{}-check-junk.pptx",
        std::process::id()
    ));
    std::fs::write(&junk, b"nope").unwrap();
    let (code, _, stderr) = run(&["check", junk.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(stderr.contains("not a zip archive"));
    for p in [path, broken, junk] {
        std::fs::remove_file(p).unwrap();
    }
}

#[test]
fn rules_lists_every_code() {
    let (code, stdout, _) = run(&["rules"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("REL004"));
    assert!(stdout.contains("[Part 2 6.5.3.4]"));
    let (_, stdout, _) = run(&["rules", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(value.as_array().unwrap().len() > 40);
}

fn features_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/features.pptx")
}

#[test]
fn info_lists_properties_sections_and_comment_flags() {
    let path = features_fixture();
    let (code, stdout, stderr) = run(&["info", path.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("section:      Opening (slides 1-2)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("section:      Closing (slides 3)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("   1  Commented [pictures, comments]"),
        "{stdout}"
    );
    assert!(stdout.contains("   2  Embedded [objects]"), "{stdout}");
    let (code, json, _) = run(&["info", "--json", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["sections"][0]["name"], "Opening");
    assert_eq!(value["sections"][0]["slides"], serde_json::json!([1, 2]));
    assert!(value["properties"].is_object());
    assert_eq!(value["slide_list"][1]["objects"], 1);
    assert_eq!(value["slide_list"][0]["has_comments"], true);
}

#[test]
fn text_appends_comments_and_alt_text_on_request() {
    let path = features_fixture();
    let (code, plain, _) = run(&["text", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(!plain.contains("[comment]"));
    assert!(!plain.contains("A blue diagram"));
    let (code, rich, stderr) = run(&["text", "--comments", "--alt-text", path.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        rich.contains("A point\nA blue diagram\n[comment] Ada Lovelace: Tighten this point"),
        "{rich}"
    );
    assert!(rich.contains("Embedded\nBudget sheet"), "{rich}");
}

#[test]
fn markdown_command_renders_the_deck() {
    let path = features_fixture();
    let (code, stdout, stderr) = run(&["markdown", "--comments", path.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.starts_with("## Commented\n\n- A point\n\n![A blue diagram](ppt/media/image1.png)\n\n> **Comment (Ada Lovelace, 2024-05-01T10:00:00.000):** Tighten this point\n\n---\n\n## Embedded\n\n*Budget sheet*"), "{stdout}");
    assert!(stdout.contains("## Figures\n\n**Chart: Revenue**\n\n|  | 2024 |\n|---|---|\n| Q1 | 10 |\n| Q2 | 12 |\n\n- Plan\n- Build\n  - Test first\n"), "{stdout}");
    let (code, text, _) = run(&["text", "--no-charts", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(text.contains("Figures\nPlan\nBuild\nTest first"), "{text}");
    assert!(!text.contains("Revenue"));
}

#[test]
fn legacy_ppt_files_read_and_check_refuses_them() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/legacy.ppt");
    let (code, stdout, stderr) = run(&["info", path.to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("format:       ppt"), "{stdout}");
    assert!(
        stdout.contains("   1  Legacy title [notes, pictures]"),
        "{stdout}"
    );
    assert!(stdout.contains("   2  Second [hidden]"), "{stdout}");
    let (code, text, _) = run(&["text", "--notes", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(
        text.starts_with("Legacy title\nFirst point\nDetail\nFree text\nSpeaker notes here\n"),
        "{text}"
    );
    let (code, _, stderr) = run(&["check", path.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(stderr.contains("compound file"), "{stderr}");
}
