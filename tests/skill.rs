mod common;

use std::path::Path;

fn install(home: &Path, args: &[&str], cwd: &Path) -> serde_json::Value {
    let output = common::git_mate()
        .args(["--json", "skill", "install"])
        .args(args)
        .env("HOME", home)
        .current_dir(cwd)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = common::parse_single_json(&output);
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "skill");
    json
}

fn read_skill(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

/// The stub has static frontmatter and pulls its body from the binary, which
/// must be pre-approved or Claude Code aborts the skill.
fn assert_is_stub(content: &str) {
    assert!(content.starts_with("---\nname: git-mate\ndescription: "));
    assert!(content.contains("\nallowed-tools: Bash(git mate skill)\n"));
    assert!(content.trim_end().ends_with("!`git mate skill`"));
}

#[test]
fn skill_prints_instructions_without_frontmatter() {
    let output = common::git_mate().arg("skill").assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.starts_with("# Using git-mate"));
    assert!(stdout.contains("--json"));
}

#[test]
fn skill_rejects_json() {
    let output = common::git_mate()
        .args(["--json", "skill"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let json = common::parse_single_json(&output);
    assert_eq!(json["error"]["code"], "unsupported");
}

#[test]
fn install_writes_stub_into_home() {
    let home = tempfile::tempdir().unwrap();
    let json = install(home.path(), &[], home.path());

    let path = home.path().join(".claude/skills/git-mate/SKILL.md");
    assert_eq!(json["data"]["action"], "created");
    assert_eq!(json["data"]["path"], path.to_str().unwrap());
    assert_is_stub(&read_skill(&path));
}

#[test]
fn install_is_idempotent_and_replaces_outdated_stub() {
    let home = tempfile::tempdir().unwrap();
    install(home.path(), &[], home.path());
    let path = home.path().join(".claude/skills/git-mate/SKILL.md");

    let json = install(home.path(), &[], home.path());
    assert_eq!(json["data"]["action"], "unchanged");

    std::fs::write(&path, "---\nname: git-mate\ndescription: old\n---\n").unwrap();
    let json = install(home.path(), &[], home.path());
    assert_eq!(json["data"]["action"], "updated");
    assert_is_stub(&read_skill(&path));
}

#[test]
fn install_project_writes_into_repository() {
    let repo = common::RepoWithoutRemote::new();
    let home = tempfile::tempdir().unwrap();
    let subdir = repo.path().join("src");
    std::fs::create_dir(&subdir).unwrap();

    install(home.path(), &["--project"], &subdir);

    assert_is_stub(&read_skill(
        &repo.path().join(".claude/skills/git-mate/SKILL.md"),
    ));
    assert!(!home.path().join(".claude").exists());
}

#[test]
fn install_dir_writes_into_given_skills_directory() {
    let home = tempfile::tempdir().unwrap();
    let skills = tempfile::tempdir().unwrap();

    install(
        home.path(),
        &["--dir", skills.path().to_str().unwrap()],
        home.path(),
    );

    assert_is_stub(&read_skill(&skills.path().join("git-mate/SKILL.md")));
    assert!(!home.path().join(".claude").exists());
}
