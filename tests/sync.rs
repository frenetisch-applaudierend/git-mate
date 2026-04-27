mod common;

use assert_cmd::prelude::*;
use std::process::Command;

fn prepare_feature_branch(setup: &common::RepoWithRemote, branch: &str) -> String {
    setup.local_git(&["checkout", "-b", branch]);
    setup.local_git(&["commit", "--allow-empty", "-m", "feature start"]);
    setup.local_git(&["push", "-u", "origin", branch]);
    setup.local_head_commit()
}

fn is_ancestor(dir: &std::path::Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(dir)
        .status()
        .unwrap()
        .success()
}

#[test]
fn no_upstream() {
    // Repo with no remote → fetch is a no-op; upstream check fails → prints notice and exits 0.
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .arg("sync")
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("No upstream"));
}

#[test]
fn fetch_and_pull() {
    let setup = common::RepoWithRemote::new();
    let before = setup.local_head_commit();
    setup.push_commit_to_remote("second commit");
    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success();
    assert_ne!(
        setup.local_head_commit(),
        before,
        "HEAD should have advanced"
    );
}

#[test]
fn rebase_flag() {
    let setup = common::RepoWithRemote::new();
    let before = setup.local_head_commit();
    setup.push_commit_to_remote("second commit");
    common::git_mate()
        .args(["sync", "--rebase"])
        .current_dir(setup.local_path())
        .assert()
        .success();
    assert_ne!(
        setup.local_head_commit(),
        before,
        "HEAD should have advanced"
    );
}

#[test]
fn merge_flag_merges_default_branch_into_current_branch() {
    let setup = common::RepoWithRemote::new();
    let before = prepare_feature_branch(&setup, "feature/merge");
    setup.push_commit_to_remote("advance main");

    common::git_mate()
        .args(["sync", "--merge"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("merged default branch 'main'"));

    let main_tip = setup.branch_tip("main");
    assert_ne!(setup.local_head_commit(), before);
    assert!(
        is_ancestor(setup.local_path(), &main_tip, "HEAD"),
        "current branch should contain the updated default branch"
    );
}

#[test]
fn auto_merge_config_merges_default_branch_into_current_branch() {
    let setup = common::RepoWithRemote::new();
    prepare_feature_branch(&setup, "feature/config-merge");
    setup.local_git(&["config", "mate.autoMerge", "true"]);
    setup.push_commit_to_remote("advance main");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("merged default branch 'main'"));

    let main_tip = setup.branch_tip("main");
    assert!(
        is_ancestor(setup.local_path(), &main_tip, "HEAD"),
        "config-enabled sync should merge the default branch"
    );
}

#[test]
fn merge_flag_overrides_disabled_config() {
    let setup = common::RepoWithRemote::new();
    prepare_feature_branch(&setup, "feature/override-merge");
    setup.local_git(&["config", "mate.autoMerge", "false"]);
    setup.push_commit_to_remote("advance main");

    common::git_mate()
        .args(["sync", "--merge"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("merged default branch 'main'"));

    let main_tip = setup.branch_tip("main");
    assert!(
        is_ancestor(setup.local_path(), &main_tip, "HEAD"),
        "--merge should override mate.autoMerge=false"
    );
}

#[test]
fn no_merge_flag_overrides_enabled_config() {
    let setup = common::RepoWithRemote::new();
    let before = prepare_feature_branch(&setup, "feature/no-merge");
    setup.local_git(&["config", "mate.autoMerge", "true"]);
    setup.push_commit_to_remote("advance main");

    common::git_mate()
        .args(["sync", "--no-merge"])
        .current_dir(setup.local_path())
        .assert()
        .success();

    assert_eq!(
        setup.local_head_commit(),
        before,
        "--no-merge should suppress auto-merge from config"
    );
}

#[test]
fn merge_runs_without_upstream_when_enabled() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/local"]);
    repo.git(&["checkout", "main"]);
    repo.git(&["commit", "--allow-empty", "-m", "advance main"]);
    let main_tip = repo.head_commit();
    repo.git(&["checkout", "feature/local"]);

    common::git_mate()
        .args(["sync", "--merge"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("No upstream"))
        .stderr(predicates::str::contains("merged default branch 'main'"));

    assert!(
        is_ancestor(repo.path(), &main_tip, "HEAD"),
        "auto-merge should still run without an upstream"
    );
}

#[test]
fn prune_deleted_branch() {
    let setup = common::RepoWithRemote::new();

    // Create feature/old on remote and fetch it so the tracking ref appears locally.
    setup.push_branch_to_remote("feature/old");
    common::git(setup.local_path(), &["fetch"]);
    assert!(
        setup.remote_tracking_exists("origin/feature/old"),
        "tracking ref should exist after fetch"
    );

    // Delete the branch from the remote.
    setup.delete_remote_branch("feature/old");

    // sync runs `git fetch --all --prune`, which should remove the stale tracking ref.
    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success();

    assert!(
        !setup.remote_tracking_exists("origin/feature/old"),
        "tracking ref should be pruned after sync"
    );
}

// --- Fast-forward non-current branches ---

#[test]
fn fast_forwards_non_current_branch() {
    let setup = common::RepoWithRemote::new();

    // Push a feature branch to remote and create a local tracking branch.
    setup.push_branch_to_remote("feature/ff");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/ff");

    let before = setup.branch_tip("feature/ff");

    // Push a new commit to the remote feature branch.
    setup.push_commit_to_remote_branch("feature/ff", "advance feature");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("feature/ff: fast-forwarded"));

    assert_ne!(
        setup.branch_tip("feature/ff"),
        before,
        "local branch should have been fast-forwarded"
    );
    assert_eq!(
        setup.branch_tip("feature/ff"),
        setup.branch_tip("origin/feature/ff"),
        "local branch should match remote"
    );
}

#[test]
fn skips_diverged_non_current_branch() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/div");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/div");

    // Make a local commit on the branch (diverges from remote).
    setup.make_local_commit_on("feature/div", "local-only commit");
    // Push a different commit to the remote branch.
    setup.push_commit_to_remote_branch("feature/div", "remote-only commit");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("cannot fast-forward"));
}

// --- Deletion of local branches whose remote was pruned ---

#[test]
fn deletes_local_branch_when_remote_pruned() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    assert!(setup.local_branch_exists("feature/gone"));

    setup.delete_remote_branch("feature/gone");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("feature/gone: deleted"));

    assert!(
        !setup.local_branch_exists("feature/gone"),
        "local branch should have been deleted"
    );
}

#[test]
fn keeps_local_branch_with_unpushed_commits_when_remote_pruned() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/keep");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/keep");
    setup.make_local_commit_on("feature/keep", "unpushed work");

    setup.delete_remote_branch("feature/keep");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("unpushed commits"));

    assert!(
        setup.local_branch_exists("feature/keep"),
        "local branch with unpushed commits should be kept"
    );
}
