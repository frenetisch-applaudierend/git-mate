# git-mate

A Git CLI extension for my personal workflow around git worktrees.

Built for Bash/Zsh in Linux. Likely works on macOS too. Windows/Powershell is not supported, WSL should work fine though.

## Commands

**Note:** Some commands automatically `cd` into a different directory. This is only supported if you configure shell integration.

### `mate new <branch>`

Creates a new branch from the default branch (e.g. `main`) and switches to it.

Choose to either check the branch out in the main worktree, or create a new linked worktree for it. Optionally,
you may specify a different parent ref.

Fetches from `origin` before branching by default. Use `--no-fetch` to skip, or set `mate.fetch = false` in git config to skip permanently.

```bash
mate new feature/login          # checkout in main worktree
mate new feature/login -w       # create a linked worktree
mate new feature/login -w --from v2.1.0  # branch from a specific ref
mate new feature/login --no-fetch        # skip fetch
```

### `mate checkout <branch>` (alias: `co`)

Checks out an existing branch — local or remote. If the branch already exists in a worktree, navigates there instead.

```bash
mate checkout feature/login           # checkout in main worktree
mate co feature/login                 # same, using the alias
mate checkout feature/login -w        # create a linked worktree and cd into it
```

### `mate move`

Moves the currently checked out branch out of the main worktree and into its own linked worktree.

This only works when:

- you are in the main worktree
- the current branch is not the default branch

If the main worktree has local changes, use `--stash` to move them too. This stashes tracked and
untracked changes before the move, creates the linked worktree, then restores the stash there.

```bash
mate move
mate move --stash
```

### `mate finish [<branch>]`

You're done with a branch. Removes the worktree (if linked) or switches to the default branch (if in main worktree), then deletes the local branch ref.

```bash
mate finish                     # finish current branch/worktree
mate finish feature/login       # finish a specific branch from anywhere
```

### `mate sync`

Fetches all remotes and prunes stale remote-tracking references, then:

- Fast-forwards other local branches whose upstream is still present and has no diverged commits
- Auto-deletes local branches whose remote was deleted (if they have no unpushed commits and a clean working tree)
- Pulls the current branch if an upstream is configured

```bash
mate sync                       # fetch + prune, then pull
mate sync --rebase              # pull with --rebase
mate sync --ff-only             # pull with --ff-only
```

## Installation

```bash
cargo install git-mate
```

### Shell Integration

Optionally add this to your shell config for shell completion and automatic `cd` support:

```bash
# ~/.zshrc  or  ~/.bashrc
eval "$(command mate init zsh)"   # zsh
eval "$(command mate init bash)"  # bash
```

### Worktree location

Worktrees are created under a root directory, organized by repository and branch names. Set the root path in the git configuration:

```bash
# Set global defaults
git config --global mate.worktreeRoot "~/worktrees"

# Override for a specific repo (run inside that repo)
git config mate.worktreeRoot "../worktrees"
```

### Configuration reference

| Key | Values | Effect |
|-----|--------|--------|
| `mate.worktreeRoot` | path | Root directory for linked worktrees |
| `mate.fetch` | `false` / `no` / `off` / `0` | Disable automatic fetch in `mate new` |
