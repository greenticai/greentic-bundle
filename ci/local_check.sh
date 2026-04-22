#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PACKAGE_ONLY=0

if [ "${1:-}" = "--package-only" ]; then
  PACKAGE_ONLY=1
fi

header() {
  printf '\n==> %s\n' "$1"
}

allow_dirty_flag() {
  if [ "${CI:-}" = "true" ]; then
    return 1
  fi
  return 0
}

run_cargo() {
  if allow_dirty_flag; then
    cargo "$@" --allow-dirty
  else
    cargo "$@"
  fi
}

verify_packaged_files() {
  crate_name="$1"
  manifest_dir="$2"
  package_list_file="$3"

  if ! grep -Eq '(^|/)Cargo\.toml$' "$package_list_file"; then
    printf 'Missing Cargo.toml from packaged output for %s\n' "$crate_name" >&2
    return 1
  fi
  if ! grep -Eq '(^|/)src/' "$package_list_file"; then
    printf 'Missing src/ from packaged output for %s\n' "$crate_name" >&2
    return 1
  fi
  if ! grep -Eq '(^|/)(README|README\.md)$' "$package_list_file"; then
    printf 'Missing README from packaged output for %s\n' "$crate_name" >&2
    return 1
  fi
  if ! grep -Eq '(^|/)(LICENSE|LICENSE\.txt|LICENSE-MIT|LICENSE-APACHE)$' "$package_list_file"; then
    printf 'Missing LICENSE from packaged output for %s\n' "$crate_name" >&2
    return 1
  fi

  for asset_dir in assets templates schemas wit; do
    if [ -d "$manifest_dir/$asset_dir" ] && ! grep -Eq "(^|/)$asset_dir/" "$package_list_file"; then
      printf 'Missing runtime asset directory %s from packaged output for %s\n' "$asset_dir" "$crate_name" >&2
      return 1
    fi
  done
}

verify_source_files() {
  crate_name="$1"
  manifest_dir="$2"

  if [ ! -f "$manifest_dir/Cargo.toml" ]; then
    printf 'Missing Cargo.toml in source tree for %s\n' "$crate_name" >&2
    return 1
  fi
  if [ ! -d "$manifest_dir/src" ]; then
    printf 'Missing src/ in source tree for %s\n' "$crate_name" >&2
    return 1
  fi
  if [ ! -f "$manifest_dir/README.md" ] && [ ! -f "$manifest_dir/README" ]; then
    printf 'Missing README in source tree for %s\n' "$crate_name" >&2
    return 1
  fi
  if [ ! -f "$manifest_dir/LICENSE" ] && [ ! -f "$manifest_dir/LICENSE.txt" ] && [ ! -f "$manifest_dir/LICENSE-MIT" ] && [ ! -f "$manifest_dir/LICENSE-APACHE" ]; then
    printf 'Missing LICENSE in source tree for %s\n' "$crate_name" >&2
    return 1
  fi

  for asset_dir in assets templates schemas wit; do
    if [ -d "$manifest_dir/$asset_dir" ]; then
      :
    fi
  done
}

run_packaging_checks() {
  header "Packaging Checks"
  while IFS="$(printf '\t')" read -r crate_name manifest_dir internal_deps; do
    [ -n "$crate_name" ] || continue
    printf 'Checking package for %s\n' "$crate_name"

    if [ -n "${internal_deps:-}" ]; then
      verify_source_files "$crate_name" "$manifest_dir"
      printf 'Skipping cargo package/publish dry-run for %s until workspace dependencies are published to crates.io: %s\n' "$crate_name" "$internal_deps"
      continue
    fi

    package_list_file="$(mktemp)"
    trap 'rm -f "$package_list_file"' EXIT

    run_cargo package --list -p "$crate_name" >"$package_list_file"
    verify_packaged_files "$crate_name" "$manifest_dir" "$package_list_file"
    run_cargo package --no-verify -p "$crate_name"
    run_cargo package -p "$crate_name"
    run_cargo publish -p "$crate_name" --dry-run

    rm -f "$package_list_file"
    trap - EXIT
  done <<EOF
$(python3 "$ROOT_DIR/ci/workspace_publish.py" plan)
EOF
}

cd "$ROOT_DIR"

if [ "$PACKAGE_ONLY" -eq 0 ]; then
  header "i18n validate"
  python3 ci/i18n_check.py validate

  header "i18n status"
  python3 ci/i18n_check.py status

  header "cargo fmt"
  cargo fmt --all -- --check

  header "cargo clippy"
  cargo clippy --all-targets --all-features -- -D warnings
  cargo clippy --features extensions --all-targets -- -D warnings

  header "cargo test"
  GREENTIC_BUNDLE_USE_BUNDLED_CATALOG=1 cargo test --all-features
  GREENTIC_BUNDLE_USE_BUNDLED_CATALOG=1 cargo test --features extensions --all-targets

  header "cargo build"
  cargo build --all-features

  header "cargo doc"
  cargo doc --no-deps --all-features
fi

# Phase A compile-regression guard: extensions-off binary must stay within
# 500 KB of the main-branch baseline. Skipped if baseline file is absent.
if [ -f /tmp/greentic-bundle-baseline.size ]; then
  cargo build --release --no-default-features
  NEW=$(stat -c '%s' target/release/greentic-bundle 2>/dev/null || stat -f '%z' target/release/greentic-bundle)
  BASE=$(cat /tmp/greentic-bundle-baseline.size)
  DELTA=$(( (NEW - BASE) / 1024 ))
  echo "binary size delta: ${DELTA} KB (baseline ${BASE}, new ${NEW})"
  if [ "$DELTA" -gt 500 ]; then
    echo "ERROR: binary grew by ${DELTA} KB (>500 KB budget)" >&2
    exit 1
  fi
fi

run_packaging_checks
