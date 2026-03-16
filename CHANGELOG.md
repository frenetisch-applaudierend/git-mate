# Changelog

All notable changes to `git-mate` are documented here.

## [0.2.0] - 2026-03-16

### Bug Fixes

- **finish**: Remove empty parent dirs after removing slash-branch worktrees

- **init**: Update tests and docs to use mate.shorthand config key

- **init**: Strip command name from completion args and clear COMPREPLY

- **finish**: Print GWT_CD when removing current worktree

- **checkout**: Distinguish file-at-path from directory-without-git

- **checkout**: Handle directory conflict and deduplicate worktree lookup

- **checkout**: Include path in create_dir_all error message

- **checkout**: Improve worktree noop guard and error message wording

- **finish**: Run git commands in main worktree context


### Documentation

- **cli**: Add flag help text and fix finish subcommand description

- Reorganize README structure

- Add subcommand descriptions to CLI help output

- Add platform support note

- Remove work-in-progress banner

- Fix shell integration description to reference wrapper function

- Document shell integration and supported git configs

- Align README with current implementation


### Features

- Rename binary from git-mate to mate [**BREAKING**]

- Implement configurable shorthands via git config

- Styled output via console crate and suppress git noise

- **init**: Migrate shell completions to clap_complete unstable-dynamic

- **init**: Only emit _MATE_CD sentinel when called from shell wrapper

- **init**: Change default wrapper name from git to gm

- **init**: Add --wrapper-name flag and mate.wrapperName config

- **init**: Add shell integration via `git mate init zsh/bash`

- **checkout**: Navigate to existing worktree instead of erroring

- **checkout**: Add checkout command for switching to existing branches

- Add review-pr-comments skill with supporting scripts

- **finish**: Implement git mate finish command

- **new**: Auto-fetch origin before branching

- **new**: Add -w flag to create linked git worktree


### Refactoring

- Move called_from_wrapper into output and fix formatting

- **git**: Make run private, expose named high-level commands

- Move worktree git-logic into git.rs

- Eliminate duplication across cmd modules and tests

- **init**: Remove --wrapper-name flag, validate wrapperName, deduplicate shell helpers

- **test**: Improve test structure and idioms

- **test**: Rename TestRepo to RepoWithoutRemote

- Migrate sync.rs to git::run and declare git module in main

- Introduce git::config submodule with typed read_string/read_bool


### Testing

- **checkout**: Add test for file-at-worktree-path error case

- **new**: Add tests for edge cases from review



## [0.1.1] - 2026-03-06

### Documentation

- Mark project as work in progress


