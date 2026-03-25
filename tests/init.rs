mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn zsh_output_includes_mate_wrapper() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("_git_mate_capture_and_cd()"))
        .stdout(predicate::str::contains("function git-mate()"))
        .stdout(predicate::str::contains("_git_mate_capture_and_cd _git_mate_run_binary"));
}

#[test]
fn zsh_wrapper_uses_auto_cd() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("_MATE_CD:"))
        .stdout(predicate::str::contains("builtin cd"));
}

#[test]
fn zsh_output_includes_completion() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("source <(COMPLETE=zsh command git-mate)"));
}

#[test]
fn bash_output_includes_mate_wrapper() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("_git_mate_capture_and_cd()"))
        .stdout(predicate::str::contains("git-mate()"))
        .stdout(predicate::str::contains("_git_mate_capture_and_cd _git_mate_run_binary"));
}

#[test]
fn bash_wrapper_uses_auto_cd() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("_MATE_CD:"))
        .stdout(predicate::str::contains("builtin cd"));
}

#[test]
fn bash_output_includes_completion() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("source <(COMPLETE=bash command git-mate)"));
}

#[test]
fn bash_output_omits_git_wrapper_by_default() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("git()").not());
}

#[test]
fn bash_output_includes_git_wrapper_when_enabled_in_config() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.gitAutoCd", "true"]);
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Optional auto-cd support for `git mate`"))
        .stdout(predicate::str::contains("git() {"))
        .stdout(predicate::str::contains("bypassed in favor of `command git`"))
        .stdout(predicate::str::contains("declare -f git").not())
        .stdout(predicate::str::contains("_git_mate_run_git_mate"))
        .stdout(predicate::str::contains("_git_mate_call_git mate \"$@\""));
}

#[test]
fn bash_output_includes_safe_git_wrapper_mode_when_enabled_in_config() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.gitAutoCd", "if-safe"]);
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("mate.gitAutoCd=if-safe skipped git() wrapper"))
        .stdout(predicate::str::contains("declare -f git").not())
        .stdout(predicate::str::contains("git() {"))
        .stdout(predicate::str::contains("_git_mate_run_git_mate"));
}

#[test]
fn zsh_output_includes_git_wrapper_when_enabled_in_config() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.gitAutoCd", "yes"]);
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Optional auto-cd support for `git mate`"))
        .stdout(predicate::str::contains("function git()"))
        .stdout(predicate::str::contains("bypassed in favor of `command git`"))
        .stdout(predicate::str::contains("functions git").not())
        .stdout(predicate::str::contains("_git_mate_run_git_mate"))
        .stdout(predicate::str::contains("_git_mate_call_git mate \"$@\""));
}

#[test]
fn zsh_output_includes_safe_git_wrapper_mode_when_enabled_in_config() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.gitAutoCd", "if-safe"]);
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("mate.gitAutoCd=if-safe skipped git() wrapper"))
        .stdout(predicate::str::contains("functions git").not())
        .stdout(predicate::str::contains("function git()"))
        .stdout(predicate::str::contains("_git_mate_run_git_mate"));
}

#[test]
fn init_fails_for_invalid_git_auto_cd_config() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.gitAutoCd", "banana"]);
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.gitAutoCd"))
        .stderr(predicate::str::contains("if-safe"));
}
