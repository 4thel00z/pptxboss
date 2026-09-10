use std::path::PathBuf;
use std::process::Command;

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

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pptxboss-create-{}-{name}", std::process::id()))
}

#[test]
fn create_text_then_read_and_check_it() {
    let out = temp("text.pptx");
    let (code, _, stderr) = run(&[
        "create",
        "text",
        out.to_str().unwrap(),
        "--title",
        "Hello",
        "--bullet",
        "one",
        "--bullet",
        "two & three",
        "--notes",
        "say hi",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let (code, stdout, _) = run(&["text", "--notes", out.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(stdout, "Hello\none\ntwo & three\nsay hi\n");
    let (code, stdout, _) = run(&["check", out.to_str().unwrap()]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains(": ok: 0 error(s), 0 warning(s)"));
    std::fs::remove_file(out).unwrap();
}

#[test]
fn create_blank_and_md() {
    let blank = temp("blank.pptx");
    let (code, _, stderr) = run(&[
        "create",
        "blank",
        blank.to_str().unwrap(),
        "--slides",
        "3",
        "--standard",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let (_, stdout, _) = run(&["info", blank.to_str().unwrap()]);
    assert!(stdout.contains("slides:       3"));
    assert!(stdout.contains("(screen4x3)"));

    let md = temp("deck.md");
    std::fs::write(&md, "# Title\nSub\n\n## Agenda\n- a\n- b\n").unwrap();
    let out = temp("md.pptx");
    let (code, _, stderr) = run(&[
        "create",
        "md",
        out.to_str().unwrap(),
        md.to_str().unwrap(),
        "--font",
        "Arial",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let (_, stdout, _) = run(&["text", out.to_str().unwrap()]);
    assert_eq!(stdout, "Title\nSub\n\nAgenda\na\nb\n");
    let (code, _, _) = run(&["check", "--quiet", out.to_str().unwrap()]);
    assert_eq!(code, 0);
    let (code, _, stderr) = run(&["create", "md", out.to_str().unwrap(), "/nonexistent.md"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("nonexistent.md"));
    for path in [blank, md, out] {
        std::fs::remove_file(path).unwrap();
    }
}
