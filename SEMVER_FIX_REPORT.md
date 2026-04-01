# SEMVER Fix Report

## Context
- Crate: `greentic-bundle`
- Semver check: `v0.4.26 -> v0.4.27`
- Reported violation: `constructible_struct_adds_field`
- Affected item: `WizardArgs.schema` in `src/cli/wizard.rs`

## Fix Applied
- Added `#[non_exhaustive]` to `pub struct WizardArgs` in `src/cli/wizard.rs`.

## Why This Fix
- `WizardArgs` is publicly constructible with struct literals.
- Adding a new public field (`schema`) is semver-breaking for downstream crates constructing it directly.
- Marking the struct `#[non_exhaustive]` prevents external exhaustive struct-literal construction and allows future field additions without further semver breakage.

## Safety / Behavior
- No runtime logic changed.
- No behavior changed.
- No tests modified.
- No version bump required for this violation.

## Files Changed
- `src/cli/wizard.rs`
- `SEMVER_FIX_REPORT.md`
