# Changelog

All notable changes to `git-mate` are documented here.

## [0.3.0] - 2026-05-29

### Bug Fixes

- **checkout**: Keep default branch in main worktree

- **protocol,init**: Address PR review feedback

- **new**: Set push tracking for linked worktrees

- **worktree**: Keep default branch in main worktree

- **checkout**: Emit shell cd marker for existing worktrees

- **new**: Set push tracking so git push targets the new branch

- **worktree**: Address review comments on copy_ignored_files


### Documentation

- Update README for co alias, finish branch deletion, sync ff, mate.fetch

- Require clean build and tests before commits and PRs


### Features

- **sync**: Add optional default-branch merge

- **new**: Handle dirty source worktrees

- **checkout**: Unify worktree moves under checkout

- **move**: Toggle branches between worktrees

- **init,protocol**: Add pwsh shell integration

- **init**: Add completion support for \`git mate\` subcommand form

- Rename binary from mate to git-mate [**BREAKING**]

- **protocol**: Add shell protocol module and _protocol command

- **config**: Add default worktree location

- **move**: Add command to move current branch to worktree

- **finish**: Add --force flag and unpushed-commits guard

- **new**: Respect mate.fetch config to skip fetching

- **worktree**: Copy gitignored local config files when creating worktrees

- **finish**: Always delete local branch after finishing

- **sync**: Fast-forward and prune local branches after fetch

- **checkout**: Allow switching to main worktree

- **checkout**: Add co alias for checkout subcommand


### Refactoring

- **protocol**: Replace stdout pipe with GIT_MATE_PROTO file

- **git**: Split git module into submodules and extract fs utilities

- **config**: Rename default branch mode setting


### Testing

- **init**: Add pwsh syntax test; skip gracefully when shell not found



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


