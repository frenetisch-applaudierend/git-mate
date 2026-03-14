mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn default_wrapper_name_is_gm() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("function gm()"));
}

#[test]
fn config_overrides_wrapper_name() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.wrapperName", "g"]);
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("function g()"));
}

#[test]
fn invalid_wrapper_name_is_rejected() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.wrapperName", "bad name!"]);
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid wrapper name"));
}

#[test]
fn zsh_output_includes_completion_function() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("_git_mate_complete"))
        .stdout(predicate::str::contains("compdef"));
}

#[test]
fn bash_output_includes_completion_function() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("_git_mate_complete"))
        .stdout(predicate::str::contains("complete -F"));
}

#[test]
fn completion_registered_for_binary_and_wrapper() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("compdef _git_mate_complete git-mate"))
        .stdout(predicate::str::contains("compdef _git_mate_complete gm"));
}

#[test]
fn completion_uses_custom_wrapper_name() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.wrapperName", "g"]);
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("compdef _git_mate_complete g"));
}

#[test]
fn zsh_completion_strips_command_name_from_words() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("${words[@]:1}"))
        .stdout(predicate::str::contains("\"${words[@]}\"").not());
}

#[test]
fn bash_completion_clears_compreply_before_filling() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("COMPREPLY=()"));
}

#[test]
fn bash_completion_strips_command_name_from_comp_words() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("${COMP_WORDS[@]:1}"));
}
