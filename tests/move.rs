mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn move_creates_worktree_and_returns_main_to_default_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);
    repo.git(&["checkout", "-b", "feature/move"]);

    let output = common::git_mate()
        .arg("move")
        .env("GIT_MATE_SHELL", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "move should succeed");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/move");
    assert!(
        stdout.contains(&format!("_MATE_CMD:CD:{}", wt_path.display())),
        "stdout should contain _MATE_CMD:CD: pointing to the new worktree, got: {stdout:?}"
    );

    assert_eq!(repo.current_branch(), "main");
    assert!(
        wt_path.exists(),
        "worktree directory should exist at {wt_path:?}"
    );

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    let branch_name = String::from_utf8(branch.stdout).unwrap().trim().to_string();
    assert_eq!(branch_name, "feature/move");
}

#[test]
fn move_fails_from_linked_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-linked");
    repo.git(&[
        "worktree",
        "add",
        "-b",
        "feature/linked",
        wt_path.to_str().unwrap(),
        "main",
    ]);

    common::git_mate()
        .arg("move")
        .current_dir(&wt_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("main worktree"));
}

#[test]
fn move_fails_on_default_branch() {
    let repo = common::RepoWithoutRemote::new();

    common::git_mate()
        .arg("move")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("default branch"));
}

#[test]
fn move_fails_on_detached_head() {
    let repo = common::RepoWithoutRemote::new();
    let sha = repo.head_commit();
    repo.git(&["checkout", "--detach", &sha]);

    common::git_mate()
        .arg("move")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("detached HEAD"));
}

#[test]
fn move_dirty_worktree_requires_stash_flag() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .arg("move")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--stash"));

    assert_eq!(repo.current_branch(), "feature/dirty");
}

#[test]
fn move_with_stash_restores_tracked_and_untracked_changes() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    repo.git(&["checkout", "-b", "feature/stash"]);

    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join("notes.txt"), "local notes\n").unwrap();

    common::git_mate()
        .args(["move", "--stash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Stashed local changes"))
        .stderr(predicate::str::contains("Restored stashed changes"));

    assert_eq!(repo.current_branch(), "main");

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/stash");
    assert!(
        wt_path.exists(),
        "worktree directory should exist at {wt_path:?}"
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "changed\n"
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join("notes.txt")).unwrap(),
        "local notes\n"
    );
}
