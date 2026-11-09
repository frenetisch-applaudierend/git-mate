mod common;

use tempfile::TempDir;

fn overview(dir: &std::path::Path) -> serde_json::Value {
    let output = common::git_mate()
        .args(["--json", "overview"])
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

fn entry<'a>(data: &'a serde_json::Value, branch: &str) -> &'a serde_json::Value {
    data["branches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["branch"] == branch)
        .unwrap_or_else(|| panic!("{branch} missing in {data}"))
}

fn commit(dir: &std::path::Path, name: &str) {
    std::fs::write(dir.join(name), name).unwrap();
    common::git(dir, &["add", name]);
    common::git(dir, &["commit", "-q", "-m", name]);
}

#[test]
fn reports_location_dirt_and_upstream_state() {
    let setup = common::RepoWithRemote::new();
    let local = setup.local_path();
    let wt_root = TempDir::new().unwrap();

    // Ahead of its upstream, checked out in a dirty linked worktree.
    setup.push_branch_to_remote("feature/ahead");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/ahead");
    let wt = wt_root.path().join("ahead");
    setup.local_git(&[
        "worktree",
        "add",
        "-q",
        wt.to_str().unwrap(),
        "feature/ahead",
    ]);
    commit(&wt, "a.txt");
    std::fs::write(wt.join("untracked.txt"), "dirty").unwrap();

    // Behind its upstream.
    setup.push_branch_to_remote("feature/behind");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/behind");
    setup.push_commit_to_remote_branch("feature/behind", "remote work");
    setup.local_fetch();

    // Remote deleted (after the fetch that pruned it).
    setup.push_branch_to_remote("feature/gone");
    setup.local_fetch();
    setup.create_local_tracking_branch("feature/gone");
    setup.delete_remote_branch("feature/gone");
    setup.local_git(&["fetch", "-q", "--prune"]);

    // Never pushed.
    setup.local_git(&["branch", "feature/local"]);

    let data = overview(local);

    let main = entry(&data, "main");
    assert_eq!(
        data["branches"][0]["branch"], "main",
        "default branch first"
    );
    assert_eq!(main["default"], true);
    assert_eq!(main["current"], true);
    assert_eq!(main["worktree"], "main");
    assert_eq!(main["dirty"], false);
    assert_eq!(main["upstream"]["state"], "up-to-date");

    let ahead = entry(&data, "feature/ahead");
    assert_eq!(ahead["current"], false);
    assert_eq!(ahead["worktree"], "linked");
    assert_eq!(ahead["path"], wt.to_str().unwrap());
    assert_eq!(ahead["dirty"], true);
    assert_eq!(ahead["upstream"]["name"], "origin/feature/ahead");
    assert_eq!(ahead["upstream"]["state"], "ahead");
    assert_eq!(ahead["upstream"]["ahead"], 1);

    let behind = entry(&data, "feature/behind");
    assert!(
        behind.get("worktree").is_none(),
        "not checked out: {behind}"
    );
    assert_eq!(behind["upstream"]["state"], "behind");
    assert_eq!(behind["upstream"]["behind"], 1);

    let gone = entry(&data, "feature/gone");
    assert_eq!(gone["upstream"]["state"], "gone");
    assert!(gone["upstream"].get("ahead").is_none());

    assert_eq!(
        entry(&data, "feature/local")["upstream"],
        serde_json::Value::Null
    );
    assert_eq!(data["detached"], serde_json::json!([]));
}

#[test]
fn current_is_the_worktree_it_runs_in() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt = wt_root.path().join("x");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "feature/x",
        wt.to_str().unwrap(),
    ]);

    let data = overview(&wt.join("."));
    assert_eq!(entry(&data, "feature/x")["current"], true);
    assert_eq!(entry(&data, "main")["current"], false);
}

#[test]
fn reports_detached_worktrees_and_unfinished_rebases() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();

    let detached = wt_root.path().join("detached");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        detached.to_str().unwrap(),
    ]);

    // A rebase stopped at a conflict: git lists the worktree as detached.
    std::fs::write(repo.path().join("f.txt"), "base\n").unwrap();
    repo.git(&["add", "f.txt"]);
    repo.git(&["commit", "-q", "-m", "base"]);
    let rebasing = wt_root.path().join("rebasing");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "feature/r",
        rebasing.to_str().unwrap(),
    ]);
    std::fs::write(rebasing.join("f.txt"), "theirs\n").unwrap();
    common::git(&rebasing, &["commit", "-q", "-am", "theirs"]);
    std::fs::write(repo.path().join("f.txt"), "ours\n").unwrap();
    repo.git(&["commit", "-q", "-am", "ours"]);
    let status = std::process::Command::new("git")
        .args(["rebase", "main"])
        .current_dir(&rebasing)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap()
        .status;
    assert!(!status.success(), "rebase should stop at the conflict");

    let data = overview(repo.path());
    let rebase = entry(&data, "feature/r");
    assert_eq!(rebase["worktree"], "linked");
    assert_eq!(rebase["operation"], "rebase");

    let detached_entries = data["detached"].as_array().unwrap();
    assert_eq!(detached_entries.len(), 1, "{data}");
    assert_eq!(detached_entries[0]["path"], detached.to_str().unwrap());
    assert_eq!(detached_entries[0]["dirty"], false);
}
