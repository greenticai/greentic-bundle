PR-BUNDLE UPDATE — authoritative decisions and sequencing overrides

This block supersedes any older contradictory wording later in this document.

Execution order

Start with operator PRs first, not bundle PRs.

Phase A — operator PRs first

PR-OP-BUNDLE-01

Separate runtime bundle consumption boundary from embedded authoring/build code.

PR-OP-BUNDLE-02

Formalize operator runtime bundle-consumption contract around built `.gtbundle` SquashFS artifacts.

PR-OP-BUNDLE-03

Deprecate embedded authoring/build commands in operator.

PR-OP-BUNDLE-04

Remove or quarantine operator-coupled authoring internals that should not survive extraction.

Then move to greentic-bundle.

Extraction/source priorities

Primary extraction source:

Use local `../greentic-operator` on `prod-dev`.

Use local sibling repos directly if available:

`../greentic-operator`

`../greentic-pack`

`../greentic-component`

Use GitHub `greentic-operator` `master` only as a reference for:

CLI conventions

i18n/help/logging conventions

runtime-boundary confirmation

path/style discipline

Cross-cutting decisions

Use `clap`.

Start as a small workspace early, not as a single crate.

Ship `en.json` only at first, while implementing full locale normalization, embedded catalogs, and fallback behavior.

Use `semver` parsing for answer document schema versions from day one.

Reuse the pack/component-style prompt layer or pattern if available; do not copy the operator ad hoc wizard spec-builder approach.

Reuse shared Greentic crates/interfaces where clear and low-friction, but do not block progress chasing perfect shared abstractions.

Update `.codex/repo_overview.md` before and after each PR.

Add tests with each PR; do not defer coverage.

PR-BUNDLE-01 decisions

Use `en.json` as the first shipped locale catalog.

Use a small workspace layout early, including the main CLI crate and a reader crate under `crates/`.

Answer document example locale should be treated as `en`, not `en-GB`.

PR-BUNDLE-02 decisions

Plan snapshots are internal deterministic test artifacts, not a public compatibility promise.

Use the smallest possible authored test workspace:

`bundle.yaml`

`tenants/<tenant>/tenant.gmap`

optional `resolved/`

optional `state/resolved/`

No demo runtime folders. No operator runtime state.

PR-BUNDLE-02A decisions

Replace the current flat create/update wizard with a staged composition wizard.

Create/update composition rules:

- Require at least one app pack before finishing create.
- Add app packs one at a time through a guided loop.
- Auto-detect app-pack and custom extension-provider references instead of asking for source type first.
- Prompt for mapping immediately after adding each app pack:
  - `global`
  - `tenant`
  - `tenant/team`
- Update bundle composition state and gmap rules from those mapping decisions.
- Remove create-flow questions for:
  - `advanced setup`
  - comma-separated app packs
  - comma-separated extension providers
  - raw comma-separated remote catalogs
  - `setup execution intent`
  - `export intent`
- Provider setup/configuration is out of scope for the composition wizard.
- Replace setup/export intent with review actions:
  - build now
  - dry-run only
  - save answers only

Reference detection rules:

- Existing local filesystem path without a URI scheme: treat as local file or directory.
- `file://`: local filesystem handling.
- `oci://`, `repo://`, and `store://`: distributor-backed references.

Common extension providers:

- Load from a public OCI-backed JSON catalog through the existing catalog/distributor seam.
- Default public catalog reference: `ghcr://packs/well-known-packs.json`
- Allow additional/override catalogs through authored workspace/catalog answers when needed.

Persistence/replay rules:

- Keep `--answers`, `--emit-answers`, `--dry-run`, `validate`, and `apply` working.
- Persist ordered app-pack entries, mapping scopes, access-rule edits, extension-provider entries, and selected catalog items in answers.
- Do not persist environment-specific provider setup output as part of composition answers.

PR-BUNDLE-03 decisions

Use `bundle.yaml` as the mutable workspace root file.

Preserve:

`tenants/<tenant>/tenant.gmap`

`tenants/<tenant>/teams/<team>/team.gmap`

`resolved/<tenant>[.<team>].yaml`

`state/resolved/<tenant>[.<team>].yaml`

Source of truth:

`bundle.yaml`

