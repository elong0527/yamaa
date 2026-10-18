"""Regression gates for honest supplemental inventories and executable evidence."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.conformance import supplemental_inventory as module


class RepositoryCatalog(unittest.TestCase):
    def test_checkout_catalog_materializes_by_discovery(self):
        root = Path(__file__).resolve().parents[2]
        catalog = module.load_catalog(
            root, root / "rust/qualification/supplemental.json"
        )
        self.assertEqual(
            {suite["path"] for suite in catalog["suites"]},
            {
                p.relative_to(root).as_posix()
                for pattern in (
                    "python/tests/installed/installed_*.py",
                    "R/yamaa/tests/*.R",
                )
                for p in root.glob(pattern)
            },
        )
        self.assertTrue(catalog["contracts"])
        self.assertTrue(catalog["support_files"])

    def test_reference_consumers_cannot_reenter_installed_qualification(self):
        for source in (
            "from yamaa.io.project import ProjectResources",
            "from yamaa.io.odm import reference_walker",
            "from yamaa import yamaa_domain",
            "from yamaa.resources import MemoryResolver",
            "from yamaa import resources",
            "import yamaa.expressions.numeric as parser",
            "def lazy():\n    from yamaa.models import TypedColumn",
            "import importlib\nimportlib.import_module('yamaa.runtime')",
            "from importlib import import_module as load\nload('yamaa.specification')",
            "import importlib\nimportlib.import_module(name='yamaa.io.project')",
            ("from importlib import import_module as load\nload(name='yamaa.runtime')"),
            (
                "import importlib\n"
                "importlib.import_module('.io.project', package='yamaa')"
            ),
            (
                "import importlib\n"
                "importlib.import_module(name='.io.project', package='yamaa')"
            ),
            "import importlib\nimportlib.import_module('.runtime', 'yamaa')",
            (
                "import importlib\n"
                "importlib.import_module('..io.project', package='yamaa.subpackage')"
            ),
            "__import__(name='yamaa.io.project')",
            (
                "__import__(name='io.project', "
                "globals={'__package__': 'yamaa'}, level=1)"
            ),
            "__import__('io.project', {'__package__': 'yamaa'}, None, (), 1)",
        ):
            with (
                self.subTest(source=source),
                self.assertRaisesRegex(ValueError, "reference semantics"),
            ):
                module.require_native_consumer(source, "installed_control.py")
        module.require_native_consumer(
            "import yamaa\nfrom yamaa import spec, project, read_odm, _native\n"
            "blocked = ('yamaa.io.project', 'yamaa.runtime', 'yamaa.resources')\n"
            "import importlib\nimportlib.import_module('yamaa._locked_functions')",
            "installed_control.py",
        )
        # The keyword and relative spellings of a native import stay allowed.
        module.require_native_consumer(
            "import importlib\n"
            "importlib.import_module(name='yamaa._locked_functions')\n"
            "importlib.import_module('.helpers', package='installed_suite')\n"
            "importlib.import_module('..helpers', package='installed_suite.sub')",
            "installed_control.py",
        )

    def test_unresolved_relative_dynamic_imports_are_rejected(self):
        # A relative name whose anchor package cannot be read statically
        # must not be classified as native from its remainder alone.
        for source in (
            "import importlib\nimportlib.import_module('.io.project')",
            "import importlib\nimportlib.import_module('.io.project', package=package)",
            "__import__('io.project', globals(), None, (), 1)",
        ):
            with (
                self.subTest(source=source),
                self.assertRaisesRegex(ValueError, "unresolved relative"),
            ):
                module.require_native_consumer(source, "installed_control.py")

    def test_non_literal_and_indirect_dynamic_imports_are_rejected(self):
        # A dynamic import whose target is not a constant string cannot be
        # classified, so it must fail closed instead of being allowed; the
        # same holds for import functions reached through getattr, a
        # __dict__ subscript, an assignment alias, or eval/exec.
        for source in (
            "import importlib\nimportlib.import_module('yamaa.' + 'io')",
            "import importlib\nimportlib.import_module(f'yamaa.io')",
            "import importlib\ntarget = 'yamaa.io'\nimportlib.import_module(target)",
            "import importlib\nimportlib.import_module()",
            "__import__('yamaa.' + 'io')",
            "__import__('io.project', globals(), None, (), level)",
            "import builtins\nbuiltins.__import__('yamaa.io')",
            "from builtins import __import__ as load\nload('yamaa.io')",
            "import importlib\ngetattr(importlib, 'import_module')('yamaa.io')",
            "import importlib\ngetattr(importlib, attr)('yamaa.io')",
            "import importlib\nimportlib.__dict__['import_module']('yamaa.io')",
            "import importlib\nload = importlib.import_module\nload('yamaa.io')",
            (
                "import importlib\n"
                "load = getattr(importlib, 'import_module')\nload('yamaa.io')"
            ),
            "eval(\"__import__('yamaa.io')\")",
            "exec('import yamaa.io')",
            "eval(code)",
            # A guard delegation does not exempt the wrapper itself:
            # direct calls to a delegating function are classified, and
            # a parameter with a default or a reassignment in the body
            # (or a lambda parameter) is not an exemption at all.
            (
                "import builtins\n"
                "original = builtins.__import__\n"
                "def load(name):\n"
                "    return original(name)\n"
                "load('yamaa.io')"
            ),
            (
                "import builtins\n"
                "original = builtins.__import__\n"
                "def reject(name='yamaa.io'):\n"
                "    return original(name)\n"
            ),
            (
                "import builtins\n"
                "original = builtins.__import__\n"
                "def reject(name):\n"
                "    name = 'yamaa.io'\n"
                "    return original(name)\n"
            ),
            (
                "import builtins\n"
                "original = builtins.__import__\n"
                "load = lambda name: original(name)\n"
                "load('yamaa.io')"
            ),
            # Eval/exec bodies share the outer alias tables, and aliases
            # defined inside an evaluated body are visible outside it.
            "from importlib import import_module as load\neval(\"load('yamaa.io')\")",
            (
                "import builtins\n"
                "original = builtins.__import__\n"
                "exec(\"original('yamaa.io')\")"
            ),
            "exec(\"from importlib import import_module as load\")\nload('yamaa.io')",
            # An import function bound to the name eval is an import
            # call first: its argument is a module target, not code.
            "from importlib import import_module as eval\neval('yamaa.io')",
        ):
            with (
                self.subTest(source=source),
                self.assertRaisesRegex(ValueError, "unresolved|reference semantics"),
            ):
                module.require_native_consumer(source, "installed_control.py")
        # The suites' runtime-guard delegation stays allowed: the saved
        # builtins.__import__ called with the guard function's own
        # parameter. The exemption is syntactic; this test does not
        # exercise runtime validation of that parameter.
        module.require_native_consumer(
            "import builtins\n"
            "original = builtins.__import__\n"
            "def reject(name, *args, **kwargs):\n"
            "    return original(name, *args, **kwargs)\n",
            "installed_control.py",
        )
        # Non-import getattr calls and literal native eval stay allowed.
        module.require_native_consumer(
            "value = getattr(obj, 'method')()\neval(\"__import__('yamaa')\")\n",
            "installed_control.py",
        )

    def test_sas_canonical_inputs_and_truth_stage_without_ownership_documents(self):
        prefix = "rust/crates/yamaa-adapters/tests/fixtures/sas/"
        catalog = {
            "contracts": [{"path": prefix + "compressed-truth.csv"}],
            "support_files": [
                {"path": prefix + "v5.xpt", "role": "input"},
                {"path": prefix + "LICENSE-haven.txt", "role": "documentation"},
                {"path": "rust/tests/host-fixtures/sas-cases.json", "role": "input"},
            ],
        }
        self.assertEqual(
            module.host_fixture_files(catalog),
            [
                (
                    prefix + "compressed-truth.csv",
                    "host-fixtures/sas/compressed-truth.csv",
                ),
                (prefix + "v5.xpt", "host-fixtures/sas/v5.xpt"),
                (
                    "rust/tests/host-fixtures/sas-cases.json",
                    "host-fixtures/sas-cases.json",
                ),
                ("tools/conformance/host_fixtures.py", "host_fixtures.py"),
            ],
        )

    def test_sas_fixture_cannot_overwrite_an_existing_shared_host_input(self):
        catalog = {
            "contracts": [],
            "support_files": [
                {"path": "rust/tests/host-fixtures/sas/v5.xpt", "role": "input"},
                {
                    "path": "rust/crates/yamaa-adapters/tests/fixtures/sas/v5.xpt",
                    "role": "input",
                },
            ],
        }
        with self.assertRaisesRegex(ValueError, "duplicate staged host fixture path"):
            module.host_fixture_files(catalog)


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
        self.write("tools/conformance/host_fixtures.py", "# staged truth codec\n")
        (self.staged / "host_fixtures.py").write_text("# staged truth codec\n")
        # The committed catalog keeps only underivable data; suites and
        # support fixtures below are discovered by glob instead.
        self.catalog_file = {
            "version": "2",
            "contracts": [
                {
                    "path": self.contract,
                    "family": "numeric",
                    "level": "component",
                    "consumers": ["python/tests/installed/installed_first.py"],
                }
            ],
            "suite_levels": {},
            "suite_ids": {},
            "support_roles": {},
        }
        for name, code in (
            ("first", 'print("failed contract")\nraise SystemExit(1)\n'),
            ("later", 'raise AssertionError("must remain unrun")\n'),
        ):
            filename = "installed_" + name + ".py"
            path = "python/tests/installed/" + filename
            self.write(path, code)
            (self.staged / filename).write_text(code)
        self.catalog_path = self.directory / "catalog.json"
        self.save_catalog_file()
        self.catalog = module.load_catalog(self.root, self.catalog_path)
        self.metadata = {
            "runtime_version": "test",
            "core_version": "test",
            "package_location": str(self.directory / "site-packages/native.so"),
        }

    def write(self, name, contents):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents)

    def save_catalog_file(self):
        self.catalog_path.write_text(json.dumps(self.catalog_file))

    def test_added_fixture_and_suite_are_discovered_without_a_catalog_edit(self):
        self.write("rust/crates/example/tests/fixtures/new.tsv", "id\na\n")
        catalog = module.load_catalog(self.root, self.catalog_path)
        self.assertIn(
            {"path": "rust/crates/example/tests/fixtures/new.tsv", "role": "input"},
            catalog["support_files"],
        )
        (self.root / "rust/crates/example/tests/fixtures/new.tsv").unlink()
        self.write("R/yamaa/tests/new.R", "stopifnot(TRUE)\n")
        catalog = module.load_catalog(self.root, self.catalog_path)
        self.assertIn("r/new", {suite["id"] for suite in catalog["suites"]})

    def test_removed_contract_consumer_is_rejected(self):
        (self.root / "python/tests/installed/installed_first.py").unlink()
        with self.assertRaisesRegex(ValueError, "missing contract consumer"):
            module.load_catalog(self.root, self.catalog_path)

    def test_whole_run_claim_is_invalid_for_supplemental_contracts(self):
        self.catalog_file["contracts"][0]["level"] = "shared_run"
        self.save_catalog_file()
        with self.assertRaisesRegex(ValueError, "invalid supplemental level"):
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

    def test_actual_suite_receives_its_checked_source_and_installed_artifact_provenance(
        self,
    ):
        code = 'import os\nprint(os.environ["YAMAA_SUPPLEMENTAL_EVIDENCE"])\n'
        self.write("python/tests/installed/installed_first.py", code)
        (self.staged / "installed_first.py").write_text(code)
        self.catalog["suites"] = self.catalog["suites"][:1]
        evidence = {
            "source_revision": "checked-revision",
            "artifact_reference": "installed-archive",
        }
        with (
            patch.object(module, "installed_metadata", return_value=self.metadata),
            patch.dict(
                module.os.environ,
                {"YAMAA_SUPPLEMENTAL_EVIDENCE": "stale inherited claim"},
            ),
        ):
            status = module.run_suites(
                self.root,
                self.catalog,
                runtime="python",
                staged_root=self.staged,
                output=self.output,
                evidence=evidence,
            )
        self.assertEqual(status, 0)
        report = json.loads((self.output / "python-first.log").read_text())
        self.assertEqual(report["evidence"], evidence)
        self.assertEqual(report["installation"], self.metadata)

    def test_suite_identity_override_must_stay_a_simple_name(self):
        # A nested identity would map two suites onto the same log name
        # after slash replacement.
        self.catalog_file["suite_ids"] = {
            "python/tests/installed/installed_first.py": "python/first/nested"
        }
        self.save_catalog_file()
        with self.assertRaisesRegex(ValueError, "host/simple-name"):
            module.load_catalog(self.root, self.catalog_path)

    def test_malformed_suite_identity_override_is_not_replaced(self):
        for value in ("", ["python/first"]):
            with self.subTest(value=value):
                self.catalog_file["suite_ids"] = {
                    "python/tests/installed/installed_first.py": value
                }
                self.save_catalog_file()
                with self.assertRaisesRegex(ValueError, "host/simple-name"):
                    module.load_catalog(self.root, self.catalog_path)

    def test_stale_suite_level_override_is_rejected(self):
        self.catalog_file["suite_levels"] = {"python/ghost": "compile"}
        self.save_catalog_file()
        with self.assertRaisesRegex(ValueError, "stale suite level"):
            module.load_catalog(self.root, self.catalog_path)

    def test_unhashable_contract_path_is_rejected(self):
        for value in (["x"], {"path": "x"}):
            with self.subTest(value=value):
                self.catalog_file["contracts"][0]["path"] = value
                self.save_catalog_file()
                with self.assertRaisesRegex(ValueError, "invalid relative path"):
                    module.load_catalog(self.root, self.catalog_path)

    def test_unhashable_contract_consumer_is_rejected(self):
        for value in (["x"], {"path": "x"}):
            with self.subTest(value=value):
                self.catalog_file["contracts"][0]["consumers"] = [value]
                self.save_catalog_file()
                with self.assertRaisesRegex(ValueError, "invalid relative path"):
                    module.load_catalog(self.root, self.catalog_path)

    def test_inventory_paths_cannot_escape_the_checkout(self):
        self.catalog_file["contracts"][0]["consumers"] = ["../catalog.json"]
        self.save_catalog_file()
        with self.assertRaisesRegex(ValueError, "path leaves root"):
            module.load_catalog(self.root, self.catalog_path)

    def test_discovered_fixture_cannot_link_outside_the_checkout(self):
        outside = self.directory / "outside.tsv"
        outside.write_text("id\na\n")
        link = self.root / "rust/tests/host-fixtures/linked.tsv"
        link.parent.mkdir(parents=True, exist_ok=True)
        link.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "path leaves root"):
            module.load_catalog(self.root, self.catalog_path)


if __name__ == "__main__":
    unittest.main()
