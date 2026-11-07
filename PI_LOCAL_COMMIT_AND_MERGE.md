# Pi Agent Instructions: Commit All Intended Work and Merge Local Branches — Never Push

## Mission

Finish the already-completed codebase work by creating local Git commits for all intended, safe project changes and integrating all relevant, unfinished **local branches** into the correct local integration branch. Verify the result and report the exact outcome.

**This is a local-only operation. Never push, publish, deploy, or otherwise send commits or project data to any remote service.** Do not stop after inspecting the repository or proposing commands; carry out the work end to end when it is safe.

Follow this document together with the project's existing instructions. If instructions conflict, preserve data, security, and the no-remote-push rule. Do not bypass safety controls to force completion.

---

## 1. Absolute Rules

1. **Never push remotely.** Do not run `git push`, `git push --tags`, `git push --mirror`, `git publish`, remote-sync commands, hosting-provider merge commands, deployment commands, or equivalent actions. Do not create or merge a pull/merge request. Do not transmit patches, source files, logs, credentials, or commit contents to remote services.
2. **Stay local.** Do not run `git fetch`, `git pull`, `git remote update`, or other network-sync operations. Use the existing local repository and its currently available local/remote-tracking references only. A remote-tracking reference may be stale; do not describe it as current remote state.
3. **Do not discard work.** Never use `git reset --hard`, `git clean -fdx`, forced checkout, blanket restore, destructive rebase, history rewriting, or other commands that can erase existing work. Do not amend or rewrite existing commits. Do not delete branches or tags.
4. **Protect user data and secrets.** Do not commit credentials, private keys, tokens, local `.env` files containing real secrets, user databases, uploads, logs, caches, machine-specific state, or other sensitive/runtime data merely because they are present. Use existing ignore rules and inspect any new or ignored files that may contain real project work. Do not print secret values in the report.
5. **Preserve unrelated changes.** Inspect every changed and untracked path before staging it. Commit all valid, intended project changes for this task and other clearly intended repository work, but do not indiscriminately stage unknown, sensitive, disposable, or unrelated files. Never overwrite or discard pre-existing user changes.
6. **Do not fake success.** Every commit, merge, test, and final status must be supported by actual command output. If an operation is blocked, record the reason and continue other safe independent work.
7. **Do not weaken repository controls.** Do not disable Git hooks, signing requirements, access checks, branch protections, tests, security checks, or other safeguards to force a commit or merge. The Pi-extension exception in Section 7 is narrow and does not authorize disabling these controls.
8. **Do not make unrelated code changes.** This task is for local commit and integration of the already-completed work. Fix merge conflicts only as needed to preserve correct behavior and integrate the branches safely. Avoid unrelated refactoring or feature work.

## 2. Inspect and Record the Starting State

Before changing Git state, inspect and record:

- The repository root using `git rev-parse --show-toplevel`.
- The current branch and status using `git branch --show-current` and `git status --short --branch`.
- Existing changes using `git diff`, `git diff --cached`, and their summary forms.
- Untracked files, ignored files that appear relevant, and nested repositories or submodules.
- Local branches and their relationships using `git branch --list`, `git log --oneline --graph --decorate --all`, and suitable `git merge-base` / ancestry checks.
- Existing merge, rebase, cherry-pick, or revert state before starting another operation.
- Additional worktrees using `git worktree list --porcelain`.
- The repository's commit identity and relevant commit settings without changing global configuration.
- Existing project-specific instructions and applicable `.gitignore` rules.

Do not read or expose secret values. Inspect file names, change types, and safe diffs first; if a file may contain credentials or personal data, avoid printing its contents.

If this directory is not a Git worktree, stop Git operations and report the exact issue. Do not initialize a new repository unless separately authorized.

If another merge, rebase, cherry-pick, or revert is already in progress, inspect its state and complete or safely recover that operation before starting a new one. Never blindly abort an operation that may contain uncommitted work.

## 3. Choose the Integration Branch

Identify the intended integration branch from repository instructions, branch history, naming, recent commits, and the project's established workflow. Prefer an explicitly documented integration branch. Otherwise, use the best-supported local base branch (`main`, `master`, `develop`, or another clearly established integration branch).

Do not assume that the current branch is the integration branch without checking. If evidence is inconclusive and choosing incorrectly could integrate unrelated work, do not guess; preserve all work and report the precise decision blocker.