`tenants/.../*.gmap`

authored local refs/config

explicit user-managed composition inputs

emitted answers if users choose to keep them

Generated:

`resolved/...`

`state/resolved/...`

`bundle.lock.json`

cache contents

setup-derived composition state

build staging outputs

Use full deterministic resolution after access mutations initially.

PR-BUNDLE-04 decisions

Use workspace-local cache first.

Use `bundle.lock.json` at the workspace root as the first public lock contract.

Keep v1 user-facing resolver controls limited to:

source/catalog override

offline mode

maybe cache path override if needed

maybe explicit resolver source later

Keep registry defaults, backend wiring, and retry behavior internal in v1.

PR-BUNDLE-05 decisions

Persist setup-derived state as JSON first.

Use `state/setup/` for local file-backed setup persistence.

Keep setup data in both:

emitted answers, for replay input and wizard history

normalized persisted composition state, for derived composition/build input

PR-BUNDLE-06 decisions

SquashFS is required from v1.

`.gtbundle` is the SquashFS artifact.

`build` must generate the final deterministic `.gtbundle`.

`export` is not the primary artifact-producing command and may be omitted or kept only for future optional transformations.

Do not persist build intermediates by default.

Initial `.gtbundle` contents should be minimal and explicit:

canonical bundle manifest

canonical lock metadata needed by runtime consumers

normalized resolved metadata

pack/provider metadata required for runtime discovery

bundle format/version markers

Do not include authoring cache, temp files, wizard scratch state, or raw emitted answers unless intentionally required.

Inside the artifact, use deterministic metadata encoding. Prefer canonical encoding if existing helpers fit cleanly; otherwise use stable sorted JSON first and do not let metadata encoding block the SquashFS build path.

PR-BUNDLE-06A decisions

Put the reader crate inside this repo under `crates/` first.

Support `.gtbundle` SquashFS reading first.

Support normalized unpacked build directories second if practical.

Minimum operator-facing API surface:

open a `.gtbundle`

read bundle format/version

read manifest and lock surface

enumerate runtime-relevant packs/providers/hooks/subscriptions/capabilities

validate basic artifact structure

return stable structured errors for invalid/incompatible bundles

PR-BUNDLE-07 decisions

Deprecate operator authoring commands first.

Keep compatibility shims for one explicit migration window or one release cycle, then remove them.

Do not leave dual ownership for long.

PR-BUNDLE-01 — Scaffold greentic-bundle repo, CLI baseline, and shared contracts
Summary

Create the new greentic-bundle repository with the baseline CLI, i18n, answers contract, and crate/module layout. This PR should not implement full bundle authoring yet. It should establish the reusable skeleton so later PRs can port code from operator/pack/component cleanly.

This PR is the foundation for all later extraction work.

Why

The audits show:

greentic-pack gives the strongest wizard UX contract

greentic-component gives the strongest deterministic generation architecture

greentic-operator prod-dev contains the highest-value real authoring logic

greentic-operator master is only the behavioral baseline

So before moving logic, we need a repo structure that matches that conclusion.

Objectives

Create:

a new greentic-bundle CLI

baseline localized help

wizard command shape

bundle AnswerDocument type

schema versioning and migration hooks

crate/module layout for future extraction

initial tests for i18n and answer document basics

Scope
Create repo layout

Suggested initial layout:

greentic-bundle/
  Cargo.toml
  README.md
  docs/
  src/
    main.rs
    cli/
      mod.rs
      wizard.rs
      build.rs
      export.rs
      inspect.rs
      doctor.rs
      add.rs
      remove.rs
      access.rs
      init.rs
    i18n/
      mod.rs
    wizard/
      mod.rs
      i18n.rs
    answers/
      mod.rs
      document.rs
      migrate.rs
    project/
      mod.rs
    catalog/
      mod.rs
    access/
      mod.rs
    setup/
      mod.rs
    build/
      mod.rs
  i18n/
    en-GB.json
  tests/
CLI shape

Implement command skeleton only:

