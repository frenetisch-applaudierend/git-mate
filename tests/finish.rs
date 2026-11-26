mod common;

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
    assert!(
        !repo.branch_exists("feature/x"),
        "feature/x should have been deleted"
    );
}

#[test]
fn finish_on_default_branch_fails() {
    let repo = common::RepoWithoutRemote::new();
    let output = common::git_mate()
        .args(["--json", "finish"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["default branch"]);
    common::assert_json_error_code(&output, "default_branch");
}

#[test]
fn finish_deletes_branch_after_switching() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/x"]);
    // feature/x is at the same commit as main, so safe delete succeeds
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "main");
    assert!(
        !repo.branch_exists("feature/x"),
        "feature/x should have been deleted"
    );
}

#[test]
fn finish_unmerged_branch_without_remote_succeeds() {
    // No remote → no "unpushed" concept; finish always succeeds
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/x"]);
    repo.git(&["commit", "--allow-empty", "-m", "unmerged work"]);
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(
        !repo.branch_exists("feature/x"),
        "feature/x should have been deleted"
    );
}

#[test]
fn finish_unpushed_commits_requires_force() {
    let repo = common::RepoWithRemote::new();
    repo.local_git(&["checkout", "-b", "feature/x"]);
    repo.local_git(&["commit", "--allow-empty", "-m", "local only"]);

    // Without --force: should fail with a clear message
    let output = common::git_mate()
        .args(["--json", "finish", "feature/x"])
        .current_dir(repo.local_path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["unpushed commits"]);
    let data = common::assert_json_error_code(&output, "unpushed_commits");
    assert_eq!(data["retryWith"], serde_json::json!(["--force"]));

    // With --force: should succeed
    common::git_mate()
        .args(["finish", "--force", "feature/x"])
        .current_dir(repo.local_path())
        .assert()
        .success();
    assert!(
        !repo.local_branch_exists("feature/x"),
        "feature/x should have been deleted"
    );
}

#[test]
fn finish_pushed_branch_succeeds_without_force() {
    // Branch created from origin/main (same commit) — no unpushed commits
    let repo = common::RepoWithRemote::new();
    repo.local_git(&["checkout", "-b", "feature/x", "origin/main"]);

    common::git_mate()
        .args(["finish", "feature/x"])
        .current_dir(repo.local_path())
        .assert()
        .success();
    assert!(
        !repo.local_branch_exists("feature/x"),
        "feature/x should have been deleted"
    );
}

