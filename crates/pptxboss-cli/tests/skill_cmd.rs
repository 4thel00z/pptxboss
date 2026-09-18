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

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pptxboss-skill-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn skill_install_short_global_flag_writes_under_home() {
    let home = scratch("home");
    let output = Command::new(env!("CARGO_BIN_EXE_pptxboss"))
        .args(["skill", "install", "-g"])
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = home.join(".claude/skills/pptxboss/SKILL.md");
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .starts_with("---\nname: pptxboss\n"));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("installed {}", path.display())
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn skill_install_global_falls_back_to_userprofile() {
    let profile = scratch("profile");
    let output = Command::new(env!("CARGO_BIN_EXE_pptxboss"))
        .args(["skill", "install", "--global"])
        .env_remove("HOME")
        .env("USERPROFILE", &profile)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(profile.join(".claude/skills/pptxboss/SKILL.md").is_file());
    std::fs::remove_dir_all(profile).unwrap();
}

#[test]
fn skill_install_global_without_home_names_both_variables() {
    let output = Command::new(env!("CARGO_BIN_EXE_pptxboss"))
        .args(["skill", "install", "--global"])
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("HOME") && stderr.contains("USERPROFILE"),
        "{stderr}"
    );
}

#[test]
fn skill_install_names_the_path_it_could_not_create() {
    let dir = scratch("blocked");
    std::fs::write(dir.join(".claude"), "not a directory").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pptxboss"))
        .args(["skill", "install"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = std::path::Path::new(".claude")
        .join("skills")
        .join("pptxboss");
    assert!(
        stderr.contains(&expected.display().to_string()),
        "stderr should name the directory: {stderr}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
