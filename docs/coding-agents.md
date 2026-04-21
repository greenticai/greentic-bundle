# Coding Agent Guide

This document is for coding agents and automation. The human-facing overview lives in the repository `README.md`.

## Read This First

When you change wizard behavior, do not treat `greentic-bundle` as an isolated CLI. In practice it is often used as one step inside a larger toolchain:

- `gtc` may launch or orchestrate bundle creation
- `greentic-bundle` writes the workspace and materializes packs
- `greentic-setup` or `gtc setup` may run after that
- `greentic-start` or related runtime tools may run after setup
- `greentic-pack` may be used to inspect or build source packs
- `greentic-dev` may be used for local development or validation around the same flow

If bundle composition fails, downstream tools should not be expected to repair that failure.

## The Core Rule

When `wizard apply` accepts an app-pack reference, it must do both of these things correctly:

1. Preserve the authored reference in `bundle.yaml` and `bundle.lock.json`
2. Materialize the actual `.gtpack` into the correct bundle directory such as `packs/`

Do not treat those as interchangeable.

Example:

- authored reference: `demos/deep-research-demo.gtpack`
- materialized file: `packs/deep-research-demo.gtpack`

That is the expected shape for many flows.

## Required Wizard Workflow For Agents

If you are generating answers non-interactively, use this sequence:

1. Call `greentic-bundle wizard --schema`
2. Build an answers document that matches the current schema
3. Run `greentic-bundle wizard validate --answers <FILE>`
4. Run `greentic-bundle wizard apply --answers <FILE>`
5. Verify the resulting workspace, especially `packs/` and `providers/`

Do not hand-write assumptions about the answer shape when the schema can be fetched directly.

## How To Think About Relative Local References

Relative references such as:

- `demos/deep-research-demo.gtpack`

can come from several places:

- the same directory as the answers file
- the original launcher working directory
- a real bundle workspace that already contains local relative references

The replay path must support the practical launcher case where:

- the answers file lives in `/tmp/...`
- the original source pack lives elsewhere
- the reference itself stays relative

For replay, the resolver should prefer:

1. `local_reference_base_dir` when present
2. the process current working directory as a fallback
3. the bundle root when resolving already-authored in-workspace references

Do not remove those fallbacks casually. They protect launcher-driven flows.

## Expectations When Testing With `gtc`

When a change is meant to fix a launcher flow:

- make sure the local `greentic-bundle` you are editing is the one actually used by the launcher
- assume `gtc` may write delegated answers to a temporary directory
- assume the original app-pack references may still be relative
- verify the bundle workspace immediately after `wizard apply`

The key checks are:

- `bundle.yaml` contains the intended reference
- `bundle.lock.json` contains the intended reference
- `packs/` contains the expected materialized `.gtpack`
- `providers/` contains expected provider packs when applicable

If `packs/` is empty, the problem is upstream of `greentic-setup`, `gtc setup`, or `greentic-start`.

## Expectations For `greentic-setup` And `gtc setup`

Treat setup as downstream of composition.

That means:

- setup should consume a correctly materialized bundle
- setup should not be used as a workaround for missing app-pack materialization
- if a bundle is missing its expected pack in `packs/`, fix `wizard apply` or materialization first

## Expectations For `greentic-pack`

Use `greentic-pack` when you need to answer questions like:

- does the source `.gtpack` actually contain the expected flows or nodes
- is the pack file itself valid
- is the problem inside the pack or inside bundle composition

Check the source pack before blaming bundle materialization.

## Expectations For `greentic-dev`

Use `greentic-dev` as a development and validation companion, not as proof that bundle composition worked.

If the bundle workspace is wrong, developer tooling later in the flow may only hide the real bug.

## Expectations For `greentic-start`

`greentic-start` should be treated as a consumer of a finished bundle pipeline.

Do not patch start-time behavior to compensate for:

- missing `packs/` materialization
- missing provider materialization
- wrong authored references in `bundle.yaml`

Fix the earlier bundle composition step first.

## Files To Inspect After `wizard apply`

Always inspect these before moving on:

- `bundle.yaml`
- `bundle.lock.json`
- `packs/`
- `providers/`
- `tenants/`

These files and directories tell you whether composition worked.

## Regression Tests Agents Should Prefer

When changing local-reference behavior, add or update tests that cover:

- answers-file-relative local references
- current-working-directory fallback resolution
- preservation of the original reference text in `bundle.yaml`
- preservation of the original reference text in `bundle.lock.json`
- correct materialization into `packs/`

The important end-to-end guarantee is:

- relative authored reference stays relative in metadata
- real pack file still appears in the bundle layout

## Documentation Rules For Agents

- Keep `README.md` focused on humans and plain-language explanations
- Put agent-specific workflow and toolchain coordination here
- Do not reintroduce milestone history as primary documentation
- When help text, README, and agent docs disagree, update the stale document immediately
- Prefer current CLI help over old prose when checking the command surface
