mod common;

use std::process::Command;
use tempfile::TempDir;

fn prepare_feature_branch(setup: &common::RepoWithRemote, branch: &str) -> String {
    setup.local_git(&["checkout", "-b", branch]);
    setup.local_git(&["commit", "--allow-empty", "-m", "feature start"]);
    setup.local_git(&["push", "-u", "origin", branch]);
    setup.local_head_commit()
}

/// Runs `git mate --json sync` with `args` in `dir` and returns the
/// envelope's `data`.
fn sync_json(dir: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = common::git_mate()
        .args(["--json", "sync"])
        .args(args)
        .current_dir(dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["ok"], true, "unexpected JSON: {json}");
    json["data"].clone()
}

/// Asserts that sync reported `action` for `branch`, and returns that entry.
fn assert_action<'a>(
    data: &'a serde_json::Value,
    branch: &str,
    action: &str,
) -> &'a serde_json::Value {
    data["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["branch"] == branch && b["action"] == action)
        .unwrap_or_else(|| {
            panic!(
                "expected {branch} to be {action}, got: {}",
                data["branches"]
            )
        })
}

fn reason(entry: &serde_json::Value) -> &str {
    entry["reason"].as_str().unwrap_or_default()
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
    let data = sync_json(repo.path(), &[]);
    assert_eq!(data["currentBranch"]["pulled"], false);
    assert_eq!(
        data["currentBranch"]["pullSkippedReason"],
        "no upstream configured for current branch"
    );
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

    let data = sync_json(setup.local_path(), &["--merge"]);
    assert_action(&data, "feature/merge", "merged-default");
    assert_eq!(data["currentBranch"]["mergedDefault"], true);

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

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "feature/config-merge", "merged-default");
    assert_eq!(data["currentBranch"]["mergedDefault"], true);

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

    let data = sync_json(setup.local_path(), &["--merge"]);
    assert_action(&data, "feature/override-merge", "merged-default");
    assert_eq!(data["currentBranch"]["mergedDefault"], true);

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

    let data = sync_json(repo.path(), &["--merge"]);
    assert_eq!(data["currentBranch"]["pulled"], false);
    assert_eq!(
        data["currentBranch"]["pullSkippedReason"],
        "no upstream configured for current branch"
    );
    assert_action(&data, "feature/local", "merged-default");
    assert_eq!(data["currentBranch"]["mergedDefault"], true);

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

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "feature/ff", "fast-forwarded");

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
    let data = sync_json(wt_dir.path(), &[]);
    assert_action(&data, "main", "fast-forwarded");

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

    let data = sync_json(setup.local_path(), &["--diverged=skip"]);
    let entry = assert_action(&data, "feature/div", "skipped");
    assert_eq!(entry["reason"], "cannot fast-forward (diverged)");
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

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "feature/div", "merged");

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

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "feature/wt", "merged");

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

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "feature/dirty", "skipped");
    assert_eq!(entry["reason"], "diverged but working tree is dirty");
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

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "feature/clash", "skipped");
    assert_eq!(entry["reason"], "diverged with conflicts");
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

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "feature/rewrite", "reset-to-upstream");
    assert!(
        reason(entry).starts_with("remote was rewritten; previous tip "),
        "unexpected reason: {entry}"
    );

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

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "feature/picked", "reset-to-upstream");

    assert_eq!(
        setup.branch_tip("feature/picked"),
        setup.branch_tip("origin/feature/picked")
    );
}

#[test]
fn diverged_dry_run_changes_nothing() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/dry");

    let data = sync_json(setup.local_path(), &["--dry-run"]);
    assert_eq!(data["dryRun"], true);
    assert_action(&data, "feature/dry", "merged");
    assert_eq!(setup.branch_tip("feature/dry"), before);
}

#[test]
fn ff_only_skips_diverged_branches() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/ffonly");

    let data = sync_json(setup.local_path(), &["--ff-only"]);
    let entry = assert_action(&data, "feature/ffonly", "skipped");
    assert_eq!(entry["reason"], "cannot fast-forward (diverged)");
    assert_eq!(setup.branch_tip("feature/ffonly"), before);
}

