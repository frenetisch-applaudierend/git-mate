mod common;

use predicates::prelude::PredicateBooleanExt;
use std::process::Command;
use tempfile::TempDir;

fn prepare_feature_branch(setup: &common::RepoWithRemote, branch: &str) -> String {
    setup.local_git(&["checkout", "-b", branch]);
    setup.local_git(&["commit", "--allow-empty", "-m", "feature start"]);
    setup.local_git(&["push", "-u", "origin", branch]);
    setup.local_head_commit()
}

/// Matches a report line: glyph, branch name, padding, then `detail`.
fn entry(branch: &str, detail: &str) -> predicates::str::RegexPredicate {
    predicates::str::is_match(format!(r"(?m)^ \S {} +{}", escape(branch), escape(detail))).unwrap()
}

/// A report line naming `branch` with no detail after it.
fn bare_entry(branch: &str) -> predicates::str::RegexPredicate {
    predicates::str::is_match(format!(r"(?m)^ \S {}$", escape(branch))).unwrap()
}

fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let special = "\\.+*?()|[]{}^$#&-~".contains(c);
            special.then_some('\\').into_iter().chain([c])
        })
        .collect()
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
        .stderr(entry("feature/ff", "fast-forwarded"));

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
        .stderr(entry("main", "fast-forwarded"));

    // The main worktree must be clean.  Before the fix, fast_forward_branch used
    // git-update-ref which moved refs/heads/main without touching the index, so
    // git-status would report new.txt as an unstaged deletion.
    let status_out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(setup.local_path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&status_out.stdout)
            .trim()
            .is_empty(),
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
    let before = setup.branch_tip("feature/div");

    common::git_mate()
        .args(["sync", "--diverged=skip"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("cannot fast-forward"));
    assert_eq!(setup.branch_tip("feature/div"), before);
}

// --- diverged non-current branches ---

/// A scratch clone of the remote with `branch` checked out, for building up
/// remote-side history (including rewrites) before pushing.
fn remote_scratch(setup: &common::RepoWithRemote, branch: &str) -> TempDir {
    let scratch = TempDir::new().unwrap();
    let bare_url = setup.bare_path().to_str().unwrap().to_string();
    common::git(scratch.path(), &["clone", "-q", &bare_url, "."]);
    common::git(scratch.path(), &["config", "user.email", "test@test.com"]);
    common::git(scratch.path(), &["config", "user.name", "Test"]);
    common::git(scratch.path(), &["checkout", "-q", branch]);
    scratch
}

fn commit_file(dir: &std::path::Path, name: &str, content: &str, message: &str) {
    std::fs::write(dir.join(name), content).unwrap();
    common::git(dir, &["add", name]);
    common::git(dir, &["commit", "-q", "-m", message]);
}

fn commit_file_on(setup: &common::RepoWithRemote, branch: &str, name: &str, content: &str) {
    let current = setup.local_current_branch();
    setup.local_git(&["checkout", "-q", branch]);
    commit_file(setup.local_path(), name, content, &format!("local {name}"));
    setup.local_git(&["checkout", "-q", &current]);
}

fn parent_count(dir: &std::path::Path, rev: &str) -> usize {
    let out = Command::new("git")
        .args(["rev-list", "--parents", "-n1", rev])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .split_whitespace()
        .count()
        - 1
}

/// Local branch with one unpushed file commit; remote gains another file commit.
fn diverged_with_local_work(setup: &common::RepoWithRemote, branch: &str) -> String {
    setup.push_branch_to_remote(branch);
    setup.local_fetch();
    setup.create_local_tracking_branch(branch);
    commit_file_on(setup, branch, "local.txt", "local");
    let scratch = remote_scratch(setup, branch);
    commit_file(scratch.path(), "remote.txt", "remote", "remote change");
    common::git(scratch.path(), &["push", "-q"]);
    setup.branch_tip(branch)
}

