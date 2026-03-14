mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn finish_feature_branch_switches_to_main() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/x"]);
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "main");
}

#[test]
fn finish_on_default_branch_fails() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("default branch"));
}

#[test]
fn finish_delete_branch_switches_and_deletes() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/x"]);
    // feature/x is at the same commit as main, so -d will succeed
    common::git_mate()
        .args(["finish", "--delete-branch"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "main");
    assert!(!repo.branch_exists("feature/x"), "feature/x should have been deleted");
}

#[test]
fn finish_delete_branch_unmerged_fails() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/x"]);
    // Make an unmerged commit
    repo.git(&["commit", "--allow-empty", "-m", "unmerged work"]);
    common::git_mate()
        .args(["finish", "--delete-branch"])
        .current_dir(repo.path())
        .assert()
        .failure();
}

#[test]
fn finish_linked_worktree_removes_it_and_prints_mate_cd() {
    let repo = common::RepoWithoutRemote::new();
    // Add a linked worktree for feature/x
    let wt_path = repo.path().join("feature-x-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/x", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();

    let output = common::git_mate()
        .args(["finish"])
        .env("GIT_MATE_SHELL", "1")
        .current_dir(&wt_canonical)
        .output()
        .unwrap();
    assert!(output.status.success(), "finish should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let main_path = std::fs::canonicalize(repo.path()).unwrap();
    assert!(
        stdout.contains(&format!("_MATE_CD:{}", main_path.display())),
        "stdout should contain _MATE_CD:<main_path>, got: {stdout:?}"
    );
    assert!(!wt_canonical.exists(), "worktree directory should be removed");
}

#[test]
fn finish_branch_name_from_main_worktree_removes_linked_wt() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-y-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/y", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();

    common::git_mate()
        .args(["finish", "feature/y"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(!wt_canonical.exists(), "worktree directory should be removed");
}

#[test]
fn finish_linked_worktree_with_delete_branch_removes_wt_and_deletes_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-del-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/del", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();

    let output = common::git_mate()
        .args(["finish", "--delete-branch"])
        .current_dir(&wt_canonical)
        .output()
        .unwrap();
    assert!(output.status.success(), "finish --delete-branch should succeed");
    assert!(!wt_canonical.exists(), "worktree directory should be removed");
    assert!(!repo.branch_exists("feature/del"), "feature/del should have been deleted");
}

#[test]
fn finish_branch_not_checked_out_no_delete_fails() {
    let repo = common::RepoWithoutRemote::new();
    // Create a branch and switch back to main without a worktree
    repo.git(&["branch", "feature/z"]);
    common::git_mate()
        .args(["finish", "feature/z"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not checked out anywhere"));
}

#[test]
fn finish_branch_not_checked_out_with_delete_deletes_it() {
    let repo = common::RepoWithoutRemote::new();
    // Create a branch (merged, same commit as main) and stay on main
    repo.git(&["branch", "feature/z"]);
    common::git_mate()
        .args(["finish", "feature/z", "--delete-branch"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(!repo.branch_exists("feature/z"), "feature/z should have been deleted");
}