greentic-bundle
greentic-bundle wizard
greentic-bundle wizard run
greentic-bundle wizard validate
greentic-bundle wizard apply
greentic-bundle doctor
greentic-bundle build
greentic-bundle export
greentic-bundle inspect
greentic-bundle add app-pack
greentic-bundle add extension-provider
greentic-bundle remove app-pack
greentic-bundle remove extension-provider
greentic-bundle access allow
greentic-bundle access forbid
greentic-bundle init
Reuse targets

From greentic-pack:

command split

localized help shape

wizard flag shape

i18n fallback behavior

From greentic-component:

answer document schema/version mindset

run / validate / apply discipline

From greentic-operator master:

top-level --locale

help / logging / CLI conventions

Required flags

At minimum wire these into command parsing, even if some are not fully used yet:

--locale

--answers

--emit-answers

--schema-version

--migrate

--dry-run

--offline

--execute only where explicitly needed for non-wizard mutation paths

AnswerDocument

Create initial bundle answer envelope:

{
  "wizard_id": "greentic-bundle.wizard.run",
  "schema_id": "greentic-bundle.wizard.answers",
  "schema_version": "1.0.0",
  "locale": "en-GB",
  "answers": {},
  "locks": {}
}

Implement:

serde model

round-trip serialization

stable sorted output

basic schema-version validation

migration stub path

i18n

Implement:

compile-time embedded locale catalog

locale normalization

language fallback

baseline en-GB

namespaced keys:

cli.*

wizard.*

bundle.*

errors.*

Do not hard-code user-facing strings inline except for unavoidable bootstrap errors.

Non-goals

Do not yet implement:

real bundle workspace mutation

gmap

provider catalogs

setup bridges

build/export logic

.gtbundle creation

Files to create

src/main.rs

