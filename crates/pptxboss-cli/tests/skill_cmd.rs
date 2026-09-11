use std::process::Command;

#[test]
fn skill_show_and_install() {
    let output = Command::new(env!("CARGO_BIN_EXE_pptxboss"))
        .args(["skill", "show"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.starts_with("---\nname: pptxboss\n"));
    let dir = std::env::temp_dir().join(format!("pptxboss-skill-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pptxboss"))
        .args(["skill", "install"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let installed = std::fs::read_to_string(dir.join(".claude/skills/pptxboss/SKILL.md")).unwrap();
    assert_eq!(installed, shown);
    std::fs::remove_dir_all(dir).unwrap();
}
