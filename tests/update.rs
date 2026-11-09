mod common;

use std::path::Path;
use std::process::Command;

fn git_output(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn tip(dir: &Path, rev: &str) -> String {
    git_output(dir, &["rev-parse", rev])
}

fn is_ancestor(dir: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(dir)
        .status()
        .unwrap()
        .success()
}

fn parent_count(dir: &Path, rev: &str) -> usize {
    git_output(dir, &["rev-list", "--parents", "-n1", rev])
        .split_whitespace()
        .count()
        - 1
}

fn commit_file(dir: &Path, name: &str, content: &str) {
    std::fs::write(dir.join(name), content).unwrap();
    common::git(dir, &["add", name]);
    common::git(dir, &["commit", "-q", "-m", &format!("{name}: {content}")]);
}

/// Commit `name` on `branch`, coming back to the current branch afterwards.
fn commit_on(dir: &Path, branch: &str, name: &str, content: &str) {
    let current = git_output(dir, &["rev-parse", "--abbrev-ref", "HEAD"]);
    common::git(dir, &["checkout", "-q", branch]);
    commit_file(dir, name, content);
    common::git(dir, &["checkout", "-q", &current]);
}

/// Runs `git mate --json update` with `args` in `dir` and returns `data`.
fn update_json(dir: &Path, args: &[&str]) -> serde_json::Value {
    let output = common::git_mate()
        .args(["--json", "update"])
        .args(args)
        .current_dir(dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = common::parse_single_json(&output);
    assert_eq!(json["ok"], true, "unexpected JSON: {json}");
    json["data"].clone()
}

fn update_error(dir: &Path, args: &[&str], needles: &[&str]) {
    let output = common::git_mate()
        .args(["--json", "update"])
        .args(args)
        .current_dir(dir)
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, needles);
}

fn entry<'a>(data: &'a serde_json::Value, branch: &str) -> &'a serde_json::Value {
    data["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["branch"] == branch)
        .unwrap_or_else(|| panic!("no entry for {branch}: {}", data["branches"]))
}

/// `feature` branched off `main` with one commit; `main` moved on since.
fn behind_main() -> common::RepoWithoutRemote {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["branch", "feature", "main"]);
    commit_on(dir, "feature", "feature.txt", "feature");
    commit_file(dir, "main.txt", "main");
    common::git(dir, &["checkout", "-q", "feature"]);
    repo
}

// --- strategies ---

#[test]
fn rebases_current_branch_onto_default_branch() {
    let repo = behind_main();
    let dir = repo.path();

    let data = update_json(dir, &[]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "rebased");
    assert_eq!(feature["base"], "main");
    assert_eq!(feature["baseSource"], "default");
    assert_eq!(feature["behind"], 1);

    assert!(is_ancestor(dir, "main", "feature"));
    assert_eq!(parent_count(dir, "feature"), 1);
    assert!(dir.join("main.txt").exists(), "worktree follows the rebase");
}

#[test]
fn merge_flag_merges_base_in() {
    let repo = behind_main();
    let dir = repo.path();
    let before = tip(dir, "feature");

    let data = update_json(dir, &["--merge"]);
    assert_eq!(entry(&data, "feature")["action"], "merged");
    assert_eq!(parent_count(dir, "feature"), 2);
    assert!(is_ancestor(dir, &before, "feature"));
    assert!(is_ancestor(dir, "main", "feature"));
}

#[test]
fn strategy_config_selects_merge() {
    let repo = behind_main();
    repo.git(&["config", "mate.update.strategy", "merge"]);

    let data = update_json(repo.path(), &[]);
    assert_eq!(entry(&data, "feature")["action"], "merged");
}

#[test]
fn rebase_flag_overrides_strategy_config() {
    let repo = behind_main();
    repo.git(&["config", "mate.update.strategy", "merge"]);

    let data = update_json(repo.path(), &["--rebase"]);
    assert_eq!(entry(&data, "feature")["action"], "rebased");
}

#[test]
fn strategy_flag_selects_merge_and_overrides_config() {
    let repo = behind_main();
    repo.git(&["config", "mate.update.strategy", "rebase"]);

    let data = update_json(repo.path(), &["--strategy=merge"]);
    assert_eq!(entry(&data, "feature")["action"], "merged");
    assert_eq!(parent_count(repo.path(), "feature"), 2);
}

#[test]
fn strategy_flag_selects_rebase() {
    let repo = behind_main();
    repo.git(&["config", "mate.update.strategy", "merge"]);

    let data = update_json(repo.path(), &["--strategy", "rebase"]);
    assert_eq!(entry(&data, "feature")["action"], "rebased");
}

#[test]
fn strategy_flag_conflicts_with_its_shortcuts() {
    let repo = behind_main();
    let before = tip(repo.path(), "feature");
    common::git_mate()
        .args(["update", "--strategy=rebase", "--merge"])
        .current_dir(repo.path())
        .assert()
        .failure();
    assert_eq!(tip(repo.path(), "feature"), before);
}

#[test]
fn invalid_strategy_config_is_an_error() {
    let repo = behind_main();
    repo.git(&["config", "mate.update.strategy", "bogus"]);
    update_error(
        repo.path(),
        &[],
        &["invalid value for mate.update.strategy"],
    );
}

