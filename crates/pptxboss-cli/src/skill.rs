//! `pptxboss skill`: the bundled agent skill, printed or installed.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use crate::Failure;

/// The skill file compiled into the binary; `skills/pptxboss/SKILL.md` mirrors it.
pub const SKILL: &str = include_str!("../skill/SKILL.md");

#[derive(Subcommand)]
pub enum Skill {
    /// Write the skill to ./.claude/skills/pptxboss/SKILL.md (or ~/.claude with --global).
    Install {
        /// Install into the home directory instead of the current one.
        #[arg(long, short = 'g')]
        global: bool,
    },
    /// Print the skill to standard output.
    Show,
}

pub fn run(command: Skill) -> Result<(), Failure> {
    match command {
        Skill::Show => {
            print!("{SKILL}");
            Ok(())
        }
        Skill::Install { global } => {
            let path = install_into(&skills_root(global)?)?;
            println!("installed {}", path.display());
            Ok(())
        }
    }
}

/// The `.claude/skills` directory to install into: the current directory's,
/// or with `--global` the home directory's (`HOME`, or `USERPROFILE` on Windows).
fn skills_root(global: bool) -> Result<PathBuf, Failure> {
    if !global {
        return Ok(PathBuf::from(".claude").join("skills"));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| Failure {
            message: "cannot locate the home directory: neither HOME nor USERPROFILE is set".into(),
            code: 2,
        })?;
    Ok(PathBuf::from(home).join(".claude").join("skills"))
}

/// Writes the skill to `root/pptxboss/SKILL.md`, creating the directories and
/// overwriting an earlier install, and returns that path.
fn install_into(root: &Path) -> Result<PathBuf, Failure> {
    let dir = root.join("pptxboss");
    std::fs::create_dir_all(&dir).map_err(|err| Failure {
        message: format!("cannot create {}: {err}", dir.display()),
        code: 1,
    })?;
    let path = dir.join("SKILL.md");
    std::fs::write(&path, SKILL).map_err(|err| Failure {
        message: format!("cannot write {}: {err}", path.display()),
        code: 1,
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::SKILL;

    #[test]
    fn the_skill_has_frontmatter_and_no_em_dashes() {
        assert!(SKILL.starts_with("---\nname: pptxboss\n"));
        assert!(SKILL.contains("description:"));
        assert!(!SKILL.contains('\u{2014}'));
        assert!(SKILL.contains("## Gotchas"));
    }
}
