# greentic-bundle

`greentic-bundle` helps people build a Greentic bundle.

A bundle is a folder, and later a `.gtbundle` file, that brings together:

- one or more app packs
- optional provider packs
- access rules
- setup state
- build metadata

This tool is meant for humans first. You do not need to be a Rust developer to use it. If you are comfortable running a few terminal commands and reading simple JSON or YAML files, you can work with this project.

If you are a coding agent, do not use this README as your main workflow guide. Go straight to [docs/coding-agents.md](docs/coding-agents.md).

## Who This Is For

This repo is especially useful for:

- app builders who want to assemble a bundle from existing `.gtpack` files
- operators who want deterministic, repeatable bundle builds
- non-technical programmers who are more comfortable with guided tools than with handwritten config
- coding agents that need to replay the wizard safely and consistently

## What The Tool Does

The main jobs of `greentic-bundle` are:

- guide you through bundle creation with a wizard
- save bundle metadata in `bundle.yaml`
- save dependency and catalog information in `bundle.lock.json`
- copy selected app packs into the bundle layout
- copy selected provider packs into the bundle layout
- generate bundle artifacts with `build`
- inspect or validate a workspace or artifact later

The most important idea is simple:

1. You choose what should go into the bundle.
2. The wizard writes that choice down.
3. The tool materializes the needed packs into the bundle folders.
4. The tool can later build a deterministic `.gtbundle` artifact from that workspace.

## The Main Commands

You do not need to memorize every command. These are the ones most people need:

```bash
greentic-bundle wizard
greentic-bundle wizard --schema
greentic-bundle wizard validate --answers answers.json
greentic-bundle wizard apply --answers answers.json
greentic-bundle build --root /path/to/bundle
greentic-bundle inspect --root /path/to/bundle
greentic-bundle doctor --root /path/to/bundle
```

Use `greentic-bundle --help` or `greentic-bundle <command> --help` for the current CLI details.

## The Easiest Way To Start

If you are creating a bundle by hand for the first time, start with the interactive wizard:

```bash
greentic-bundle wizard
```

The wizard walks through the bundle step by step.

You will usually be asked for:

- a bundle name
- a bundle id
- an output directory
- one or more app pack references
- where each pack should be available
- any extension providers you want included

When the wizard runs in execute mode, it writes a real bundle workspace to disk.

## A Very Simple Mental Model

Think of the bundle workspace like this:

- `bundle.yaml`
  This is the main authored description of the bundle.
- `bundle.lock.json`
  This records resolved dependency and catalog information.
- `packs/`
  This is where global app packs are copied.
- `providers/`
  This is where provider packs are copied.
- `tenants/`
  This holds tenant and team access rules.
- `state/`
  This holds generated state such as caches, resolved outputs, setup state, and build state.
- `dist/`
  This is where built `.gtbundle` artifacts usually end up.

## Important Detail About App Pack References

There are two different things to keep in mind:

- the reference written into `bundle.yaml` and `bundle.lock.json`
- the actual `.gtpack` file copied into the bundle layout

Those are related, but they are not the same thing.

For example, a bundle may keep the authored reference:

```yaml
app_packs:
  - demos/deep-research-demo.gtpack
```

At the same time, the wizard may copy the real pack file into:

```text
packs/deep-research-demo.gtpack
```

That is expected. The metadata preserves what you selected. The materialized pack is what later build and runtime steps need.

## Interactive Wizard Vs Answer Replay

There are two common ways to use the wizard.

### 1. Interactive Wizard

This is the best option for a person sitting at a terminal.

```bash
greentic-bundle wizard
```

The wizard asks questions and writes the workspace.

### 2. Replay From Answers

This is useful when another tool, launcher, or coding agent prepares a JSON answers file first.

```bash
greentic-bundle wizard validate --answers answers.json
greentic-bundle wizard apply --answers answers.json
```

`validate` checks the plan without writing files.

`apply` writes the bundle workspace and materializes packs and providers.

## Common Human Workflow

Here is a safe beginner-friendly flow:

1. Run `greentic-bundle wizard`.
2. Choose your app pack or packs.
3. Let the wizard write the bundle.
4. Check that `bundle.yaml` exists.
5. Check that `packs/` contains the expected materialized `.gtpack` files.
6. Run `greentic-bundle inspect --root <bundle-dir>` if you want a structured view.
7. Run `greentic-bundle build --root <bundle-dir>` when you are ready to create a `.gtbundle`.

## If Something Looks Wrong

The most common problems are:

- `bundle.yaml` has the pack reference, but `packs/` is empty
- the wrong pack filename was materialized
- provider packs were copied, but app packs were not
- a later tool such as setup or start behaves as if no app pack exists

When that happens, check the bundle workspace first.

Look at:

- `bundle.yaml`
- `bundle.lock.json`
- `packs/`
- `providers/`

If the expected app pack is not in `packs/`, the problem is usually earlier than runtime or setup.

## Build And Inspect

When your workspace looks correct, you can build it:

```bash
greentic-bundle build --root /path/to/bundle
```

To inspect the workspace or the built artifact:

```bash
greentic-bundle inspect --root /path/to/bundle
greentic-bundle doctor --root /path/to/bundle
```

## For People Editing This Repo

This repository is a Rust workspace.

The main local verification command is:

```bash
bash ci/local_check.sh
```

That script runs formatting, linting, tests, docs, packaging checks, and i18n validation.

If you only want a lighter check while working on a small change, common commands are:

```bash
cargo fmt --all
cargo test
cargo run -- --help
```

## Where To Find The Right Documentation

- `README.md`
  Start here if you are a human trying to understand what the tool is for.
- [docs/coding-agents.md](docs/coding-agents.md)
  Required reading for coding agents and launcher-driven automation.
- `greentic-bundle --help`
  Best source for the current command surface.
- `greentic-bundle wizard --help`
  Best source for the current wizard replay and schema options.

## About Older Docs

Older milestone and baseline documentation has been retired from the main README because it was too hard to follow and had drifted out of date.

If you notice documentation that contradicts this README or the CLI help text, treat the CLI help and the dedicated agent guide as the current source of truth and update the stale document.