#[test]
fn up_to_date_branch_is_left_alone() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["checkout", "-q", "-b", "feature"]);
    commit_file(dir, "feature.txt", "feature");
    let before = tip(dir, "feature");

    let data = update_json(dir, &[]);
    assert_eq!(entry(&data, "feature")["action"], "up-to-date");
    assert_eq!(tip(dir, "feature"), before);
}

#[test]
fn conflicting_rebase_leaves_branch_untouched() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["branch", "feature", "main"]);
    commit_on(dir, "feature", "shared.txt", "feature");
    commit_file(dir, "shared.txt", "main");
    common::git(dir, &["checkout", "-q", "feature"]);
    let before = tip(dir, "feature");

    let data = update_json(dir, &[]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "skipped");
    assert_eq!(feature["reason"], "rebasing onto main would conflict");

    assert_eq!(tip(dir, "feature"), before);
    assert_eq!(repo.current_branch(), "feature");
    let rebase_dir = git_output(dir, &["rev-parse", "--git-path", "rebase-merge"]);
    assert!(!dir.join(rebase_dir).exists(), "no rebase left in progress");
}

#[test]
fn conflicting_merge_leaves_branch_untouched() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["branch", "feature", "main"]);
    commit_on(dir, "feature", "shared.txt", "feature");
    commit_file(dir, "shared.txt", "main");
    common::git(dir, &["checkout", "-q", "feature"]);
    let before = tip(dir, "feature");

    let data = update_json(dir, &["--merge"]);
    assert_eq!(
        entry(&data, "feature")["reason"],
        "merging main would conflict"
    );
    assert_eq!(tip(dir, "feature"), before);
}

#[test]
fn uncommitted_changes_are_carried_through_the_rebase() {
    let repo = behind_main();
    let dir = repo.path();
    std::fs::write(dir.join("feature.txt"), "uncommitted").unwrap();
    std::fs::write(dir.join("untracked.txt"), "untracked").unwrap();

    let data = update_json(dir, &[]);
    assert_eq!(entry(&data, "feature")["action"], "rebased");
    assert!(is_ancestor(dir, "main", "feature"));
    assert_eq!(
        std::fs::read_to_string(dir.join("feature.txt")).unwrap(),
        "uncommitted"
    );
    assert!(dir.join("untracked.txt").exists());
}

#[test]
fn updates_named_branch_that_is_not_checked_out() {
    for (strategy, action) in [("--rebase", "rebased"), ("--merge", "merged")] {
        let repo = behind_main();
        let dir = repo.path();
        common::git(dir, &["checkout", "-q", "main"]);
        let main_before = tip(dir, "main");

        let data = update_json(dir, &["feature", strategy]);
        assert_eq!(entry(&data, "feature")["action"], action);
        assert!(is_ancestor(dir, "main", "feature"));
        assert_eq!(repo.current_branch(), "main");
        assert_eq!(tip(dir, "HEAD"), main_before);
        assert_eq!(git_output(dir, &["status", "--porcelain"]), "");
    }
}

#[test]
fn branch_merged_with_a_merge_commit_is_fast_forwarded_when_not_checked_out() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["branch", "feature", "main"]);
    commit_on(dir, "feature", "feature.txt", "feature");
    common::git(
        dir,
        &["merge", "-q", "--no-ff", "-m", "merge feature", "feature"],
    );

    let data = update_json(dir, &["--all"]);
    assert_eq!(entry(&data, "feature")["action"], "rebased");
    assert_eq!(tip(dir, "feature"), tip(dir, "main"));
}

#[test]
fn updates_branch_checked_out_in_linked_worktree() {
    let repo = behind_main();
    let dir = repo.path();
    common::git(dir, &["checkout", "-q", "main"]);
    let wt = tempfile::TempDir::new().unwrap();
    let wt_path = wt.path().join("feature");
    common::git(
        dir,
        &[
            "worktree",
            "add",
            "-q",
            wt_path.to_str().unwrap(),
            "feature",
        ],
    );

    let data = update_json(dir, &["feature"]);
    assert_eq!(entry(&data, "feature")["action"], "rebased");
    assert!(is_ancestor(dir, "main", "feature"));
    assert!(wt_path.join("main.txt").exists(), "linked worktree follows");
    assert_eq!(git_output(&wt_path, &["status", "--porcelain"]), "");
}

#[test]
fn skips_branch_with_rebase_in_progress() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["branch", "feature", "main"]);
    commit_on(dir, "feature", "shared.txt", "feature");
    commit_file(dir, "shared.txt", "main");
    commit_file(dir, "other.txt", "main");
    common::git(dir, &["checkout", "-q", "feature"]);
    // Leave a conflicted rebase behind.
    let _ = Command::new("git")
        .args(["rebase", "main~1"])
        .current_dir(dir)
        .output()
        .unwrap();

    let data = update_json(dir, &["feature"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "skipped");
    assert_eq!(feature["reason"], "a rebase is in progress in its worktree");
}