#[test]
fn finish_force_removes_dirty_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-dirty-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&[
        "worktree",
        "add",
        "-b",
        "feature/dirty",
        wt_path_str,
        "main",
    ]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();
    // Create an untracked file in the worktree to make it dirty
    std::fs::write(wt_canonical.join("untracked.txt"), "dirty").unwrap();

    // Without --force it should fail
    let output = common::git_mate()
        .args(["--json", "finish", "feature/dirty"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let data = common::assert_json_error_code(&output, "uncommitted_changes");
    assert_eq!(data["retryWith"], serde_json::json!(["--force"]));
    assert!(wt_canonical.exists(), "worktree must be kept");

    // With --force it should succeed
    common::git_mate()
        .args(["finish", "--force", "feature/dirty"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(
        !wt_canonical.exists(),
        "worktree directory should be removed"
    );
    assert!(
        !repo.branch_exists("feature/dirty"),
        "branch should have been deleted"
    );
}

#[test]
fn finish_force_discards_changes_in_main_worktree() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("work.txt"), "committed").unwrap();
    repo.git(&["add", "work.txt"]);
    repo.git(&["commit", "-m", "add work.txt"]);
    // Modified, so switching to main (where it doesn't exist) would lose it
    std::fs::write(repo.path().join("work.txt"), "modified").unwrap();

    // Without --force the checkout refuses to overwrite the change
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .failure();
    assert_eq!(repo.current_branch(), "feature/dirty");

    // With --force the change is discarded
    common::git_mate()
        .args(["finish", "--force"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "main");
    assert!(
        !repo.path().join("work.txt").exists(),
        "modified file should have been discarded"
    );
    assert!(
        !repo.branch_exists("feature/dirty"),
        "branch should have been deleted"
    );
}

#[test]
fn finish_linked_worktree_removes_it_and_prints_mate_cd() {
    let repo = common::RepoWithoutRemote::new();
    // Add a linked worktree for feature/x
    let wt_path = repo.path().join("feature-x-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/x", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["finish"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&wt_canonical)
        .output()
        .unwrap();
    assert!(output.status.success(), "finish should succeed");
    let proto = std::fs::read_to_string(&proto_path).unwrap();
    let main_path = std::fs::canonicalize(repo.path()).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", main_path.display())),
        "protocol file should contain CD:<main_path>, got: {proto:?}"
    );
    assert!(
        !wt_canonical.exists(),
        "worktree directory should be removed"
    );
    assert!(
        !repo.branch_exists("feature/x"),
        "branch should have been deleted"
    );
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
    assert!(
        !wt_canonical.exists(),
        "worktree directory should be removed"
    );
    assert!(
        !repo.branch_exists("feature/y"),
        "branch should have been deleted"
    );
}

#[test]
fn finish_linked_worktree_removes_wt_and_deletes_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-del-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/del", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();

    let output = common::git_mate()
        .args(["finish"])
        .current_dir(&wt_canonical)
        .output()
        .unwrap();
    assert!(output.status.success(), "finish should succeed");
    assert!(
        !wt_canonical.exists(),
        "worktree directory should be removed"
    );
    assert!(
        !repo.branch_exists("feature/del"),
        "feature/del should have been deleted"
    );
}

#[test]
fn finish_branch_not_checked_out_deletes_it() {
    let repo = common::RepoWithoutRemote::new();
    // Create a branch (merged, same commit as main) and stay on main
    repo.git(&["branch", "feature/z"]);
    common::git_mate()
        .args(["finish", "feature/z"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(
        !repo.branch_exists("feature/z"),
        "feature/z should have been deleted"
    );
}

#[test]
fn finish_removes_empty_parent_dirs_from_slash_branch() {
    use tempfile::TempDir;

    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);

    let project_name = repo.path().file_name().unwrap().to_str().unwrap();
    let feature_dir = wt_root.path().join(project_name).join("feature");
    let wt_path = feature_dir.join("login");
    std::fs::create_dir_all(&feature_dir).unwrap();

    repo.git(&[
        "worktree",
        "add",
        "-b",
        "feature/login",
        wt_path.to_str().unwrap(),
        "main",
    ]);

    common::git_mate()
        .args(["finish", "feature/login"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert!(!wt_path.exists(), "worktree dir should be removed");
    assert!(
        !feature_dir.exists(),
        "empty 'feature' dir should be cleaned up"
    );
    assert!(
        !wt_root.path().join(project_name).exists(),
        "empty project container dir should be cleaned up"
    );
    assert!(
        wt_root.path().exists(),
        "worktree root itself must not be deleted"
    );
}

#[test]
fn finish_completion_offers_only_linked_worktree_branches() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = tempfile::TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");
    repo.git(&[
        "worktree",
        "add",
        wt_path.to_str().unwrap(),
        "-b",
        "linked-branch",
        "main",
    ]);
    repo.git(&["branch", "plain-branch"]);

    let output = common::git_mate()
        .args(["--", "git-mate", "finish", ""])
        .env("COMPLETE", "bash")
        .env("_CLAP_COMPLETE_INDEX", "2")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let candidates = String::from_utf8_lossy(&output.stdout);
    let branches: Vec<&str> = candidates.lines().filter(|c| !c.starts_with('-')).collect();

    assert_eq!(branches, vec!["linked-branch"]);
}

#[test]
fn json_reports_where_the_branch_was_finished() {
    let repo = common::RepoWithoutRemote::new();
    let finish = |dir: &std::path::Path, args: &[&str]| {
        let output = common::git_mate()
            .args(["--json", "finish"])
            .args(args)
            .current_dir(dir)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        common::parse_single_json(&output)["data"].clone()
    };

    // Checked out in the main worktree: switched back to the default branch.
    repo.git(&["checkout", "-q", "-b", "feature/main"]);
    let data = finish(repo.path(), &[]);
    assert_eq!(data["branch"], "feature/main");
    assert_eq!(data["worktree"], "main");
    assert_eq!(data["switchedTo"], "main");

    // Checked out in a linked worktree: the worktree is removed.
    let wt_path = repo.path().join("feature-linked-wt");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "feature/linked",
        wt_path.to_str().unwrap(),
        "main",
    ]);
    let data = finish(repo.path(), &["feature/linked"]);
    assert_eq!(data["worktree"], "linked");
    assert_eq!(data["path"], wt_path.to_str().unwrap());
    assert_eq!(data["mainWorktree"], repo.path().to_str().unwrap());
    assert!(data.get("switchedTo").is_none());

    // Not checked out anywhere: only the branch is deleted.
    repo.git(&["branch", "feature/loose"]);
    let data = finish(repo.path(), &["feature/loose"]);
    assert_eq!(data["branch"], "feature/loose");
    assert!(data.get("worktree").is_none());
    assert!(data.get("path").is_none());
}

/// Run `git-mate <args>` from `dir` after `prepare` (a shell snippet run
/// inside `dir`) has deleted it, the way a worktree vanishes when it is
/// removed from another shell.
fn git_mate_in_deleted_dir(
    dir: &std::path::Path,
    prepare: &str,
    args: &[&str],
    proto: &std::path::Path,
) -> std::process::Output {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("cd \"$1\" && {prepare} && shift && exec \"$@\""))
        .arg("sh")
        .arg(dir)
        .arg(assert_cmd::cargo::cargo_bin!("git-mate"))
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_MATE_PROTO", proto)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

/// A repo with `mate.worktreeRoot` set and linked worktrees for `branches`.
fn repo_with_linked_worktrees(
    branches: &[&str],
) -> (
    common::RepoWithoutRemote,
    tempfile::TempDir,
    std::path::PathBuf,
) {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = tempfile::TempDir::new().unwrap();
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);
    let project = repo.path().file_name().unwrap().to_str().unwrap();
    let container = std::fs::canonicalize(wt_root.path()).unwrap().join(project);
    for branch in branches {
        let path = container.join(branch);
        repo.git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            branch,
            path.to_str().unwrap(),
            "main",
        ]);
    }
    (repo, wt_root, container)
}