Merge only branches that belong to this project and contain relevant work intended for integration. Inspect every local branch with commits not already contained in the target. Do not silently ignore such branches. Conversely, do not merge archived, experimental, unrelated, or clearly obsolete branches just because they exist; document why they were excluded.

Do not treat remote-tracking branches as local branches to merge. Do not fetch to refresh them.

## 4. Review, Stage, and Commit the Existing Work

Before staging anything, inspect the full intended change set, including tracked changes, staged changes, and untracked files. Use `git diff --check` and appropriate safe inspections. Confirm that generated files, test results, caches, local databases, private configuration, credentials, and user data are not accidentally included.

Then:

1. Commit all intended, safe project changes attributable to the completed audit/repair work, plus other clearly intended repository work covered by the user's request.
2. Preserve meaningful commit boundaries when the changes naturally form separate units. Use clear messages that accurately describe each change; do not create empty or misleading commits.
3. Stage reviewed paths explicitly. Use broad staging only after confirming every affected and untracked path is appropriate for version control.
4. Review `git diff --cached` and `git diff --cached --check` immediately before each commit. Verify that the staged content matches the intended commit.
5. Never include real secrets, private data, or disposable runtime artifacts merely to make `git status` clean. If such files are present, keep them safe and explain their status in the report.
6. Do not amend previous commits or rewrite history. Add new commits instead.

If commit identity is missing, do not invent a name or email and do not change global Git identity. Use an already-authorized repository-local identity only if it is clearly established; otherwise, report the commit blocker and preserve the changes.

If a commit hook or signing check fails, inspect and resolve the actual cause where safe. Do not bypass the hook or signing requirement to force a commit.

## 5. Merge Relevant Local Branches

After the current working changes are safely committed, merge relevant local branches into the chosen integration branch.

For each candidate branch:

1. Inspect its unmerged commits, file changes, ancestry, purpose, and relationship to the target.
2. Skip it if its work is already contained in the target; record that it was already integrated.
3. Check whether it is checked out in another worktree. Do not modify that worktree or disturb its uncommitted changes.
4. Determine a sensible merge order. Integrate prerequisite/foundation branches before branches that depend on them when the history and code show such a dependency. Re-check ancestry after every successful merge.
5. Use ordinary Git merge semantics and preserve existing history. Do not rebase or rewrite existing commits merely to obtain a clean graph. Avoid unrelated-history merges unless the repository's established structure clearly requires them and the merge can be validated safely.
6. Prefer a merge that can be inspected before the merge commit is finalized. Review the resulting staged diff, check for whitespace errors, and run relevant tests before recording the merge commit when practical.
7. Use a truthful merge message identifying the source branch. Do not manufacture a merge commit when the branch is already integrated or has no changes to merge.

### Merge conflicts

Resolve conflicts based on intended behavior and the actual base/ours/theirs changes, not by blindly choosing one side. Inspect relevant callers, tests, interface contracts, and dependent files. Preserve valid changes from both sides where possible. Do not resolve every conflict with a blanket `--ours` or `--theirs` operation.

After resolving conflicts:

- Inspect every resolved file and the complete staged diff.
- Check that no code, configuration, tests, or documentation was accidentally dropped.
- Run the narrowest relevant tests, then broader checks for affected workflows.
- Complete the merge commit only when the resolution is understood and validated sufficiently.

If a conflict cannot be resolved safely, do not invent behavior. Keep the prior commits intact, safely abort only the current incomplete merge if doing so will not discard valuable work, verify the repository returns to a known state, record the blocker, and continue with other independent branches when possible. Do not claim all branches were merged if any relevant branch remains unmerged.

Never delete a source branch after merging. Retain local branches and tags for recovery unless the user separately authorizes cleanup.

## 6. Verify the Integrated Repository

After all safe merges, remain on the integration branch unless the documented workflow requires otherwise. Verify:

- The current branch is the intended integration branch.
- No merge, rebase, cherry-pick, or revert remains unintentionally in progress.
- `git status --short --branch` and `git log --oneline --graph --decorate --all` match the expected outcome.
- Every relevant local branch has been merged, is already contained in the target, or has a documented reason/blocker for exclusion.
- The new commits contain the intended files and no secrets or disposable data.
- The final diff and commit history do not show accidental deletions or unrelated modifications.
- `git diff --check` and applicable project tests/build checks pass where they can run safely.
- The application remains consistent with the completed codebase audit; do not rerun destructive setup, migrations, or cleanup routines.