#[test]
fn diverged_strategy_config_skip_is_respected() {
    let setup = common::RepoWithRemote::new();
    let before = diverged_with_local_work(&setup, "feature/cfg");
    setup.local_git(&["config", "mate.divergedStrategy", "skip"]);

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "feature/cfg", "skipped");
    assert_eq!(entry["reason"], "cannot fast-forward (diverged)");
    assert_eq!(setup.branch_tip("feature/cfg"), before);
}

#[test]
fn invalid_diverged_strategy_config_is_an_error() {
    let setup = common::RepoWithRemote::new();
    setup.local_git(&["config", "mate.divergedStrategy", "bogus"]);

    let output = common::git_mate()
        .args(["--json", "sync"])
        .current_dir(setup.local_path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["ok"], false);
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("invalid value for mate.divergedStrategy"),
        "unexpected JSON: {json}"
    );
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

// --- the current branch is synced like any other ---

/// Check out `branch`, which diverged from its upstream with the remote
/// writing `remote_content` to the same file the local commit wrote.
fn diverged_current_branch(setup: &common::RepoWithRemote, branch: &str, remote_content: &str) {
    setup.push_branch_to_remote(branch);
    setup.local_fetch();
    setup.create_local_tracking_branch(branch);
    setup.local_git(&["checkout", "-q", branch]);
    commit_file(setup.local_path(), "shared.txt", "local", "local change");
    let scratch = remote_scratch(setup, branch);
    let name = if remote_content == "local" {
        "remote.txt"
    } else {
        "shared.txt"
    };
    commit_file(scratch.path(), name, remote_content, "remote change");
    common::git(scratch.path(), &["push", "-q"]);
}

fn git_path_exists(setup: &common::RepoWithRemote, path: &str) -> bool {
    let out = Command::new("git")
        .args(["rev-parse", "--git-path", path])
        .current_dir(setup.local_path())
        .output()
        .unwrap();
    let path = String::from_utf8(out.stdout).unwrap();
    setup.local_path().join(path.trim()).exists()
}

#[test]
fn merges_upstream_into_diverged_current_branch() {
    let setup = common::RepoWithRemote::new();
    // "local" makes the remote touch a different file: no conflict.
    diverged_current_branch(&setup, "feature/cur", "local");
    let before = setup.local_head_commit();

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "feature/cur", "merged");

    let head = setup.local_head_commit();
    assert_eq!(parent_count(setup.local_path(), &head), 2);
    assert!(is_ancestor(setup.local_path(), &before, &head));
    assert!(is_ancestor(setup.local_path(), "origin/feature/cur", &head));
    assert!(setup.local_path().join("remote.txt").exists());
}

#[test]
fn rebase_flag_rebases_diverged_current_branch() {
    let setup = common::RepoWithRemote::new();
    diverged_current_branch(&setup, "feature/cur", "local");

    let data = sync_json(setup.local_path(), &["--rebase"]);
    assert_action(&data, "feature/cur", "rebased");

    let head = setup.local_head_commit();
    assert_eq!(parent_count(setup.local_path(), &head), 1);
    assert!(is_ancestor(setup.local_path(), "origin/feature/cur", &head));
}

#[test]
fn diverged_current_branch_with_conflicts_needs_attention() {
    let setup = common::RepoWithRemote::new();
    diverged_current_branch(&setup, "feature/cur", "remote");
    let before = setup.local_head_commit();

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "feature/cur", "skipped");
    assert_eq!(entry["reason"], "diverged with conflicts");

    assert_eq!(setup.local_head_commit(), before);
    assert!(!git_path_exists(&setup, "MERGE_HEAD"));
}

#[test]
fn rebase_with_conflicts_is_aborted() {
    let setup = common::RepoWithRemote::new();
    diverged_current_branch(&setup, "feature/cur", "remote");
    let before = setup.local_head_commit();

    let data = sync_json(setup.local_path(), &["--rebase"]);
    let entry = assert_action(&data, "feature/cur", "skipped");
    assert_eq!(entry["reason"], "diverged with conflicts; rebase aborted");

    assert_eq!(setup.local_head_commit(), before);
    assert_eq!(setup.local_current_branch(), "feature/cur");
    assert!(!git_path_exists(&setup, "rebase-merge"));
    assert!(!git_path_exists(&setup, "rebase-apply"));
}

