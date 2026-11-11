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

Checks out an existing branch — local or remote. If the branch is not known locally (neither as a
local branch nor as a remote-tracking branch), the remotes are fetched first, so a branch someone just
pushed can be checked out right away.

Without `-m` or `-w`, this follows `mate.defaultBranchMode` for branches that are not checked out yet.
If the branch is already checked out in any worktree, it navigates there instead. The default branch
still implies `-m`, regardless of `mate.defaultBranchMode`.

Use `-m` or `-w` to make the destination explicit:

- `-m` ensures the branch ends up in the main worktree
- `-w` ensures the branch ends up in a linked worktree

When `-m` or `-w` relocates a branch out of the worktree you are in, its uncommitted changes (tracked
and untracked) come along: they are stashed in the source worktree and reapplied in the destination. If
reapplying fails, the stash entry is kept. If the branch is relocated out of *another* worktree with
uncommitted changes, `git mate checkout` fails unless you choose explicitly:

- `--take-changes` brings those changes along
- `--start-fresh` leaves them in the main worktree, which switches to the default branch (only when
  moving out of the main worktree; a linked worktree is removed, so its changes cannot stay behind)

Ignored local config files (such as `.env.local`) are copied into the destination either way, without
overwriting files that already exist there.

The default branch is only allowed in the main worktree, so `-w` rejects it.

With `--json`, `git mate checkout` prints a single JSON document on stdout describing the result
(`branch`, `worktree`, `path`, `action` — `navigated`, `switched`, `created` or `moved` — `stashed`
and `fetched`).

```bash
git mate checkout feature/login                    # follow default placement, or jump to where it's already checked out
git mate co feature/login                          # same, using the alias
git mate checkout -w                               # move the current branch and its uncommitted changes into a linked worktree
git mate checkout -m                               # move the current branch and its uncommitted changes into the main worktree
git mate checkout feature/login -w                 # ensure the branch is in a linked worktree
git mate checkout feature/login -m                 # ensure the branch is in the main worktree
git mate checkout feature/login -m --take-changes  # bring changes along from a linked worktree you're not in
git mate checkout feature/login -w --start-fresh   # leave uncommitted changes in the main worktree
```

### `git mate finish [<branch>]`

You're done with a branch. Removes the worktree (if linked) or switches to the default branch (if in main worktree), then deletes the local branch ref.

With `--json`, it prints a single JSON document on stdout (`branch`, plus `worktree` and `path` if the
branch was checked out, `switchedTo` if the main worktree switched branches, and `mainWorktree`).

Without `--force`, `finish` refuses to delete a branch with unpushed commits or to remove a linked
worktree with uncommitted changes. `--force` discards both.

If the worktree you are in was already removed (say, finished from another shell), `git mate finish`
recognizes the vanished directory, finishes whatever is left of its branch and takes you to the main
worktree (`alreadyFinished` is `true` in `--json` output when nothing was left). Other commands run
from a vanished worktree continue in the main worktree as well.

```bash
git mate finish                     # finish current branch/worktree
git mate finish feature/login       # finish a specific branch from anywhere
```

### `git mate overview`

Shows every local branch: where it is checked out (main worktree, a linked worktree, or
nowhere), whether that worktree has uncommitted changes or an unfinished rebase or merge, and
how the branch compares to its upstream (up to date, ahead, behind, diverged, remote gone, or
no upstream). The branch you are on is marked with `●`.

It only reads local state and never fetches, so ahead/behind counts are as of the last fetch;
run `git mate sync` to refresh them.

With `--json`, it prints `branches` (`branch`, `current`, `default`, `worktree`, `path`,
`dirty`, `operation` when checked out, and `upstream` with `name`, `state`, `ahead`,
`behind`, or `null` without an upstream) and `detached` (worktrees with a detached HEAD).

To run it as `git overview`, add a git alias:

```bash
git config --global alias.overview "mate overview"
```

### `git mate sync`

Fetches all remotes and prunes stale remote-tracking references, then:

- Fast-forwards every local branch whose upstream is still present, including the current one
- Resolves branches that diverged from their upstream: resets them if the remote was rewritten (force-pushed) and they have no local work, otherwise merges the upstream in when that is conflict-free (`--diverged=rebase` rebases local commits onto the upstream instead, also for branches that aren't checked out; `--diverged=skip` leaves them alone); branches with conflicts or uncommitted changes in the way are left untouched and listed under "Needs attention" — sync never leaves a half-done merge or rebase behind
- Leaves branches you rebased locally but haven't pushed yet alone, listing them as needing a force-push instead of merging the old commits back in
- Points out when the current branch is behind the branch it is based on (e.g. `main` moved on since you branched), suggesting `git mate update`; with `--json` this is `currentBranch.behindBase` (`base`, `count`, and `mergedParent` if the branch it was stacked on has been merged)
- Offers to delete local branches whose remote was deleted (if their working tree is clean; a branch with unpushed commits is only preselected for deletion when everything on it is already in the default branch): pick delete, keep or ask later per branch, with branches already merged into the default branch preselected for deletion and the rest for asking again next sync (`--apply-recommended` applies those preselections without asking; `--delete <branch>` and `--keep <branch>` decide for single branches without asking, leaving the others as they are unless combined with `--apply-recommended`)
- Offers the same for local branches without an upstream (never pushed, or pushed without `-u`): pick push (`git push -u`), delete, keep local or ask later per branch. Branches whose commits already landed in the default branch (e.g. squash-merged), and branches without commits of their own that haven't moved for a day (except the current branch), are preselected for deletion, the rest for asking again; a branch whose worktree has uncommitted changes can't be deleted. `--push <branch>` pushes one without asking. Kept branches are marked with `branch.<name>.mateLocal` and not offered again (`git config --unset branch.<name>.mateLocal` undoes that). The default branch, and repositories without a remote, are left out

```bash
git mate sync                       # fetch + prune, then bring every branch up to date
git mate sync --diverged=rebase     # rebase diverged branches onto their upstream instead of merging
git mate sync --diverged=skip       # only fast-forward; leave diverged branches untouched
git mate sync --delete feature/a --keep feature/b  # decide for these branches without the chooser
git mate sync --push feature/c      # push a branch that has no upstream yet, and track it
git config mate.sync.diverged rebase  # make rebase the default
```

With `--json`, sync doesn't ask: branches whose remote was deleted are listed with
`action: "deletion-candidate"`, branches without upstream with `action: "no-upstream"`, each
with a `detail`, their `recommended` action and `choices`, until a flag decides for them
(`pushed`, `deleted`, `kept`). A branch kept local is listed as `no-upstream` with reason
`kept local` and no `choices`. Branches named by `--delete`/`--keep`/`--push` that weren't
up for a decision are listed in `ignoredDecisions`; naming a branch that doesn't exist fails
before anything is fetched.

Sync never merges the default branch into your branches; `mate.autoMerge` and the
`--merge`, `--no-merge`, `--rebase` and `--ff-only` flags were removed. The
`mate.divergedStrategy` setting was renamed to `mate.sync.diverged`; the old name is still
read, with a warning, for now.

### `git mate update [<branch>...]`

Brings branches up to date with the branch they are based on: rebases them onto it (the
default) or, with `--merge`, merges it in. Without arguments it updates the current
branch; `--all` updates every local branch except the default branch.

The base is read from the commit graph, so it works for any branch, however it was created:

- A branch that contains commits of another local branch which the default branch doesn't
  have is **stacked** on that branch. When both have moved on since, the branch's reflog
  ("Created from …") decides which one is the parent.
- Any other branch is based on the **default branch**.
- If the branch it is stacked on was merged into the default branch since (also squash- or
  rebase-merged), the branch is rebased onto the default branch and the parent's commits
  are dropped.
- If several branches could be the base, the branch is skipped; name it with `--onto`.

All bases are detected before anything changes and stacked branches are updated after
their parents, so a whole stack moves along in one run. A rebase or merge only goes
through if it is conflict-free; otherwise the branch is left as it was and listed under
"Needs attention". Uncommitted changes in a branch's worktree are stashed and reapplied.
Branches that aren't checked out are updated without touching any worktree (rebasing them
uses `git replay`, Git 2.44+).

**Stacks.** When a branch you name is stacked on branches of which one is behind its own
base (say `foo` on `bar`, and `main` moved on), `update` asks whether to update those
first. `--stack` does so without asking, `--no-stack` never does; without a terminal the
stack is left alone and reported (`parentBehind` in `--json`).

**Pushing.** `--push` pushes each updated (or already up-to-date) branch afterwards when
that is a fast-forward or creates the remote branch. It pushes where `git push` would: to
`branch.<name>.pushRemote` or `remote.pushDefault` if set, else to the upstream (setting
`origin` as upstream when there is none yet). A branch that is already merged and whose
upstream was deleted is not pushed, so the remote branch isn't brought back
(`push: "skipped"`). A rebased
branch that was pushed before needs a force-push: `--push` only reports it, `--push=ask`
asks first, `--push=force` just does it. A force-push uses `--force-with-lease` against
what the last fetch saw, so it fails rather than overwrite commits someone else pushed
meanwhile.

**Syncing first.** `update` works with the local base branches. `--sync` runs
`git mate sync` first, so they match the remote (with `--json`, sync's result is included
as `sync`).