#[test]
fn merges_diverged_branch_with_local_work() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/div");
    let main_before = setup.local_head_commit();

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry("feature/div", "merged upstream"))
        .stderr(predicates::str::contains("1 merged"));

    let tip = setup.branch_tip("feature/div");
    assert_eq!(parent_count(setup.local_path(), &tip), 2);
    assert!(is_ancestor(setup.local_path(), &before, &tip));
    assert!(is_ancestor(setup.local_path(), "origin/feature/div", &tip));
    assert_eq!(
        setup.local_head_commit(),
        main_before,
        "current branch untouched"
    );
    assert_eq!(setup.local_current_branch(), "main");
}

#[test]
fn merges_diverged_branch_checked_out_in_clean_worktree() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/wt");
    let wt_dir = TempDir::new().unwrap();
    setup.local_git(&[
        "worktree",
        "add",
        wt_dir.path().to_str().unwrap(),
        "feature/wt",
    ]);

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry("feature/wt", "merged upstream"));

    let tip = setup.branch_tip("feature/wt");
    assert!(is_ancestor(setup.local_path(), &before, &tip));
    assert!(
        wt_dir.path().join("remote.txt").exists(),
        "worktree files should follow the merge"
    );
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(wt_dir.path())
        .output()
        .unwrap();
    assert!(status.stdout.is_empty(), "worktree should be clean");
}

#[test]
fn skips_diverged_branch_with_dirty_worktree() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/dirty");
    let wt_dir = TempDir::new().unwrap();
    setup.local_git(&[
        "worktree",
        "add",
        wt_dir.path().to_str().unwrap(),
        "feature/dirty",
    ]);
    std::fs::write(wt_dir.path().join("wip.txt"), "uncommitted").unwrap();

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry(
            "feature/dirty",
            "diverged, but worktree has uncommitted changes",
        ));
    assert_eq!(setup.branch_tip("feature/dirty"), before);
}

#[test]
fn skips_diverged_branch_with_conflicts() {
    let setup = common::RepoWithRemote::new();
    setup.push_branch_to_remote("feature/clash");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/clash");
    commit_file_on(&setup, "feature/clash", "same.txt", "local");
    let scratch = remote_scratch(&setup, "feature/clash");
    commit_file(scratch.path(), "same.txt", "remote", "remote change");
    common::git(scratch.path(), &["push", "-q"]);
    let before = setup.branch_tip("feature/clash");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry(
            "feature/clash",
            "diverged, merging upstream would conflict",
        ))
        .stderr(predicates::str::contains(
            "resolve with: git checkout feature/clash && git pull",
        ));
    assert_eq!(
        setup.branch_tip("feature/clash"),
        before,
        "branch untouched"
    );
}

#[test]
fn resets_branch_when_remote_was_rewritten() {
    let setup = common::RepoWithRemote::new();
    setup.push_branch_to_remote("feature/rewrite");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/rewrite");

    // Someone amends the remote tip and force-pushes; local has no work of its own.
    let scratch = remote_scratch(&setup, "feature/rewrite");
    common::git(
        scratch.path(),
        &[
            "commit",
            "-q",
            "--amend",
            "--allow-empty",
            "-m",
            "rewritten",
        ],
    );
    common::git(scratch.path(), &["push", "-q", "--force"]);

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry(
            "feature/rewrite",
            "reset to rewritten upstream (was ",
        ))
        .stderr(predicates::str::contains("1 reset"));

    assert_eq!(
        setup.branch_tip("feature/rewrite"),
        setup.branch_tip("origin/feature/rewrite")
    );
}