#[test]
fn dry_run_changes_nothing() {
    let repo = behind_main();
    let dir = repo.path();
    let before = tip(dir, "feature");

    let data = update_json(dir, &["--dry-run"]);
    assert_eq!(data["dryRun"], true);
    assert_eq!(entry(&data, "feature")["action"], "rebased");
    assert_eq!(tip(dir, "feature"), before);
}

// --- choosing branches ---

#[test]
fn errors_for_default_branch() {
    let repo = behind_main();
    update_error(repo.path(), &["main"], &["default branch"]);
}

#[test]
fn errors_for_unknown_branch() {
    let repo = behind_main();
    update_error(repo.path(), &["nope"], &["no local branch named 'nope'"]);
}

#[test]
fn errors_on_detached_head() {
    let repo = behind_main();
    repo.git(&["checkout", "-q", "--detach"]);
    update_error(repo.path(), &[], &["detached HEAD"]);
}

#[test]
fn onto_needs_exactly_one_branch() {
    let repo = behind_main();
    repo.git(&["branch", "other", "main"]);
    update_error(
        repo.path(),
        &["feature", "other", "--onto", "main"],
        &["--onto needs exactly one branch"],
    );
}

#[test]
fn onto_rejects_non_branch() {
    let repo = behind_main();
    update_error(repo.path(), &["--onto", "HEAD~1"], &["no branch named"]);
}

// --- base detection ---

/// `main` ← `bar` ← `foo`, each with a commit of its own, created the way
/// `git branch <new> <start>` records it.
fn stack() -> common::RepoWithoutRemote {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["branch", "bar", "main"]);
    commit_on(dir, "bar", "bar.txt", "bar");
    common::git(dir, &["branch", "foo", "bar"]);
    commit_on(dir, "foo", "foo.txt", "foo");
    repo
}

#[test]
fn stacked_branch_is_based_on_its_parent() {
    let repo = stack();
    let dir = repo.path();
    commit_on(dir, "bar", "bar2.txt", "bar");

    let data = update_json(dir, &["foo"]);
    let foo = entry(&data, "foo");
    assert_eq!(foo["base"], "bar");
    assert_eq!(foo["baseSource"], "stacked");
    assert_eq!(foo["action"], "rebased");
    assert!(is_ancestor(dir, "bar", "foo"));
}

#[test]
fn branch_that_merged_another_branch_is_stacked_on_it() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["branch", "bar", "main"]);
    commit_on(dir, "bar", "bar.txt", "bar");
    common::git(dir, &["branch", "foo", "main"]);
    commit_on(dir, "foo", "foo.txt", "foo");
    common::git(dir, &["checkout", "-q", "foo"]);
    common::git(dir, &["merge", "-q", "--no-ff", "--no-edit", "bar"]);
    common::git(dir, &["checkout", "-q", "main"]);

    let data = update_json(dir, &["foo", "--dry-run"]);
    assert_eq!(entry(&data, "foo")["base"], "bar");
    assert_eq!(entry(&data, "foo")["baseSource"], "stacked");
}

#[test]
fn parent_is_not_based_on_its_child() {
    let repo = stack();
    let dir = repo.path();
    commit_file(dir, "main.txt", "main");

    let data = update_json(dir, &["bar", "--dry-run"]);
    assert_eq!(entry(&data, "bar")["base"], "main");
    assert_eq!(entry(&data, "bar")["baseSource"], "default");
}

#[test]
fn sibling_branches_are_based_on_the_default_branch() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["branch", "one", "main"]);
    common::git(dir, &["branch", "two", "main"]);
    commit_on(dir, "one", "one.txt", "one");
    commit_on(dir, "two", "two.txt", "two");

    let data = update_json(dir, &["one", "two", "--dry-run"]);
    assert_eq!(entry(&data, "one")["base"], "main");
    assert_eq!(entry(&data, "two")["base"], "main");
}

#[test]
fn both_advanced_stack_uses_reflog_to_tell_parent_from_child() {
    let repo = stack();
    let dir = repo.path();
    commit_on(dir, "bar", "bar2.txt", "bar");

    let data = update_json(dir, &["foo", "bar", "--dry-run"]);
    assert_eq!(entry(&data, "foo")["base"], "bar");
    assert_eq!(entry(&data, "bar")["base"], "main");
}

#[test]
fn checkout_b_from_parent_is_recognized() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["checkout", "-q", "-b", "bar"]);
    commit_file(dir, "bar.txt", "bar");
    common::git(dir, &["checkout", "-q", "-b", "foo"]);
    commit_file(dir, "foo.txt", "foo");
    commit_on(dir, "bar", "bar2.txt", "bar");

    let data = update_json(dir, &["foo", "bar", "--dry-run"]);
    assert_eq!(entry(&data, "foo")["base"], "bar");
    assert_eq!(entry(&data, "bar")["base"], "main");
}