With `--json`, it prints a single JSON document with one entry per branch: `branch`,
`action` (`rebased`, `merged`, `up-to-date`, `skipped` or `failed`), `base`, `baseSource`
(`default`, `stacked`, `mergedParent` or `onto`), `mergedParent` (the merged parent whose
commits were dropped), `strategy`, `behind`, `stashKept` (`true` when uncommitted changes
could not be reapplied and were left in the stash), `parentBehind`, `reason` / `hint` when
skipped or failed, and with `--push`, `push` (`pushed`, `forced`, `up-to-date`,
`needs-force-push`, `declined`, `no-remote`, `skipped` or `failed`) and `pushReason`.

```bash
git mate update                     # rebase the current branch onto its base
git mate update --merge             # merge the base in instead (--strategy=merge)
git mate update feature/a feature/b # update these branches (parents first)
git mate update --all               # update every branch, whole stacks included
git mate update --onto main         # rebase onto main, whatever the detected base
git mate update --all --dry-run     # preview
git mate update --stack             # also update the branches it is stacked on
git mate update --sync --push       # sync first, push afterwards (no force-push)
git mate update --push=ask          # ask before force-pushing a rebased branch
git config mate.update.strategy merge  # merge by default
git config mate.update.sync true       # always sync first
```

## JSON output

With `--json`, every command except `init` and `skill` prints exactly one JSON document on stdout:

```json
{"ok": true, "command": "new", "schemaVersion": 1, "data": {...}}
{"ok": false, "command": "new", "schemaVersion": 1, "error": {"message": "...", "code": "..."}, "data": {...}}
```

On failure, `error.code` tells what went wrong, and `data` (when present) gives details:

| Code | Meaning | `data` |
|------|---------|--------|
| `uncommitted_changes` | Uncommitted changes are in the way | `path` (the worktree), `retryWith` (flags that resolve it, in place of `--take-changes`/`--start-fresh` if given; empty: commit or stash first) |
| `unpushed_commits` | `finish` would lose unpushed commits | `branch`, `retryWith` |
| `branch_not_found` | The branch doesn't exist (`checkout` also looked on the remotes) | `branch` |
| `branch_exists` | `new` was given a branch that already exists | `branch` |
| `invalid_branch_name` | Not a valid branch name | `branch` |
| `default_branch` | Not possible with the default branch (`finish` it, or put it in a linked worktree) | `branch` |
| `stash_reapply_failed` | `new` succeeded, but carried-over changes could not be reapplied; they are kept in the stash | the command's usual result |
| `unsupported` | `--json` isn't supported by this command | |
| `error` | Anything else | |

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

### AI coding agents

git-mate ships instructions for AI coding agents: when to use which command, why to pass
`--json`, and that the automatic `cd` doesn't reach an agent's shell. `git mate skill`
prints them. To make them available to Claude Code as a skill:

```bash
git mate skill install            # for all your projects (~/.claude/skills/git-mate)
git mate skill install --project  # for this repository (.claude/skills/git-mate)
git mate skill install --dir <dir>  # into another skills directory
```

The installed skill only holds its description and loads the instructions from
`git mate skill` whenever it is used, so it stays current when you upgrade git-mate. It
pre-approves `git mate skill` via `allowed-tools`; if your settings disable shell
execution in skills (`disableSkillShellExecution`), the skill can't load.

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
| `mate.defaultBranchMode` | `main` / `linked` | Default target for `git mate new` and `git mate checkout` |
| `mate.new.from` | `default` / `current` / any ref | Starting point for `git mate new` when `--from` is omitted (default: `default`) |
| `mate.new.changes` | `auto` / `take` / `startFresh` | What `git mate new` does with uncommitted changes when neither `--take-changes` nor `--start-fresh` is given (default: `auto`) |
| `mate.new.copyConfig` | boolean | Copy ignored local config files into the new branch's worktree (default: `true`) |
| `mate.new.fetch` | boolean | Fetch from `origin` before `git mate new` branches from a remote ref, tag or commit (default: `true`) |
| `mate.sync.diverged` | `merge` / `rebase` / `skip` | How `git mate sync` handles branches that diverged from their upstream when `--diverged` is omitted (default: `merge`) |
| `mate.update.strategy` | `rebase` / `merge` | How `git mate update` brings in the base when `--strategy` (or `--rebase` / `--merge`) is omitted (default: `rebase`) |
| `mate.update.stack` | `ask` / `true` / `false` | Whether `git mate update` also updates the branches a branch is stacked on when they are behind (default: `ask`) |
| `mate.update.push` | `false` / `ff` / `ask` / `force` | Whether and how `git mate update` pushes afterwards (default: `false`) |
| `mate.update.sync` | boolean | Run `git mate sync` before `git mate update` (default: `false`) |
| `mate.shellIntegration` | `false` / `true` / `force` | Control the `git()` shell wrapper (default: `false`) |