#[test]
fn resets_branch_whose_local_patches_are_already_upstream() {
    let setup = common::RepoWithRemote::new();
    setup.push_branch_to_remote("feature/picked");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/picked");
    commit_file_on(&setup, "feature/picked", "a.txt", "same change");

    // The remote gets another commit plus the same patch as the local one.
    let scratch = remote_scratch(&setup, "feature/picked");
    commit_file(scratch.path(), "b.txt", "other", "other change");
    commit_file(
        scratch.path(),
        "a.txt",
        "same change",
        "same change, upstream",
    );
    common::git(scratch.path(), &["push", "-q"]);

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry("feature/picked", "reset to rewritten upstream"));

    assert_eq!(
        setup.branch_tip("feature/picked"),
        setup.branch_tip("origin/feature/picked")
    );
}

#[test]
fn diverged_dry_run_changes_nothing() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/dry");

    common::git_mate()
        .args(["sync", "--dry-run"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry("feature/dry", "would merge upstream"));
    assert_eq!(setup.branch_tip("feature/dry"), before);
}

#[test]
fn ff_only_skips_diverged_branches() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/ffonly");

    common::git_mate()
        .args(["sync", "--ff-only"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("cannot fast-forward (diverged)"));
    assert_eq!(setup.branch_tip("feature/ffonly"), before);
}

#[test]
fn diverged_strategy_config_skip_is_respected() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/cfg");
    setup.local_git(&["config", "mate.divergedStrategy", "skip"]);

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("cannot fast-forward (diverged)"));
    assert_eq!(setup.branch_tip("feature/cfg"), before);
}

#[test]
fn invalid_diverged_strategy_config_is_an_error() {
    let setup = common::RepoWithRemote::new();
    setup.local_git(&["config", "mate.divergedStrategy", "bogus"]);

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "invalid value for mate.divergedStrategy",
        ));
}

#[test]
fn json_reports_merged_diverged_branch() {
    let setup = common::RepoWithRemote::new();
    diverged_with_local_work(&setup, "feature/json");

    let output = common::git_mate()
        .args(["--json", "sync"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();
    assert!(
        stdout.contains(r#""branch":"feature/json","action":"merged""#),
        "unexpected JSON: {stdout}"
    );
}

// --- a single branch's error must not abort the rest of the run ---

#[test]
fn branch_failure_does_not_abort_remaining_branches() {
    let setup = common::RepoWithRemote::new();

    // feature/broken will fail to fast-forward: its worktree has an untracked
    // file that collides with a file the remote is about to add, so
    // `git merge --ff-only` refuses to overwrite it.
    setup.push_branch_to_remote("feature/broken");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/broken");

    let wt_dir = TempDir::new().unwrap();
    setup.local_git(&[
        "worktree",
        "add",
        wt_dir.path().to_str().unwrap(),
        "feature/broken",
    ]);
    std::fs::write(wt_dir.path().join("clash.txt"), "local uncommitted").unwrap();

    let scratch = TempDir::new().unwrap();
    common::git(
        scratch.path(),
        &["clone", setup.bare_path().to_str().unwrap(), "."],
    );
    common::git(scratch.path(), &["config", "user.email", "test@test.com"]);
    common::git(scratch.path(), &["config", "user.name", "Test"]);
    common::git(scratch.path(), &["checkout", "feature/broken"]);
    std::fs::write(scratch.path().join("clash.txt"), "remote version").unwrap();
    common::git(scratch.path(), &["add", "clash.txt"]);
    common::git(scratch.path(), &["commit", "-m", "add clash.txt"]);
    common::git(scratch.path(), &["push"]);

    // feature/ok is an ordinary fast-forward with nothing in its way.
    setup.push_branch_to_remote("feature/ok");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/ok");
    setup.push_commit_to_remote_branch("feature/ok", "advance feature/ok");

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry("feature/broken", "failed: "))
        .stderr(entry("feature/ok", "fast-forwarded"))
        .stderr(predicates::str::contains("1 failed"));

    assert_eq!(
        setup.branch_tip("feature/ok"),
        setup.branch_tip("origin/feature/ok"),
        "feature/ok should still be fast-forwarded despite feature/broken failing"
    );
}

// --- final status message ---

#[test]
fn final_status_reflects_other_branch_work_when_current_branch_untouched() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/ff");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/ff");
    setup.push_commit_to_remote_branch("feature/ff", "advance feature");

    // Switch to a branch with no upstream, so the current-branch pull/merge
    // path contributes nothing to the final status — before the fix, that
    // meant no final status line printed at all even though feature/ff was
    // fast-forwarded.
    setup.local_git(&["checkout", "-b", "scratch"]);

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry("feature/ff", "fast-forwarded"))
        .stderr(predicates::str::contains("Synced."));
}

