#!/usr/bin/env python3
"""Dependency-free documentation, claim, link, and secret checks."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable
from urllib.parse import unquote


IGNORED_PARTS = {".git", "target", ".codingmage", "node_modules"}
TEXT_SUFFIXES = {".md", ".json", ".toml", ".yaml", ".yml", ".py", ".rs", ".txt"}
LINK_PATTERN = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
HEADING_PATTERN = re.compile(r"^(#{1,6})\s+\S")
MERMAID_STARTS = (
    "flowchart ",
    "graph ",
    "sequenceDiagram",
    "stateDiagram",
    "classDiagram",
    "erDiagram",
    "journey",
    "gantt",
    "pie",
    "mindmap",
    "timeline",
    "gitGraph",
)
SECRET_PATTERNS = {
    "aws-access-key": re.compile(r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
    "github-token": re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{30,255}\b"),
    "private-key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    "slack-token": re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"),
}


@dataclass(frozen=True, order=True)
class Finding:
    path: str
    line: int
    rule: str
    message: str

    def render(self) -> str:
        return f"{self.path}:{self.line}: {self.rule}: {self.message}"


def repository_files(root: Path) -> Iterable[Path]:
    for path in sorted(root.rglob("*")):
        if not path.is_file() or any(part in IGNORED_PARTS for part in path.parts):
            continue
        if path.name in {"LICENSE", ".markdownlint.json"} or path.suffix in TEXT_SUFFIXES:
            yield path


def markdown_files(root: Path) -> Iterable[Path]:
    return (path for path in repository_files(root) if path.suffix == ".md")


def relative(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def check_required(root: Path, policy: dict[str, object]) -> list[Finding]:
    findings = []
    for item in policy.get("required_files", []):
        candidate = root / str(item)
        if not candidate.is_file():
            findings.append(Finding(str(item), 1, "required-file", "required file is missing"))
    return findings


def check_markdown(root: Path, path: Path) -> list[Finding]:
    findings: list[Finding] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    display = relative(root, path)
    previous_heading = 0
    in_fence = False

    if not lines or not lines[0].startswith("# "):
        findings.append(Finding(display, 1, "markdown-first-heading", "file must start with one H1"))

    for number, line in enumerate(lines, 1):
        if line.rstrip() != line:
            findings.append(Finding(display, number, "markdown-trailing-space", "trailing whitespace"))
        if "\t" in line:
            findings.append(Finding(display, number, "markdown-tab", "tab character"))
        if line.startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        heading = HEADING_PATTERN.match(line)
        if heading:
            level = len(heading.group(1))
            if previous_heading and level > previous_heading + 1:
                findings.append(Finding(display, number, "markdown-heading-jump", "heading level skipped"))
            previous_heading = level

    if in_fence:
        findings.append(Finding(display, len(lines) or 1, "markdown-fence", "unclosed code fence"))
    return findings


def check_links(root: Path, path: Path) -> list[Finding]:
    findings: list[Finding] = []
    display = relative(root, path)
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        for raw_target in LINK_PATTERN.findall(line):
            target = raw_target.strip().split(maxsplit=1)[0].strip("<>")
            if not target or target.startswith(("#", "http://", "https://", "mailto:")):
                continue
            local = unquote(target.split("#", 1)[0])
            if not local:
                continue
            resolved = (path.parent / local).resolve()
            try:
                resolved.relative_to(root.resolve())
            except ValueError:
                findings.append(Finding(display, number, "local-link-boundary", "link leaves repository"))
                continue
            if not resolved.exists():
                findings.append(Finding(display, number, "local-link", "local link target is missing"))
    return findings


def check_mermaid(root: Path, path: Path) -> list[Finding]:
    findings: list[Finding] = []
    display = relative(root, path)
    lines = path.read_text(encoding="utf-8").splitlines()
    in_mermaid = False
    start_line = 0
    body: list[str] = []

    for number, line in enumerate(lines, 1):
        if not in_mermaid and line.strip() == "```mermaid":
            in_mermaid = True
            start_line = number
            body = []
        elif in_mermaid and line.strip() == "```":
            meaningful = [entry.strip() for entry in body if entry.strip() and not entry.lstrip().startswith("%%")]
            if not meaningful or not meaningful[0].startswith(MERMAID_STARTS):
                findings.append(Finding(display, start_line, "mermaid-declaration", "unknown or missing diagram declaration"))
            in_mermaid = False
        elif in_mermaid:
            body.append(line)

    if in_mermaid:
        findings.append(Finding(display, start_line, "mermaid-fence", "unclosed Mermaid fence"))
    return findings


def check_claims(root: Path, path: Path, policy: dict[str, object]) -> list[Finding]:
    findings: list[Finding] = []
    display = relative(root, path)
    claims = [str(item).casefold() for item in policy.get("prohibited_claims", [])]
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        folded = line.casefold()
        for index, claim in enumerate(claims, 1):
            if claim in folded:
                findings.append(Finding(display, number, f"claim-{index}", "unsupported release or assurance claim"))
    return findings


def check_secrets(root: Path, path: Path) -> list[Finding]:
    findings: list[Finding] = []
    display = relative(root, path)
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except UnicodeDecodeError:
        return findings
    for number, line in enumerate(lines, 1):
        for rule, pattern in SECRET_PATTERNS.items():
            if pattern.search(line):
                findings.append(Finding(display, number, f"secret-{rule}", "possible sensitive value redacted"))
    return findings


def run_checks(root: Path, policy_path: Path | None = None) -> list[Finding]:
    root = root.resolve()
    policy_path = policy_path or root / "docs/check-policy.json"
    try:
        policy = json.loads(policy_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return [Finding(relative(root, policy_path), 1, "policy", f"cannot load check policy: {type(error).__name__}")]

    findings = check_required(root, policy)
    markdown = list(markdown_files(root))
    for path in markdown:
        findings.extend(check_markdown(root, path))
        findings.extend(check_links(root, path))
        findings.extend(check_mermaid(root, path))
        findings.extend(check_claims(root, path, policy))
    for path in repository_files(root):
        findings.extend(check_secrets(root, path))
    return sorted(set(findings))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--policy", type=Path)
    args = parser.parse_args()
    findings = run_checks(args.root, args.policy)
    if findings:
        print(f"documentation checks failed with {len(findings)} finding(s):", file=sys.stderr)
        for finding in findings:
            print(finding.render(), file=sys.stderr)
        return 1
    print("documentation checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
