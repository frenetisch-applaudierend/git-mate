# git-mate

> **Work in progress** — not ready for public use.

A focused Git CLI extension for workflows that blend branch checkout with optional worktrees. Built for my specific workflow using git.

## Requirements

- Rust (for building from source)
- `git`
- `gh` (GitHub CLI, for issue/PR integration)

## Installation

```bash
cargo install git-mate
```

Because the binary is named `git-mate`, Git's extension mechanism picks it up automatically — no aliases needed. Every command is available as `git mate <cmd>` immediately after installation.

Then add the following to your `.zshrc`, `.bashrc`, or `config.fish` to enable automatic directory switching:

```bash
eval "$(git mate init zsh)"   # or bash / fish
```

This installs a shell function that wraps `git-mate` and handles automatic `cd` for commands that change your working directory (`new`, `co`, `finish`). Without it, `git-mate` still works but won't navigate your shell.

## Commands

### `git mate init <shell>`

Prints the shell integration script to stdout. Pipe it through `eval` in your shell config to enable automatic directory switching.

```bash
git mate init zsh    # zsh
git mate init bash   # bash
git mate init fish   # fish
```

### `git mate new <branch>`

Creates a new branch from the default branch (`main` or `master`, auto-detected) and switches to it.

```bash
git mate new feature/login          # checkout in main worktree
git mate new feature/login -w       # create a linked worktree and cd into it
git mate new feature/login -w --from v2.1.0  # branch from a specific ref
```

### `git mate co <branch>`

Checks out an existing branch — local or remote. If the branch already exists in a worktree, navigates there instead.

```bash
git mate co feature/login           # checkout in main worktree
git mate co feature/login -w        # create a linked worktree and cd into it
git mate co 142                     # resolve GitHub issue #142 to its branch and check it out
git mate co 142 -w                  # same, in a worktree
```

When passing a GitHub issue number, `git-mate` uses `gh` to look up the associated branch. If no branch exists yet, it prompts you to create one.

### `git mate finish [branch]`

You're done with a branch. Cleans up and returns you to the main worktree.

- If you're inside a linked worktree: removes the worktree, navigates to main
- If you're in the main worktree on a feature branch: switches to the default branch
- Optionally deletes the local branch with `--delete-branch` (only if merged)

```bash
git mate finish                     # finish current branch/worktree
git mate finish feature/login       # finish a specific branch from anywhere
git mate finish --delete-branch     # also delete the local branch
```

### `git mate sync`

Fetches all remotes and prunes stale local tracking references. Optionally cleans up local branches whose upstream is gone.

```bash
git mate sync                       # fetch + prune remote refs
git mate sync --clean               # also delete local branches with no upstream
git mate sync --clean --dry-run     # preview what would be deleted
```

### `git mate list`

Shows all worktrees with their branch and status. Marks the main worktree and highlights dirty state.

```bash
git mate list
# main      /home/markus/project          [main]
# worktree  /home/markus/project-login    [feature/login]  (dirty)
# worktree  /home/markus/project-auth     [feature/auth]
```

## Worktree location

By default, worktrees are created as siblings of the main worktree, named after the branch:

```
~/projects/
  my-repo/          ← main worktree
  my-repo-feature-login/   ← worktree for feature/login
  my-repo-feature-auth/    ← worktree for feature/auth
```

Override the base path in config (see below).

## Configuration

`git-mate` reads configuration from git config, giving you global defaults with per-repo overrides for free.

```bash
# Set global defaults
git config --global mate.worktreeBase "~/worktrees"
git config --global mate.copyFiles ".env,.env.local"

# Override for a specific repo (run inside that repo)
git config mate.worktreeBase "../worktrees"
git config mate.postCreate "npm install"
```

Or edit `~/.gitconfig` and `.git/config` directly:

```ini
# ~/.gitconfig
[gwt]
    worktreeBase = ~/worktrees
    copyFiles = .env,.env.local

# .git/config (per-repo override)
[gwt]
    worktreeBase = ../worktrees
    postCreate = npm install
```

| Key | Description | Default |
|-----|-------------|---------|
| `mate.worktreeBase` | Where to create linked worktrees | sibling of main worktree |
| `mate.postCreate` | Command to run after creating a worktree | — |
| `mate.copyFiles` | Comma-separated files to copy from main worktree | — |

## How the shell integration works

`git mate init <shell>` outputs a small shell function that wraps the `git-mate` binary. When commands need to change your working directory, the binary prints a directive to stdout:

```
GWT_CD:/path/to/worktree
```

The shell function captures this and calls `cd` in the current shell process. All other output passes through unchanged. This is the same approach used by zoxide and direnv — the binary itself never touches your shell state directly.

## Non-goals

- GUI or TUI — this is a keyboard-first tool
- Bare repository workflows — regular clones only
- Replacing `git` — `git-mate` shells out to `git` and `gh`, it doesn't reimplement them
