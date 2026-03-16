# git-mate

A Git CLI extension for my personal workflow around git worktrees.

Built for Bash/Zsh in Linux. Likely works on macOS too. Windows/Powershell is not supported, WSL should work fine though.

## Commands

**Note:** Some commands automatically `cd` into a different directory. This is only supported if you configure shell integration.

### `mate new <branch>`

Creates a new branch from the default branch (e.g. `main`) and switches to it.

Choose to either check the branch out in the main worktree, or create a new linked worktree for it. Optionally,
you may specify a different parent ref.

```bash
mate new feature/login          # checkout in main worktree
mate new feature/login -w       # create a linked worktree
mate new feature/login -w --from v2.1.0  # branch from a specific ref
```

### `mate checkout <branch>`

Checks out an existing branch — local or remote. If the branch already exists in a worktree, navigates there instead.

```bash
mate checkout feature/login           # checkout in main worktree
mate checkout feature/login -w        # create a linked worktree and cd into it
```

### `mate finish [<branch>]`

You're done with a branch. Depending on where the branch is checked out:

- If it's in a linked worktree: removes the worktree and navigates back to the main worktree
- If it's checked out in the main worktree: switches to the default branch

```bash
mate finish                     # finish current branch/worktree
mate finish feature/login       # finish a specific branch from anywhere
```

### `mate sync`

Fetches all remotes and prunes stale local tracking references, then pulls the current branch if an upstream is configured.

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
