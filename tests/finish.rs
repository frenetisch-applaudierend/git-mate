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
    common::git_mate()
        .args(["finish", "feature/dirty"])
        .current_dir(repo.path())
        .assert()
        .failure();

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
