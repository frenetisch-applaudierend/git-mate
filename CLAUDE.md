# git-mate — Agent Instructions

## Commit messages

This project uses **Conventional Commits** (https://www.conventionalcommits.org).
All commits — including those made by AI agents — MUST follow this format:

```
<type>[optional scope]: <description>

[optional body]

[optional footer]
```

### Types

| Type       | When to use                                       | Version bump |
|------------|---------------------------------------------------|--------------|
| `feat`     | New user-facing feature                           | minor        |
| `fix`      | Bug fix                                           | patch        |
| `docs`     | Documentation only                                | none         |
| `test`     | Adding or updating tests                          | none         |
| `refactor` | Code change that is neither a fix nor a feature   | none         |
| `perf`     | Performance improvement                           | patch        |
| `chore`    | Build process, dependency updates, tooling        | none         |
| `ci`       | CI/CD configuration                               | none         |

### Breaking changes

Append `!` after the type, or add `BREAKING CHANGE:` in the footer:
```
feat!: remove --ff-only flag from sync command
```
This triggers a **major** version bump.

### Examples

```
feat(new): add --no-track flag to skip upstream setup
fix(sync): handle repos with no commits gracefully
docs: add conventional commits guide to CLAUDE.md
test: add integration test for sync --rebase flag
chore: update assert_cmd to 2.2
```

### Why this matters

The release workflow (release-plz) reads these commit messages to:
1. Determine the correct semver bump for the next release
2. Populate CHANGELOG.md automatically

Non-conventional commits are silently ignored by the release tooling.

## Code structure

`mod.rs` files must never contain logic — only `pub mod`/`mod` declarations and `pub use` re-exports.
Put all implementation in named submodule files.

## Configuration keys

Settings that apply to a single command are named `mate.<command>.<key>`, where `<key>` matches
the flag whose default they set (e.g. `--diverged` ↔ `mate.sync.diverged`, `--no-fetch` ↔
`mate.new.fetch`). Only settings shared by several commands (e.g. `mate.defaultBranchMode`,
`mate.worktreeRoot`) live directly under `mate.`. When renaming a key, keep reading the old one
for a release with a deprecation warning.

## Build and test hygiene

Before committing, pushing, or creating a PR, always ensure:
1. `cargo build` succeeds with no errors
2. `cargo test` passes with no failures

Fix all build errors and test failures before proceeding, unless explicitly instructed otherwise.

## Tests

Integration tests must not assert on human-readable (stderr) output; only the `--json` output is canonical and may be asserted. Test behaviour (repo state) or JSON instead.
