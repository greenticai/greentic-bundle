GLOBAL RULE – REPO OVERVIEW, CI, AND REUSE OF GREENTIC REPOS

For THIS REPOSITORY, you must ALWAYS:

1. Maintain `.codex/repo_overview.md` using the “Repo Overview Maintenance” routine BEFORE starting any new PR and AFTER finishing it.
2. Run `ci/local_check.sh` at the end of your work and ensure it passes, or explain precisely why it cannot be made to pass as part of this PR.
3. Prefer using existing Greentic repos/crates (interfaces, types, secrets, oauth, messaging, events, etc.) instead of reinventing types, interfaces, or behaviour locally.
4. Keep all translation catalogs up to date whenever user-facing CLI or wizard text changes (and ensure newly added locales are included in the embedded i18n assets).

Treat these as built-in prerequisites and finalisation steps for ALL work in this repo.

---

# Codex Execution Rules (PR-by-PR)

You are working in a repo with a numbered PR series (PR-01, PR-02, …).  
You MUST work strictly PR-by-PR.

## 1) One PR at a time
- Only work on the currently assigned PR (e.g., PR-03).
- Do NOT start tasks from PR-04+ until explicitly authorized.

## 2) Always declare PR state
At the top of every response, you MUST print one of these headers:

- `STATUS: IN_FLIGHT (PR-XX)`  
  Meaning: you are actively working on PR-XX and it is not complete.

- `STATUS: READY_FOR_REVIEW (PR-XX)`  
  Meaning: PR-XX is complete, tests/docs updated, and you are waiting.

- `STATUS: BLOCKED (PR-XX)`  
  Meaning: you cannot proceed due to a concrete blocker. State exactly what.

## 3) Definition of “finished PR”
You may mark `READY_FOR_REVIEW` only if:
- All requested code changes for PR-XX are implemented
- Tests are added/updated and run (or explicitly noted if none exist)
- Docs/help output updated as required
- `git status` is clean OR you clearly list uncommitted changes

## 4) Handoff discipline (“go” gate)
When you reach `READY_FOR_REVIEW`, STOP.  
Do not begin PR-(XX+1) until the user replies with exactly: **`go`** (or “go PR-YY”).

## 5) Work log format
While `IN_FLIGHT`, keep a short running checklist:

- ✅ done
- 🔧 in progress
- ⛔ blocked

## 6) Pre-authorized routine actions
You are pre-authorized to:
- edit/add/remove files required for the current PR-XX
- run formatting/lints/tests for the current PR-XX
- update README/help text for the current PR-XX
- add minimal scaffolding/fixtures required by PR-XX

You are NOT authorized to:
- delete large subsystems
- change public CLI behavior outside PR-XX scope
- start future PRs early
- make breaking schema/interface changes

## 7) If scope creep is detected
If PR-XX appears to require work from PR-(XX+1), do not do it.  
Mark `STATUS: BLOCKED (PR-XX)` and explain exactly what dependency is needed.

---

### Workflow for EVERY PR

Whenever I ask you to implement a change, feature, refactor, or bugfix (i.e. PR-style work), follow this workflow:

1. PRE-PR SYNC (MANDATORY)
   - Check out the target branch for this work (usually the default/main branch or the branch I specify).
   - Run the “Repo Overview Maintenance” routine:
     - Fully refresh `.codex/repo_overview.md` so it accurately reflects the current state of the repo *before* making any changes.
   - Show me the updated `.codex/repo_overview.md` if it changed in a meaningful way.

2. IMPLEMENT THE PR
   - Apply the requested changes (code, tests, docs, configs, etc.).
   - **Greentic reuse-first policy:**
     - Before adding new core types, interfaces, or cross-cutting functionality, CHECK whether they already exist in other Greentic repos (for example):
       - `greentic-interfaces`
       - `greentic-types`
       - `greentic-secrets`
       - `greentic-oauth`
       - `greentic-messaging`
       - `greentic-events`
       - (and other existing shared crates as relevant)
     - If a suitable type or interface exists, USE IT instead of re-defining it locally.
     - Do NOT fork or duplicate cross-repo models unless there is a clear, documented reason.
     - Only introduce new shared concepts when there is no existing crate that fits; if you do, clearly mention this in the PR summary.
   - Run the appropriate build/test commands while you work (language-appropriate), and fix issues related to your changes.

3. POST-PR SYNC (MANDATORY)
   - Re-run the “Repo Overview Maintenance” routine, now based on the UPDATED codebase:
     - Update `.codex/repo_overview.md` to reflect:
       - New functionality you added.
       - Any TODO/WIP/stub entries you created or resolved.
       - Any new failing tests or resolved failures.
   - Run the repo’s CI wrapper:
     - Execute: `ci/local_check.sh` from the repo root (or as documented in this repo).
     - If it fails due to your changes, fix the issues until it passes.
     - If it fails for reasons outside the scope of your changes (e.g. pre-existing flaky tests or external constraints), do NOT hide it:
       - Capture the failing steps and key error messages.
       - Clearly document in the PR summary which checks are still failing and why they could not be fixed as part of this PR.
   - Ensure:
     - `.codex/repo_overview.md` is consistent and up-to-date, and
     - Any necessary changes to make `ci/local_check.sh` pass (within scope) are included.
   - In your final PR summary, explicitly mention:
     - That the repo overview was refreshed.
     - That `ci/local_check.sh` was run and its outcome (pass / fail with reasons).

---

### Behavioural Rules

- Do **not** ask for permission to:
  - Run the Repo Overview Maintenance routine,
  - Run `ci/local_check.sh`,
  - Or reuse existing Greentic crates. These are always required unless I explicitly say otherwise for a specific task.
- Never leave `.codex/repo_overview.md` in a partially updated or obviously inconsistent state.
- Never introduce new core types or interfaces that duplicate what exists in shared Greentic crates without a strong, documented justification.
- If the build/test/CI commands are unclear and you cannot infer them from the repo (README, CI config, `ci/` scripts, etc.), ask a concise question; otherwise, proceed autonomously.

---

The “Repo Overview Maintenance” routine is defined in `.codex/repo_overview_task.md`. Follow it exactly whenever instructed above.