#[test]
fn finish_in_a_worktree_finished_elsewhere_reports_it_as_already_finished() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&["feature/gone", "other"]);
    let main = std::fs::canonicalize(repo.path()).unwrap();
    let wt = container.join("feature/gone");
    let (_guard, proto) = common::proto_file();

    let prepare = format!(
        "git -C '{main}' worktree remove '{wt}' && git -C '{main}' branch -q -D feature/gone",
        main = main.display(),
        wt = wt.display()
    );
    let output = git_mate_in_deleted_dir(&wt, &prepare, &["--json", "finish"], &proto);
    assert!(output.status.success(), "{output:?}");

    let data = common::parse_single_json(&output.stdout)["data"].clone();
    assert_eq!(data["branch"], "feature/gone");
    assert_eq!(data["alreadyFinished"], true);
    let proto = std::fs::read_to_string(&proto).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", main.display())),
        "{proto:?}"
    );
}

#[test]
fn finish_in_a_deleted_worktree_directory_finishes_its_branch() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&["feature/deleted", "other"]);
    let wt = container.join("feature/deleted");
    let (_guard, proto) = common::proto_file();

    let prepare = format!("rm -rf '{}'", wt.display());
    let output = git_mate_in_deleted_dir(&wt, &prepare, &["--json", "finish"], &proto);
    assert!(output.status.success(), "{output:?}");

    let data = common::parse_single_json(&output.stdout)["data"].clone();
    assert_eq!(data["branch"], "feature/deleted");
    assert!(data.get("alreadyFinished").is_none());
    assert!(!repo.branch_exists("feature/deleted"));
}

