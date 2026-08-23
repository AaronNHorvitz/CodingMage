#!/usr/bin/env python3
"""Generate and verify CodingMage's deterministic public-surface test inventory."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs" / "evidence" / "verification-inventory.json"
DECLARATION = re.compile(
    r"^\s*pub(?:\([^)]*\))?\s+(?:"
    r"(?:(?:async|const|unsafe)\s+)*fn\s+(?P<fn_name>[A-Za-z_][A-Za-z0-9_]*)"
    r"|(?P<kind>struct|enum|trait|type|const|static)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
    r")",
    re.MULTILINE,
)
RUST_TEST = re.compile(
    r"#\[(?:tokio::)?test\][\s\S]{0,240}?\bfn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*\("
)
ERROR_CODE = re.compile(r'"(?P<code>codingmage(?:\.[a-z0-9_]+){2,})"')
CATEGORIES = (
    "positive",
    "negative",
    "boundary",
    "malformed_input",
    "unknown_field",
    "repeatability",
)
KEYWORDS = {
    "positive": ("accept", "allow", "complete", "pass", "round_trip", "success", "valid"),
    "negative": (
        "block",
        "deny",
        "fail",
        "forbid",
        "invalid",
        "mismatch",
        "refus",
        "reject",
        "unsafe",
    ),
    "boundary": ("bound", "ceiling", "empty", "limit", "max", "min", "overflow", "oversiz"),
    "malformed_input": ("invalid", "malformed", "parse", "syntax", "torn"),
    "unknown_field": ("extra", "future", "schema", "unknown"),
    "repeatability": ("canonical", "determin", "idempot", "repeat", "round_trip", "stable"),
}


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def rust_files() -> list[Path]:
    return sorted((ROOT / "crates").glob("*/src/**/*.rs"))


def crate_root(path: Path) -> Path:
    return next(parent for parent in path.parents if parent.parent == ROOT / "crates")


def tests_for_crate(crate: Path) -> dict[str, list[str]]:
    categorized = {category: [] for category in CATEGORIES}
    sources = sorted(crate.glob("src/**/*.rs")) + sorted(crate.glob("tests/**/*.rs"))
    for source in sources:
        text = source.read_text(encoding="utf-8")
        for match in RUST_TEST.finditer(text):
            name = match.group("name")
            reference = f"{relative(source)}::{name}"
            lowered = name.lower()
            for category, keywords in KEYWORDS.items():
                if any(keyword in lowered for keyword in keywords):
                    categorized[category].append(reference)
    for category in categorized:
        categorized[category] = sorted(set(categorized[category]))
    return categorized


def applicability(kind: str, name: str, context: str) -> list[str]:
    lowered = f"{name} {context}".lower()
    categories = {"positive", "repeatability"}
    if kind in {"enum", "struct", "trait", "fn"}:
        categories.add("negative")
    if any(word in lowered for word in ("parse", "decode", "deserialize", "schema", "report", "config")):
        categories.update(("malformed_input", "unknown_field"))
    if any(word in lowered for word in ("bound", "capacity", "count", "identifier", "limit", "quota", "resource", "size", "timeout")):
        categories.add("boundary")
    return [category for category in CATEGORIES if category in categories]


def public_items() -> list[dict[str, object]]:
    cache: dict[Path, dict[str, list[str]]] = {}
    items: list[dict[str, object]] = []
    for source in rust_files():
        text = source.read_text(encoding="utf-8")
        crate = crate_root(source)
        tests = cache.setdefault(crate, tests_for_crate(crate))
        for match in DECLARATION.finditer(text):
            kind = match.group("kind") or "fn"
            name = match.group("name") or match.group("fn_name")
            line = text.count("\n", 0, match.start()) + 1
            context = text[max(0, match.start() - 240) : match.end() + 240]
            applicable = applicability(kind, name, context)
            mappings = {
                category: tests[category][:8]
                for category in applicable
                if tests[category]
            }
            items.append(
                {
                    "id": f"rust:{relative(source)}:{kind}:{name}:{line}",
                    "surface": "rust_public_api",
                    "path": relative(source),
                    "line": line,
                    "kind": kind,
                    "name": name,
                    "applicable_categories": applicable,
                    "test_mappings": mappings,
                }
            )
    return items


def schema_items() -> list[dict[str, object]]:
    items = []
    for path in sorted((ROOT / "schemas").rglob("*.json")):
        document = json.loads(path.read_text(encoding="utf-8"))
        items.append(
            {
                "id": f"schema:{relative(path)}",
                "surface": "json_schema",
                "path": relative(path),
                "line": 1,
                "kind": "schema",
                "name": document.get("title", path.stem),
                "applicable_categories": list(CATEGORIES),
                "test_mappings": {},
            }
        )
    return items


def error_items() -> list[dict[str, object]]:
    items = []
    seen: set[str] = set()
    for source in rust_files():
        text = source.read_text(encoding="utf-8")
        crate = crate_root(source)
        tests = tests_for_crate(crate)
        for match in ERROR_CODE.finditer(text):
            code = match.group("code")
            if code in seen:
                continue
            seen.add(code)
            line = text.count("\n", 0, match.start()) + 1
            mappings = {
                category: tests[category][:8]
                for category in ("positive", "negative", "repeatability")
                if tests[category]
            }
            items.append(
                {
                    "id": f"error:{code}",
                    "surface": "stable_error_code",
                    "path": relative(source),
                    "line": line,
                    "kind": "error_code",
                    "name": code,
                    "applicable_categories": ["positive", "negative", "repeatability"],
                    "test_mappings": mappings,
                }
            )
    return items


def build() -> dict[str, object]:
    items = sorted(public_items() + schema_items() + error_items(), key=lambda item: item["id"])
    gaps = [
        {"id": item["id"], "category": category, "status": "uncovered"}
        for item in items
        for category in item["applicable_categories"]
        if not item["test_mappings"].get(category)
    ]
    encoded = json.dumps(items, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return {
        "schema_version": 1,
        "inventory_id": "codingmage.public_verification.v1",
        "categories": list(CATEGORIES),
        "items_sha256": hashlib.sha256(encoded).hexdigest(),
        "coverage_gaps": gaps,
        "items": items,
    }


def errors(document: dict[str, object]) -> list[str]:
    findings: list[str] = []
    ids: set[str] = set()
    for item in document.get("items", []):
        identifier = item.get("id")
        if not isinstance(identifier, str) or identifier in ids:
            findings.append("inventory.id")
            continue
        ids.add(identifier)
        path = item.get("path")
        if not isinstance(path, str) or not (ROOT / path).is_file():
            findings.append(f"inventory.path:{identifier}")
        mappings = item.get("test_mappings", {})
        for category in item.get("applicable_categories", []):
            references = mappings.get(category, [])
            for reference in references:
                test_path, separator, symbol = reference.partition("::")
                if not separator or not (ROOT / test_path).is_file():
                    findings.append(f"inventory.test:{identifier}:{category}")
                    continue
                test_text = (ROOT / test_path).read_text(encoding="utf-8")
                if not re.search(rf"\b(?:fn|def)\s+{re.escape(symbol)}\s*\(", test_text):
                    findings.append(f"inventory.symbol:{identifier}:{category}:{symbol}")
    return sorted(set(findings))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    generated = build()
    findings = errors(generated)
    if findings:
        print("verification inventory failed:", file=sys.stderr)
        for finding in findings:
            print(finding, file=sys.stderr)
        return 1
    content = json.dumps(generated, sort_keys=True, indent=2) + "\n"
    if args.write:
        OUTPUT.write_text(content, encoding="utf-8")
    elif not OUTPUT.is_file() or OUTPUT.read_text(encoding="utf-8") != content:
        print("verification inventory is stale", file=sys.stderr)
        return 1
    print(
        "verification inventory passed: "
        f"{len(generated['items'])} items, {len(generated['coverage_gaps'])} explicit gaps"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
