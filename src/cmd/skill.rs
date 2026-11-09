use std::path::{Path, PathBuf};

/// The instructions for agents, printed by `git mate skill`.
const INSTRUCTIONS: &str = include_str!("../../skill/SKILL.md");

/// What `git mate skill install` writes. It only carries the frontmatter
/// (the description is always in an agent's context, so it must be static);
/// the body is pulled from the installed binary each time the skill is
/// loaded, so it never goes stale.
const STUB: &str = "\
---
name: git-mate
description: Create, switch, relocate, sync, update and finish branches and their worktrees with git-mate (`git mate`). Use for branch and worktree workflow; use plain git for commits, diffs and pushes of the current branch.
allowed-tools: Bash(git mate skill)
---

!`git mate skill`
";

const SKILL_NAME: &str = "git-mate";

#[derive(clap::Args)]
#[command(long_about = "\
Print instructions for AI coding agents on how to use git-mate.

`git mate skill install` installs a Claude Code skill that loads these instructions
from the installed git-mate whenever an agent uses it, so it stays current across
upgrades.")]
pub struct SkillArgs {
    #[command(subcommand)]
    pub command: Option<SkillCommand>,
}

#[derive(clap::Subcommand)]
pub enum SkillCommand {
    /// Install the git-mate skill for Claude Code
    Install(InstallArgs),
}

#[derive(clap::Args)]
pub struct InstallArgs {
    /// Install into the current repository's .claude/skills instead of ~/.claude/skills
    #[arg(long, conflicts_with = "dir")]
    pub project: bool,
    /// Install into this skills directory (the skill goes into <DIR>/git-mate/SKILL.md)
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
}

/// What `skill install` did, reported in `--json` mode.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallData {
    path: PathBuf,
    /// "created", "updated" or "unchanged".
    action: &'static str,
}

pub fn run(args: SkillArgs) -> Result<(), crate::output::CommandError> {
    match args.command {
        None if crate::output::json_mode() => Err(crate::output::CommandError::new(
            "--json is not supported for skill".to_string(),
            "unsupported",
        )),
        None => {
            print!("{INSTRUCTIONS}");
            Ok(())
        }
        Some(SkillCommand::Install(args)) => install(args).map_err(Into::into),
    }
}

fn install(args: InstallArgs) -> Result<(), String> {
    let skills_dir = match (args.dir, args.project) {
        (Some(dir), _) => dir,
        (None, true) => crate::git::current_worktree_root()?
            .join(".claude")
            .join("skills"),
        (None, false) => home_dir()?.join(".claude").join("skills"),
    };
    let path = skills_dir.join(SKILL_NAME).join("SKILL.md");
    let action = write_stub(&path)?;

    if crate::output::json_mode() {
        crate::output::emit_json_success("skill", InstallData { path, action });
    } else {
        let shown = crate::output::display_path(&path);
        match action {
            "unchanged" => crate::output::info(&format!("Skill already up to date at {shown}")),
            "updated" => crate::output::success(&format!("Updated skill at {shown}")),
            _ => crate::output::success(&format!("Installed skill at {shown}")),
        }
    }
    Ok(())
}

fn write_stub(path: &Path) -> Result<&'static str, String> {
    let action = match std::fs::read_to_string(path) {
        Ok(existing) if existing == STUB => return Ok("unchanged"),
        Ok(_) => "updated",
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "created",
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, STUB).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(action)
}

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set; pass --dir or --project".to_string())
}

#[cfg(test)]
mod tests {
    use super::INSTRUCTIONS;

    /// Every `git mate <command> ... --flag` the instructions mention must be
    /// a flag that command (or the CLI globally) accepts, so the skill can't
    /// silently drift away from the CLI.
    #[test]
    fn instructions_only_mention_existing_flags() {
        let cli = crate::build_cli();
        for snippet in code_snippets(INSTRUCTIONS) {
            let mut words = snippet.split_whitespace();
            if words.next() != Some("git") || words.next() != Some("mate") {
                continue;
            }
            let Some(name) = words.next() else { continue };
            let command = cli
                .find_subcommand(name)
                .unwrap_or_else(|| panic!("unknown command `{name}` in `{snippet}`"));
            for word in words {
                for flag in word.split('/') {
                    assert_flag_exists(&cli, command, flag, snippet);
                }
            }
        }
    }

    /// Flags mentioned on their own (`--take-changes`) belong to the command
    /// of the section they appear in; check they exist on some command.
    #[test]
    fn instructions_only_mention_flags_some_command_has() {
        let cli = crate::build_cli();
        for snippet in code_snippets(INSTRUCTIONS) {
            if snippet.starts_with("git ") {
                continue;
            }
            for flag in snippet.split(" / ") {
                let flag = flag.split_whitespace().next().unwrap_or_default();
                if !(flag.starts_with('-') && flag.len() > 1) {
                    continue;
                }
                let exists = cli
                    .get_subcommands()
                    .any(|command| has_flag(&cli, command, flag));
                assert!(exists, "no command accepts `{flag}` (in `{snippet}`)");
            }
        }
    }

    fn assert_flag_exists(cli: &clap::Command, command: &clap::Command, flag: &str, snippet: &str) {
        if !flag.starts_with('-') || flag == "-" {
            return;
        }
        assert!(
            has_flag(cli, command, flag),
            "`git mate {}` has no flag `{flag}` (in `{snippet}`)",
            command.get_name()
        );
    }

    /// Whether `flag` (`--long`, `--long=value` or `-s`) is accepted by
    /// `command` or globally.
    fn has_flag(cli: &clap::Command, command: &clap::Command, flag: &str) -> bool {
        let matches = |arg: &clap::Arg| match flag.strip_prefix("--") {
            Some(long) => {
                let long = long.split('=').next().unwrap_or(long);
                arg.get_long() == Some(long)
                    || arg.get_all_aliases().is_some_and(|a| a.contains(&long))
            }
            None => flag
                .strip_prefix('-')
                .and_then(|short| short.chars().next())
                .is_some_and(|short| arg.get_short() == Some(short)),
        };
        let builtin = matches!(flag, "--help" | "-h");
        builtin || command.get_arguments().any(matches) || cli.get_arguments().any(matches)
    }

    /// The contents of every inline `code span` in `markdown`.
    fn code_snippets(markdown: &str) -> Vec<&str> {
        markdown.split('`').skip(1).step_by(2).collect()
    }

    #[test]
    fn code_snippets_finds_spans() {
        assert_eq!(code_snippets("a `b` c `d e` f"), vec!["b", "d e"]);
    }
}