#[test]
fn deleted_directory_outside_any_repository_is_an_error() {
    let scratch = tempfile::TempDir::new().unwrap();
    let dir = scratch.path().join("gone");
    std::fs::create_dir(&dir).unwrap();
    let (_guard, proto) = common::proto_file();

    let prepare = format!("rmdir '{}'", dir.display());
    let output = git_mate_in_deleted_dir(&dir, &prepare, &["--json", "finish"], &proto);
    assert!(!output.status.success());
    common::assert_json_error(&output.stdout, &["no longer exists"]);
}

#[test]
fn json_unknown_branch_is_branch_not_found() {
    let repo = common::RepoWithoutRemote::new();

    let output = common::git_mate()
        .args(["--json", "finish", "no-such-branch"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        common::assert_json_error_code(&output, "branch_not_found")["branch"],
        "no-such-branch"
    );
}

fn finish_all_json(dir: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = common::git_mate()
        .args(["--json", "finish", "--all"])
        .args(args)
        .current_dir(dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    common::parse_single_json(&output)["data"].clone()
}

#[test]
fn finish_all_removes_linked_worktrees_and_switches_main_to_default() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&["feature/a", "feature/b"]);
    repo.git(&["checkout", "-q", "-b", "feature/main"]);
    let detached = container.join("detached");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        detached.to_str().unwrap(),
        "main",
    ]);
    repo.git(&["branch", "loose"]);

    let data = finish_all_json(repo.path(), &[]);
    assert_eq!(data["finished"].as_array().unwrap().len(), 4, "{data}");
    assert_eq!(data["skipped"], serde_json::json!([]));

    assert_eq!(repo.current_branch(), "main");
    for branch in ["feature/a", "feature/b", "feature/main"] {
        assert!(!repo.branch_exists(branch), "{branch} should be deleted");
    }
    assert!(
        repo.branch_exists("loose"),
        "branches not checked out are kept"
    );
    assert!(!container.join("feature/a").exists());
    assert!(!detached.exists());
}

#[test]
fn finish_all_skips_what_fails_the_checks_and_finishes_the_rest() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&["feature/dirty", "feature/ok"]);
    let dirty = container.join("feature/dirty");
    std::fs::write(dirty.join("untracked.txt"), "dirty").unwrap();
    // A detached main worktree with a commit no branch refers to.
    repo.git(&["checkout", "-q", "--detach"]);
    repo.git(&["commit", "-q", "--allow-empty", "-m", "orphan"]);

    let data = finish_all_json(repo.path(), &[]);
    assert_eq!(data["finished"].as_array().unwrap().len(), 1, "{data}");
    assert_eq!(data["finished"][0]["branch"], "feature/ok");
    let skipped = data["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 2, "{data}");
    assert_eq!(skipped[0]["branch"], "feature/dirty");
    assert_eq!(skipped[0]["code"], "uncommitted_changes");
    assert_eq!(skipped[0]["retryWith"], serde_json::json!(["--force"]));
    assert_eq!(skipped[1]["worktree"], "main");
    assert_eq!(skipped[1]["code"], "unpushed_commits");

    assert!(dirty.exists(), "dirty worktree must be kept");
    assert!(repo.branch_exists("feature/dirty"));
    assert!(!repo.branch_exists("feature/ok"));
    assert_eq!(
        repo.current_branch(),
        "HEAD",
        "main worktree stays detached"
    );

    let data = finish_all_json(repo.path(), &["--force"]);
    assert_eq!(data["skipped"], serde_json::json!([]), "{data}");
    assert!(!dirty.exists());
    assert!(!repo.branch_exists("feature/dirty"));
    assert_eq!(repo.current_branch(), "main");
}

#[test]
fn finish_all_from_a_linked_worktree_moves_to_the_main_worktree() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&["feature/here"]);
    let wt = container.join("feature/here");
    let (_guard, proto) = common::proto_file();

    common::git_mate()
        .args(["finish", "--all"])
        .env("GIT_MATE_PROTO", &proto)
        .current_dir(&wt)
        .assert()
        .success();
    assert!(!wt.exists());
    let main = std::fs::canonicalize(repo.path()).unwrap();
    let proto = std::fs::read_to_string(&proto).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", main.display())),
        "{proto:?}"
    );
}