#[test]
fn reflog_of_an_earlier_branch_with_the_same_name_is_ignored() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    // Long ago, a `foo` was created from `zed`, and later deleted.
    common::git(dir, &["branch", "zed", "main"]);
    commit_on(dir, "zed", "zed.txt", "zed");
    common::git_at(dir, 1_600_000_000, &["checkout", "-q", "zed"]);
    common::git_at(dir, 1_600_000_000, &["checkout", "-q", "-b", "foo"]);
    common::git_at(dir, 1_600_000_000, &["checkout", "-q", "main"]);
    common::git(dir, &["branch", "-q", "-D", "foo"]);
    // Now `foo` is created from `bar`, and both advance.
    common::git_at(dir, 1_700_000_000, &["checkout", "-q", "-b", "bar"]);
    commit_file(dir, "bar.txt", "bar");
    common::git_at(dir, 1_700_000_100, &["checkout", "-q", "-b", "foo"]);
    commit_file(dir, "foo.txt", "foo");
    commit_on(dir, "bar", "bar2.txt", "bar");

    let data = update_json(dir, &["foo", "--dry-run"]);
    assert_eq!(entry(&data, "foo")["base"], "bar");
}

#[test]
fn ambiguous_base_is_skipped_and_onto_resolves_it() {
    let repo = stack();
    let dir = repo.path();
    commit_on(dir, "bar", "bar2.txt", "bar");
    common::git(dir, &["reflog", "expire", "--expire=now", "--all"]);
    let before = tip(dir, "foo");

    let data = update_json(dir, &["foo"]);
    let foo = entry(&data, "foo");
    assert_eq!(foo["action"], "skipped");
    assert!(
        foo["reason"]
            .as_str()
            .unwrap()
            .starts_with("could not tell which branch it is based on"),
        "{foo}"
    );
    assert_eq!(tip(dir, "foo"), before);

    let data = update_json(dir, &["foo", "--onto", "bar"]);
    let foo = entry(&data, "foo");
    assert_eq!(foo["baseSource"], "onto");
    assert_eq!(foo["action"], "rebased");
    assert!(is_ancestor(dir, "bar", "foo"));
}

#[test]
fn squash_merged_parent_is_dropped() {
    let repo = stack();
    let dir = repo.path();
    let old_bar = tip(dir, "bar");
    common::git(dir, &["merge", "-q", "--squash", "bar"]);
    common::git(dir, &["commit", "-q", "-m", "squashed bar"]);

    let data = update_json(dir, &["foo"]);
    let foo = entry(&data, "foo");
    assert_eq!(foo["base"], "main");
    assert_eq!(foo["baseSource"], "mergedParent");
    assert_eq!(foo["mergedParent"], "bar");
    assert_eq!(foo["action"], "rebased");

    assert!(is_ancestor(dir, "main", "foo"));
    assert!(!is_ancestor(dir, &old_bar, "foo"));
    assert_eq!(git_output(dir, &["rev-list", "--count", "main..foo"]), "1");
}

#[test]
fn merging_in_a_squash_merged_parent_happens_only_once() {
    let repo = stack();
    let dir = repo.path();
    common::git(dir, &["merge", "-q", "--squash", "bar"]);
    common::git(dir, &["commit", "-q", "-m", "squashed bar"]);

    let data = update_json(dir, &["--merge", "foo"]);
    assert_eq!(entry(&data, "foo")["action"], "merged");
    let merged = tip(dir, "foo");

    let data = update_json(dir, &["--merge", "foo"]);
    assert_eq!(entry(&data, "foo")["action"], "up-to-date");
    assert_eq!(tip(dir, "foo"), merged);
}

#[test]
fn tag_named_like_the_base_branch_is_ignored() {
    let repo = behind_main();
    let dir = repo.path();
    let old_main = git_output(dir, &["merge-base", "main", "feature"]);
    common::git(dir, &["tag", "main", &old_main]);

    let data = update_json(dir, &[]);
    assert_eq!(entry(&data, "feature")["action"], "rebased");
    assert!(is_ancestor(dir, "refs/heads/main", "feature"));
}

#[test]
fn rebase_that_cannot_start_is_a_failure_not_a_conflict() {
    let repo = behind_main();
    let dir = repo.path();
    // main added main.txt; an untracked copy in the worktree blocks the rebase.
    std::fs::write(dir.join("main.txt"), "untracked").unwrap();
    let before = tip(dir, "feature");

    let data = update_json(dir, &[]);
    assert_eq!(entry(&data, "feature")["action"], "failed");
    assert_eq!(tip(dir, "feature"), before);
    assert_eq!(
        std::fs::read_to_string(dir.join("main.txt")).unwrap(),
        "untracked"
    );
}

#[test]
fn backup_of_a_branch_is_not_taken_for_its_base() {
    let repo = common::RepoWithoutRemote::new();
    let dir = repo.path();
    common::git(dir, &["checkout", "-q", "-b", "feat"]);
    commit_file(dir, "feat.txt", "feat");
    common::git(dir, &["branch", "feat-backup"]);
    commit_file(dir, "feat2.txt", "feat");
    commit_on(dir, "main", "main.txt", "main");
    let backup = tip(dir, "feat-backup");

    let data = update_json(dir, &["feat", "--stack"]);
    let feat = entry(&data, "feat");
    assert_eq!(feat["base"], "main");
    assert_eq!(feat["action"], "rebased");
    assert!(is_ancestor(dir, "main", "feat"));
    assert_eq!(tip(dir, "feat-backup"), backup);
}

// --- --all and stacks ---

