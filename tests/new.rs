mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

fn git_mate() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("git-mate"))
}

#[test]
fn explicit_from() {
    let repo = common::TestRepo::new();
    git_mate()
        .args(["new", "feat/x", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn detects_main() {
    let repo = common::TestRepo::new();
    git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn detects_master() {
    let repo = common::TestRepo::new();
    repo.git(&["branch", "-m", "main", "master"]);
    git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn prefers_origin_head() {
    // Remote HEAD points to "develop"; local clone has no main/master branch,
    // so detect_default_branch must succeed via the origin/HEAD path.
    let setup = common::RepoWithRemote::with_default_branch("develop");
    git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();
    assert_eq!(setup.local_current_branch(), "feat/x");
}

#[test]
fn no_default_branch() {
    let repo = common::TestRepo::new();
    // Detach HEAD then delete main so no fallback branch exists (no remote either).
    let sha = repo.head_commit();
    repo.git(&["checkout", "--detach", &sha]);
    repo.git(&["branch", "-D", "main"]);
    git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--from"));
}

#[test]
fn worktree_mode_creates_worktree() {
    let repo = common::TestRepo::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);

    git_mate()
        .args(["new", "feature/login", "-w", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Derive expected path: <wt_root>/<repo-dir-name>/feature/login
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/login");
    assert!(wt_path.exists(), "worktree directory should exist at {wt_path:?}");

    // The worktree should have feature/login checked out
    let branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    let branch_name = String::from_utf8(branch.stdout).unwrap().trim().to_string();
    assert_eq!(branch_name, "feature/login");
}

#[test]
fn worktree_mode_missing_config_fails() {
    let repo = common::TestRepo::new();
    git_mate()
        .args(["new", "feat/x", "-w", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.worktreeRoot"));
}