#[test]
fn finish_all_from_a_linked_worktree_finishes_the_worktrees_after_it() {
    let (repo, _wt_root, _container) =
        repo_with_linked_worktrees(&["feature/a", "feature/b", "feature/c"]);
    repo.git(&["checkout", "-q", "-b", "feature/main"]);
    // Run from the first linked worktree git lists, so that the others are
    // finished after the current directory is gone.
    let list = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let list = String::from_utf8(list.stdout).unwrap();
    let first_linked = list
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .nth(1)
        .unwrap()
        .to_string();
    let (_guard, proto) = common::proto_file();

    let output = common::git_mate()
        .args(["--json", "finish", "--all"])
        .env("GIT_MATE_PROTO", &proto)
        .current_dir(&first_linked)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let data = common::parse_single_json(&output)["data"].clone();
    assert_eq!(data["skipped"], serde_json::json!([]), "{data}");
    assert_eq!(data["finished"].as_array().unwrap().len(), 4, "{data}");

    assert_eq!(repo.current_branch(), "main");
    for branch in ["feature/a", "feature/b", "feature/c", "feature/main"] {
        assert!(!repo.branch_exists(branch), "{branch} should be deleted");
    }
    let main = std::fs::canonicalize(repo.path()).unwrap();
    let proto = std::fs::read_to_string(&proto).unwrap();
    assert_eq!(
        proto.matches("CD:").count(),
        1,
        "exactly one cd to the main worktree: {proto:?}"
    );
    assert!(
        proto.contains(&format!("CD:{}", main.display())),
        "{proto:?}"
    );
}

#[test]
fn finish_all_keeps_the_default_branch_checked_out_in_a_linked_worktree() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&[]);
    repo.git(&["checkout", "-q", "-b", "feature/x"]);
    let main_wt = container.join("main");
    repo.git(&["worktree", "add", "-q", main_wt.to_str().unwrap(), "main"]);

    let data = finish_all_json(repo.path(), &[]);
    assert_eq!(data["skipped"], serde_json::json!([]), "{data}");
    let finished = data["finished"].as_array().unwrap();
    assert_eq!(finished.len(), 2, "{data}");
    assert_eq!(finished[0]["branch"], "main");
    assert_eq!(finished[0]["worktree"], "linked");
    assert_eq!(finished[0]["branchKept"], true);
    assert_eq!(finished[1]["branch"], "feature/x");
    assert!(finished[1].get("branchKept").is_none(), "{data}");

    assert!(!main_wt.exists(), "its worktree is removed");
    assert!(repo.branch_exists("main"), "the default branch is kept");
    assert_eq!(repo.current_branch(), "main");
    assert!(!repo.branch_exists("feature/x"));
}

#[test]
fn finish_all_with_nothing_to_finish_succeeds() {
    let repo = common::RepoWithoutRemote::new();
    let data = finish_all_json(repo.path(), &[]);
    assert_eq!(data["finished"], serde_json::json!([]));
    assert_eq!(data["skipped"], serde_json::json!([]));
}

#[test]
fn finish_all_conflicts_with_a_branch() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["branch", "feature/x"]);
    common::git_mate()
        .args(["finish", "--all", "feature/x"])
        .current_dir(repo.path())
        .assert()
        .failure();
    assert!(repo.branch_exists("feature/x"));
}

#[test]
fn finish_dry_run_changes_nothing() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-dry-wt");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "feature/linked",
        wt_path.to_str().unwrap(),
        "main",
    ]);
    repo.git(&["checkout", "-q", "-b", "feature/main"]);
    repo.git(&["branch", "feature/loose"]);

    for (branch, worktree) in [
        ("feature/main", Some("main")),
        ("feature/linked", Some("linked")),
        ("feature/loose", None),
    ] {
        let output = common::git_mate()
            .args(["--json", "finish", "--dry-run", branch])
            .current_dir(repo.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let data = common::parse_single_json(&output)["data"].clone();
        assert_eq!(data["dryRun"], true, "{data}");
        assert_eq!(data["branch"], branch);
        assert_eq!(data["worktree"].as_str(), worktree);
        assert!(repo.branch_exists(branch), "{branch} must be kept");
    }
    assert_eq!(repo.current_branch(), "feature/main");
    assert!(wt_path.exists(), "worktree must be kept");
}

