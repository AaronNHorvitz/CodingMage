from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "docs_check.py"
SPEC = importlib.util.spec_from_file_location("docs_check", SCRIPT)
assert SPEC and SPEC.loader
docs_check = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = docs_check
SPEC.loader.exec_module(docs_check)


class DocumentationChecksTest(unittest.TestCase):
    def fixture(self, markdown: str, claims: list[str] | None = None) -> tuple[Path, tempfile.TemporaryDirectory[str]]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        (root / "README.md").write_text(markdown, encoding="utf-8")
        policy = {"required_files": ["README.md"], "prohibited_claims": claims or []}
        (root / "policy.json").write_text(json.dumps(policy), encoding="utf-8")
        return root, temporary

    def rules(self, root: Path) -> set[str]:
        return {finding.rule for finding in docs_check.run_checks(root, root / "policy.json")}

    def test_valid_markdown_mermaid_and_local_link_pass(self) -> None:
        root, temporary = self.fixture(
            "# Fixture\n\n[Details](details.md)\n\n```mermaid\nflowchart LR\n    A --> B\n```\n"
        )
        self.addCleanup(temporary.cleanup)
        (root / "details.md").write_text("# Details\n", encoding="utf-8")
        self.assertEqual([], docs_check.run_checks(root, root / "policy.json"))

    def test_missing_local_link_fails(self) -> None:
        root, temporary = self.fixture("# Fixture\n\n[Missing](missing.md)\n")
        self.addCleanup(temporary.cleanup)
        self.assertIn("local-link", self.rules(root))

    def test_malformed_mermaid_fails(self) -> None:
        root, temporary = self.fixture("# Fixture\n\n```mermaid\nnot-a-diagram\n```\n")
        self.addCleanup(temporary.cleanup)
        self.assertIn("mermaid-declaration", self.rules(root))

    def test_prohibited_claim_fails(self) -> None:
        claim = "synthetic product has completed every gate"
        root, temporary = self.fixture(f"# Fixture\n\n{claim}.\n", [claim])
        self.addCleanup(temporary.cleanup)
        self.assertIn("claim-1", self.rules(root))

    def test_secret_finding_redacts_value(self) -> None:
        token = "AK" + "IA" + "A" * 16
        root, temporary = self.fixture(f"# Fixture\n\nSynthetic: {token}\n")
        self.addCleanup(temporary.cleanup)
        findings = docs_check.run_checks(root, root / "policy.json")
        rendered = "\n".join(finding.render() for finding in findings)
        self.assertIn("secret-aws-access-key", rendered)
        self.assertNotIn(token, rendered)

    def test_cli_never_prints_secret_value(self) -> None:
        token = "gh" + "p_" + "A" * 30
        root, temporary = self.fixture(f"# Fixture\n\nSynthetic: {token}\n")
        self.addCleanup(temporary.cleanup)
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            findings = docs_check.run_checks(root, root / "policy.json")
            for finding in findings:
                print(finding.render(), file=stderr)
        self.assertNotIn(token, stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