#[test]
fn pull_rebase_config_rebases_diverged_current_branch() {
    let setup = common::RepoWithRemote::new();
    diverged_current_branch(&setup, "feature/cur", "local");
    setup.local_git(&["config", "pull.rebase", "true"]);

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "feature/cur", "rebased");
}

#[test]
fn diverged_current_branch_with_uncommitted_changes_needs_attention() {
    let setup = common::RepoWithRemote::new();
    diverged_current_branch(&setup, "feature/cur", "local");
    std::fs::write(setup.local_path().join("shared.txt"), "uncommitted").unwrap();
    let before = setup.local_head_commit();

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "feature/cur", "skipped");
    assert_eq!(entry["reason"], "diverged but working tree is dirty");

    assert_eq!(setup.local_head_commit(), before);
    assert_eq!(
        std::fs::read_to_string(setup.local_path().join("shared.txt")).unwrap(),
        "uncommitted"
    );
}

#[test]
fn current_branch_blocked_by_uncommitted_changes_needs_attention() {
    let setup = common::RepoWithRemote::new();
    setup.push_file_commit_to_remote("clash.txt", "remote", "add clash.txt");
    std::fs::write(setup.local_path().join("clash.txt"), "local").unwrap();
    let before = setup.local_head_commit();

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "main", "skipped");
    assert!(
        reason(entry).starts_with("could not fast-forward: "),
        "unexpected reason: {entry}"
    );

    assert_eq!(setup.local_head_commit(), before);
}

#[test]
fn json_reports_current_branch_needing_attention() {
    let setup = common::RepoWithRemote::new();
    diverged_current_branch(&setup, "feature/cur", "remote");

    let output = common::git_mate()
        .args(["--json", "sync"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["data"]["currentBranch"]["pulled"], false);
    assert_eq!(
        json["data"]["currentBranch"]["pullSkippedReason"],
        "diverged with conflicts"
    );
    let branches = json["data"]["branches"].as_array().unwrap();
    assert!(
        branches
            .iter()
            .any(|b| b["branch"] == "feature/cur" && b["action"] == "skipped"),
        "unexpected branches: {branches:?}"
    );
}

#[test]
fn branch_ahead_of_upstream_is_left_alone() {
    let setup = common::RepoWithRemote::new();
    prepare_feature_branch(&setup, "feature/ahead");
    setup.local_git(&["commit", "--allow-empty", "-m", "unpushed"]);
    let before = setup.local_head_commit();
    setup.local_git(&["checkout", "-q", "main"]);
    setup.local_git(&["checkout", "-q", "-b", "other"]);

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "feature/ahead", "up-to-date");

    assert_eq!(setup.branch_tip("feature/ahead"), before);
}

#[test]
fn current_branch_ahead_of_upstream_is_left_alone() {
    let setup = common::RepoWithRemote::new();
    setup.local_git(&["commit", "--allow-empty", "-m", "unpushed"]);
    let before = setup.local_head_commit();

    let data = sync_json(setup.local_path(), &[]);
    assert_action(&data, "main", "up-to-date");

    assert_eq!(setup.local_head_commit(), before);
}

#[test]
fn conflicting_auto_merge_needs_attention_and_leaves_branch_untouched() {
    let setup = common::RepoWithRemote::new();
    prepare_feature_branch(&setup, "feature/clash");
    commit_file(
        setup.local_path(),
        "shared.txt",
        "feature",
        "feature change",
    );
    setup.local_git(&["push", "-q"]);
    let before = setup.local_head_commit();
    setup.push_file_commit_to_remote("shared.txt", "main", "main change");

    let data = sync_json(setup.local_path(), &["--merge"]);
    let entry = assert_action(&data, "feature/clash", "skipped");
    assert_eq!(
        entry["reason"],
        "merging default branch 'main' would conflict"
    );
    assert_eq!(data["currentBranch"]["mergedDefault"], false);

    assert_eq!(setup.local_head_commit(), before);
    assert!(!git_path_exists(&setup, "MERGE_HEAD"));
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

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "feature/broken", "skipped");
    assert!(
        reason(entry).starts_with("could not fast-forward: "),
        "unexpected reason: {entry}"
    );
    assert_action(&data, "feature/ok", "fast-forwarded");

    assert_eq!(
        setup.branch_tip("feature/ok"),
        setup.branch_tip("origin/feature/ok"),
        "feature/ok should still be fast-forwarded despite feature/broken failing"
    );
}