#[test]
fn final_status_reports_nothing_to_do() {
    let setup = common::RepoWithRemote::new();
    setup.local_git(&["checkout", "-b", "scratch"]);

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("Already up to date."));
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
        .stderr(entry("feature/gone", "merged into origin/main"))
        .stderr(predicates::str::contains("1 deleted"));

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
        .stderr(predicates::str::contains("1 kept"));

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
        .stderr(predicates::str::contains("1 kept"));

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
        .stderr(predicates::str::contains("Delete these 2 branches?"))
        .stderr(predicates::str::contains("2 deleted"));

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
        .stderr(predicates::str::contains("Delete feature/one?"))
        .stderr(predicates::str::contains("Delete feature/two?"))
        .stderr(predicates::str::contains("1 deleted · 1 kept"));

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

// --- --delete-pruned ---

#[test]
fn delete_pruned_deletes_without_prompting() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    assert!(setup.local_branch_exists("feature/gone"));

    setup.delete_remote_branch("feature/gone");

    // No stdin provided: if --delete-pruned still prompted, this would hang
    // waiting on input rather than complete.
    common::git_mate()
        .args(["sync", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("1 deleted"));

    assert!(
        !setup.local_branch_exists("feature/gone"),
        "local branch should have been deleted"
    );
}

#[test]
fn delete_pruned_conflicts_with_dry_run() {
    let setup = common::RepoWithRemote::new();

    common::git_mate()
        .args(["sync", "--delete-pruned", "--dry-run"])
        .current_dir(setup.local_path())
        .assert()
        .failure();
}

#[test]
fn delete_pruned_makes_json_actually_delete() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    setup.delete_remote_branch("feature/gone");

    let output = common::git_mate()
        .args(["--json", "sync", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["ok"], true);
    let branches = json["data"]["branches"].as_array().unwrap();
    assert!(
        branches
            .iter()
            .any(|b| b["branch"] == "feature/gone" && b["action"] == "deleted"),
        "expected feature/gone reported as deleted, got: {branches:?}"
    );

    assert!(
        !setup.local_branch_exists("feature/gone"),
        "local branch should have been deleted"
    );
}

// --- Branches whose upstream was already gone before sync ---

/// Local branch with two file commits, pushed with upstream tracking; its
/// remote branch is then deleted and the tracking ref pruned by a plain fetch,
/// so sync never sees it disappear.
fn gone_branch_with_changes(setup: &common::RepoWithRemote, branch: &str) {
    setup.local_git(&["checkout", "-q", "-b", branch]);
    commit_file(setup.local_path(), "a.txt", "a", "add a");
    commit_file(setup.local_path(), "b.txt", "b", "add b");
    setup.local_git(&["push", "-q", "-u", "origin", branch]);
    setup.local_git(&["checkout", "-q", "main"]);
    setup.delete_remote_branch(branch);
    setup.local_fetch();
    assert!(!setup.remote_tracking_exists(&format!("origin/{branch}")));
}

#[test]
fn offers_gone_branch_that_was_merged() {
    let setup = common::RepoWithRemote::new();
    gone_branch_with_changes(&setup, "feature/merged");
    setup.local_git(&["merge", "-q", "--no-ff", "-m", "merge", "feature/merged"]);
    setup.local_git(&["push", "-q", "origin", "main"]);

    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .write_stdin("a\n")
        .assert()
        .success()
        .stderr(entry("feature/merged", "merged in "))
        .stderr(predicates::str::contains("1 deleted"));

    assert!(!setup.local_branch_exists("feature/merged"));
}