Do not claim the working tree is clean if relevant uncommitted files remain. If local runtime files are intentionally untracked or ignored, explain that separately from uncommitted source changes.

## 7. Pi Extension That Interrupts the Merge Workflow

If, and only if, there is concrete evidence that a **Pi Agent extension** is interrupting or stopping Pi during an otherwise safe local Git commit or merge operation, diagnose the extension before changing its state.

Examples of useful evidence include repeatable interruption tied to that extension, relevant extension logs, or clear extension behavior. A normal Git conflict, failed test, missing Git identity, signing failure, permission problem, or explicit safety gate is not by itself evidence that a Pi extension is responsible.

If the extension is confirmed to be the blocker:

1. Identify the exact extension and the specific behavior preventing the operation. Do not guess or disable extensions indiscriminately.
2. Check the installed Pi version's supported mechanism for temporarily disabling an extension. Use the narrowest supported setting; do not uninstall the extension or edit its source code.
3. Preserve the original extension configuration exactly before changing it. Record which setting is being changed and how it will be restored. Avoid exposing tokens or other secrets in logs or reports.
4. Temporarily disable **only that confirmed extension**, only for the duration needed to complete the blocked local Git operation. Do not disable core Pi safeguards, approval/permission controls, security extensions, backup tools, Git hooks, or unrelated extensions.
5. Confirm Pi remains operational, complete the necessary safe Git operation, and verify the resulting Git state.
6. Restore the extension's original enabled/disabled state and configuration exactly, including on failure. Verify the restoration and remove only the temporary backup created for this purpose once restoration is confirmed.
7. Report the extension name, evidence for the interference, the temporary change made, and the restoration result. Do not claim restoration unless verified.

Do not modify global or user-wide settings unless the extension's own configuration can only be managed there and the temporary change is explicitly limited, backed up, and fully reversible. Do not disable an extension merely because it requests human approval, reports a genuine risk, or prevents an unsafe operation. If safe temporary disablement is not possible, leave it enabled and report the blocker.

## 8. Strict No-Remote-Push Guarantee

Throughout this task:

- Never push commits, branches, or tags.
- Never publish a release, deploy, or trigger a remote workflow.
- Never use a website, API, MCP tool, or extension to merge/publish remotely.
- Never fetch or pull as a shortcut to resolving local divergence.
- Never change remote URLs or remote configuration.

Local commits and local branch merges are permitted. Network access is not needed for this task and must not be used to publish or synchronize repository contents.

## 9. Final Report

Provide a concise but evidence-based report containing:

1. **Repository and target branch:** Repository root, starting branch, and final integration branch.
2. **Commits created:** Local commit hashes, messages, and the main changes in each.
3. **Branches reviewed:** Each relevant local branch and whether it was merged, already integrated, excluded with a reason, or blocked.
4. **Conflicts:** Conflicts encountered, how they were resolved, and which tests validated the resolution.
5. **Validation:** Exact commands run and their actual PASS / FAIL / BLOCKED / NOT RUN results. Do not claim success without evidence.
6. **Pi extension:** Whether any extension was temporarily disabled, why, and whether its original state was restored and verified.
7. **Workspace status:** Whether source changes remain uncommitted, why any files were intentionally left out, and whether the working tree is otherwise clean.
8. **Remote status:** Explicitly state that no push, publish, deploy, fetch, or pull was performed.
9. **Remaining blockers:** Exact issue, impact, and next safe action for anything not completed.

## 10. Execution Directive

Begin by inspecting the repository and current state. Protect the existing work, create local commits for all intended and safe changes, integrate all relevant local branches into the correct integration branch, resolve conflicts carefully, run meaningful validation, restore any temporarily disabled Pi extension, and verify the final state.

Proceed autonomously through routine, reversible operations. Stop only the specific operation that would risk data loss, violate the rules above, require unavailable authorization, or cannot be resolved safely. Continue other independent safe work and report any remaining blocker honestly.

**Local commits and local merges only. No remote push under any circumstances.**
