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

Fetches from `origin` before branching by default. Use `--no-fetch` to skip, or set `mate.new.fetch = false` in git config to skip permanently.
The fetch is skipped automatically when starting from the current HEAD or a local branch, since it could not change the starting point.

`--from .` starts from the current HEAD: the branch you are on, or the commit when HEAD is detached.
Set `mate.new.from` to change the starting point used when `--from` is omitted (`default`, `current`, or any ref).

Uncommitted changes (tracked and untracked) follow you when you start from the branch checked out in
your current worktree: they move into the new branch and the current worktree is left clean. If the
starting point is checked out in *another* worktree with uncommitted changes, `git mate new` fails
unless you choose explicitly:

- `--take-changes` brings those changes into the new branch (they are stashed in the source worktree and
  reapplied in the destination; if reapplying fails, the stash entry is kept)
- `--start-fresh` starts from the committed state and leaves uncommitted changes where they are

Set `mate.new.changes` to `take` or `startFresh` to make either choice the default (`auto`, the default,
is the behavior described above). The flags override the config.

Ignored local config files (such as `.env.local`) are copied from the starting point's worktree into the
new branch's worktree, whichever option you choose. Use `--no-copy-config` (or set
`mate.new.copyConfig = false`) to skip this; `--copy-config` overrides the config. When the starting
point is not checked out in any worktree, nothing is transferred.

With `--json`, `git mate new` prints a single JSON document on stdout describing the new branch
(`branch`, `from`, `worktree`, `path`, `fetched`, `upstream`, `carriedFrom`, `copiedConfigFiles`).

```bash
git mate new feature/login          # checkout in main worktree
git mate new feature/login -w       # same as --linked-worktree
git mate new feature/login -m --from v2.1.0  # same as --main-worktree
git mate new feature/login --no-fetch        # skip fetch
git mate new feature/login -w --from .       # branch off the current branch, taking your changes along
git mate new feature/login -w --start-fresh  # leave uncommitted changes behind
```

### `git mate checkout <branch>` (alias: `co`)

Checks out an existing branch — local or remote.

Without `-m` or `-w`, this follows `mate.defaultBranchMode` for branches that are not checked out yet.
If the branch is already checked out in any worktree, it navigates there instead. The default branch
still implies `-m`, regardless of `mate.defaultBranchMode`.

Use `-m` or `-w` to make the destination explicit:

- `-m` ensures the branch ends up in the main worktree
- `-w` ensures the branch ends up in a linked worktree

When relocating a dirty branch between worktrees, add `--stash` to stash changes in the source
worktree and reapply them in the destination. If reapplying fails, the stash entry is kept.

The default branch is only allowed in the main worktree, so `-w` rejects it.

```bash
git mate checkout feature/login                 # follow default placement, or jump to where it's already checked out
git mate co feature/login                       # same, using the alias
git mate checkout -w                            # move the current branch into a linked worktree
git mate checkout -m                            # move the current branch into the main worktree
git mate checkout feature/login -w              # ensure the branch is in a linked worktree
git mate checkout feature/login -m              # ensure the branch is in the main worktree
git mate checkout feature/login -w --stash      # move dirty changes from main to linked
git mate checkout feature/login -m --stash      # move dirty changes from linked to main
```

### `git mate finish [<branch>]`

You're done with a branch. Removes the worktree (if linked) or switches to the default branch (if in main worktree), then deletes the local branch ref.

```bash
git mate finish                     # finish current branch/worktree
git mate finish feature/login       # finish a specific branch from anywhere
```

### `git mate sync`

Fetches all remotes and prunes stale remote-tracking references, then:

- Fast-forwards other local branches whose upstream is still present
- Resolves other local branches that diverged from their upstream: resets them if the remote was rewritten (force-pushed) and they have no local work, otherwise merges the upstream in when that is conflict-free; branches with conflicts or a dirty worktree are left untouched
- Auto-deletes local branches whose remote was deleted (if they have no unpushed commits and a clean working tree)
- Pulls the current branch if an upstream is configured
- Optionally merges the default branch into the current branch

```bash
git mate sync                       # fetch + prune, then pull
git mate sync --rebase              # pull with --rebase
git mate sync --ff-only             # pull with --ff-only (also skips diverged branches)
git mate sync --diverged=skip       # leave diverged branches untouched (or: git config mate.divergedStrategy skip)
git mate sync --merge               # also merge the default branch into the current branch
git mate sync --no-merge            # skip auto-merge for this run
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
| `mate.autoMerge` | `true` / `yes` / `on` / `1`, `false` / `no` / `off` / `0` | Auto-merge the default branch during `git mate sync` |
| `mate.defaultBranchMode` | `main` / `linked` | Default target for `git mate new` and `git mate checkout` |
| `mate.new.from` | `default` / `current` / any ref | Starting point for `git mate new` when `--from` is omitted (default: `default`) |
| `mate.new.changes` | `auto` / `take` / `startFresh` | What `git mate new` does with uncommitted changes when neither `--take-changes` nor `--start-fresh` is given (default: `auto`) |
| `mate.new.copyConfig` | boolean | Copy ignored local config files into the new branch's worktree (default: `true`) |
| `mate.new.fetch` | boolean | Fetch from `origin` before `git mate new` branches from a remote ref, tag or commit (default: `true`) |
| `mate.shellIntegration` | `false` / `true` / `force` | Control the `git()` shell wrapper (default: `false`) |