#[test]
fn offers_gone_branch_that_was_squash_merged() {
    let setup = common::RepoWithRemote::new();
    gone_branch_with_changes(&setup, "feature/squashed");
    let scratch = remote_scratch(&setup, "main");
    std::fs::write(scratch.path().join("a.txt"), "a").unwrap();
    common::git(scratch.path(), &["add", "a.txt"]);
    commit_file(scratch.path(), "b.txt", "b", "Squashed feature (#42)");
    // main moves on afterwards; the branch is still recognised as merged.
    commit_file(scratch.path(), "c.txt", "c", "later work");
    common::git(scratch.path(), &["push", "-q"]);

    common::git_mate()
        .args(["sync", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry("feature/squashed", "merged in #42"))
        .stderr(predicates::str::contains("1 deleted"));

    assert!(!setup.local_branch_exists("feature/squashed"));
}

#[test]
fn offers_gone_branch_that_was_squash_merged_before_files_moved_and_edited() {
    let setup = common::RepoWithRemote::new();
    gone_branch_with_changes(&setup, "feature/moved");
    let scratch = remote_scratch(&setup, "main");
    std::fs::write(scratch.path().join("a.txt"), "a").unwrap();
    common::git(scratch.path(), &["add", "a.txt"]);
    commit_file(scratch.path(), "b.txt", "b", "squashed feature");
    // Later work moves and rewrites the merged files, so merging the branch
    // into today's main would conflict.
    std::fs::create_dir(scratch.path().join("moved")).unwrap();
    common::git(scratch.path(), &["mv", "a.txt", "moved/a.txt"]);
    common::git(scratch.path(), &["mv", "b.txt", "moved/b.txt"]);
    std::fs::write(scratch.path().join("moved/a.txt"), "rewritten a").unwrap();
    std::fs::write(scratch.path().join("moved/b.txt"), "rewritten b").unwrap();
    common::git(scratch.path(), &["commit", "-q", "-am", "move and rewrite"]);
    common::git(scratch.path(), &["push", "-q"]);

    common::git_mate()
        .args(["sync", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("1 deleted"));

    assert!(!setup.local_branch_exists("feature/moved"));
}

#[test]
fn offers_gone_branch_that_was_rebase_merged() {
    let setup = common::RepoWithRemote::new();
    gone_branch_with_changes(&setup, "feature/rebased");
    let scratch = remote_scratch(&setup, "main");
    commit_file(scratch.path(), "c.txt", "c", "unrelated work first");
    commit_file(scratch.path(), "a.txt", "a", "add a (rebased)");
    commit_file(scratch.path(), "b.txt", "b", "add b (rebased)");
    common::git(scratch.path(), &["push", "-q"]);

    common::git_mate()
        .args(["sync", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("1 deleted"));

    assert!(!setup.local_branch_exists("feature/rebased"));
}

#[test]
fn skips_gone_branch_with_unmerged_changes_and_explains() {
    let setup = common::RepoWithRemote::new();
    gone_branch_with_changes(&setup, "feature/unmerged");

    common::git_mate()
        .args(["sync", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "Remote gone, not merged into origin/main",
        ))
        .stderr(bare_entry("feature/unmerged"))
        .stderr(predicates::str::contains("delete: git branch -D <branch>"))
        .stderr(predicates::str::contains(
            "keep: git branch --unset-upstream <branch>",
        ))
        .stderr(predicates::str::contains("could not resolve upstream").not());

    assert!(setup.local_branch_exists("feature/unmerged"));
}

#[test]
fn skips_gone_branch_when_only_part_was_squash_merged() {
    let setup = common::RepoWithRemote::new();
    gone_branch_with_changes(&setup, "feature/partial");
    let scratch = remote_scratch(&setup, "main");
    commit_file(scratch.path(), "a.txt", "a", "only part of the feature");
    common::git(scratch.path(), &["push", "-q"]);

    common::git_mate()
        .args(["sync", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "Remote gone, not merged into origin/main",
        ))
        .stderr(bare_entry("feature/partial"));

    assert!(setup.local_branch_exists("feature/partial"));
}

#[test]
fn kept_pruned_branch_stops_tracking_and_is_not_offered_again() {
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
        .stderr(predicates::str::contains(
            "stopped tracking the deleted upstream",
        ));

    assert!(setup.local_branch_exists("feature/gone"));

    // No stdin: a second prompt would read EOF and keep — assert it never asks.
    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("Delete this").not())
        .stderr(predicates::str::contains("kept").not());
}

// --- end-of-run summary ---

#[test]
fn summary_reports_counts_grouped_by_outcome() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/ff");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/ff");
    setup.push_commit_to_remote_branch("feature/ff", "advance feature");

    setup.push_branch_to_remote("feature/div");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/div");
    setup.make_local_commit_on("feature/div", "local-only commit");
    setup.push_commit_to_remote_branch("feature/div", "remote-only commit");

    common::git_mate()
        .args(["sync", "--diverged=skip"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains(
            "1 fast-forwarded · 1 need attention",
        ))
        .stderr(entry("feature/div", "cannot fast-forward (diverged)"));
}

#[test]
fn summary_omitted_in_json_mode() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/ff");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/ff");
    setup.push_commit_to_remote_branch("feature/ff", "advance feature");

    let output = common::git_mate()
        .args(["--json", "sync"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();

    assert!(
        String::from_utf8_lossy(&output).trim().is_empty(),
        "--json mode should not print the human-readable summary"
    );
}

// --- --dry-run ---

#[test]
fn dry_run_does_not_pull_or_fast_forward() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/ff");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/ff");
    let feature_before = setup.branch_tip("feature/ff");
    setup.push_commit_to_remote_branch("feature/ff", "advance feature");

    let main_before = setup.local_head_commit();
    setup.push_commit_to_remote("advance main");

    common::git_mate()
        .args(["sync", "--dry-run"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(entry("feature/ff", "would fast-forward"))
        .stderr(predicates::str::contains("would pull"));

    assert_eq!(
        setup.branch_tip("feature/ff"),
        feature_before,
        "dry run must not fast-forward feature/ff"
    );
    assert_eq!(
        setup.local_head_commit(),
        main_before,
        "dry run must not pull the current branch"
    );
}

#[test]
fn dry_run_does_not_delete_pruned_branch_or_prompt() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    assert!(setup.local_branch_exists("feature/gone"));

    setup.delete_remote_branch("feature/gone");

    // No stdin provided: if --dry-run still prompted, this would hang or read
    // EOF and be interpreted as "keep" anyway, so also assert on the message
    // to prove the prompt path was skipped entirely.
    common::git_mate()
        .args(["sync", "--dry-run"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicates::str::contains("dry run: not deleting anything"));

    assert!(
        setup.local_branch_exists("feature/gone"),
        "dry run must not delete the local branch"
    );
}

#[test]
fn dry_run_json_reports_plan_without_acting() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    setup.delete_remote_branch("feature/gone");

    let main_before = setup.local_head_commit();
    setup.push_commit_to_remote("advance main");

    let output = common::git_mate()
        .args(["--json", "sync", "--dry-run"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["dryRun"], true);
    assert_eq!(json["data"]["currentBranch"]["pulled"], true);

    let branches = json["data"]["branches"].as_array().unwrap();
    assert!(
        branches
            .iter()
            .any(|b| b["branch"] == "feature/gone" && b["action"] == "deletion-candidate"),
        "expected feature/gone reported as a deletion candidate, got: {branches:?}"
    );

    assert!(
        setup.local_branch_exists("feature/gone"),
        "dry run must not delete the local branch"
    );
    assert_eq!(
        setup.local_head_commit(),
        main_before,
        "dry run must not pull the current branch"
    );
}