#[test]
fn finish_dry_run_still_refuses_what_finish_would() {
    let repo = common::RepoWithRemote::new();
    repo.local_git(&["checkout", "-b", "feature/x"]);
    repo.local_git(&["commit", "--allow-empty", "-m", "local only"]);

    let output = common::git_mate()
        .args(["--json", "finish", "--dry-run"])
        .current_dir(repo.local_path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error_code(&output, "unpushed_commits");
    assert!(repo.local_branch_exists("feature/x"));
}

#[test]
fn finish_all_dry_run_reports_without_changing_anything() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&["feature/dirty", "feature/ok"]);
    let dirty = container.join("feature/dirty");
    std::fs::write(dirty.join("untracked.txt"), "dirty").unwrap();
    repo.git(&["checkout", "-q", "-b", "feature/main"]);
    let (_guard, proto) = common::proto_file();

    let output = common::git_mate()
        .args(["--json", "finish", "--all", "--dry-run"])
        .env("GIT_MATE_PROTO", &proto)
        .current_dir(container.join("feature/ok"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let data = common::parse_single_json(&output)["data"].clone();
    assert_eq!(data["dryRun"], true);
    let finished: Vec<_> = data["finished"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["branch"].as_str().unwrap())
        .collect();
    assert_eq!(finished, ["feature/ok", "feature/main"], "{data}");
    assert_eq!(data["skipped"][0]["branch"], "feature/dirty");

    for branch in ["feature/dirty", "feature/ok", "feature/main"] {
        assert!(repo.branch_exists(branch), "{branch} must be kept");
    }
    assert!(container.join("feature/ok").exists());
    assert_eq!(repo.current_branch(), "feature/main");
    assert!(
        std::fs::read_to_string(&proto).unwrap().is_empty(),
        "a dry run must not move the shell"
    );
}

/// Commit a change to `conflict.txt` on the branch checked out in `dir`, and a
/// conflicting one on a new branch `base` from where it started, then rebase
/// onto `base`, which stops at the conflict.
fn stop_rebase_at_conflict(dir: &std::path::Path, base: &str) {
    let branch = String::from_utf8(
        std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(dir)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let branch = branch.trim();
    common::git(dir, &["checkout", "-q", "-b", base]);
    std::fs::write(dir.join("conflict.txt"), "base").unwrap();
    common::git(dir, &["add", "conflict.txt"]);
    common::git(dir, &["commit", "-q", "-m", "base side"]);
    common::git(dir, &["checkout", "-q", branch]);
    std::fs::write(dir.join("conflict.txt"), "branch").unwrap();
    common::git(dir, &["add", "conflict.txt"]);
    common::git(dir, &["commit", "-q", "-m", "branch side"]);
    let status = std::process::Command::new("git")
        .args(["rebase", base])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .current_dir(dir)
        .output()
        .unwrap()
        .status;
    assert!(!status.success(), "rebase should stop at the conflict");
}

fn rebase_in_progress(dir: &std::path::Path) -> bool {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "rebase-merge"])
        .current_dir(dir)
        .output()
        .unwrap();
    dir.join(String::from_utf8(out.stdout).unwrap().trim())
        .exists()
}

#[test]
fn finish_all_skips_a_worktree_mid_rebase_without_force() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&["feature/r"]);
    let wt = container.join("feature/r");
    stop_rebase_at_conflict(&wt, "base-r");

    let data = finish_all_json(repo.path(), &[]);
    assert_eq!(data["finished"], serde_json::json!([]), "{data}");
    let skipped = &data["skipped"][0];
    assert_eq!(skipped["branch"], "feature/r", "{data}");
    assert_eq!(skipped["code"], "operation_in_progress");
    assert_eq!(skipped["retryWith"], serde_json::json!(["--force"]));
    assert!(rebase_in_progress(&wt));
    assert!(repo.branch_exists("feature/r"));
}