#[test]
fn all_updates_a_whole_stack_parents_first() {
    let repo = stack();
    let dir = repo.path();
    commit_file(dir, "main.txt", "main");
    // Listed child first: the order must not depend on it.
    let data = update_json(dir, &["foo", "bar"]);
    assert_eq!(entry(&data, "bar")["action"], "rebased");
    assert_eq!(entry(&data, "foo")["action"], "rebased");
    assert!(is_ancestor(dir, "main", "bar"));
    assert!(is_ancestor(dir, "bar", "foo"));
    assert_eq!(git_output(dir, &["rev-list", "--count", "bar..foo"]), "1");
}

#[test]
fn all_flag_covers_every_branch_but_the_default() {
    let repo = stack();
    let dir = repo.path();
    common::git(dir, &["branch", "side", "main"]);
    commit_on(dir, "side", "side.txt", "side");
    commit_file(dir, "main.txt", "main");

    let data = update_json(dir, &["--all"]);
    let names: Vec<&str> = data["branches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["branch"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["bar", "foo", "side"]);
    for branch in ["bar", "side"] {
        assert!(is_ancestor(dir, "main", branch));
    }
    assert!(is_ancestor(dir, "bar", "foo"));
}

#[test]
fn dry_run_reports_children_of_branches_that_would_move() {
    let repo = stack();
    let dir = repo.path();
    commit_file(dir, "main.txt", "main");
    let before = tip(dir, "foo");

    let data = update_json(dir, &["--all", "--dry-run"]);
    assert_eq!(entry(&data, "bar")["action"], "rebased");
    assert_eq!(entry(&data, "foo")["action"], "rebased");
    assert_eq!(tip(dir, "foo"), before);
}

// --- --stack ---

/// `stack()` with `main` moved on, so `bar` is behind its base.
fn stack_behind_main() -> common::RepoWithoutRemote {
    let repo = stack();
    commit_file(repo.path(), "main.txt", "main");
    repo
}

#[test]
fn without_terminal_parent_behind_is_reported_not_updated() {
    let repo = stack_behind_main();
    let dir = repo.path();
    let bar_before = tip(dir, "bar");

    let data = update_json(dir, &["foo"]);
    assert_eq!(data["branches"].as_array().unwrap().len(), 1);
    let foo = entry(&data, "foo");
    assert_eq!(foo["action"], "up-to-date");
    assert_eq!(foo["parentBehind"]["branch"], "bar");
    assert_eq!(foo["parentBehind"]["base"], "main");
    assert_eq!(foo["parentBehind"]["behind"], 1);
    assert_eq!(tip(dir, "bar"), bar_before);
}

#[test]
fn stack_flag_updates_parent_first() {
    let repo = stack_behind_main();
    let dir = repo.path();

    let data = update_json(dir, &["foo", "--stack"]);
    assert_eq!(entry(&data, "bar")["action"], "rebased");
    assert_eq!(entry(&data, "foo")["action"], "rebased");
    assert!(entry(&data, "foo").get("parentBehind").is_none());
    assert!(is_ancestor(dir, "main", "bar"));
    assert!(is_ancestor(dir, "bar", "foo"));
}

#[test]
fn stack_reaches_down_a_deeper_stack() {
    let repo = stack_behind_main();
    let dir = repo.path();
    common::git(dir, &["branch", "baz", "foo"]);
    commit_on(dir, "baz", "baz.txt", "baz");

    update_json(dir, &["baz", "--stack"]);
    assert!(is_ancestor(dir, "main", "bar"));
    assert!(is_ancestor(dir, "bar", "foo"));
    assert!(is_ancestor(dir, "foo", "baz"));
}

#[test]
fn stack_config_true_updates_parent() {
    let repo = stack_behind_main();
    repo.git(&["config", "mate.update.stack", "true"]);

    let data = update_json(repo.path(), &["foo"]);
    assert_eq!(entry(&data, "bar")["action"], "rebased");
}

#[test]
fn no_stack_overrides_config() {
    let repo = stack_behind_main();
    repo.git(&["config", "mate.update.stack", "true"]);
    let bar_before = tip(repo.path(), "bar");

    let data = update_json(repo.path(), &["foo", "--no-stack"]);
    assert_eq!(entry(&data, "foo")["parentBehind"]["branch"], "bar");
    assert_eq!(tip(repo.path(), "bar"), bar_before);
}

#[test]
fn invalid_stack_config_is_an_error() {
    let repo = stack_behind_main();
    repo.git(&["config", "mate.update.stack", "bogus"]);
    update_error(
        repo.path(),
        &["foo"],
        &["invalid value for mate.update.stack"],
    );
}

// --- --push ---

fn remote_tip(setup: &common::RepoWithRemote, branch: &str) -> Option<String> {
    bare_tip(setup.bare_path(), branch)
}

fn bare_tip(bare: &Path, branch: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .current_dir(bare)
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).unwrap().trim().to_string())
}

