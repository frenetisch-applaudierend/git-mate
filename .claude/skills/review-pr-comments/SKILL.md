---
name: review-pr-comments
description: Use this skill when asked to review, respond to, or address pull request review comments. Invoke whenever the user says things like "respond to PR comments", "address the review feedback", "handle the PR review", "fix the issues from the PR review", "look at the PR comments", "deal with review feedback", "go through the review", or any time they want action taken on code review comments in a GitHub pull request. Also trigger when the user asks you to "fix what the reviewer said" or "take care of the feedback". Use this skill proactively — if the user asks you to work on a PR branch and there are open review threads, consider handling them.
version: 1.0.0
---

# Review PR Comments

This skill helps you work through pull request review comments systematically: fetch them, analyze each one carefully, and take the right action — fixing the code or replying with a clear explanation.

## Available scripts

| Script                                                         | Purpose                                                             |
| -------------------------------------------------------------- | ------------------------------------------------------------------- |
| `scripts/pr-comments [PR_NUMBER]`                              | List unresolved threads; caches data to `/tmp/pr-threads-<PR>.json` |
| `scripts/pr-reply <number\|thread-id> "<message>" [PR_NUMBER]` | Post a reply to a thread                                            |
| `scripts/pr-resolve <number\|thread-id\|all> [PR_NUMBER]`      | Mark a thread as resolved                                           |

## Workflow

### Step 1: Fetch the comments

Run `scripts/pr-comments` first. It lists all unresolved review threads and prints them numbered for easy reference. The cache it writes is required by the other scripts.

### Step 2: Analyze each thread carefully

For each unresolved thread, before deciding anything:

1. **Read the source file** at the indicated path and line number. You need the surrounding context — the function, the module, the pattern — not just the flagged line.
2. **Understand what the reviewer actually wants.** Comments are often terse; read between the lines. Is this a blocking objection or a gentle suggestion? Is there an implied preference about style or approach?
3. **Classify** it:
   - **Code change needed** — the reviewer wants something modified, renamed, refactored, or added
   - **Question or discussion** — they're asking why something was done a certain way, or raising a concern for discussion
   - **Nitpick / style** — minor aesthetic preference; decide whether it's worth addressing
   - **Ambiguous** — the intent isn't clear enough to act on without clarification

The quality of your analysis determines the quality of the outcome. Don't rush this step.

### Step 3: Address each thread

**Code change needed:**

1. Implement the fix, respecting the existing code style and conventions.
2. Reply to the thread: `scripts/pr-reply <N> "Fixed in <file>: <one-sentence description of what changed>"`
3. Resolve: `scripts/pr-resolve <N>`

**Question or discussion:**

The author knows the codebase and intent better than you do — don't post a reply to the reviewer without first checking with them. Present your read of the comment and a draft reply, then wait for their confirmation or corrections before posting.

1. Present to the user: your interpretation of the reviewer's concern, and a draft reply (explaining the reasoning or acknowledging the suggestion).
2. Wait for the user to confirm, correct, or refine the reply.
3. Post the approved reply: `scripts/pr-reply <N> "<reply>"`
4. Resolve if the matter is settled: `scripts/pr-resolve <N>`

**Ambiguous:**

Same principle — don't post a clarifying question to the reviewer without the user's input first. The user may already know what the reviewer meant.

1. Present your interpretation (or lack thereof) to the user and ask how they'd like to respond.
2. Once you have their answer, either act on it directly (if it resolves the ambiguity) or post the clarifying question they approve: `scripts/pr-reply <N> "<question>"`
3. Leave the thread open so the reviewer can respond.

**Mixed (implied change + question):**

- Make the change if it's clearly warranted, reply explaining what you did, then resolve.

### Step 4: Verify

Run `scripts/pr-comments` again at the end to confirm no threads remain unresolved.

## Tone for replies

Keep replies professional and brief. When you've made a fix, say what you changed in one sentence. When declining a suggestion, acknowledge it respectfully and explain the reasoning concisely. Don't over-explain, and don't be defensive.

## Things to avoid

- Don't resolve a thread silently — always reply first, so reviewers know what happened
- Don't guess at ambiguous comments; ask instead
- Don't change code style arbitrarily while addressing a comment — stay consistent with the surrounding code
- Don't address only the literal words of a comment while missing the underlying concern
