#!/usr/bin/env python3
"""Reject internal Cargo dependency edges not granted by architecture policy."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


def validate_edges(metadata: dict[str, object], policy: dict[str, object]) -> list[str]:
    packages = metadata.get("packages", [])
    internal = {package["name"] for package in packages if package["name"].startswith("codingmage-")}
    allowed = policy.get("allowed_internal_dependencies", {})
    allowed_dev = policy.get("allowed_internal_dev_dependencies", {})
    findings: list[str] = []
    for package in packages:
        name = package["name"]
        if name not in internal:
            continue
        if name not in allowed:
            findings.append(f"architecture.package.unlisted: {name}")
            continue
        granted = set(allowed[name])
        for dependency in package.get("dependencies", []):
            target = dependency["name"]
            dependency_grants = granted
            if dependency.get("kind") == "dev":
                dependency_grants = granted | set(allowed_dev.get(name, []))
            if target in internal and target not in dependency_grants:
                findings.append(f"architecture.edge.forbidden: {name} -> {target}")
    return sorted(findings)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    policy_path = args.root / "docs/architecture/dependency-policy.json"
    policy = json.loads(policy_path.read_text(encoding="utf-8"))
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=args.root,
        check=True,
        capture_output=True,
        text=True,
    )
    findings = validate_edges(json.loads(result.stdout), policy)
    if findings:
        print("architecture checks failed:", file=sys.stderr)
        for finding in findings:
            print(finding, file=sys.stderr)
        return 1
    print("architecture checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