/// `feature` branched off `main` with one commit and checked out; `main`
/// moved on, locally and on the remote. Pushed first if `pushed`.
fn remote_feature_behind_main(pushed: bool) -> common::RepoWithRemote {
    let setup = common::RepoWithRemote::new();
    let dir = setup.local_path();
    common::git(dir, &["checkout", "-q", "-b", "feature"]);
    commit_file(dir, "feature.txt", "feature");
    if pushed {
        common::git(dir, &["push", "-q", "-u", "origin", "feature"]);
    }
    common::git(dir, &["checkout", "-q", "main"]);
    commit_file(dir, "main.txt", "main");
    common::git(dir, &["push", "-q", "origin", "main"]);
    common::git(dir, &["checkout", "-q", "feature"]);
    setup
}

#[test]
fn push_creates_remote_branch_and_sets_upstream() {
    let setup = remote_feature_behind_main(false);
    let dir = setup.local_path();

    let data = update_json(dir, &["--push"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "rebased");
    assert_eq!(feature["push"], "pushed");
    assert_eq!(remote_tip(&setup, "feature"), Some(tip(dir, "feature")));
    assert_eq!(
        git_output(dir, &["rev-parse", "--abbrev-ref", "feature@{upstream}"]),
        "origin/feature"
    );
}

#[test]
fn push_does_not_force_by_default() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    let remote_before = remote_tip(&setup, "feature");

    let data = update_json(dir, &["--push"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "rebased");
    assert_eq!(feature["push"], "needs-force-push");
    assert_eq!(remote_tip(&setup, "feature"), remote_before);
}

#[test]
fn push_ask_without_terminal_does_not_force() {
    let setup = remote_feature_behind_main(true);
    let remote_before = remote_tip(&setup, "feature");

    let data = update_json(setup.local_path(), &["--push=ask"]);
    assert_eq!(entry(&data, "feature")["push"], "needs-force-push");
    assert_eq!(remote_tip(&setup, "feature"), remote_before);
}

#[test]
fn push_force_rewrites_remote_branch() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();

    let data = update_json(dir, &["--push=force"]);
    assert_eq!(entry(&data, "feature")["push"], "forced");
    assert_eq!(remote_tip(&setup, "feature"), Some(tip(dir, "feature")));
}

#[test]
fn push_config_force_is_respected_and_no_push_overrides_it() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    setup.local_git(&["config", "mate.update.push", "force"]);
    let remote_before = remote_tip(&setup, "feature");

    let data = update_json(dir, &["--no-push"]);
    assert!(entry(&data, "feature").get("push").is_none());
    assert_eq!(remote_tip(&setup, "feature"), remote_before);

    let data = update_json(dir, &[]);
    assert_eq!(entry(&data, "feature")["push"], "forced");
    assert_eq!(remote_tip(&setup, "feature"), Some(tip(dir, "feature")));
}

#[test]
fn force_push_fails_if_remote_moved_since_last_fetch() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    setup.push_commit_to_remote_branch("feature", "someone else's work");
    let remote_before = remote_tip(&setup, "feature");

    let data = update_json(dir, &["--push=force"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "rebased");
    assert_eq!(feature["push"], "failed");
    assert_eq!(remote_tip(&setup, "feature"), remote_before);
}

#[test]
fn force_push_never_overwrites_commits_the_branch_never_had() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    setup.push_commit_to_remote_branch("feature", "someone else's work");
    common::git(dir, &["fetch", "-q"]);
    let remote_before = remote_tip(&setup, "feature");

    let data = update_json(dir, &["--push=force"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "rebased");
    assert_eq!(feature["push"], "failed");
    assert_eq!(remote_tip(&setup, "feature"), remote_before);
}

#[test]
fn push_refuses_upstream_with_a_different_name() {
    let setup = common::RepoWithRemote::new();
    let dir = setup.local_path();
    common::git(
        dir,
        &["checkout", "-q", "-b", "topic", "--track", "origin/main"],
    );
    commit_file(dir, "topic.txt", "topic");
    let main_before = remote_tip(&setup, "main");

    let data = update_json(dir, &["--push", "topic"]);
    assert_eq!(entry(&data, "topic")["push"], "failed");
    assert_eq!(remote_tip(&setup, "main"), main_before);
    assert_eq!(remote_tip(&setup, "topic"), None);
}

#[test]
fn merged_branch_is_pushed_without_force() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();

    let data = update_json(dir, &["--merge", "--push"]);
    assert_eq!(entry(&data, "feature")["push"], "pushed");
    assert_eq!(remote_tip(&setup, "feature"), Some(tip(dir, "feature")));
}

#[test]
fn up_to_date_branch_rebased_earlier_is_force_pushed() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    common::git(dir, &["rebase", "-q", "main"]);

    let data = update_json(dir, &["--push=force"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "up-to-date");
    assert_eq!(feature["push"], "forced");
    assert_eq!(remote_tip(&setup, "feature"), Some(tip(dir, "feature")));
}

#[test]
fn push_dry_run_reports_without_pushing() {
    let setup = remote_feature_behind_main(true);
    let remote_before = remote_tip(&setup, "feature");

    let data = update_json(setup.local_path(), &["--push", "--dry-run"]);
    assert_eq!(entry(&data, "feature")["push"], "needs-force-push");
    let data = update_json(setup.local_path(), &["--push=force", "--dry-run"]);
    assert_eq!(entry(&data, "feature")["push"], "forced");
    assert_eq!(remote_tip(&setup, "feature"), remote_before);
}