src/cli/*

src/i18n/mod.rs

src/wizard/i18n.rs

src/answers/document.rs

src/answers/migrate.rs

i18n/en-GB.json

docs/cli.md

tests/i18n_smoke.rs

tests/answer_document.rs

Acceptance criteria

cargo test passes

greentic-bundle --help is localized through embedded i18n

greentic-bundle wizard run --help shows intended flags

answer document round-trips deterministically

locale fallback works

no bundle logic copied yet beyond baseline patterns

PR-BUNDLE-02 — Port wizard execution model and answer replay from pack/component
Summary

Implement the deterministic wizard execution contract for greentic-bundle, using:

pack for public wizard UX and navigation

component for internal deterministic generation flow

operator prod-dev for proven authoring/replay behavior

This PR should establish the real core of bundle authoring:

run

validate

apply

replayable answers

emitted answers

plan-first execution

dry-run behavior

Why

This is the highest-value reusable architecture across the audits.

We do not want a one-off interactive wizard. We want a reproducible authoring system.

Objectives

Implement:

wizard orchestration core

answer replay

normalized internal request model

deterministic plan envelope

dry-run execution

emitted answer docs

minimal menu flow

Reuse guidance
From greentic-pack

Copy/adapt:

wizard, wizard run, wizard validate, wizard apply

navigation contract:

main menu: 0) Exit

submenu: 0) Back

M) Main Menu

--answers

--emit-answers

--schema-version

--migrate

--dry-run

QA-driver style prompt orchestration

wizard tests structure

From greentic-component

Copy/adapt:

deterministic execution pipeline

normalized request before render/execute

plan-before-side-effects

schema migration mindset

explicit validate/apply split

From greentic-operator prod-dev

Mine and adapt:

src/wizard.rs

tests/wizard_paths.rs

src/wizard_i18n.rs where useful

Do not copy:

operator/demo naming

operator-specific wizard spec builder

provider registry file conventions as final public contract

Required implementation
New internal flow
wizard input
  -> load/migrate answers
  -> normalize request
  -> build deterministic plan
  -> validate
  -> execute or dry-run
  -> emit normalized answers if requested
Plan envelope

Add a stable bundle wizard plan type, e.g.:

metadata

target root

requested action/mode

normalized input summary

ordered step list

expected file writes

warnings

Initial step kinds can include:

ensure_workspace

write_bundle_file

update_access_rules

resolve_refs

write_lock

build_bundle

export_bundle

The plan does not need full implementation of all steps yet, but the type system should exist.

Minimal wizard modes for v1 of this PR

create

update

doctor

Initial prompt flow

Keep minimal-first:

bundle name / id

output directory

advanced setup?

Only if advanced:

app packs

extension providers

remote catalogs

setup execution intent

export intent

Tests

Port/adapt scenarios inspired by:

greentic-pack wizard tests

greentic-component answer replay tests

greentic-operator tests/wizard_paths.rs

Required tests:

emit answers after run

validate is side-effect free

apply replays from answers

migrate older answer schema

dry-run suppresses writes but still builds plan

locale-aware wizard rendering

Acceptance criteria

wizard commands work end-to-end on a minimal fake workspace

answer replay is deterministic

plan is stable enough for snapshots

no operator-specific prompt builder remains

all user-facing strings are i18n-keyed

PR-BUNDLE-03 — Extract gmap and authoring workspace mutation primitives from operator prod-dev
Summary

Move the real bundle authoring mutation logic out of operator and into greentic-bundle, starting with the cleanest and most reusable extraction targets:

gmap

workspace mutation

resolver output bookkeeping

access allow/forbid operations

This PR should define the canonical mutable authoring workspace model.

Why

The audits showed this is the strongest real extraction path from prod-dev, and that gmap is one of the cleanest early wins.

Objectives

Implement:

src/access/gmap.rs and related modules

authoring workspace layout model

access mutation commands

deterministic update behavior

resolved-output file handling in the workspace

Reuse guidance
From greentic-operator prod-dev

Primary source:

src/gmap/*

src/project/mod.rs

src/project/layout.rs

src/project/resolve.rs

src/project/tenants.rs

src/project/scan.rs

command behavior from demo allow / demo forbid

From master

Use only for:

path conventions that are intentional cross-tool contracts

filesystem discipline

atomic persistence style

Implementation requirements
Access module

Create:

src/access/gmap.rs

src/access/edit.rs

src/access/eval.rs

src/access/parse.rs

Prefer extracting almost as-is where possible, with renames only.

Project module

Define explicit workspace model, separating:

source-of-truth bundle/workspace files

generated resolved outputs

optional cache

future artifact build inputs

Do not preserve demo naming like greentic.demo.yaml as the primary public authoring contract unless intentionally chosen.

Commands

Implement deterministic mutations for:

greentic-bundle access allow

greentic-bundle access forbid

Behavior should be:

mutate gmap

rerun relevant resolution/update steps

persist normalized files

support dry-run with “would write” output

Important design rule

This PR must explicitly separate:

mutable workspace
from

future immutable .gtbundle

Do not blur them.

Tests

Port/adapt:

tests/gmap_edit.rs

tests/gmap_eval.rs

mutation scenarios inspired by current operator allow/forbid flows

Add:

dry-run access mutation test

stable formatting/path test

workspace layout validation test

Acceptance criteria

gmap lives in greentic-bundle

allow/forbid no longer depend conceptually on operator

deterministic workspace mutation exists

no runtime lifecycle/state code is imported

PR-BUNDLE-04 — Add catalog/distributor seam and reference resolution pipeline
Summary

Implement the bundle composition-time catalog and artifact resolution seam, based primarily on prod-dev provider_registry.rs, but reshaped behind a single clean abstraction for:

GHCR now

repo/store later

offline replay

pinning and locking

deterministic resolution

Why

This is one of the highest-value extraction points and one of the most important places to avoid duplicated logic and GHCR-specific assumptions.

Objectives

Create a single catalog/client seam for:

registry lookup

artifact fetch by reference

digest/pinning resolution

cache policy

offline behavior

Reuse guidance
From greentic-operator prod-dev

Mine/adapt:

src/provider_registry.rs

src/wizard.rs::resolve_pack_refs

pack metadata readers from src/domains/mod.rs

From master

Use as wording/behavior reference only:

error style

offline flag behavior

path/cache discipline

From pack/component

Use only the determinism mindset, not business logic.

Implementation requirements
New modules

src/catalog/client.rs

src/catalog/registry.rs

src/catalog/cache.rs

src/catalog/resolve.rs

Adapter trait

Define a seam around greentic-distributor-client, e.g. conceptually for:

resolve catalog ref

fetch catalog

fetch artifact by ref

return digest/pinned metadata

support offline/no-network mode

Do not call distributor-client ad hoc from multiple unrelated places.

Locks

This PR is where locks starts being real.

Populate lock material with at least:

app-pack refs

extension-provider refs

resolved digests if available

catalog ref/digest

tool/schema/build format version

Cache policy

Decide and implement one explicit initial policy:

workspace-local cache, or

user-global cache

But make it configurable enough to evolve later.

Do not silently bake .greentic/cache/provider-registry/... in as a forever public contract.

Tests

Required:

offline replay from cached catalog

same input resolves to stable lock output

GHCR defaults can be configured, not hard-coded as unchangeable behavior

sorted inspection / lock output

failure cases with recovery hints

Acceptance criteria

all composition-time fetch/resolve logic goes through one seam

lock content is deterministic

offline mode works

no direct GHCR-only assumptions remain in the core flow

PR-BUNDLE-05 — Port setup bridges and define composition-time setup persistence
Summary

Bring over the reusable setup/question bridge logic from operator prod-dev, but replace the runtime-coupled persistence model with a bundle-owned composition-time persistence seam.

Why

The audits showed:

setup_to_formspec.rs is a strong migrate-as-is candidate

demo/qa_bridge.rs is a useful migrate-with-reshaping candidate

qa_persist.rs should be discarded and rewritten

This PR handles that split cleanly.

Objectives

Implement:

legacy setup spec → FormSpec bridge

provider QA → normalized form bridge

composition-time persistence backend abstraction

wizard integration for optional setup flows

Reuse guidance
Migrate nearly as-is

src/setup_to_formspec.rs

Migrate with reshaping

src/demo/qa_bridge.rs

Rewrite

src/qa_persist.rs

New modules

src/setup/legacy_formspec.rs

src/setup/qa_bridge.rs

src/setup/persist.rs

src/setup/backend.rs

Required design
Persistence seam

Do not write into operator runtime paths.

Instead define a bundle-side backend abstraction for setup results, supporting at least:

local file-backed persistence for workspace/dev

no-op or in-memory mode for validate/dry-run/tests

Design so later backends could support:

external secret stores

config stores

CI-driven injection

Wizard integration

Support setup questions only as an optional advanced path.

Setup flows must:

be replayable

emit answers

work in dry-run

not require runtime operator state

Tests

legacy setup spec converts deterministically

provider QA bridge output is stable

dry-run setup does not persist side effects

replayed setup answers produce same normalized persisted output

no runtime path assumptions remain

Acceptance criteria

setup flows are authoring concerns in bundle, not runtime concerns in operator

no persistence into state/runtime/...

setup remains optional and replayable

PR-BUNDLE-06 — Implement build/export pipeline and first .gtbundle artifact contract
Summary

Implement the first real build/export path for greentic-bundle, replacing the demo-rooted export behavior from operator with a bundle-native contract.

This PR should define:

canonical build model

deterministic export plan

initial .gtbundle layout

optional squashfs production if in scope

Why

The audits were clear that demo build and src/demo/build.rs should be treated as evidence only, not as the implementation to preserve.

So this PR is where bundle becomes real.

Objectives

Implement:

canonical bundle document model

build pipeline from workspace → normalized build state

export pipeline from normalized build state → .gtbundle

deterministic metadata ordering

inspectable output

doctor/build/export coherence

Reuse guidance
Reference only

From greentic-pack:

deterministic archive/build discipline

sorted output expectations

stable inspect output

From greentic-component:

canonicalization before write

plan envelope mindset

From operator prod-dev:

use demo build only as source of requirements, not as structure

New modules

src/build/manifest.rs

src/build/lock.rs

src/build/plan.rs

src/build/export.rs

src/build/squashfs.rs if in scope

src/cli/build.rs

src/cli/export.rs

src/cli/inspect.rs

src/cli/doctor.rs

Required design
Define three layers explicitly
1. Answers model

User/replayed intent.

2. Canonical request / build model

Normalized composition request with refs, policies, and resolved state.

3. Final artifact model

Immutable .gtbundle.

Build vs export

build:

compute normalized build state

validate consistency

write deterministic intermediate metadata as needed

export:

materialize .gtbundle

optionally materialize squashfs payload

normalize timestamps/order/metadata

Inspect / doctor

Implement:

inspect --json

doctor --json

with sorted stable output.

Artifact contract

Initial contract should be intentionally minimal and explicit.

If squashfs is included in v1:

builder belongs in greentic-bundle

runtime mount/userspace selection stays in operator

Tests

same workspace produces byte-stable artifact

inspect output sorted and stable

doctor validates both source workspace and built artifact

dry-run export computes full plan without writing final artifact

lock drift detected

Acceptance criteria

no demo-rooted export layout remains

.gtbundle is a real deterministic output

bundle build/export is clearly separate from operator runtime consumption

PR-BUNDLE-06A — Add shared bundle reader/access library for runtime consumers
Summary

Create a shared read-only library that exposes the built .gtbundle contract to runtime consumers, especially greentic-operator, without requiring them to understand bundle internal layout.

Why

This creates a stable boundary between:

bundle build/export internals

runtime bundle consumption

and prevents operator from coupling itself to artifact layout details.

Scope

Implement a library crate that can:

open a bundle artifact or normalized build directory

read canonical bundle metadata

enumerate packs/providers/hooks/subscriptions/capabilities

validate artifact structure

expose stable typed views to runtime consumers

Non-goals

Do not include:

authoring workspace logic

build/export code

wizard/answers logic

runtime lifecycle/orchestration logic

Suggested crate layout

If inside the greentic-bundle repo:

crates/
  greentic-bundle-reader/
    src/
      lib.rs
      open.rs
      manifest.rs
      lock.rs
      files.rs
      validate.rs
      error.rs

Or similar.

API direction

Expose a high-level type such as conceptually:

BundleReader

OpenedBundle

BundleManifestView

BundlePackView

BundleRuntimeSurface

The important thing is that operator should ask questions like:

what packs are present?

what providers/hooks/subscriptions are declared?

what runtime-facing metadata exists?

without needing to know physical paths.

Acceptance criteria

operator can depend on this crate for bundle reading

no operator code needs to parse internal artifact layout directly

format/version compatibility checks are centralized

the library is read-only and layout-hiding

PR-BUNDLE-07 — Operator extraction follow-up and compatibility cleanup
Summary

After greentic-bundle is functional enough, update greentic-operator to remove or deprecate embedded authoring flows and replace them with compatibility guidance toward greentic-bundle.

Why

The goal is not only to create greentic-bundle, but also to stop operator from remaining cluttered with authoring concerns.

Scope

In greentic-operator:

deprecate or remove:

demo new

demo build

demo wizard

demo allow

demo forbid

keep only runtime consumption/lifecycle concerns

optionally add compatibility message or docs pointing to greentic-bundle

Acceptance criteria

operator scope becomes runtime-only or clearly runtime-primary

bundle authoring docs point to greentic-bundle

no accidental runtime/bundle responsibility overlap remains

Codex working instructions

Use this with Codex for greentic-bundle:

Source priorities
Primary local source

Use local ../greentic-operator on prod-dev for extraction candidates:

src/wizard.rs

src/provider_registry.rs

src/project/*

src/gmap/*

src/setup_to_formspec.rs

src/demo/qa_bridge.rs

relevant tests/docs

Online reference source

Use github.com/greenticai/greentic-operator master only as a baseline for:

CLI conventions

i18n/help/logging conventions

runtime boundary confirmation

Additional reference repos

Use greentic-pack for:

wizard UX

i18n structure

replay flags

test patterns

Use greentic-component for:

deterministic generation model

canonicalization

schema-versioned answers

plan-before-side-effects

Extraction rules
Copy/adapt

command split and wizard UX from pack

answer schema/versioning and plan-first architecture from component

real authoring logic from operator prod-dev

Move early

gmap

answer/replay flow

provider catalog seam

project mutation/resolve

Rewrite

demo scaffold/build/export

wizard spec builder

runtime-coupled QA persistence

Never move into bundle

runtime bundle access

warm/activate/rollback

runtime core

control plane

runtime provider config persistence

My recommended execution order

Start with:

PR-BUNDLE-01

PR-BUNDLE-02

PR-BUNDLE-03

PR-BUNDLE-04

PR-BUNDLE-05

PR-BUNDLE-06

PR-BUNDLE-06A

PR-BUNDLE-07

That order minimizes rework and maximizes reuse.
