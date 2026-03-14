# git-mate

A Git CLI extension for my personal workflow around git worktrees.

Built for Bash/Zsh in Linux. Likely works on macOS too. Windows/Powershell is not supported, WSL should work fine.

## Installation

```bash
cargo install git-mate
```

### Shell Integration

Optionally add this to your shell config for completion and automatic `cd` support:

```bash
# ~/.zshrc  or  ~/.bashrc
eval "$(git mate init zsh)"   # zsh
eval "$(git mate init bash)"  # bash
```

## Commands

The installed binary is named `git-mate`, which means git's extension mechanism picks it up and allows it to be called using `git mate`.
Following are the supported commands.

**Note:** Some commands automatically `cd` into a different directory. This is only supported if you configure shell integration.

### `git mate new <branch>`

Creates a new branch from the default branch (e.g. `main`) and switches to it.

Choose to either check the branch out in the main worktree, or create a new linked worktree for it. Optionally,
you may specify a different parent ref.

```bash
git mate new feature/login          # checkout in main worktree
git mate new feature/login -w       # create a linked worktree
git mate new feature/login -w --from v2.1.0  # branch from a specific ref
```

### `git mate checkout <branch>`

Checks out an existing branch — local or remote. If the branch already exists in a worktree, navigates there instead.

```bash
git mate checkout feature/login           # checkout in main worktree
git mate checkout feature/login -w        # create a linked worktree and cd into it
```

### `git mate finish [<branch>]`

You're done with a branch. Depending on where the branch is checked out:

- If it's in a linked worktree: removes the worktree and navigates back to the main worktree
- If it's checked out in the main worktree: switches to the default branch

Optionally pass `--delete-branch` to also delete the local branch. Git will refuse if the branch has not been merged.

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

## Shorthands

You may configure shorthands for both the executable and for the individual commands. By default no shorthands are
registered. To add a shorthand for the executable add it to your git configuration:

```bash
git config --global mate.shorthand "gm" # enables e.g. `gm checkout feature/login`
```

For the commands add it to the relevant command config, e.g. for checkout:

```bash
git config --global mate.checkout.shorthand "co" # enables `git mate co` and if configured `gm co`
```

## Worktree location

By default, worktrees are created under a root directory named after the repo:

```
~/worktrees/
  my-repo/
    feature-login/    ← worktree for feature/login
    feature-auth/     ← worktree for feature/auth
```

Override the root path in the git configuration:

```bash
# Set global defaults
git config --global mate.worktreeRoot "~/worktrees"

# Override for a specific repo (run inside that repo)
git config mate.worktreeRoot "../worktrees"
```