#[test]
fn push_without_remote_is_reported() {
    let repo = behind_main();
    let data = update_json(repo.path(), &["--push"]);
    assert_eq!(entry(&data, "feature")["push"], "no-remote");
}

#[test]
fn push_flag_does_not_swallow_branch_names() {
    let setup = remote_feature_behind_main(false);
    let dir = setup.local_path();
    common::git(dir, &["checkout", "-q", "main"]);

    let data = update_json(dir, &["--push", "feature"]);
    assert_eq!(entry(&data, "feature")["push"], "pushed");
}

#[test]
fn push_does_not_recreate_a_remote_branch_deleted_after_merge() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    common::git(dir, &["checkout", "-q", "main"]);
    common::git(dir, &["merge", "-q", "--no-ff", "--no-edit", "feature"]);
    common::git(dir, &["push", "-q", "origin", "main"]);
    commit_file(dir, "later.txt", "later");
    common::git(dir, &["push", "-q", "origin", "main"]);
    setup.delete_remote_branch("feature");
    setup.local_fetch();

    let data = update_json(dir, &["--all", "--push"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["push"], "skipped");
    assert_eq!(remote_tip(&setup, "feature"), None);
}

#[test]
fn push_does_not_recreate_a_merged_branch_when_local_main_lags_and_origin_head_is_unset() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    // Merge on the server only: local main never sees it.
    let scratch = tempfile::TempDir::new().unwrap();
    let bare = setup.bare_path().to_str().unwrap().to_string();
    common::git(scratch.path(), &["clone", "-q", &bare, "."]);
    common::git(scratch.path(), &["config", "user.email", "test@test.com"]);
    common::git(scratch.path(), &["config", "user.name", "Test"]);
    common::git(
        scratch.path(),
        &["merge", "-q", "--no-ff", "--no-edit", "origin/feature"],
    );
    common::git(scratch.path(), &["push", "-q", "origin", "main"]);
    setup.delete_remote_branch("feature");
    setup.local_fetch();
    common::git(dir, &["remote", "set-head", "origin", "-d"]);

    let data = update_json(dir, &["feature", "--push"]);
    assert_eq!(entry(&data, "feature")["push"], "skipped");
    assert_eq!(remote_tip(&setup, "feature"), None);
}

#[test]
fn push_creates_remote_branch_for_branch_made_with_new() {
    let setup = common::RepoWithRemote::new();
    let dir = setup.local_path();
    common::git_mate()
        .args(["new", "feature", "--no-fetch"])
        .current_dir(dir)
        .assert()
        .success();
    common::git(dir, &["checkout", "-q", "feature"]);
    commit_file(dir, "feature.txt", "feature");
    common::git(dir, &["checkout", "-q", "main"]);
    commit_file(dir, "main.txt", "main");
    common::git(dir, &["push", "-q", "origin", "main"]);

    let data = update_json(dir, &["feature", "--push"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "rebased");
    assert_eq!(feature["push"], "pushed");
    assert_eq!(remote_tip(&setup, "feature"), Some(tip(dir, "feature")));
}

/// A second bare repository, added to the clone at `dir` as remote `fork`.
fn add_fork_remote(dir: &Path) -> tempfile::TempDir {
    let fork = tempfile::TempDir::new().unwrap();
    common::git(fork.path(), &["init", "-q", "--bare"]);
    common::git(
        dir,
        &["remote", "add", "fork", fork.path().to_str().unwrap()],
    );
    fork
}

#[test]
fn push_goes_to_the_branch_push_remote() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    let fork = add_fork_remote(dir);
    common::git(dir, &["config", "branch.feature.pushRemote", "fork"]);
    let origin_before = remote_tip(&setup, "feature");

    let data = update_json(dir, &["--push"]);
    let feature = entry(&data, "feature");
    assert_eq!(feature["action"], "rebased");
    assert_eq!(feature["push"], "pushed");
    assert_eq!(bare_tip(fork.path(), "feature"), Some(tip(dir, "feature")));
    assert_eq!(remote_tip(&setup, "feature"), origin_before);
    assert_eq!(
        git_output(dir, &["rev-parse", "--abbrev-ref", "feature@{upstream}"]),
        "origin/feature"
    );
}

#[test]
fn force_push_goes_to_the_default_push_remote() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    let fork = add_fork_remote(dir);
    common::git(dir, &["push", "-q", "fork", "feature"]);
    common::git(dir, &["config", "remote.pushDefault", "fork"]);
    let origin_before = remote_tip(&setup, "feature");

    let data = update_json(dir, &["--push=force"]);
    assert_eq!(entry(&data, "feature")["push"], "forced");
    assert_eq!(bare_tip(fork.path(), "feature"), Some(tip(dir, "feature")));
    assert_eq!(remote_tip(&setup, "feature"), origin_before);
}

