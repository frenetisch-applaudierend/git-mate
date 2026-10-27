//! Recovery for a current directory that no longer exists — typically a
//! linked worktree that was removed from another shell while this one was
//! still inside it.

use std::path::{Path, PathBuf};

static VANISHED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// How deep below an existing ancestor to look for a sibling worktree.
/// Branch names nest (`feature/hdl/x`), so one level is not enough.
const SIBLING_SEARCH_DEPTH: usize = 4;

/// How many existing ancestors to search, starting at the closest one. Keeps
/// the search inside the worktree root instead of crawling the home directory.
const SEARCHED_ANCESTORS: usize = 3;

/// The directory the command was started in, when it no longer existed and
/// the command continued from a recovered directory instead.
pub fn vanished() -> Option<&'static Path> {
    VANISHED.get().map(PathBuf::as_path)
}

/// If the current directory no longer exists, continue from the closest
/// directory that still belongs to the same repository, and ask the shell to
/// go there too.
pub fn recover() -> Result<(), String> {
    if std::env::current_dir().is_ok() {
        return Ok(());
    }
    let missing = "the current directory no longer exists; cd to an existing directory";
    let gone = std::env::var_os("PWD")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or(missing)?;
    let gone = canonicalize_existing_prefix(&gone).ok_or(missing)?;
    let target = find_repository(&gone).ok_or_else(|| {
        format!(
            "{} no longer exists and no repository was found next to it; cd into a repository",
            crate::output::display_path(&gone)
        )
    })?;
    std::env::set_current_dir(&target)
        .map_err(|e| format!("could not change to {}: {e}", target.display()))?;
    crate::output::info(&format!(
        "{} no longer exists, continuing in {}",
        crate::output::display_path(&gone),
        crate::output::display_path(&target)
    ));
    crate::shell_protocol::emit_cd(&target);
    let _ = VANISHED.set(gone);
    Ok(())
}

/// Resolve symlinks in the part of `path` that still exists, so it compares
/// equal to the canonical paths git reports for worktrees.
fn canonicalize_existing_prefix(path: &Path) -> Option<PathBuf> {
    let existing = path.ancestors().find(|p| p.is_dir())?;
    let rest = path.strip_prefix(existing).ok()?;
    Some(std::fs::canonicalize(existing).ok()?.join(rest))
}

fn find_repository(gone: &Path) -> Option<PathBuf> {
    let existing = gone.ancestors().skip(1).find(|p| p.is_dir())?;
    // Only a subdirectory vanished; its worktree is still there.
    if is_inside_work_tree(existing) {
        return Some(existing.to_path_buf());
    }
    // The worktree itself vanished: reach the main worktree through one of
    // its siblings, and make sure the vanished path is where that repository
    // puts its linked worktrees.
    let home = std::env::var_os("HOME").map(PathBuf::from);
    existing
        .ancestors()
        .take(SEARCHED_ANCESTORS)
        .take_while(|dir| dir.parent().is_some() && Some(*dir) != home.as_deref())
        .find_map(|dir| sibling_main_worktree(dir, gone, SIBLING_SEARCH_DEPTH))
}

fn sibling_main_worktree(dir: &Path, gone: &Path, depth: usize) -> Option<PathBuf> {
    if dir.join(".git").exists() {
        return main_worktree_of(dir).filter(|main| owns_worktree_path(main, gone));
    }
    if depth == 0 {
        return None;
    }
    let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    children.sort();
    children
        .iter()
        .find_map(|child| sibling_main_worktree(child, gone, depth - 1))
}

fn is_inside_work_tree(dir: &Path) -> bool {
    git_in(dir, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|out| out == "true")
}

fn main_worktree_of(dir: &Path) -> Option<PathBuf> {
    let common = git_in(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common = PathBuf::from(common);
    (common.file_name()? == ".git").then(|| common.parent().map(Path::to_path_buf))?
}

/// Whether `gone` lies where `main`'s repository creates linked worktrees
/// (`<mate.worktreeRoot>/<project>/...`).
fn owns_worktree_path(main: &Path, gone: &Path) -> bool {
    let Some(root) = git_in(main, &["config", "mate.worktreeRoot"]) else {
        return false;
    };
    let Some(project) = main.file_name() else {
        return false;
    };
    let container = main.join(crate::git::expand_tilde(&root)).join(project);
    let container = std::fs::canonicalize(&container).unwrap_or(container);
    gone.starts_with(container)
}

fn git_in(dir: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
