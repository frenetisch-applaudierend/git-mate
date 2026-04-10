# git-mate

A Git CLI extension for my personal workflow around git worktrees.

Built for Bash/Zsh in Linux. Likely works on macOS too. Windows/Powershell is not supported, WSL should work fine though.

## Commands

**Note:** Some commands automatically `cd` into a different directory. This is only supported if you configure shell integration.

### `git mate new <branch>`

Creates a new branch from the default branch (e.g. `main`) and switches to it.

By default this operates in the main worktree. Set `mate.defaultBranchMode = linked` if you want
`git mate new` and `git mate co` to create linked worktrees by default. Use explicit override flags to force
either mode for a single invocation. Optionally, you may specify a different parent ref.

The default branch always stays in the main worktree, even when linked mode is configured.

Fetches from `origin` before branching by default. Use `--no-fetch` to skip, or set `mate.fetch = false` in git config to skip permanently.

```bash
git mate new feature/login          # checkout in main worktree
git mate new feature/login -w       # same as --linked-worktree
git mate new feature/login -m --from v2.1.0  # same as --main-worktree
git mate new feature/login --no-fetch        # skip fetch
```

### `git mate checkout <branch>` (alias: `co`)

Checks out an existing branch — local or remote. If the branch already exists in a worktree, navigates there instead.

The default branch is only allowed in the main worktree, so linked-worktree mode rejects it.

```bash
git mate checkout feature/login           # checkout in main worktree
git mate co feature/login                 # same, using the alias
git mate checkout feature/login -w        # same as --linked-worktree
git mate checkout feature/login -m        # same as --main-worktree
```

### `git mate move`

Moves a branch between the main worktree and its linked worktree.

Without an argument, it moves the current branch. With a branch name, it moves that specific
branch.

This only works when:

- the branch exists and is checked out in a worktree
- the branch is not the default branch
- the source worktree is clean
- the destination is safe to use (for example, a linked branch can only move back into a clean main
  worktree that is currently on the default branch)

Moving from main to linked creates the linked worktree at the configured `mate.worktreeRoot`.
Moving from linked back to main removes the linked worktree directory.

```bash
git mate move
git mate move feature/login
```

### `git mate finish [<branch>]`

You're done with a branch. Removes the worktree (if linked) or switches to the default branch (if in main worktree), then deletes the local branch ref.

```bash
git mate finish                     # finish current branch/worktree
git mate finish feature/login       # finish a specific branch from anywhere
```

### `git mate sync`

Fetches all remotes and prunes stale remote-tracking references, then:

- Fast-forwards other local branches whose upstream is still present and has no diverged commits
- Auto-deletes local branches whose remote was deleted (if they have no unpushed commits and a clean working tree)
- Pulls the current branch if an upstream is configured

```bash
git mate sync                       # fetch + prune, then pull
git mate sync --rebase              # pull with --rebase
git mate sync --ff-only             # pull with --ff-only
```

## Installation

```bash
cargo install git-mate
```

### Shell Integration

Optionally add this to your shell config for shell completion and automatic `cd` support.
The shell integration defines a `git()` wrapper function — enable it explicitly via git config:

```bash
# Choose one: 'true' (safe — skips if git is already a function) or 'force' (always define)
git config --global mate.shellIntegration true
```

Then source the init script in your shell config:

```bash
# ~/.zshrc  or  ~/.bashrc
eval "$(command git-mate init zsh)"   # zsh
eval "$(command git-mate init bash)"  # bash
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
| `mate.fetch` | `false` / `no` / `off` / `0` | Disable automatic fetch in `git mate new` |
| `mate.defaultBranchMode` | `main` / `linked` | Default target for `git mate new` and `git mate checkout` |
| `mate.shellIntegration` | `false` / `true` / `force` | Control the `git()` shell wrapper (default: `false`) |