#[test]
fn force_push_to_push_remote_leases_its_tracking_branch() {
    let setup = remote_feature_behind_main(true);
    let dir = setup.local_path();
    let fork = add_fork_remote(dir);
    common::git(dir, &["push", "-q", "fork", "feature"]);
    common::git(dir, &["config", "remote.pushDefault", "fork"]);
    // Someone else pushes to the fork; this clone hasn't fetched it.
    let other = tempfile::TempDir::new().unwrap();
    common::git(
        other.path(),
        &[
            "clone",
            "-q",
            "-b",
            "feature",
            fork.path().to_str().unwrap(),
            ".",
        ],
    );
    common::git(
        other.path(),
        &[
            "-c",
            "user.name=Other",
            "-c",
            "user.email=other@test.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "someone else's work",
        ],
    );
    common::git(other.path(), &["push", "-q", "origin", "feature"]);
    let fork_before = bare_tip(fork.path(), "feature");

    let data = update_json(dir, &["--push=force"]);
    assert_eq!(entry(&data, "feature")["push"], "failed");
    assert_eq!(bare_tip(fork.path(), "feature"), fork_before);
}

// --- --sync ---

/// `feature` checked out; `main` moved on only on the remote.
fn main_moved_on_remote() -> common::RepoWithRemote {
    let setup = common::RepoWithRemote::new();
    let dir = setup.local_path();
    common::git(dir, &["checkout", "-q", "-b", "feature"]);
    commit_file(dir, "feature.txt", "feature");
    setup.push_file_commit_to_remote("main.txt", "main", "remote main");
    setup
}

#[test]
fn without_sync_the_local_base_is_used() {
    let setup = main_moved_on_remote();
    let data = update_json(setup.local_path(), &[]);
    assert_eq!(entry(&data, "feature")["action"], "up-to-date");
    assert!(data.get("sync").is_none());
}

#[test]
fn sync_flag_syncs_first() {
    let setup = main_moved_on_remote();
    let dir = setup.local_path();

    let data = update_json(dir, &["--sync"]);
    assert_eq!(data["sync"]["dryRun"], false);
    let main = data["sync"]["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["branch"] == "main")
        .unwrap();
    assert_eq!(main["action"], "fast-forwarded");
    assert_eq!(entry(&data, "feature")["action"], "rebased");
    assert!(is_ancestor(dir, "origin/main", "feature"));
}

#[test]
fn sync_config_syncs_first_and_no_sync_overrides_it() {
    let setup = main_moved_on_remote();
    let dir = setup.local_path();
    setup.local_git(&["config", "mate.update.sync", "true"]);

    let data = update_json(dir, &["--no-sync"]);
    assert!(data.get("sync").is_none());
    assert_eq!(entry(&data, "feature")["action"], "up-to-date");

    let data = update_json(dir, &[]);
    assert!(data.get("sync").is_some());
    assert_eq!(entry(&data, "feature")["action"], "rebased");
}

/// `main` ← `bar` ← `foo`, `bar` pushed and checked out is `foo`; on the
/// remote, `bar` was then squash-merged into `main` and deleted.
#[cfg(unix)]
fn stack_with_parent_squash_merged_on_remote() -> common::RepoWithRemote {
    let setup = common::RepoWithRemote::new();
    let dir = setup.local_path();
    common::git(dir, &["checkout", "-q", "-b", "bar"]);
    commit_file(dir, "bar.txt", "1");
    commit_file(dir, "bar.txt", "2");
    common::git(dir, &["push", "-q", "-u", "origin", "bar"]);
    common::git(dir, &["checkout", "-q", "-b", "foo"]);
    commit_file(dir, "foo.txt", "foo");

    let other = tempfile::TempDir::new().unwrap();
    let o = other.path();
    common::git(
        o,
        &["clone", "-q", setup.bare_path().to_str().unwrap(), "."],
    );
    common::git(o, &["config", "user.email", "test@test.com"]);
    common::git(o, &["config", "user.name", "Test"]);
    common::git(o, &["merge", "-q", "--squash", "origin/bar"]);
    common::git(o, &["commit", "-q", "-m", "squashed bar"]);
    common::git(o, &["push", "-q", "origin", "main", ":bar"]);
    setup
}

/// Make the first ref update in `dir` (from now on) delete local branch
/// `branch`, as `sync` does when told to delete a branch whose remote is
/// gone.
#[cfg(unix)]
fn delete_branch_on_next_ref_update(dir: &Path, branch: &str) {
    use std::os::unix::fs::PermissionsExt;
    let marker = dir.join(".git").join("deleted-by-hook");
    let hook = dir.join(".git").join("hooks").join("reference-transaction");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\ntest \"$1\" = committed || exit 0\ntest -e '{marker}' && exit 0\n\
             touch '{marker}'\ngit branch -q -D {branch}\n",
            marker = marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn merged_parent_deleted_by_sync_is_still_dropped() {
    let setup = stack_with_parent_squash_merged_on_remote();
    let dir = setup.local_path();
    delete_branch_on_next_ref_update(dir, "bar");

    let data = update_json(dir, &["--sync", "foo"]);
    assert!(!setup.local_branch_exists("bar"), "the hook deleted bar");
    let foo = entry(&data, "foo");
    assert_eq!(foo["base"], "main");
    assert_eq!(foo["baseSource"], "mergedParent");
    assert_eq!(foo["mergedParent"], "bar");
    assert_eq!(foo["action"], "rebased");
    assert!(is_ancestor(dir, "origin/main", "foo"));
    assert_eq!(git_output(dir, &["rev-list", "--count", "main..foo"]), "1");
}
