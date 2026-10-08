"""Installed shared schema loading, inheritance and execution against authored truth."""

import copy
import json
import tempfile
import unittest
from contextlib import ExitStack
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import yamaa
from yamaa import _native as yamaa_native
from yamaa.adapters import native_specification
from yamaa.adapters._native_schema_interpreter import NativeSchemaInterpreter
from yamaa.adapters._native_schema_wire import (
    NativeSchemaLimitError,
    NativeSchemaUnsupportedError,
    decode_nodes,
    encode_tree,
    request,
    response,
)
from yamaa.adapters._native_yaml import NativeYamlReader
from yamaa.adapters.native_datasets import execute_with_source_provider
from yamaa.io import ProjectResources, load_source_tables, render_artifact
from yamaa.schema import resolve_specification
from yamaa.specification import SpecificationError
from yamaa.specification import schema as reference
from yamaa.specification._yaml import read_yaml_document

ROOT = Path(__file__).parent
SCHEMA = ROOT / "specification-yaml"


class InstalledSchemaLoading(unittest.TestCase):
    def test_shared_traversal_preserves_implicated_file_context(self):
        """REQ-0653/0654/0656 source identities survive the installed Python boundary."""
        bundle = native_specification.load_schema_bundle(SCHEMA)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            entry, parent = root / "entry.yaml", root / "parent.yaml"
            entry.write_text(
                'schema_version: "1.0"\nparents: parent.yaml\n', encoding="ascii"
            )
            for document, condition, path, requirement, context in (
                (
                    {"schema_version": "0.0"},
                    "schema_version_mismatch",
                    "parents",
                    "REQ-0656",
                    {
                        "entry_version": "1.0",
                        "parent_version": "0.0",
                        "source": str(parent),
                        "entry": str(entry),
                    },
                ),
                (
                    {"parents": []},
                    "schema_version_mismatch",
                    "schema_version",
                    "REQ-0656",
                    {
                        "expected": "1.0",
                        "actual": None,
                        "source": str(parent),
                        "entry": str(entry),
                    },
                ),
                (
                    {
                        "schema_version": "1.0",
                        "parents": "https://example.test/base.yaml",
                    },
                    "invalid_parent_path",
                    "parents",
                    "REQ-0653",
                    {
                        "reason": "remote_reference",
                        "source": str(parent),
                        "parent": "https://example.test/base.yaml",
                    },
                ),
                (
                    {"schema_version": "1.0", "parents": "missing.yaml"},
                    "parent_not_found",
                    "parents",
                    "REQ-0654",
                    {"path": "missing.yaml", "source": str(parent)},
                ),
                (
                    {"schema_version": "1.0", "unexpected": "value"},
                    "unknown_field",
                    "unexpected",
                    "REQ-0658",
                    {
                        "field": "unexpected",
                        "class": "root_class",
                        "source": str(parent),
                        "entry": str(entry),
                    },
                ),
            ):
                with self.subTest(document=document):
                    parent.write_text(json.dumps(document), encoding="ascii")
                    with self.assertRaises(SpecificationError) as caught:
                        resolve_specification(entry, bundle)
                    self.assertEqual(
                        [
                            d.model_dump(mode="json")
                            for d in caught.exception.diagnostics
                        ],
                        [
                            {
                                "phase": "validation",
                                "condition": condition,
                                "spec_paths": [path],
                                "requirement": requirement,
                                "context": context,
                            }
                        ],
                    )

            from yamaa.schema import inheritance

            original = inheritance.read_bundle_document
            reads = []

            def read(path, captured):
                reads.append(path)
                if path == parent:
                    raise OSError("unreadable parent")
                return original(path, captured)

            with (
                patch(
                    "yamaa.schema.inheritance.read_bundle_document", side_effect=read
                ),
                self.assertRaises(SpecificationError) as caught,
            ):
                resolve_specification(entry, bundle)
            self.assertEqual(
                caught.exception.diagnostics[0].context,
                {"path": str(parent), "source": str(entry)},
            )
            self.assertEqual(reads, [entry, parent])

    def test_shared_traversal_deduplicates_filesystem_symlink_identity(self):
        from yamaa.schema import inheritance

        bundle = native_specification.load_schema_bundle(SCHEMA)
        original = inheritance.read_bundle_document
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            parent = root / "base.yaml"
            parent.write_text(
                json.dumps(
                    {
                        "schema_version": "1.0",
                        "domain": "OUT",
                        "keys": ["ID"],
                        "input": {"DM": "data.csv"},
                        "base": "DM",
                        "output": {"path": "out.csv", "columns": ["ID"]},
                        "columns": [
                            {"name": "ID", "type": "str", "derivation": "DM.ID"}
                        ],
                    }
                ),
                encoding="ascii",
            )
            (root / "alias.yaml").symlink_to(parent)
            entry = root / "entry.yaml"
            entry.write_text(
                'schema_version: "1.0"\nparents: [base.yaml, alias.yaml]\n',
                encoding="ascii",
            )
            reads = []

            def read(path, captured):
                reads.append(Path(path))
                return original(path, captured)

            with patch(
                "yamaa.schema.inheritance.read_bundle_document", side_effect=read
            ):
                result = resolve_specification(entry, bundle)
            self.assertEqual(reads, [entry, parent])
            self.assertEqual(result.layers, (parent, entry))
            self.assertEqual(result.provenance["input.DM.path"].file, parent)
            self.assertEqual(result.layer_paths["input.DM.path"].written, "data.csv")

    def test_shared_traversal_reports_complete_canonical_cycles_without_rereads(self):
        """REQ-0655 keeps non-entry cycles and source access order in installed Rust."""
        from yamaa.schema import inheritance

        bundle = native_specification.load_schema_bundle(SCHEMA)
        original = inheritance.read_bundle_document
        for repeated in ("entry.yaml", "a.yaml"):
            with (
                self.subTest(repeated=repeated),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory).resolve()
                (root / "nested").mkdir()
                for name, parent in (
                    ("entry.yaml", "a.yaml"),
                    ("a.yaml", "b.yaml"),
                    ("b.yaml", f"nested/../{repeated}"),
                ):
                    (root / name).write_text(
                        f'schema_version: "1.0"\nparents: {parent}\n', encoding="ascii"
                    )
                reads = []

                def read(path, captured, recorded=reads):
                    self.assertIs(captured, bundle)
                    recorded.append(Path(path))
                    return original(path, captured)

                with (
                    patch(
                        "yamaa.schema.inheritance.read_bundle_document",
                        side_effect=read,
                    ),
                    self.assertRaises(SpecificationError) as caught,
                ):
                    resolve_specification(root / "entry.yaml", bundle)
                diagnostic = caught.exception.diagnostics[0]
                self.assertEqual(diagnostic.condition, "inheritance_cycle")
                names = (
                    ["entry.yaml", "a.yaml", "b.yaml", "entry.yaml"]
                    if repeated == "entry.yaml"
                    else ["a.yaml", "b.yaml", "a.yaml"]
                )
                self.assertEqual(
                    diagnostic.context["cycle"], [str(root / n) for n in names]
                )
                self.assertEqual(
                    reads, [root / n for n in ("entry.yaml", "a.yaml", "b.yaml")]
                )

    def test_shared_traversal_validates_entry_before_missing_parent_access(self):
        """All layer defects precede version checks and further source effects."""
        from yamaa.schema import inheritance

        bundle = native_specification.load_schema_bundle(SCHEMA)
        original = inheritance.read_bundle_document
        with tempfile.TemporaryDirectory() as directory:
            entry = Path(directory).resolve() / "entry.yaml"
            entry.write_text(
                'schema_version: "0.0"\nparents: missing.yaml\nunknown: value\n',
                encoding="ascii",
            )
            reads = []

            def read(path, captured):
                reads.append(Path(path))
                return original(path, captured)

            with (
                patch(
                    "yamaa.schema.inheritance.read_bundle_document", side_effect=read
                ),
                self.assertRaises(SpecificationError) as caught,
            ):
                resolve_specification(entry, bundle)
            self.assertEqual(caught.exception.diagnostics[0].condition, "unknown_field")
            self.assertEqual(reads, [entry])

    def test_count_relation_survives_pruning_with_explicit_execution_refusal(self):
        """Pruning keeps COUNT(D.*); unqualified runtime scope refuses before IO."""
        document = {
            "schema_version": "1.0",
            "parents": [],
            "domain": "OUT",
            "keys": ["ID"],
            "input": {"DM": "dm.csv", "EX": "ex.csv", "DEAD": "never-read.csv"},
            "base": "DM",
            "output": {"path": "out.csv", "columns": ["ID", "N"]},
            "columns": [
                {"name": "ID", "type": "str", "derivation": "DM.ID"},
                {
                    "name": "N",
                    "type": "int",
                    "derivation": {"aggregate": {"expr": "COUNT(EX.*)"}},
                },
                {
                    "name": "UNUSED",
                    "type": "int",
                    "derivation": {"aggregate": {"expr": "COUNT(DEAD.*)"}},
                },
            ],
        }
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "spec.yaml").write_text(json.dumps(document), encoding="ascii")
            (root / "dm.csv").write_text("ID\na\nb\n", encoding="ascii")
            (root / "ex.csv").write_text("ID\na\na\nb\n", encoding="ascii")
            loaded = native_specification.load_specification(root / "spec.yaml", SCHEMA)
            self.assertEqual(list(loaded.specification.input), ["DM", "EX"])
            calls = []

            def provider(declarations):
                calls.append("read")
                return load_source_tables(declarations, ProjectResources(root))

            actual = execute_with_source_provider(loaded.specification, provider)
            # Qualified key-matched aggregates remain an explicit executor limitation.
            # The reference test independently verifies the expected a=2, b=1 counts.
            self.assertEqual(calls, [])
            self.assertEqual(actual.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in actual.result.features],
                [("aggregate_scope_or_expression", "columns.N.derivation.aggregate")],
            )

    def test_custom_input_key_coercion_keeps_order_and_validates_replaced_members(self):
        """Custom scalar identifiers may converge to one normalized text name."""
        schema = {
            "version": "1.0",
            "root_class": [
                {"schema_version": {"type": "str"}},
                {"input": {"type": "dict[identifier, dataset_class]"}},
            ],
            "dataset_class": [{"path": {"type": "str"}}],
            "identifier": {"type": ["str", "int"]},
            "project_path": {"type": "str"},
        }
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "schema.yaml").write_text(json.dumps(schema), encoding="ascii")
            interpreter = native_specification.load_schema_bundle(root).interpreter
            value, findings = interpreter.normalize_layer(
                {
                    "schema_version": "1.0",
                    "input": {
                        2: {"path": "first.csv"},
                        "KEEP": "middle.csv",
                        "2": {"path": "last.csv"},
                    },
                }
            )
            self.assertEqual(findings, [])
            self.assertEqual(list(value["input"]), ["2", "KEEP"])
            self.assertEqual(
                value["input"],
                {
                    "2": {"path": "last.csv"},
                    "KEEP": {"path": "middle.csv"},
                },
            )
            value, findings = interpreter.normalize_layer(
                {
                    "schema_version": "1.0",
                    "input": {
                        2: {"path": False},
                        "2": {"path": "last.csv"},
                    },
                }
            )
            self.assertIsNone(value)
            self.assertEqual(
                [(d.condition, d.spec_paths) for d in findings],
                [
                    ("invalid_field_type", ("input[2].path",)),
                ],
            )

    def test_layer_diagnostic_paths_distinguish_indices_from_text_names(self):
        """Positional and bool/int paths keep brackets; textual names keep dots."""
        bundle = native_specification.load_schema_bundle(SCHEMA)
        value, findings = bundle.interpreter.normalize_layer(
            {
                "schema_version": "1.0",
                "input": {True: [], 2: [], "X": []},
                "columns": [{}, {"name": "X", "type": 1}],
            }
        )
        self.assertIsNone(value)
        self.assertEqual(
            [d.spec_paths for d in findings],
            [
                ("input.key(True)",),
                ("input[True]",),
                ("input.key(2)",),
                ("input[2]",),
                ("input.X",),
                ("columns[0].name",),
                ("columns.X.type",),
            ],
        )

    def test_layer_admission_collects_ordered_field_errors_before_normalization(self):
        """Both authored errors retain field paths in schema order, not write order."""
        document = {
            "schema_version": "1.0",
            "parents": [],
            "columns": [{"label": 2, "name": "X", "type": 1}],
        }
        bundle = native_specification.load_schema_bundle(SCHEMA)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "spec.yaml"
            path.write_text(json.dumps(document), encoding="ascii")
            with self.assertRaises(SpecificationError) as caught:
                resolve_specification(path, bundle)
            self.assertEqual(
                [
                    (d.condition, d.spec_paths, d.context)
                    for d in caught.exception.diagnostics
                ],
                [
                    (
                        "invalid_field_type",
                        ("columns.X.type",),
                        {
                            "expected": "column_type",
                            "actual": "int",
                            "source": str(path.resolve()),
                            "entry": str(path.resolve()),
                        },
                    ),
                    (
                        "invalid_field_type",
                        ("columns.X.label",),
                        {
                            "expected": "str",
                            "actual": "int",
                            "source": str(path.resolve()),
                            "entry": str(path.resolve()),
                        },
                    ),
                ],
            )

    def test_standalone_null_handler_survives_workflow_resolution(self):
        document = self.window_document()
        document["intermediates"] = [
            {"id": "LOOK", "dataset": "SRC", "key": ["ID"], "no_match": None}
        ]
        bundle = native_specification.load_schema_bundle(SCHEMA)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "spec.yaml"
            path.write_text(json.dumps(document), encoding="ascii")
            resolved = resolve_specification(path, bundle)
            member = resolved.document["intermediates"][0]
            self.assertIn("no_match", member)
            self.assertIsNone(member["no_match"])
            self.assertEqual(
                resolved.provenance["intermediates.LOOK.no_match"].file, path.resolve()
            )

    def test_new_keyed_members_cannot_clear_values_they_never_inherited(self):
        bundle = native_specification.load_schema_bundle(SCHEMA)
        cases = [
            (
                "columns",
                [{"name": "X", "type": "str", "label": None}],
                "columns.X.label",
            ),
            ("input", {"DS": {"path": "data.csv", "types": None}}, "input.DS.types"),
            ("rows", [{"id": "R", "filter": None}], "rows.R.filter"),
            ("intermediates", [{"id": "I", "filter": None}], "intermediates.I.filter"),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            for collection, members, logical in cases:
                with self.subTest(collection=collection):
                    path = Path(temporary) / "spec.yaml"
                    path.write_text(
                        json.dumps(
                            {
                                "schema_version": "1.0",
                                "parents": [],
                                collection: members,
                            }
                        ),
                        encoding="ascii",
                    )
                    with self.assertRaises(SpecificationError) as caught:
                        resolve_specification(path, bundle)
                    self.assertEqual(
                        [
                            (d.condition, d.spec_paths, d.requirement)
                            for d in caught.exception.diagnostics
                        ],
                        [("invalid_clear", (logical,), "REQ-0660")],
                    )

    def setUp(self):
        self.assertIn("site-packages", str(Path(yamaa.__file__).resolve()))
        self.assertIn("site-packages", str(Path(yamaa_native.__file__).resolve()))
        self.stack = self.enterContext(ExitStack())
        for name in (
            "class_fields",
            "matching_type",
            "split_type_arguments",
            "validate_descriptor_value",
        ):
            self.stack.enter_context(
                patch(
                    f"yamaa.schema.windows.{name}",
                    side_effect=AssertionError("host window traversal invoked"),
                )
            )
        for name in (
            "_merge_member",
            "_compose_value",
            "_materialize_fragments",
            "_validate_partial_member",
            "_validate_layer",
            "_is_nonlocal_parent",
            "_references",
            "_language_references",
            "_prune",
            "_order_columns",
        ):
            self.stack.enter_context(
                patch(
                    f"yamaa.schema.inheritance.{name}",
                    side_effect=AssertionError("host layer composition invoked"),
                )
            )
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

    @staticmethod
    def window_document():
        """Authored scope fixture; expected rows are specified separately below."""
        return {
            "schema_version": "1.0",
            "domain": "OUT",
            "base": "SRC",
            "keys": ["ID"],
            "input": {
                "SRC": {"path": "input.csv", "types": {"SEQ": "int", "VAL": "int"}}
            },
            "output": {"path": "out.csv", "columns": ["ID", "PREV"]},
            "windows": {"VISITS": {"group_by": ["G"], "order_by": ["SEQ"]}},
            "columns": [
                {"name": name, "type": kind, "label": name, "derivation": f"SRC.{name}"}
                for name, kind in [
                    ("ID", "str"),
                    ("G", "str"),
                    ("SEQ", "int"),
                    ("VAL", "int"),
                ]
            ]
            + [
                {
                    "name": "PREV",
                    "type": "int",
                    "label": "Previous value",
                    "derivation": {
                        "row_value": {"source": "VAL", "offset": -1, "window": "VISITS"}
                    },
                }
            ],
        }

    @staticmethod
    def write_window_document(directory, value, name="spec.yaml"):
        """JSON is authored ASCII YAML; no host YAML encoder participates."""
        path = directory / name
        path.write_text(json.dumps(value), encoding="ascii")
        (directory / "input.csv").write_bytes(
            b"ID,G,SEQ,VAL\n01,a,1,10\n02,a,2,20\n03,b,1,30\n04,b,2,40\n"
        )
        return path

    def test_shared_window_composition_preserves_definition_provenance_and_csv(self):
        bundle = native_specification.load_schema_bundle(SCHEMA)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            parent = self.window_document()
            parent["windows"]["UNUSED"] = {"group_by": ["NOT_A_COLUMN"]}
            parent_path = self.write_window_document(directory, parent, "parent.yaml")
            child = {
                "schema_version": "1.0",
                "parents": "parent.yaml",
                "windows": {
                    "VISITS": {"order_by": [{"variable": "SEQ", "direction": "desc"}]}
                },
            }
            path = self.write_window_document(directory, child)
            resolved = resolve_specification(path, bundle)
            self.assertNotIn("windows", resolved.document)
            self.assertNotIn(
                "G", [column["name"] for column in resolved.document["columns"]]
            )
            column = next(
                c for c in resolved.document["columns"] if c["name"] == "PREV"
            )
            self.assertEqual(
                column["derivation"]["value"]["row_value"]["window"],
                {
                    "order_by": [
                        {"variable": "SEQ", "direction": "desc", "nulls": "last"}
                    ]
                },
            )
            prefix = "columns.PREV.derivation.value.row_value.window"
            self.assertEqual(resolved.provenance[prefix].file, parent_path.resolve())
            self.assertEqual(
                resolved.provenance[prefix + ".order_by"].file, path.resolve()
            )
            self.assertEqual(
                resolved.provenance[prefix + ".order_by"].spec_path,
                "windows.VISITS.order_by",
            )
            actual = execute_with_source_provider(
                resolved.specification,
                lambda declarations: load_source_tables(
                    declarations, ProjectResources(directory)
                ),
            )
            self.assertEqual(actual.result.status, "success")
            self.assertEqual(
                render_artifact(actual.result.artifact),
                b"ID,PREV\n01,40\n02,\n03,10\n04,20\n",
            )

    def test_shared_window_unknowns_follow_pruning_and_keep_use_site(self):
        bundle = native_specification.load_schema_bundle(SCHEMA)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            parent = self.window_document()
            parent["columns"][-1]["derivation"]["row_value"]["window"] = "UNKNOWN"
            self.write_window_document(directory, parent, "parent.yaml")
            child = {
                "schema_version": "1.0",
                "parents": "parent.yaml",
                "output": {"path": "out.csv", "columns": ["ID"]},
            }
            path = self.write_window_document(directory, child)
            resolved = resolve_specification(path, bundle)
            self.assertEqual([c.name for c in resolved.specification.columns], ["ID"])
            child.pop("output")
            self.write_window_document(directory, child)
            with self.assertRaises(SpecificationError) as caught:
                resolve_specification(path, bundle)
            finding = caught.exception.diagnostics[0]
            self.assertEqual(
                (
                    finding.condition,
                    finding.requirement,
                    finding.spec_paths,
                    finding.context,
                ),
                (
                    "unknown_window",
                    "REQ-1253",
                    ("columns.PREV.derivation.row_value.window",),
                    {"window": "UNKNOWN"},
                ),
            )

    def test_shared_window_loading_keeps_template_scope_and_explicit_runtime_refusal(
        self,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            document = self.window_document()
            document["windows"]["VISITS"] = {"order_by": ["SEQ"]}
            derivations = {
                column["name"]: column.pop("derivation")
                for column in document["columns"]
            }
            document["rows"] = [
                {
                    "id": group,
                    "dataset": "SRC",
                    "filter": f"SRC.G = '{group}'",
                    "derivations": copy.deepcopy(derivations),
                }
                for group in ("a", "b")
            ]
            path = self.write_window_document(directory, document)
            loaded = native_specification.load_specification(path, SCHEMA)
            from yamaa.runtime import execute_with_source_provider as reference_execute

            # The independent CSV establishes the loaded caller scopes. Native
            # row-template window execution is a separate, still-open capability.
            reference_result = reference_execute(
                loaded.specification,
                lambda declarations: load_source_tables(
                    declarations, ProjectResources(directory)
                ),
            )
            self.assertEqual(reference_result.status, "success")
            self.assertEqual(
                render_artifact(reference_result.artifact),
                b"ID,PREV\n01,\n02,10\n03,\n04,30\n",
            )
            calls = []
            refused = execute_with_source_provider(
                loaded.specification, lambda declarations: calls.append("read")
            )
            self.assertEqual(refused.result.status, "unsupported")
            self.assertEqual(
                [(f.operation, f.spec_path) for f in refused.result.features],
                [
                    ("window_scope", "rows[0].derivations.PREV.row_value"),
                    ("window_scope", "rows[1].derivations.PREV.row_value"),
                ],
            )
            self.assertEqual(calls, [])

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

    def test_schema_entry_and_all_inherited_sources_use_shared_yaml(self):
        case = ROOT / "specification-inheritance"
        expected = read_yaml_document(case / "expected/spec_resolved.yaml")
        paths = []
        original_read = NativeYamlReader.read_document

        def read(reader, path):
            paths.append(path.name)
            return original_read(reader, path)

        with (
            patch(
                "yamaa.specification._yaml.read_yaml_bytes",
                side_effect=AssertionError("host YAML decoder invoked"),
            ),
            patch.object(NativeYamlReader, "read_document", new=read),
        ):
            bundle = native_specification.load_schema_bundle(SCHEMA)
            # Replacing the module entry point after capture must not affect
            # any later entry/parent read through this bundle.
            with patch.object(
                yamaa_native, "decode_yaml", side_effect=AssertionError("recapture")
            ):
                resolved = resolve_specification(case / "spec_study.yaml", bundle)
        self.assertEqual(resolved.document, expected)
        self.assertEqual(
            paths,
            ["spec_study.yaml", "spec_organization.yaml", "spec_compound.yaml"],
        )

    def test_native_yaml_errors_and_host_policy_preserve_paths_and_fail_closed(self):
        import sys

        reader = NativeYamlReader(yamaa_native)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "parent.yaml"
            path.write_bytes(b'x: "\\uD800"\n')
            with self.assertRaises(SpecificationError) as caught:
                reader.read_document(path)
            finding = caught.exception.diagnostics[0]
            self.assertEqual(finding.condition, "invalid_text")
            self.assertEqual(finding.spec_paths, ("$.x",))
            self.assertEqual(finding.context, {"code_point": "U+D800", "offset": 0})
            path.write_bytes(b"x: \xff\n")
            with self.assertRaises(SpecificationError) as caught:
                reader.read_document(path)
            self.assertEqual(
                caught.exception.diagnostics[0].context,
                {"path": str(path), "line": 1, "column": 4},
            )
            path.write_bytes(b"9" * 4097)
            with self.assertRaises(NativeSchemaLimitError) as caught:
                reader.read_document(path)
            self.assertEqual(
                (caught.exception.resource, caught.exception.limit),
                ("numeric_digits", 4096),
            )
            previous = sys.get_int_max_str_digits()
            try:
                sys.set_int_max_str_digits(640)
                path.write_bytes(b"9" * 641)
                with self.assertRaises(NativeSchemaLimitError) as caught:
                    reader.read_document(path)
                self.assertEqual(caught.exception.resource, "host_integer_digits")
                self.assertEqual(sys.get_int_max_str_digits(), 640)
            finally:
                sys.set_int_max_str_digits(previous)
            path.write_bytes(b"{9223372036854775808: a, 9223372036854775809: b}")
            self.assertEqual(
                reader.read_document(path),
                {9223372036854775808: "a", 9223372036854775809: "b"},
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
        native = SimpleNamespace(
            _compile_schema=yamaa_native._compile_schema,
            decode_yaml=yamaa_native.decode_yaml,
        )
        original_open = Path.open
        effects = []

        def read(path, *args, **kwargs):
            effects.append(path.name)
            native._compile_schema = lambda _: self.fail("service replaced after IO")
            native.decode_yaml = lambda _: self.fail("decoder replaced after IO")
            return original_open(path, *args, **kwargs)

        with patch.object(Path, "open", new=read):
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
                    original_open = Path.open

                    def read(
                        path,
                        *args,
                        effects=effects,
                        original_open=original_open,
                        **kwargs,
                    ):
                        effects.append(path.name)
                        return original_open(path, *args, **kwargs)

                    with (
                        patch.object(Path, "open", new=read),
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

    def test_admission_messages_identify_includes_and_registry_entries_once(self):
        """Real shared admission findings retain their specific user-facing locations."""
        cases = [
            (
                'version: "1.0"\nincludes: nope\nroot_class: []\n',
                "includes must be a list",
                "schema.yaml",
                "includes",
                {"code": "includes_list"},
            ),
            (
                """version: "1.0"
includes: [schema_other.yaml]
root_class: []
ops:
  one: {type: str}
""",
                "duplicate registry entry ops.one",
                "schema_other.yaml",
                "ops",
                {"code": "duplicate_registry_entry", "name": "one"},
            ),
        ]
        for source, reason, module_name, path, issue in cases:
            with (
                self.subTest(reason=reason),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory).resolve()
                (root / "schema.yaml").write_text(source, encoding="ascii")
                (root / "schema_other.yaml").write_text(
                    'version: "1.0"\nops:\n  one: {type: str}\n',
                    encoding="ascii",
                )
                with self.assertRaises(SpecificationError) as caught:
                    native_specification.load_schema_bundle(root)
                error = caught.exception
                self.assertEqual(len(error.diagnostics), 1)
                self.assertEqual(
                    error.diagnostics[0].condition, "invalid_schema_bundle"
                )
                self.assertEqual(error.diagnostics[0].spec_paths, ("$",))
                self.assertEqual(
                    error.diagnostics[0].context,
                    {"path": str(root / module_name), "reason": reason},
                )
                finding = error.native_outcome["issues"][0]
                self.assertEqual(finding["path"], path)
                self.assertEqual(finding["issue"], issue)

    def test_constraint_diagnostic_bounds_keep_authored_integer_types(self):
        """Wire decimal bounds render as exact Python integers in diagnostic context."""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "schema.yaml").write_text(
                """version: "1.0"
root_class:
  - schema_version: {type: str}
  - text: {type: str, min_length: 3}
  - items: {type: list, size: 2}
""",
                encoding="ascii",
            )
            bundle = native_specification.load_schema_bundle(root)
            findings = reference.validate_document(
                {
                    "schema_version": "1.0",
                    "text": "a",
                    "items": [],
                },
                bundle,
                "root_class",
            )
            self.assertEqual(
                [finding.condition for finding in findings],
                [
                    "minimum_length",
                    "invalid_size",
                ],
            )
            self.assertEqual(
                [finding.context for finding in findings],
                [
                    {"minimum": 3},
                    {"size": 2},
                ],
            )
            self.assertEqual(
                [finding.spec_paths for finding in findings],
                [("text",), ("items",)],
            )
            self.assertIs(type(findings[0].context["minimum"]), int)
            self.assertIs(type(findings[1].context["size"]), int)

        # The decoded service supports arbitrary-width integers independently
        # of the host YAML decoder's numeric range.
        descriptor = {"type": "str", "min_length": 10**40}
        tree = encode_tree({"version": "1.0", "root_class": [{"wide": descriptor}]})
        snapshot, text = yamaa_native._compile_schema(
            request(
                {
                    "schema": {
                        "modules": [{"name": "schema.yaml", "document": tree}],
                        "entry": 0,
                        "root_class": "root_class",
                    }
                }
            )
        )
        service = NativeSchemaInterpreter(
            snapshot, response(text), [decode_nodes(tree)]
        )
        finding = service.validate_descriptor("x", descriptor, "wide", False)[0]
        self.assertEqual(finding.context, {"minimum": 10**40})
        self.assertIs(type(finding.context["minimum"]), int)


if __name__ == "__main__":
    unittest.main()
