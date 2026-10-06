"""Installed shared schema loading, inheritance and execution against authored truth."""

import copy
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import yamaa
import yamaa_native
from yamaa.adapters import native_specification
from yamaa.adapters._native_schema_wire import (
    NativeSchemaLimitError,
    NativeSchemaUnsupportedError,
)
from yamaa.adapters.native_datasets import execute_with_source_provider
from yamaa.io import ProjectResources, load_source_tables, render_artifact
from yamaa.schema import resolve_specification
from yamaa.specification import SpecificationError
from yamaa.specification import schema as reference
from yamaa.specification._yaml import read_yaml_document

ROOT = Path(__file__).parent
SCHEMA = ROOT / "specification-yaml"


class InstalledSchemaLoading(unittest.TestCase):
    def setUp(self):
        self.assertIn("site-packages", str(Path(yamaa.__file__).resolve()))
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        self.stack = self.enterContext(ExitStack())
        for name in (
            "load_schema_bundle",
            "_validate_schema_bundle",
            "_expand_class_fields",
            "_parse_type_expression",
            "_validate_type",
            "_validate_single",
            "_validate_descriptor",
            "_normalize_type",
            "_normalize_single",
        ):
            self.stack.enter_context(
                patch.object(
                    reference,
                    name,
                    side_effect=AssertionError(
                        f"reference schema interpreter invoked: {name}"
                    ),
                )
            )

    def test_inheritance_and_column_composition_match_committed_resolved_artifacts(
        self,
    ):
        bundle = native_specification.load_schema_bundle(SCHEMA)
        for directory in (
            "specification-inheritance",
            "specification-column-composition",
            "specification-inherited-output",
        ):
            with self.subTest(directory=directory):
                case = ROOT / directory
                resolved = resolve_specification(case / "spec_study.yaml", bundle)
                expected = read_yaml_document(case / "expected/spec_resolved.yaml")
                self.assertEqual(resolved.document, expected)
                loaded = native_specification.load_specification(
                    case / "spec_study.yaml", SCHEMA
                )
                self.assertEqual(loaded.specification, resolved.specification)
                self.assertIsNone(loaded.specification.parents)
        self.assertFalse(yamaa_native.engine_info()["execution_supported"])

    def test_loaded_specifications_execute_to_unchanged_artifact_bytes(self):
        for directory, artifact in (
            ("specification-adlb", "adlb.csv"),
            ("specification-windows", "advs.csv"),
            ("specification-advs-bmi", "advs.csv"),
        ):
            with self.subTest(directory=directory):
                case = ROOT / directory
                spec = native_specification.load_specification(
                    case / "spec.yaml", SCHEMA
                ).specification
                calls = []

                def provider(declarations, calls=calls, case=case):
                    calls.append("read")
                    return load_source_tables(declarations, ProjectResources(case))

                actual = execute_with_source_provider(spec, provider)
                self.assertEqual(calls, ["read"])
                self.assertEqual(actual.result.status, "success")
                self.assertEqual(
                    render_artifact(actual.result.artifact),
                    (case / "expected" / artifact).read_bytes(),
                )

    def test_version_precedence_and_constraint_context_are_preserved(self):
        bundle = native_specification.load_schema_bundle(SCHEMA)
        diagnostics = reference.validate_document(
            {"schema_version": "old", "other": None}, bundle, "root_class"
        )
        self.assertEqual(
            [d.model_dump() for d in diagnostics],
            [
                {
                    "phase": "validation",
                    "condition": "schema_version_mismatch",
                    "spec_paths": ("schema_version",),
                    "requirement": None,
                    "context": {"expected": "1.0", "actual": "old"},
                }
            ],
        )
        descriptor = {"type": "identifier"}
        diagnostics = reference.validate_descriptor_value(
            "bad name", descriptor, bundle, "columns[0].name"
        )
        self.assertEqual(len(diagnostics), 1)
        self.assertEqual(diagnostics[0].spec_paths, ("columns[0].name",))
        self.assertIn("pattern", diagnostics[0].context)
        self.assertEqual(reference.matching_type("DM", [], bundle, fragment=True), None)
        self.assertEqual(
            reference.matching_type("DM", ["identifier", "str"], bundle), "identifier"
        )

    def test_service_capture_precedes_yaml_io_and_metadata_is_independent(self):
        native = SimpleNamespace(_compile_schema=yamaa_native._compile_schema)
        original_read = native_specification.read_yaml_bytes
        effects = []

        def read(raw, path):
            effects.append(path.name)
            native._compile_schema = lambda _: self.fail("service replaced after IO")
            return original_read(raw, path)

        with patch.object(native_specification, "read_yaml_bytes", side_effect=read):
            bundle = native_specification.load_schema_bundle(SCHEMA, native=native)
        self.assertGreater(len(effects), 1)
        descriptor = copy.deepcopy(reference.class_fields(bundle, "root_class")["keys"])
        bundle.classes.clear()
        bundle.aliases.clear()
        bundle.registries.clear()
        with patch.object(
            yamaa_native, "_compile_schema", side_effect=AssertionError("recompile")
        ):
            self.assertEqual(
                reference.normalize_descriptor_value(["ID"], descriptor, bundle), ["ID"]
            )
            self.assertEqual(
                reference.matching_type("ID", "identifier", bundle), "identifier"
            )
        descriptor["unexpected"] = True
        with self.assertRaises(NativeSchemaUnsupportedError) as error:
            reference.validate_descriptor_value([], descriptor, bundle, "keys")
        self.assertEqual(error.exception.feature, "uncaptured_descriptor")

    def test_structure_defaults_field_reuse_and_fragment_semantics(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "schema.yaml").write_text(
                """version: "1.0"
root_class:
  - schema_version: {type: str}
  - fields_from: base
base:
  - item: {type: record}
record:
  - name: {type: str, required: true}
  - enabled: {type: bool, default: true}
""",
                encoding="ascii",
            )
            bundle = native_specification.load_schema_bundle(root)
            descriptor = reference.class_fields(bundle, "root_class")["item"]
            self.assertEqual(
                reference.validate_descriptor_value(
                    {}, descriptor, bundle, "item", fragment=True
                ),
                [],
            )
            self.assertEqual(
                reference.normalize_descriptor_value(
                    {}, descriptor, bundle, fragment=True
                ),
                {},
            )
            self.assertEqual(
                reference.normalize_descriptor_value(
                    {"name": "DM"}, descriptor, bundle
                ),
                {"name": "DM", "enabled": True},
            )
            self.assertEqual(
                reference.validate_descriptor_value({}, descriptor, bundle, "item")[
                    0
                ].spec_paths,
                ("item.name",),
            )
            (root / "schema.yaml").write_text(
                """version: "1.0"
root_class:
  - x: {type: str, default: 2}
""",
                encoding="ascii",
            )
            with self.assertRaises(SpecificationError) as error:
                native_specification.load_schema_bundle(root)
            self.assertEqual(
                error.exception.native_outcome["status"], "invalid_defaults"
            )
            self.assertEqual(
                error.exception.diagnostics[0].context["reason"],
                "root_class.x.default: invalid default (invalid_field_type)",
            )

    def test_closed_includes_and_yaml_restrictions_precede_specification_io(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outside = root / "outside.yaml"
            outside.write_text("not: a schema", encoding="ascii")
            bundle_root = root / "bundle"
            bundle_root.mkdir()
            (bundle_root / "schema_other.yaml").symlink_to(outside)
            for include in [
                "../outside.yaml",
                "schema_other.yaml",
                "schema_missing.yaml",
            ]:
                with self.subTest(include=include):
                    (bundle_root / "schema.yaml").write_text(
                        f'version: "1.0"\nincludes: [{include}]\nroot_class: []\n',
                        encoding="ascii",
                    )
                    effects = []
                    original_read = native_specification.read_yaml_bytes

                    def read(raw, path, effects=effects, original_read=original_read):
                        effects.append(path.name)
                        return original_read(raw, path)

                    with (
                        patch.object(
                            native_specification, "read_yaml_bytes", side_effect=read
                        ),
                        self.assertRaises(SpecificationError),
                    ):
                        native_specification.load_specification(
                            root / "missing-spec.yaml", bundle_root
                        )
                    self.assertEqual(effects, ["schema.yaml"])
            for invalid in [
                'version: "1.0"\nversion: "1.0"',
                'version: "1.0"\nroot_class: &a []',
            ]:
                (bundle_root / "schema.yaml").write_text(invalid, encoding="ascii")
                with self.assertRaises(SpecificationError) as error:
                    native_specification.load_schema_bundle(bundle_root)
                self.assertEqual(
                    error.exception.diagnostics[0].condition, "invalid_yaml"
                )

    def test_environment_entry_and_policy_refusal_are_separate(self):
        bundle = native_specification.load_schema_bundle(
            SCHEMA, entry_name="schema_environment.yaml", root_class="environment_class"
        )
        diagnostics = reference.validate_document(
            {"schema_version": "old"}, bundle, "environment_class"
        )
        self.assertEqual(diagnostics[0].condition, "schema_version_mismatch")
        value = None
        for _ in range(65):
            value = [value]
        with self.assertRaises(NativeSchemaLimitError) as error:
            reference.matching_type(value, "list", bundle)
        self.assertEqual(
            (error.exception.phase, error.exception.resource),
            ("decoded_document", "depth"),
        )


if __name__ == "__main__":
    unittest.main()
