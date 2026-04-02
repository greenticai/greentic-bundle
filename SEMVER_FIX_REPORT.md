# SEMVER Fix Report

## Context
`cargo-semver-checks` reported 2 semver violations for `greentic-bundle` v0.4.27:

1. `struct_pub_field_missing`
2. `struct_pub_field_now_doc_hidden`

Both failures referenced the same API surface: `WizardArgs.schema` in `src/cli/wizard.rs`.

## Root Cause
`WizardArgs.schema` was no longer publicly exposed (`pub(crate)`), which made the previously public field unavailable to downstream users.

## Fix Applied
- Restored field visibility from `pub(crate)` to `pub`:
  - `src/cli/wizard.rs`
  - Change: `pub(crate) schema: bool` -> `pub schema: bool`

## Why This Is Semver-Safe
- Restores the previously published public field exactly under the same name and type.
- No behavior or runtime logic changed.
- No version bump required because compatibility was restored.

## Files Modified
- `src/cli/wizard.rs`
- `SEMVER_FIX_REPORT.md`