#[test]
fn finish_all_force_aborts_rebases_and_finishes_their_branches() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&["feature/r"]);
    let wt = container.join("feature/r");
    stop_rebase_at_conflict(&wt, "base-r");
    repo.git(&["checkout", "-q", "-b", "feature/m"]);
    stop_rebase_at_conflict(repo.path(), "base-m");

    let dry = finish_all_json(repo.path(), &["--force", "--dry-run"]);
    assert_eq!(dry["skipped"], serde_json::json!([]), "{dry}");
    assert!(rebase_in_progress(&wt) && rebase_in_progress(repo.path()));
    assert!(repo.branch_exists("feature/r") && repo.branch_exists("feature/m"));

    let data = finish_all_json(repo.path(), &["--force"]);
    assert_eq!(data["skipped"], serde_json::json!([]), "{data}");
    for report in [&dry, &data] {
        let finished = report["finished"].as_array().unwrap();
        assert_eq!(finished.len(), 2, "{report}");
        assert_eq!(finished[0]["branch"], "feature/r");
        assert_eq!(finished[0]["worktree"], "linked");
        assert_eq!(finished[0]["abortedOperation"], "rebase");
        assert_eq!(finished[1]["branch"], "feature/m");
        assert_eq!(finished[1]["worktree"], "main");
        assert_eq!(finished[1]["abortedOperation"], "rebase");
    }

    assert!(!wt.exists());
    assert!(!repo.branch_exists("feature/r"));
    assert!(!repo.branch_exists("feature/m"));
    assert_eq!(repo.current_branch(), "main");
    assert!(
        !rebase_in_progress(repo.path()),
        "the rebase must be aborted"
    );
}

#[test]
fn finish_force_in_a_main_worktree_mid_rebase_aborts_it() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-q", "-b", "feature/m"]);
    stop_rebase_at_conflict(repo.path(), "base-m");

    let output = common::git_mate()
        .args(["--json", "finish", "--force"])
        .current_dir(repo.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let data = common::parse_single_json(&output)["data"].clone();
    assert_eq!(data["branch"], "feature/m", "{data}");
    assert_eq!(data["abortedOperation"], "rebase");
    assert_eq!(repo.current_branch(), "main");
    assert!(!rebase_in_progress(repo.path()));
    assert!(!repo.branch_exists("feature/m"));
}

#[test]
fn finish_all_keeps_a_detached_worktree_whose_commit_only_a_tag_holds() {
    let (repo, _wt_root, container) = repo_with_linked_worktrees(&[]);
    let detached = container.join("detached");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        detached.to_str().unwrap(),
        "main",
    ]);
    common::git(
        &detached,
        &["commit", "-q", "--allow-empty", "-m", "tagged"],
    );
    common::git(&detached, &["tag", "v1"]);

    let data = finish_all_json(repo.path(), &[]);
    assert_eq!(data["skipped"], serde_json::json!([]), "{data}");
    assert_eq!(data["finished"].as_array().unwrap().len(), 1, "{data}");
    assert!(!detached.exists());
}

#[test]
fn finish_dry_run_predicts_local_changes_blocking_the_switch() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-q", "-b", "feature/m"]);
    std::fs::write(repo.path().join("work.txt"), "committed").unwrap();
    repo.git(&["add", "work.txt"]);
    repo.git(&["commit", "-q", "-m", "add work.txt"]);

    // A change the switch carries over doesn't block it.
    std::fs::write(repo.path().join("unrelated.txt"), "new").unwrap();
    let data = finish_all_json(repo.path(), &["--dry-run"]);
    assert_eq!(data["skipped"], serde_json::json!([]), "{data}");

    // One it would overwrite does, in a dry run as in a real one.
    std::fs::write(repo.path().join("work.txt"), "modified").unwrap();
    for args in [&["--dry-run"][..], &[]] {
        let data = finish_all_json(repo.path(), args);
        assert_eq!(data["finished"], serde_json::json!([]), "{data}");
        assert_eq!(data["skipped"][0]["code"], "uncommitted_changes", "{data}");
        assert_eq!(
            data["skipped"][0]["retryWith"],
            serde_json::json!(["--force"])
        );
    }
    assert_eq!(repo.current_branch(), "feature/m");
}