// --- final status message ---

// --- Deletion of local branches whose remote was pruned ---

#[test]
fn leaves_pruned_branch_as_is_without_terminal() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    setup.delete_remote_branch("feature/gone");

    // Without a terminal nobody can answer, so nothing is asked or changed:
    // the branch survives and still tracks its (deleted) upstream, so the
    // next sync offers it again.
    common::git_mate()
        .arg("sync")
        .current_dir(setup.local_path())
        .assert()
        .success();

    let upstream = std::process::Command::new("git")
        .args(["config", "branch.feature/gone.merge"])
        .current_dir(setup.local_path())
        .output()
        .unwrap();
    assert!(
        upstream.status.success(),
        "upstream of feature/gone should still be configured"
    );
    assert!(setup.local_branch_exists("feature/gone"));
}

#[test]
fn keeps_local_branch_with_unpushed_commits_when_remote_pruned() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/keep");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/keep");
    setup.make_local_commit_on("feature/keep", "unpushed work");

    setup.delete_remote_branch("feature/keep");

    let data = sync_json(setup.local_path(), &[]);
    let entry = assert_action(&data, "feature/keep", "skipped");
    assert_eq!(
        entry["reason"],
        "remote deleted but has unpushed commits, skipping"
    );

    assert!(
        setup.local_branch_exists("feature/keep"),
        "local branch with unpushed commits should be kept"
    );
}

// --- --delete-pruned ---

#[test]
fn delete_pruned_deletes_pruned_branch() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    assert!(setup.local_branch_exists("feature/gone"));

    setup.delete_remote_branch("feature/gone");

    common::git_mate()
        .args(["sync", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .success();

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
        branches.iter().any(|b| b["branch"] == "feature/gone"
            && b["action"] == "deleted"
            && b["decidedBy"] == "flag"),
        "expected feature/gone reported as deleted by flag, got: {branches:?}"
    );

    assert!(
        !setup.local_branch_exists("feature/gone"),
        "local branch should have been deleted"
    );
}

#[test]
fn json_reports_deletion_candidates_with_recommended_action() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/merged");
    setup.push_branch_to_remote("feature/ff");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/merged");
    setup.create_local_tracking_branch("feature/ff");
    setup.push_commit_to_remote_branch("feature/ff", "advance feature");

    // Pushed work that never landed in main.
    setup.local_git(&["checkout", "-q", "-b", "feature/unmerged"]);
    commit_file(setup.local_path(), "u.txt", "u", "unmerged work");
    setup.local_git(&["push", "-q", "-u", "origin", "feature/unmerged"]);
    setup.local_git(&["checkout", "-q", "main"]);

    setup.delete_remote_branch("feature/merged");
    setup.delete_remote_branch("feature/unmerged");

    let output = common::git_mate()
        .args(["--json", "sync"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let branches = json["data"]["branches"].as_array().unwrap();
    let branch = |name: &str| {
        branches
            .iter()
            .find(|b| b["branch"] == name)
            .unwrap_or_else(|| panic!("{name} missing from {branches:?}"))
    };

    let merged = branch("feature/merged");
    assert_eq!(merged["action"], "deletion-candidate");
    assert_eq!(merged["detail"], "merged into origin/main");
    assert_eq!(merged["recommended"], "delete");
    assert_eq!(merged["choices"], serde_json::json!(["delete", "keep"]));
    assert!(merged.get("decidedBy").is_none(), "nothing was decided yet");

    let unmerged = branch("feature/unmerged");
    assert_eq!(unmerged["action"], "deletion-candidate");
    assert_eq!(unmerged["detail"], "not merged into origin/main");
    assert_eq!(unmerged["recommended"], "keep");

    let ff = branch("feature/ff");
    assert_eq!(ff["action"], "fast-forwarded");
    assert_eq!(ff["decidedBy"], "auto");
    assert!(ff.get("recommended").is_none());

    assert!(setup.local_branch_exists("feature/merged"));
    assert!(setup.local_branch_exists("feature/unmerged"));
}

#[test]
fn apply_recommended_deletes_merged_and_keeps_unmerged_branches() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/merged");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/merged");

    // Pushed work that never landed in main.
    setup.local_git(&["checkout", "-q", "-b", "feature/unmerged"]);
    commit_file(setup.local_path(), "u.txt", "u", "unmerged work");
    setup.local_git(&["push", "-q", "-u", "origin", "feature/unmerged"]);
    setup.local_git(&["checkout", "-q", "main"]);

    setup.delete_remote_branch("feature/merged");
    setup.delete_remote_branch("feature/unmerged");

    let data = sync_json(setup.local_path(), &["--apply-recommended"]);
    let merged = assert_action(&data, "feature/merged", "deleted");
    assert_eq!(merged["decidedBy"], "flag");
    let unmerged = assert_action(&data, "feature/unmerged", "kept");
    assert_eq!(unmerged["decidedBy"], "flag");
    assert_eq!(unmerged["detail"], "not merged into origin/main");

    assert!(!setup.local_branch_exists("feature/merged"));
    assert!(setup.local_branch_exists("feature/unmerged"));
    let upstream = std::process::Command::new("git")
        .args(["config", "branch.feature/unmerged.merge"])
        .current_dir(setup.local_path())
        .output()
        .unwrap();
    assert!(
        !upstream.status.success(),
        "kept branch should no longer track its deleted upstream"
    );
}

