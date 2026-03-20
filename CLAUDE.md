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

## Build and test hygiene

Before committing, pushing, or creating a PR, always ensure:
1. `cargo build` succeeds with no errors
2. `cargo test` passes with no failures

Fix all build errors and test failures before proceeding, unless explicitly instructed otherwise.
