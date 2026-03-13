# git-mate

> **Work in progress** — not ready for public use.

A Git CLI extension for my personal workflow around git worktrees.

## Requirements

- Rust (for building from source)
- `git`

## Installation

```bash
cargo install git-mate
```

Because the binary is named `git-mate`, Git's extension mechanism picks it up automatically — no aliases needed. Every command is available as `git mate <cmd>` immediately after installation.

## Commands

### `git mate new <branch>`

Creates a new branch from the default branch (`main` or `master`, auto-detected) and switches to it.

```bash
git mate new feature/login          # checkout in main worktree
git mate new feature/login -w       # create a linked worktree and cd into it
git mate new feature/login -w --from v2.1.0  # branch from a specific ref
```

### `git mate checkout <branch>`

Checks out an existing branch — local or remote. If the branch already exists in a worktree, navigates there instead.

```bash
git mate checkout feature/login           # checkout in main worktree
git mate checkout feature/login -w        # create a linked worktree and cd into it
```

### `git mate finish [branch]`

You're done with a branch. Cleans up and returns you to the main worktree.

- If you're inside a linked worktree: removes the worktree
- If you're in the main worktree on a feature branch: switches to the default branch
- Optionally deletes the local branch with `--delete-branch` (only if merged)

```bash
git mate finish                     # finish current branch/worktree
git mate finish feature/login       # finish a specific branch from anywhere
git mate finish --delete-branch     # also delete the local branch
```

### `git mate sync`

Fetches all remotes and prunes stale local tracking references, then pulls the current branch if an upstream is configured.

```bash
git mate sync                       # fetch + prune, then pull
git mate sync --rebase              # pull with --rebase
git mate sync --ff-only             # pull with --ff-only
```

## Shell integration

`git mate init` emits a shell wrapper function (`gm` by default) whose
subcommands `co`, `new`, and `finish` handle directory changes (e.g. switching
into a new worktree) in your current shell session.

Add this to your shell config:

```bash
# ~/.zshrc  or  ~/.bashrc
eval "$(git mate init zsh)"   # zsh
eval "$(git mate init bash)"  # bash
```

This defines a `gm` wrapper function. Commands it doesn't recognise are passed
straight through to the real `git` binary via `command git`.

### Custom wrapper name

Override the default `gm` function name via git config:

```bash
git config --global mate.wrapperName g
eval "$(git mate init zsh)"
```

## Worktree location

By default, worktrees are created under a root directory named after the repo:

```
~/worktrees/
  my-repo/
    feature-login/    ← worktree for feature/login
    feature-auth/     ← worktree for feature/auth
```

Override the root path in config (see below).

## Configuration

`git-mate` reads configuration from git config, giving you global defaults with per-repo overrides for free.

```bash
# Set global defaults
git config --global mate.worktreeRoot "~/worktrees"

# Override for a specific repo (run inside that repo)
git config mate.worktreeRoot "../worktrees"
```

Or edit `~/.gitconfig` and `.git/config` directly:

```ini
# ~/.gitconfig
[mate]
    worktreeRoot = ~/worktrees

# .git/config (per-repo override)
[mate]
    worktreeRoot = ../worktrees
```

| Key | Description | Default |
|-----|-------------|---------|
| `mate.worktreeRoot` | Where to create linked worktrees | sibling of main worktree |
| `mate.wrapperName` | Shell wrapper function name emitted by `git mate init` | `gm` |

## Non-goals

- GUI or TUI — this is a keyboard-first tool
- Bare repository workflows — regular clones only
- Replacing `git` — `git-mate` shells out to `git`, it doesn't reimplement it
