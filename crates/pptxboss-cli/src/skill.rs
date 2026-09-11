//! `pptxboss skill`: the bundled agent skill, printed or installed.

use std::path::PathBuf;

use clap::Subcommand;

use crate::Failure;

/// The skill file compiled into the binary; `skills/pptxboss/SKILL.md` mirrors it.
pub const SKILL: &str = include_str!("../skill/SKILL.md");

#[derive(Subcommand)]
pub enum Skill {
    /// Write the skill to ./.claude/skills/pptxboss/SKILL.md (or ~/.claude with --global).
    Install {
        /// Install into the home directory instead of the current one.
        #[arg(long)]
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
            let base = match global {
                true => std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .ok_or_else(|| Failure {
                        message: "HOME is not set".into(),
                        code: 2,
                    })?,
                false => PathBuf::from("."),
            };
            let dir = base.join(".claude").join("skills").join("pptxboss");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join("SKILL.md");
            std::fs::write(&path, SKILL)?;
            println!("installed {}", path.display());
            Ok(())
        }
    }
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
