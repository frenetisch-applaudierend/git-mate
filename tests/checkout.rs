mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

fn git_mate() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("git-mate"))
}

#[test]
fn checkout_in_place_from_main_worktree() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["branch", "existing-branch"]);

    git_mate()
        .args(["checkout", "existing-branch"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "existing-branch");
}

#[test]
fn checkout_in_place_from_linked_worktree_fails() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "-b", "linked-branch", "main"]);
    repo.git(&["branch", "other-branch"]);

    git_mate()
        .args(["checkout", "other-branch"])
        .current_dir(&wt_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("main worktree"));
}

#[test]
fn checkout_worktree_creates_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "feature/checkout"]);

    let output = git_mate()
        .args(["checkout", "feature/checkout", "-w"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("_MATE_CD:"), "stdout should contain _MATE_CD:, got: {stdout:?}");

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/checkout");
    assert!(wt_path.exists(), "worktree directory should exist at {wt_path:?}");

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    let branch_name = String::from_utf8(branch.stdout).unwrap().trim().to_string();
    assert_eq!(branch_name, "feature/checkout");
}

#[test]
fn checkout_worktree_noop_if_path_exists() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "existing"]);

    // First call creates the worktree
    git_mate()
        .args(["checkout", "existing", "-w"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Second call should be a no-op
    git_mate()
        .args(["checkout", "existing", "-w"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("already checked out"));
}

#[test]
fn checkout_worktree_fails_if_directory_exists_but_is_not_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "existing"]);

    // Create a plain directory at the worktree path (no .git file)
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("existing");
    std::fs::create_dir_all(&wt_path).unwrap();

    git_mate()
        .args(["checkout", "existing", "-w"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not appear to be a git worktree"));
}

#[test]
fn checkout_worktree_fails_if_file_exists_at_path() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "existing"]);

    // Create a plain file at the worktree path
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("existing");
    std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();
    std::fs::write(&wt_path, "not a directory").unwrap();

    git_mate()
        .args(["checkout", "existing", "-w"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a directory"));
}

#[test]
fn checkout_worktree_missing_config_fails() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["branch", "some-branch"]);

    git_mate()
        .args(["checkout", "some-branch", "-w"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.worktreeRoot"));
}

#[test]
fn checkout_in_place_navigates_to_existing_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["branch", "feature/x"]);
    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "feature/x"]);

    git_mate()
        .args(["checkout", "feature/x"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(wt_path.to_str().unwrap()));
}

#[test]
fn checkout_worktree_navigates_to_existing_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["branch", "feature/x"]);
    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "feature/x"]);

    git_mate()
        .args(["checkout", "feature/x", "-w"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(wt_path.to_str().unwrap()));
}
