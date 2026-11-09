# Using git-mate

`git mate` is a git extension that manages branches and their worktrees. Use it to create,
switch, relocate, finish, sync and update branches. Use plain git for everything that
stays on one branch: status, diff, log, add, commit, stash, push of the current branch.

## Rules

- **Always pass `--json`.** Every command then prints exactly one JSON document on stdout:
  `{"ok": true, "command": "...", "schemaVersion": 1, "data": {...}}`, or on failure
  `{"ok": false, ..., "error": {"message": "...", "code": "..."}}` (sometimes with `data`
  describing what did happen). Human-readable text goes to stderr; don't parse it.
- **Decide from `error.code`, not the message.** The ones that need a decision:
  - `uncommitted_changes`: `data.retryWith` lists the flags that resolve it (e.g.
    `--take-changes` brings the changes along, `--start-fresh` leaves them in
    `data.path`). Ask the user which one, then retry with it. Empty `retryWith`: the user
    has to commit or stash in `data.path` first. `--force` (from `finish`) discards them:
    only with the user's explicit consent.
  - `unpushed_commits`: `finish` would delete commits that exist nowhere else. Ask before
    retrying with `--force`.
  - `branch_exists`, `branch_not_found`, `invalid_branch_name`, `default_branch`: report
    and ask; don't guess another name.
  - `stash_reapply_failed`: the command succeeded, but the carried-over changes are still
    in the stash. Tell the user.
- **git-mate never prompts you.** With `--json` (or without a terminal) every question is
  answered by flags or left undecided and reported. Choose flags deliberately.
- **Directory changes don't reach your shell.** The user's shell integration moves *them*
  into the right worktree; your shell has no such integration. After `new`, `checkout` or
  `finish`, read `data.path` and `cd` there yourself before touching files. Otherwise you
  keep working in the old worktree.
- **Don't manage these worktrees with plain git.** No `git worktree add/remove`, no
  `git branch -d/-D` on branches git-mate created; git-mate places worktrees under
  `mate.worktreeRoot` and cleans them up itself.
- **Leave Claude Code's own worktrees alone** (`.claude/worktrees/`). git-mate is for the
  user's branches.
- **Don't repair failures with raw git.** If a command fails or leaves a branch under
  "needs attention", report `error.message` / the branch's `reason` to the user. Don't
  `reset --hard`, force-remove worktrees or force-push on your own.

## Which command

| The user wants to… | Command |
|---|---|
| start work on a new branch | `git mate new <branch> --json` |
| switch to an existing (local or remote) branch | `git mate checkout <branch> --json` |
| move a branch into / out of a linked worktree | `git mate checkout [<branch>] -w` / `-m` |
| bring all local branches up to date with the remote | `git mate sync --json` |
| rebase a branch (or a stack) onto the branch it is based on | `git mate update --json` |
| drop a branch they are done with | `git mate finish [<branch>] --json` |

## Commands

### `new <branch>`

Creates the branch from the default branch (fetching first) and switches to it.

- `-w` puts it in a linked worktree, `-m` in the main worktree; without either, the user's
  `mate.defaultBranchMode` decides. Prefer omitting both unless the user said which.
- `--from <ref>` picks another starting point; `--from .` branches off the current HEAD.
- Uncommitted changes in the current worktree follow along when starting from its branch.
  If the starting point has uncommitted changes in *another* worktree, the command fails
  until you pass `--take-changes` (bring them) or `--start-fresh` (leave them). Ask the
  user which one if it isn't obvious.
- Ignored config files (`.env.local` and similar) are copied into the new worktree
  (`data.copiedConfigFiles`); `--no-copy-config` skips that.
- Result: `data.branch`, `data.path`, `data.worktree` (`main`/`linked`). `cd` to
  `data.path`.

### `checkout <branch>` (alias `co`)

Switches to an existing branch, fetching if it isn't known locally. If it is already
checked out somewhere, it just reports that worktree (`data.action: "navigated"`).

- `-w` / `-m` make sure it ends up in a linked / the main worktree, moving it if needed.
  Without a branch name they relocate the current branch with its uncommitted changes.
- `--take-changes` / `--start-fresh` as for `new`, when moving a branch out of another
  worktree that has uncommitted changes.
- Result: `data.path`, `data.action` (`navigated`, `switched`, `created`, `moved`),
  `data.stashed`. `cd` to `data.path`.

### `sync`

Fetches and prunes all remotes, fast-forwards every local branch, resolves branches that
diverged from their upstream (merge by default; `--diverged=rebase` or `--diverged=skip`),
and finds local branches whose remote was deleted.

- `--dry-run` previews without changing anything. Use it first when the user only asked
  what sync would do.
- Branches whose remote is gone are only reported with `--json` (their `recommended`
  choice and possible `choices`). To act on them: `--apply-recommended` deletes the ones
  already merged into the default branch; `--delete-pruned` deletes all of them, including
  unmerged ones. Only use `--delete-pruned` when the user asked for it.
- Result: `data.branches[]` (`branch`, `action`, `reason`); report every branch with a
  `reason`. `data.currentBranch.behindBase` means the current branch is behind the branch
  it is based on; suggest `git mate update`.

### `update [<branch>...]`

Rebases branches onto the branch they are based on (the default branch, or the branch
they are stacked on; detected from the commit graph). Conflict-free or not at all.

- No arguments: the current branch. `--all`: every branch, stacks in order.
- `--merge` merges the base in instead of rebasing. `--onto <branch>` overrides the base.
- `--dry-run` previews. `--sync` runs `sync` first.
- `--stack` also updates branches the named branch is stacked on; without it, a parent
  that is behind is only reported (`parentBehind`).
- `--push` pushes afterwards when that is a fast-forward or creates the remote branch. A
  rebased branch that was pushed before then reports `push: "needs-force-push"`.
  `--push=force` force-pushes (with lease). Only use `--push=force` when the user asked
  for a force-push.
- Result: `data` lists one entry per branch: `branch`, `action` (`rebased`, `merged`,
  `up-to-date`, `skipped`, `failed`), `base`, `reason`/`hint` when skipped or failed,
  `stashKept: true` when uncommitted changes could not be reapplied and remain in the
  stash (tell the user).

### `finish [<branch>]`

Deletes a branch the user is done with: removes its linked worktree, or switches the main
worktree to the default branch, then deletes the local branch.

- Refuses when the branch has unpushed commits (`unpushed_commits`) or its linked
  worktree has uncommitted changes (`uncommitted_changes`). `--force` discards both.
  Never pass `--force` without the user's explicit consent.
- `data.path` is the worktree the branch was in. If it was linked, it no longer exists:
  if you were inside it, `cd` to `data.mainWorktree`.
