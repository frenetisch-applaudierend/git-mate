# git-mate

A Git CLI extension for my personal workflow around git worktrees.

Built for Bash/Zsh in Linux. Likely works on macOS too. Windows/Powershell is not supported, WSL should work fine though.

## Commands

**Note:** Some commands automatically `cd` into a different directory. This is only supported if you configure shell integration.

### `git-mate new <branch>`

Creates a new branch from the default branch (e.g. `main`) and switches to it.

By default this operates in the main worktree. Set `mate.defaultBranchMode = linked` if you want
`git-mate new` and `git-mate co` to create linked worktrees by default. Use explicit override flags to force
either mode for a single invocation. Optionally, you may specify a different parent ref.

Fetches from `origin` before branching by default. Use `--no-fetch` to skip, or set `mate.fetch = false` in git config to skip permanently.

```bash
git-mate new feature/login          # checkout in main worktree
git-mate new feature/login -w       # same as --linked-worktree
git-mate new feature/login -m --from v2.1.0  # same as --main-worktree
git-mate new feature/login --no-fetch                   # skip fetch
```

### `git-mate checkout <branch>` (alias: `co`)

Checks out an existing branch — local or remote. If the branch already exists in a worktree, navigates there instead.

```bash
git-mate checkout feature/login           # checkout in main worktree
git-mate co feature/login                 # same, using the alias
git-mate checkout feature/login -w        # same as --linked-worktree
git-mate checkout feature/login -m        # same as --main-worktree
```

### `git-mate move`

Moves the currently checked out branch out of the main worktree and into its own linked worktree.

This only works when:

- you are in the main worktree
- the current branch is not the default branch

If the main worktree has local changes, use `--stash` to move them too. This stashes tracked and
untracked changes before the move, creates the linked worktree, then restores the stash there.

```bash
git-mate move
git-mate move --stash
```

### `git-mate finish [<branch>]`

You're done with a branch. Removes the worktree (if linked) or switches to the default branch (if in main worktree), then deletes the local branch ref.

```bash
git-mate finish                     # finish current branch/worktree
git-mate finish feature/login       # finish a specific branch from anywhere
```

### `git-mate sync`

Fetches all remotes and prunes stale remote-tracking references, then:

- Fast-forwards other local branches whose upstream is still present and has no diverged commits
- Auto-deletes local branches whose remote was deleted (if they have no unpushed commits and a clean working tree)
- Pulls the current branch if an upstream is configured

```bash
git-mate sync                       # fetch + prune, then pull
git-mate sync --rebase              # pull with --rebase
git-mate sync --ff-only             # pull with --ff-only
```

## Installation

```bash
cargo install git-mate
```

### Shell Integration

Optionally add this to your shell config for shell completion and automatic `cd` support:

```bash
# ~/.zshrc  or  ~/.bashrc
eval "$(command git-mate init zsh)"   # zsh
eval "$(command git-mate init bash)"  # bash
```

Direct `git-mate ...` auto-`cd` support is always included in that generated shell snippet.

`git mate ...` auto-`cd` is separate and off by default. Enable it with:

```bash
git config --global mate.gitAutoCd true
```

If you want a safer mode, use this instead:

```bash
git config --global mate.gitAutoCd if-safe
```

Then reload your shell so `git-mate init` can emit the optional `git()` wrapper too.

With `mate.gitAutoCd=true`, the generated wrapper overrides `git` in your interactive shell, does
not preserve aliases, and bypasses any existing `git()` shell function by calling `command git`
directly. If that breaks another customization, disable it with:

```bash
git config --global --unset mate.gitAutoCd
```

Then reload your shell. If you pasted the generated shell code manually instead of using
`eval "$(command git-mate init ...)"`, remove the generated `git()` block as well.

With `mate.gitAutoCd=if-safe`, the generated shell code skips the wrapper when `git` is already a
shell function and prints a warning on shell startup so you can see why `git mate ...` auto-`cd`
was not enabled.

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
| `mate.fetch` | `false` / `no` / `off` / `0` | Disable automatic fetch in `git-mate new` |
| `mate.defaultBranchMode` | `main` / `linked` | Default target for `git-mate new` and `git-mate checkout` |
| `mate.gitAutoCd` | `true` / `yes` / `on` / `1` / `if-safe` | Make `git-mate init` emit an optional `git()` wrapper so `git mate ...` can auto-`cd`; `if-safe` skips the wrapper when `git` is already a function |
