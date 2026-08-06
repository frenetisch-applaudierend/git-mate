mod common;

use std::process::Command;
use tempfile::TempDir;

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
fn fast_forward_in_worktree_context_updates_main_worktree_index() {
    let setup = common::RepoWithRemote::new();
    let wt_dir = TempDir::new().unwrap();

    // Set up a linked worktree on a feature branch.  main stays in the main worktree.
    setup.local_git(&[
        "worktree",
        "add",
        wt_dir.path().to_str().unwrap(),
        "-b",
        "feature/wt",
    ]);

    // Push a commit that adds a real file to origin/main.  An empty commit would
    // not expose the bug: if only the ref moves without updating the index, the
    // unchanged tree makes git-status appear clean even with the wrong approach.
    setup.push_file_commit_to_remote("new.txt", "hello", "add new.txt");

    // Run sync from inside the linked worktree so main is a non-current branch.
    common::git_mate()
        .arg("sync")
        .current_dir(wt_dir.path())
        .assert()
        .success()
        .stderr(predicates::str::contains("main: fast-forwarded"));

    // The main worktree must be clean.  Before the fix, fast_forward_branch used
    // git-update-ref which moved refs/heads/main without touching the index, so
    // git-status would report new.txt as an unstaged deletion.
    let status_out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(setup.local_path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&status_out.stdout).trim().is_empty(),
        "main worktree should be clean after fast-forward, got:\n{}",
        String::from_utf8_lossy(&status_out.stdout)
    );

    assert_eq!(
        setup.branch_tip("main"),
        setup.branch_tip("origin/main"),
        "local main should match origin/main"
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
fn deletes_local_branch_when_remote_pruned_and_user_confirms_all() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    assert!(setup.local_branch_exists("feature/gone"));

    setup.delete_remote_branch("feature/gone");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .write_stdin("a\n")
        .assert()
        .success()
        .stderr(predicates::str::contains("feature/gone: deleted"));

    assert!(
        !setup.local_branch_exists("feature/gone"),
        "local branch should have been deleted"
    );
}

#[test]
fn keeps_local_branch_when_remote_pruned_and_user_declines() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");

    setup.delete_remote_branch("feature/gone");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .write_stdin("n\n")
        .assert()
        .success()
        .stderr(predicates::str::contains("feature/gone: kept"));

    assert!(
        setup.local_branch_exists("feature/gone"),
        "local branch should have been kept"
    );
}

#[test]
fn keeps_local_branch_when_remote_pruned_and_no_input_given() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");

    setup.delete_remote_branch("feature/gone");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("feature/gone: kept"));

    assert!(
        setup.local_branch_exists("feature/gone"),
        "local branch should default to kept when no confirmation is given"
    );
}

#[test]
fn prompts_once_for_multiple_pruned_branches_and_deletes_all() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/one");
    setup.push_branch_to_remote("feature/two");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/one");
    setup.create_local_tracking_branch("feature/two");

    setup.delete_remote_branch("feature/one");
    setup.delete_remote_branch("feature/two");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .write_stdin("a\n")
        .assert()
        .success()
        .stderr(predicates::str::contains("feature/one: deleted"))
        .stderr(predicates::str::contains("feature/two: deleted"));

    assert!(!setup.local_branch_exists("feature/one"));
    assert!(!setup.local_branch_exists("feature/two"));
}

#[test]
fn decide_choice_prompts_per_branch() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/one");
    setup.push_branch_to_remote("feature/two");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/one");
    setup.create_local_tracking_branch("feature/two");

    setup.delete_remote_branch("feature/one");
    setup.delete_remote_branch("feature/two");

    // "d" picks decide-per-branch; then confirm the first, decline the second.
    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .write_stdin("d\ny\nn\n")
        .assert()
        .success()
        .stderr(predicates::str::contains("feature/one: deleted"))
        .stderr(predicates::str::contains("feature/two: kept"));

    assert!(!setup.local_branch_exists("feature/one"));
    assert!(setup.local_branch_exists("feature/two"));
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
