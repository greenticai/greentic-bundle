#!/usr/bin/env python3
"""Workspace helpers for publishable Rust crates."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from collections import defaultdict, deque
from pathlib import Path


def cargo_metadata() -> dict:
    root = Path(__file__).resolve().parent.parent
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def is_publishable(package: dict) -> bool:
    publish = package.get("publish")
    return publish is None or len(publish) > 0


def workspace_packages(metadata: dict) -> list[dict]:
    workspace_member_ids = set(metadata["workspace_members"])
    packages = [pkg for pkg in metadata["packages"] if pkg["id"] in workspace_member_ids]
    packages_by_id = {pkg["id"]: pkg for pkg in packages}

    graph = defaultdict(set)
    indegree = {pkg["id"]: 0 for pkg in packages}
    resolve = metadata.get("resolve") or {}
    for node in resolve.get("nodes", []):
        package_id = node["id"]
        if package_id not in packages_by_id:
            continue
        for dep_id in node.get("dependencies", []):
            if dep_id not in packages_by_id:
                continue
            graph[dep_id].add(package_id)
            indegree[package_id] += 1

    ready = deque(sorted([pkg_id for pkg_id, degree in indegree.items() if degree == 0]))
    ordered = []
    while ready:
        pkg_id = ready.popleft()
        ordered.append(packages_by_id[pkg_id])
        for dependent in sorted(graph[pkg_id]):
            indegree[dependent] -= 1
            if indegree[dependent] == 0:
                ready.append(dependent)

    if len(ordered) != len(packages):
        ordered = sorted(packages, key=lambda pkg: pkg["name"])
    return ordered


def primary_package(metadata: dict, packages: list[dict]) -> dict:
    primary_name = os.environ.get("PRIMARY_PACKAGE")
    if primary_name:
        for package in packages:
            if package["name"] == primary_name:
                return package
        raise SystemExit(f"PRIMARY_PACKAGE={primary_name!r} does not match a workspace package")

    root_package = metadata.get("resolve", {}).get("root")
    if root_package:
        for package in packages:
            if package["id"] == root_package and is_publishable(package):
                return package

    publishable = [package for package in packages if is_publishable(package)]
    if len(publishable) == 1:
        return publishable[0]
    raise SystemExit("unable to determine primary package; set PRIMARY_PACKAGE")


def internal_workspace_dependencies(
    package: dict, packages_by_id: dict[str, dict], publishable_ids: set[str]
) -> list[str]:
    workspace_names = {
        packages_by_id[pkg_id]["name"]: packages_by_id[pkg_id]
        for pkg_id in publishable_ids
        if pkg_id in packages_by_id
    }
    dependency_ids = []
    for dependency in package.get("dependencies", []):
        dep_id = dependency.get("pkg")
        if dep_id in packages_by_id and dep_id in publishable_ids:
            dependency_ids.append(packages_by_id[dep_id]["name"])
            continue
        dep_name = dependency.get("name")
        if dep_name in workspace_names:
            dependency_ids.append(workspace_names[dep_name]["name"])
    return sorted(set(dependency_ids))


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: workspace_publish.py <list|plan|primary-name|primary-version|primary-binary>"
        )

    metadata = cargo_metadata()
    packages = workspace_packages(metadata)
    publishable = [package for package in packages if is_publishable(package)]
    packages_by_id = {pkg["id"]: pkg for pkg in packages}
    publishable_ids = {pkg["id"] for pkg in publishable}

    command = sys.argv[1]
    if command == "list":
        for package in publishable:
            manifest_dir = Path(package["manifest_path"]).resolve().parent
            print(f"{package['name']}\t{manifest_dir}")
        return 0
    if command == "plan":
        for package in publishable:
            manifest_dir = Path(package["manifest_path"]).resolve().parent
            internal_deps = ",".join(
                internal_workspace_dependencies(package, packages_by_id, publishable_ids)
            )
            print(f"{package['name']}\t{manifest_dir}\t{internal_deps}")
        return 0

    primary = primary_package(metadata, packages)
    if command == "primary-name":
        print(primary["name"])
        return 0
    if command == "primary-version":
        print(primary["version"])
        return 0
    if command == "primary-binary":
        print(primary["name"].replace("_", "-"))
        return 0

    raise SystemExit(f"unknown command: {command}")


if __name__ == "__main__":
    raise SystemExit(main())