#[test]
fn apply_recommended_conflicts_with_delete_pruned() {
    let setup = common::RepoWithRemote::new();

    common::git_mate()
        .args(["sync", "--apply-recommended", "--delete-pruned"])
        .current_dir(setup.local_path())
        .assert()
        .failure();
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

    let data = sync_json(setup.local_path(), &["--delete-pruned"]);
    let entry = assert_action(&data, "feature/merged", "deleted");
    assert!(
        entry["detail"].as_str().unwrap().starts_with("merged in "),
        "unexpected detail: {entry}"
    );

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

    let data = sync_json(setup.local_path(), &["--delete-pruned"]);
    let entry = assert_action(&data, "feature/squashed", "deleted");
    assert_eq!(entry["detail"], "merged in #42");

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

    let data = sync_json(setup.local_path(), &["--delete-pruned"]);
    assert_action(&data, "feature/moved", "deleted");

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

    let data = sync_json(setup.local_path(), &["--delete-pruned"]);
    assert_action(&data, "feature/rebased", "deleted");

    assert!(!setup.local_branch_exists("feature/rebased"));
}

#[test]
fn skips_gone_branch_with_unmerged_changes_and_explains() {
    let setup = common::RepoWithRemote::new();
    gone_branch_with_changes(&setup, "feature/unmerged");

    let data = sync_json(setup.local_path(), &["--delete-pruned"]);
    let entry = assert_action(&data, "feature/unmerged", "skipped");
    assert!(
        reason(entry).contains("has changes not in origin/main"),
        "unexpected reason: {entry}"
    );
    assert!(
        !reason(entry).contains("could not resolve upstream"),
        "unexpected reason: {entry}"
    );

    assert!(setup.local_branch_exists("feature/unmerged"));
}

#[test]
fn skips_gone_branch_when_only_part_was_squash_merged() {
    let setup = common::RepoWithRemote::new();
    gone_branch_with_changes(&setup, "feature/partial");
    let scratch = remote_scratch(&setup, "main");
    commit_file(scratch.path(), "a.txt", "a", "only part of the feature");
    common::git(scratch.path(), &["push", "-q"]);

    let data = sync_json(setup.local_path(), &["--delete-pruned"]);
    let entry = assert_action(&data, "feature/partial", "skipped");
    assert!(
        reason(entry).contains("has changes not in origin/main"),
        "unexpected reason: {entry}"
    );

    assert!(setup.local_branch_exists("feature/partial"));
}

// --- end-of-run summary ---

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

    let data = sync_json(setup.local_path(), &["--dry-run"]);
    assert_eq!(data["dryRun"], true);
    assert_action(&data, "feature/ff", "fast-forwarded");
    assert_action(&data, "main", "fast-forwarded");

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
fn dry_run_does_not_delete_pruned_branch() {
    let setup = common::RepoWithRemote::new();

    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    assert!(setup.local_branch_exists("feature/gone"));

    setup.delete_remote_branch("feature/gone");

    common::git_mate()
        .args(["sync", "--dry-run"])
        .current_dir(setup.local_path())
        .assert()
        .success();

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
