"""Regression gates for honest supplemental inventories and executable evidence."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "supplemental_inventory",
    Path(__file__).resolve().parents[1] / "tools/supplemental_inventory.py",
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SupplementalEvidence(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.root = self.directory / "checkout"
        self.staged = self.directory / "staged"
        self.staged.mkdir()
        self.output = self.directory / "evidence"
        self.contract = "rust/crates/example/tests/fixtures/cases.tsv"
        self.write(self.contract, "id\texpected\na\ttrue\n")
        self.catalog = {
            "version": "1",
            "support_files": [],
            "contracts": [
                {
                    "path": self.contract,
                    "family": "numeric",
                    "level": "component",
                    "consumers": ["rust/tests/installed_first.py"],
                }
            ],
            "suites": [],
        }
        for name, code in (
            ("first", 'print("failed contract")\nraise SystemExit(1)\n'),
            ("later", 'raise AssertionError("must remain unrun")\n'),
        ):
            filename = "installed_" + name + ".py"
            path = "rust/tests/" + filename
            self.write(path, code)
            (self.staged / filename).write_text(code)
            self.catalog["suites"].append(
                {
                    "id": "python/" + name,
                    "runtime": "python",
                    "path": path,
                    "staged_path": filename,
                    "level": "component",
                }
            )
        self.catalog_path = self.directory / "catalog.json"
        self.catalog_path.write_text(json.dumps(self.catalog))
        self.metadata = {
            "runtime_version": "test",
            "core_version": "test",
            "package_location": str(self.directory / "site-packages/native.so"),
        }

    def write(self, name, contents):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents)

    def test_reconciliation_rejects_an_added_contract_and_a_removed_suite(self):
        module.load_catalog(self.root, self.catalog_path)
        self.write("rust/crates/example/tests/fixtures/new.tsv", "id\na\n")
        with self.assertRaisesRegex(ValueError, "uncataloged"):
            module.load_catalog(self.root, self.catalog_path)
        (self.root / "rust/crates/example/tests/fixtures/new.tsv").unlink()
        (self.root / "rust/tests/installed_first.py").unlink()
        with self.assertRaisesRegex(ValueError, "missing contract consumer"):
            module.load_catalog(self.root, self.catalog_path)

    def test_whole_run_claim_is_invalid_for_supplemental_contracts(self):
        self.catalog["contracts"][0]["level"] = "shared_run"
        self.catalog_path.write_text(json.dumps(self.catalog))
        with self.assertRaisesRegex(ValueError, "invalid supplemental level"):
            module.load_catalog(self.root, self.catalog_path)

    def test_unknown_suite_cannot_disappear_from_inventory(self):
        self.write("R/yamaanative/tests/new.R", "stopifnot(TRUE)\n")
        with self.assertRaisesRegex(ValueError, "suites differ"):
            module.load_catalog(self.root, self.catalog_path)

    def test_failed_command_retains_its_log_and_unrun_suffix(self):
        evidence = {"source_revision": "revision", "artifact_reference": "archive"}
        with patch.object(module, "installed_metadata", return_value=self.metadata):
            status = module.run_suites(
                self.root,
                self.catalog,
                runtime="python",
                staged_root=self.staged,
                output=self.output,
                evidence=evidence,
            )
        self.assertEqual(status, 1)
        report = json.loads((self.output / "supplemental.json").read_text())
        self.assertEqual(report["evidence"], evidence)
        self.assertEqual(
            [r["result"] for r in report["suites"]], ["failure", "not_exercised"]
        )
        self.assertEqual(report["suites"][0]["exit_code"], 1)
        self.assertIn(
            "failed contract", (self.output / report["suites"][0]["log"]).read_text()
        )
        self.assertIsNone(report["suites"][1]["log"])

    def test_modified_staged_script_is_not_tested_revision_evidence(self):
        (self.staged / "installed_later.py").write_text("print('different')\n")
        with (
            patch.object(module, "installed_metadata", return_value=self.metadata),
            self.assertRaisesRegex(ValueError, "staged suite differs"),
        ):
            module.run_suites(
                self.root,
                self.catalog,
                runtime="python",
                staged_root=self.staged,
                output=self.output,
                evidence={},
            )
        self.assertFalse(self.output.exists())

    def test_distinct_suite_ids_cannot_overwrite_the_same_log(self):
        # Both IDs used to map to python-first-nested.log after slash replacement.
        self.catalog["suites"][0]["id"] = "python/first/nested"
        self.catalog["suites"][1]["id"] = "python/first-nested"
        self.catalog_path.write_text(json.dumps(self.catalog))
        with self.assertRaisesRegex(ValueError, "host/simple-name"):
            module.load_catalog(self.root, self.catalog_path)

    def test_inventory_paths_cannot_escape_the_checkout(self):
        self.catalog["contracts"][0]["consumers"] = ["../catalog.json"]
        self.catalog_path.write_text(json.dumps(self.catalog))
        with self.assertRaisesRegex(ValueError, "path leaves root"):
            module.load_catalog(self.root, self.catalog_path)


if __name__ == "__main__":
    unittest.main()
