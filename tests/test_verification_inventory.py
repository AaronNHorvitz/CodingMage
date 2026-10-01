"""Mutation and freshness tests for the public verification inventory."""

from __future__ import annotations

import copy
import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verification_inventory", ROOT / "scripts" / "verification_inventory.py"
)
assert SPEC and SPEC.loader
INVENTORY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INVENTORY)


class VerificationInventoryTests(unittest.TestCase):
    def test_generated_inventory_is_complete_and_fresh(self) -> None:
        document = INVENTORY.build()
        self.assertEqual(INVENTORY.errors(document), [])
        self.assertEqual(document["schema_version"], 1)
        self.assertTrue(document["items"])
        self.assertTrue(document["coverage_gaps"])

    def test_missing_category_mapping_remains_an_explicit_gap(self) -> None:
        document = INVENTORY.build()
        changed = copy.deepcopy(document)
        item = next(value for value in changed["items"] if value["test_mappings"])
        category = next(iter(item["test_mappings"]))
        item["test_mappings"].pop(category)
        gaps = [
            {"id": value["id"], "category": candidate, "status": "uncovered"}
            for value in changed["items"]
            for candidate in value["applicable_categories"]
            if not value["test_mappings"].get(candidate)
        ]
        self.assertIn(
            {"id": item["id"], "category": category, "status": "uncovered"}, gaps
        )

    def test_missing_source_and_test_symbols_fail_closed(self) -> None:
        document = INVENTORY.build()
        changed = copy.deepcopy(document)
        changed["items"][0]["path"] = "missing"
        item = next(value for value in changed["items"] if value["test_mappings"])
        category = next(iter(item["test_mappings"]))
        item["test_mappings"][category] = ["TASKS.md::missing_symbol"]
        findings = INVENTORY.errors(changed)
        self.assertTrue(any(value.startswith("inventory.path:") for value in findings))
        self.assertTrue(any(value.startswith("inventory.symbol:") for value in findings))

    def test_failure_panel_boundary_does_not_follow_adjacent_source(self) -> None:
        path = "crates/codingmage-ui/src/app/mod.rs"
        expected = ["positive", "negative", "boundary", "repeatability"]
        for neighbor in ("Copy identifier", "Unrelated navigation", "oversized input"):
            self.assertEqual(
                INVENTORY.applicability(path, "fn", "failure_box", neighbor),
                expected,
            )
        document = INVENTORY.build()
        item = next(
            entry
            for entry in document["items"]
            if entry["path"] == path
            and entry["kind"] == "fn"
            and entry["name"] == "failure_box"
        )
        self.assertEqual(item["applicable_categories"], expected)
        self.assertTrue(item["test_mappings"].get("boundary"))

    def test_setup_authorization_categories_do_not_follow_adjacent_text(self) -> None:
        cases = (
            ("crates/codingmage-ui/src/app/setup_screen.rs", "fn", "apply_authorization_record", "boundary"),
            ("crates/codingmage-ui/src/app/setup_screen.rs", "fn", "accept_authorization_write", "unknown_field"),
            ("crates/codingmage-ui/src/app/setup_screen.rs", "struct", "SetupState", "malformed_input"),
            ("crates/codingmage-ui/src/backend/worker.rs", "struct", "Binding", "boundary"),
        )
        for path, kind, name, required in cases:
            first = INVENTORY.applicability(path, kind, name, "unrelated navigation")
            second = INVENTORY.applicability(path, kind, name, "oversized malformed schema")
            self.assertEqual(first, second)
            self.assertIn(required, first)

    def test_campaign_setup_categories_do_not_follow_adjacent_text(self) -> None:
        cases = (
            ("crates/codingmage-ui/src/app/setup_screen.rs", "apply_campaign_form", "boundary"),
            ("crates/codingmage-ui/src/app/setup_screen.rs", "accept_campaign_write", "unknown_field"),
            ("crates/codingmage-ui/src/setup.rs", "build_with_authorization_digest", "malformed_input"),
        )
        for path, name, required in cases:
            first = INVENTORY.applicability(path, "fn", name, "unrelated navigation")
            second = INVENTORY.applicability(path, "fn", name, "oversized malformed schema")
            self.assertEqual(first, second)
            self.assertIn(required, first)


if __name__ == "__main__":
    unittest.main()
