mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn checkout_in_place_from_main_worktree() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["branch", "existing-branch"]);

    common::git_mate()
        .args(["checkout", "existing-branch"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "existing-branch");
}

#[test]
fn checkout_in_place_from_linked_worktree_cds_to_main_and_switches() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "-b", "linked-branch", "main"]);
    repo.git(&["branch", "other-branch"]);

    let output = common::git_mate()
        .args(["checkout", "other-branch"])
        .env("GIT_MATE_SHELL", "1")
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let main_path = repo.path().to_str().unwrap();
    assert!(
        stdout.contains("_MATE_CD:") && stdout.contains(main_path),
        "stdout should contain _MATE_CD: pointing to main worktree, got: {stdout:?}"
    );

    assert_eq!(repo.current_branch(), "other-branch");
}

#[test]
fn checkout_in_place_from_linked_worktree_navigates_to_existing_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_a = wt_root.path().join("a");
    let wt_b = wt_root.path().join("b");

    repo.git(&["branch", "branch-a"]);
    repo.git(&["branch", "branch-b"]);
    repo.git(&["worktree", "add", wt_a.to_str().unwrap(), "branch-a"]);
    repo.git(&["worktree", "add", wt_b.to_str().unwrap(), "branch-b"]);

    let output = common::git_mate()
        .args(["checkout", "branch-b"])
        .env("GIT_MATE_SHELL", "1")
        .current_dir(&wt_a)
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("_MATE_CD:") && stdout.contains(wt_b.to_str().unwrap()),
        "stdout should contain _MATE_CD: pointing to branch-b worktree, got: {stdout:?}"
    );
}

#[test]
fn checkout_worktree_creates_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "feature/checkout"]);

    let output = common::git_mate()
        .args(["checkout", "feature/checkout", "-w"])
        .env("GIT_MATE_SHELL", "1")
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
    common::git_mate()
        .args(["checkout", "existing", "-w"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Second call should be a no-op
    common::git_mate()
        .args(["checkout", "existing", "-w"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("already checked out"));
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

    common::git_mate()
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

    common::git_mate()
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

    common::git_mate()
        .args(["checkout", "some-branch", "-w"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
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

    common::git_mate()
        .args(["checkout", "feature/x"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(wt_path.to_str().unwrap()));
}

#[test]
fn checkout_worktree_navigates_to_existing_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["branch", "feature/x"]);
    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "feature/x"]);

    common::git_mate()
        .args(["checkout", "feature/x", "-w"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(wt_path.to_str().unwrap()));
}
