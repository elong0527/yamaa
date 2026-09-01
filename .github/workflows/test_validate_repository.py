import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import yaml


TOOL_PATH = Path(__file__).parent / "validate_repository.py"
SPEC = importlib.util.spec_from_file_location("validate_repository", TOOL_PATH)
assert SPEC is not None and SPEC.loader is not None
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)


class TestYamlLoader(unittest.TestCase):
    def test_yaml_12_boolean_resolution(self):
        loaded = yaml.load(
            "yes_value: yes\n"
            "on_value: ON\n"
            "true_value: true\n"
            "title_true_value: True\n"
            "upper_false_value: FALSE\n"
            "mixed_true_value: TrUe\n"
            "mixed_false_value: FaLsE\n",
            Loader=VALIDATOR.UniqueKeyLoader,
        )
        self.assertEqual(loaded["yes_value"], "yes")
        self.assertEqual(loaded["on_value"], "ON")
        self.assertIs(loaded["true_value"], True)
        self.assertIs(loaded["title_true_value"], True)
        self.assertIs(loaded["upper_false_value"], False)
        self.assertEqual(loaded["mixed_true_value"], "TrUe")
        self.assertEqual(loaded["mixed_false_value"], "FaLsE")

    def test_yaml_12_core_scalar_resolution(self):
        loaded = yaml.load(
            "date_value: 2025-01-02\n"
            "decimal_value: 012\n"
            "octal_value: 0o12\n"
            "signed_octal_value: -0o12\n"
            "sexagesimal_value: 1:20\n"
            "exponent_value: 1e3\n"
            "underscored_value: 1_000\n",
            Loader=VALIDATOR.UniqueKeyLoader,
        )
        self.assertEqual(loaded["date_value"], "2025-01-02")
        self.assertEqual(loaded["decimal_value"], 12)
        self.assertEqual(loaded["octal_value"], 10)
        self.assertEqual(loaded["signed_octal_value"], "-0o12")
        self.assertEqual(loaded["sexagesimal_value"], "1:20")
        self.assertEqual(loaded["exponent_value"], 1000.0)
        self.assertEqual(loaded["underscored_value"], "1_000")

    def test_aliases_are_rejected(self):
        with self.assertRaises(yaml.YAMLError):
            yaml.load(
                "first: &value text\nsecond: *value\n",
                Loader=VALIDATOR.UniqueKeyLoader,
            )

    def test_merge_keys_are_rejected(self):
        with self.assertRaises(yaml.YAMLError):
            yaml.load(
                "base: &base {a: 1}\nmerged: {<<: *base}\n",
                Loader=VALIDATOR.UniqueKeyLoader,
            )

    def test_explicit_tags_are_rejected(self):
        with self.assertRaises(yaml.YAMLError):
            yaml.load(
                "value: !!str 123\n",
                Loader=VALIDATOR.UniqueKeyLoader,
            )


class TestTypeValidation(unittest.TestCase):
    def test_int_rejects_string(self):
        errors = VALIDATOR.validate_type(
            "not-an-int",
            ["int"],
            {"classes": {}, "aliases": {}, "registries": {}},
            "spec.count",
        )
        self.assertTrue(errors)
        self.assertIn("expected int", errors[0])

    def test_registry_value_requires_exactly_one_operation(self):
        env = {
            "classes": {},
            "aliases": {"operation": {"registry": "operations"}},
            "registries": {
                "operations": {
                    "first": {"type": "str"},
                    "second": {"type": "str"},
                }
            },
        }
        errors = VALIDATOR.validate_type(
            {"first": "a", "second": "b"},
            ["operation"],
            env,
            "spec.operation",
        )
        self.assertTrue(errors)
        self.assertIn("exactly one", errors[0])

    def test_alias_values_constraint(self):
        env = {
            "classes": {},
            "aliases": {"status": {"type": "str", "values": ["A", "B"]}},
            "registries": {},
        }
        errors = VALIDATOR.validate_type("C", ["status"], env, "spec.status")
        self.assertTrue(errors)
        self.assertIn("allowed values", errors[0])

    def test_alias_pattern_constraint(self):
        env = {
            "classes": {},
            "aliases": {"variable": {"type": "str", "pattern": "^[A-Z]+$"}},
            "registries": {},
        }
        errors = VALIDATOR.validate_type("lower", ["variable"], env, "spec.name")
        self.assertTrue(errors)
        self.assertIn("pattern", errors[0])

    def test_alias_min_length_constraint(self):
        env = {
            "classes": {},
            "aliases": {"variables": {"type": "list[str]", "min_length": 1}},
            "registries": {},
        }
        errors = VALIDATOR.validate_type([], ["variables"], env, "spec.keys")
        self.assertTrue(errors)
        self.assertIn("minimum length", errors[0])

    def test_alias_size_constraint(self):
        env = {
            "classes": {},
            "aliases": {"pair": {"type": "list[str]", "size": 2}},
            "registries": {},
        }
        errors = VALIDATOR.validate_type(["one"], ["pair"], env, "spec.pair")
        self.assertTrue(errors)
        self.assertIn("size 2", errors[0])

    def test_descriptor_rejects_constraint_on_wrong_type(self):
        errors = VALIDATOR.check_descriptor(
            {"type": "int", "pattern": "^[0-9]+$"},
            False,
            "schema:count",
        )
        self.assertTrue(errors)
        self.assertIn("pattern", "\n".join(errors))

    def test_descriptor_rejects_default_on_required_field(self):
        errors = VALIDATOR.check_descriptor(
            {"type": "str", "required": True, "default": "x"},
            True,
            "schema:name",
        )
        self.assertTrue(errors)
        self.assertIn("default", "\n".join(errors))

    def test_nested_dict_type_expression(self):
        env = {"classes": {}, "aliases": {}, "registries": {}}
        valid = {"outer": {"inner": 1}}
        self.assertEqual(
            VALIDATOR.validate_type(
                valid,
                ["dict[str, dict[str, int]]"],
                env,
                "spec.mapping",
            ),
            [],
        )
        errors = VALIDATOR.validate_type(
            {"outer": {"inner": "wrong"}},
            ["dict[str, dict[str, int]]"],
            env,
            "spec.mapping",
        )
        self.assertTrue(errors)
        self.assertIn("expected int", errors[0])

    def test_bare_list_and_dict_types(self):
        env = {"classes": {}, "aliases": {}, "registries": {}}
        self.assertEqual(
            VALIDATOR.validate_type([], ["list"], env, "spec.items"), []
        )
        self.assertEqual(
            VALIDATOR.validate_type({}, ["dict"], env, "spec.mapping"), []
        )

    def test_size_accepts_bare_list_and_dict_types(self):
        for type_name in ("list", "dict"):
            with self.subTest(type_name=type_name):
                self.assertEqual(
                    VALIDATOR.check_descriptor(
                        {"type": type_name, "size": 0},
                        is_class_field=False,
                        path="schema.value",
                    ),
                    [],
                )


class TestDateImputeSchema(unittest.TestCase):
    def setUp(self):
        self.env, schema_errors = VALIDATOR.build_schema_env(TOOL_PATH.parents[2])
        self.assertEqual(schema_errors, [])

    def test_accepts_month_minimum_source_precision(self):
        errors = VALIDATOR.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": 15,
                    "minimum_source_precision": "month",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertEqual(errors, [])

    def test_rejects_unknown_minimum_source_precision(self):
        errors = VALIDATOR.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": 15,
                    "minimum_source_precision": "day",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertTrue(errors)
        self.assertIn("allowed values", errors[0])


class TestGroupedRows(unittest.TestCase):
    def test_accepts_driver_qualified_group_variables(self):
        spec = {
            "base": "ADLBIN",
            "rows": [
                {
                    "id": "derived",
                    "group_by": ["ADLBIN.USUBJID", "ADLBIN.VISIT"],
                }
            ],
        }

        errors = VALIDATOR.validate_grouped_rows(spec, "example/spec.yaml")

        self.assertEqual(errors, [])

    def test_rejects_empty_group(self):
        spec = {
            "base": "ADLBIN",
            "rows": [{"id": "derived", "group_by": []}],
        }

        errors = VALIDATOR.validate_grouped_rows(spec, "example/spec.yaml")

        self.assertTrue(errors)
        self.assertIn("at least one", "\n".join(errors))

    def test_rejects_unqualified_or_wrong_driver_variable(self):
        spec = {
            "base": "ADLBIN",
            "rows": [
                {
                    "id": "derived",
                    "dataset": "ADLBIN",
                    "group_by": ["USUBJID", "OTHER.VISIT"],
                }
            ],
        }

        errors = VALIDATOR.validate_grouped_rows(spec, "example/spec.yaml")

        self.assertEqual(len(errors), 2)
        self.assertIn("driver 'ADLBIN'", "\n".join(errors))

    def test_rejects_duplicate_group_variable(self):
        spec = {
            "base": "ADLBIN",
            "rows": [
                {
                    "id": "derived",
                    "group_by": ["ADLBIN.USUBJID", "ADLBIN.USUBJID"],
                }
            ],
        }

        errors = VALIDATOR.validate_grouped_rows(spec, "example/spec.yaml")

        self.assertTrue(errors)
        self.assertIn("duplicate", "\n".join(errors))


class TestSpecNames(unittest.TestCase):
    def test_accepts_resolved_unique_names(self):
        spec = {
            "domain": "ADSL",
            "datasets": {"DM": "dm.csv", "EX": "ex.csv"},
            "base": "DM",
            "record_lookups": [{"id": "dose", "dataset": "EX"}],
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID"}],
            "rows": [{"id": "subjects", "dataset": "DM"}],
        }

        self.assertEqual(
            VALIDATOR.validate_spec_names(spec, "example/spec.yaml"), []
        )

    def test_rejects_duplicate_and_unresolved_columns(self):
        spec = {
            "domain": "ADSL",
            "datasets": {"DM": "dm.csv"},
            "keys": ["MISSING", "MISSING"],
            "output": {"columns": ["USUBJID", "USUBJID"]},
            "columns": [{"name": "USUBJID"}, {"name": "USUBJID"}],
        }

        errors = VALIDATOR.validate_spec_names(spec, "example/spec.yaml")

        message = "\n".join(errors)
        self.assertIn("duplicate column name", message)
        self.assertIn("duplicate key column", message)
        self.assertIn("duplicate output column", message)
        self.assertIn("undeclared column 'MISSING'", message)

    def test_rejects_empty_keys(self):
        spec = {
            "domain": "ADSL",
            "datasets": {"DM": "dm.csv"},
            "keys": [],
            "output": {"columns": ["USUBJID"]},
            "columns": [{"name": "USUBJID"}],
        }

        errors = VALIDATOR.validate_spec_names(spec, "example/spec.yaml")

        self.assertIn("at least one key column", "\n".join(errors))

    def test_rejects_dataset_and_lookup_namespace_errors(self):
        spec = {
            "domain": "ADSL",
            "datasets": {"ADSL": "input.csv"},
            "base": "MISSING",
            "record_lookups": [
                {"id": "ADSL", "dataset": "MISSING"},
                {"id": "ADSL", "dataset": "ADSL"},
            ],
            "keys": [],
            "output": {"columns": []},
            "columns": [],
            "rows": [{"id": "row"}, {"id": "row", "dataset": "MISSING"}],
        }

        errors = VALIDATOR.validate_spec_names(spec, "example/spec.yaml")

        message = "\n".join(errors)
        self.assertIn("must not equal the output domain", message)
        self.assertIn("undeclared dataset 'MISSING'", message)
        self.assertIn("duplicate record lookup id", message)
        self.assertIn("conflicts with a dataset or domain", message)
        self.assertIn("duplicate row id", message)


class TestSpecContracts(unittest.TestCase):
    def test_rejects_missing_base_and_incomplete_column_coverage(self):
        spec = {
            "domain": "ADSL",
            "datasets": {"DM": "dm.csv"},
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID", "AGE"]},
            "columns": [
                {"name": "USUBJID", "derivation": {"source": "DM.USUBJID"}},
                {"name": "AGE"},
            ],
        }

        errors = VALIDATOR.validate_spec_contracts(spec, "example/spec.yaml")

        message = "\n".join(errors)
        self.assertIn("base is required", message)
        self.assertIn("AGE", message)
        self.assertIn("no derivation", message)

    def test_rejects_lookup_pairing_and_verification_constraints(self):
        spec = {
            "domain": "ADSL",
            "datasets": {"DM": "dm.csv"},
            "base": "DM",
            "record_lookups": [
                {"id": "LAST", "dataset": "DM", "source": "USUBJID"},
                {"id": "FIRST", "dataset": "DM", "order_by": ["DM.DATE"]},
            ],
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {
                    "name": "USUBJID",
                    "derivation": {"source": "DM.USUBJID"},
                    "verifications": {"range": {}},
                }
            ],
            "verifications": [
                {"row_count": {}},
                {"all_or_none": {"id": "complete", "columns": ["USUBJID"]}},
                {"predicate": {"id": "complete", "assert": "TRUE"}},
            ],
        }

        errors = VALIDATOR.validate_spec_contracts(spec, "example/spec.yaml")

        message = "\n".join(errors)
        self.assertIn("source and key", message)
        self.assertIn("order_by and keep", message)
        self.assertIn("requires at least one bound", message)
        self.assertIn("at least two distinct columns", message)
        self.assertIn("duplicate dataset verification id", message)
        self.assertIn("range requires an int or float column", message)

    def test_rejects_missing_source_and_type_for_absent_csv_field(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            input_dir = example_dir / "input"
            input_dir.mkdir()
            (input_dir / "dm.csv").write_text("USUBJID\n01\n")
            spec_path = example_dir / "spec.yaml"
            spec = {
                "domain": "ADSL",
                "datasets": {
                    "DM": {
                        "path": "input/dm.csv",
                        "types": {"AGE": "int"},
                    },
                    "EX": "input/missing.csv",
                },
                "base": "DM",
                "keys": ["USUBJID"],
                "output": {"columns": ["USUBJID"]},
                "columns": [
                    {
                        "name": "USUBJID",
                        "derivation": {"source": "DM.USUBJID"},
                    }
                ],
            }

            errors = VALIDATOR.validate_spec_contracts(
                spec, "example/spec.yaml", spec_path
            )

        message = "\n".join(errors)
        self.assertIn("types.AGE", message)
        self.assertIn("source path does not exist", message)


class TestSourceSidecars(unittest.TestCase):
    def setUp(self):
        self.env, schema_errors = VALIDATOR.build_schema_env(
            TOOL_PATH.parents[2]
        )
        self.assertEqual(schema_errors, [])
        self.test_dir = tempfile.TemporaryDirectory()
        self.example_dir = Path(self.test_dir.name)
        self.input_dir = self.example_dir / "input"
        self.input_dir.mkdir()
        self.spec_path = self.example_dir / "spec.yaml"
        (self.input_dir / "dm.csv").write_text(
            "STUDYID,AGE\nSTUDY1,42\n", encoding="utf-8"
        )

    def tearDown(self):
        self.test_dir.cleanup()

    def write_sidecar(self, content):
        (self.input_dir / "dm.schema.yaml").write_text(
            content, encoding="utf-8"
        )

    def validate(self, source=None):
        if source is None:
            source = {
                "path": "input/dm.csv",
                "schema": "input/dm.schema.yaml",
            }
        return VALIDATOR.validate_source_sidecars(
            {"datasets": {"DM": source}},
            "example/spec.yaml",
            self.spec_path,
            self.env,
        )

    def test_accepts_complete_sidecar_from_main_schema_bundle(self):
        self.write_sidecar(
            'version: "1.0"\n'
            "fields:\n"
            "  AGE: integer\n"
            "  STUDYID: string\n"
        )

        self.assertEqual(self.validate(), [])

    def test_rejects_inline_types_with_sidecar(self):
        self.write_sidecar(
            'version: "1.0"\n'
            "fields: {STUDYID: string, AGE: integer}\n"
        )

        errors = self.validate(
            {
                "path": "input/dm.csv",
                "schema": "input/dm.schema.yaml",
                "types": {"AGE": "int"},
            }
        )

        self.assertIn(
            "example/spec.yaml.datasets.DM.types.AGE", "\n".join(errors)
        )

        empty_types_errors = self.validate(
            {
                "path": "input/dm.csv",
                "schema": "input/dm.schema.yaml",
                "types": {},
            }
        )
        self.assertIn(
            "example/spec.yaml.datasets.DM.types: inline types cannot be "
            "combined with a source schema",
            "\n".join(empty_types_errors),
        )

    def test_rejects_version_mismatch_and_empty_fields(self):
        self.write_sidecar('version: "2.0"\nfields: {}\n')

        message = "\n".join(self.validate())

        self.assertIn("does not match bundle version", message)
        self.assertIn("fields must not be empty", message)

    def test_rejects_nonmatching_csv_header(self):
        self.write_sidecar(
            'version: "1.0"\n'
            "fields: {AGE: integer, OTHER: string}\n"
        )

        message = "\n".join(self.validate())

        self.assertIn("match the CSV header exactly", message)
        self.assertIn("absent from schema: STUDYID", message)
        self.assertIn("absent from source: OTHER", message)

    def test_rejects_yamaa_internal_sidecar_vocabulary(self):
        self.write_sidecar(
            'schema_version: "1.0"\n'
            "fields: {STUDYID: str, AGE: int}\n"
        )

        message = "\n".join(self.validate())

        self.assertIn("missing required field 'version'", message)
        self.assertIn("unknown field 'schema_version'", message)
        self.assertIn("value 'str'", message)
        self.assertIn("value 'int'", message)

    def test_rejects_unknown_fields_and_types(self):
        self.write_sidecar(
            'version: "1.0"\n'
            "fields: {STUDYID: string, AGE: decimal}\n"
            "extra: true\n"
        )

        message = "\n".join(self.validate())

        self.assertIn("allowed values", message)
        self.assertIn("unknown field 'extra'", message)

    def test_rejects_missing_sidecar(self):
        message = "\n".join(self.validate())

        self.assertIn("source schema does not exist", message)


class TestValidatorCLI(unittest.TestCase):
    def setUp(self):
        self.tool_path = Path(__file__).parent / 'validate_repository.py'
        self.test_dir = tempfile.TemporaryDirectory()
        self.root_dir = Path(self.test_dir.name)

    def tearDown(self):
        self.test_dir.cleanup()

    def test_cli_help(self):
        result = subprocess.run([sys.executable, str(self.tool_path), '--help'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0)
        self.assertIn('--root', result.stdout)
        self.assertIn('--warnings-as-errors', result.stdout)

    def test_cli_missing_schema_fails(self):
        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('schema.yaml', result.stdout)

    def test_yaml_duplicate_keys(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir()
        bad_yaml = yaml_dir / 'bad.yaml'
        bad_yaml.write_text("a: 1\na: 2\n")

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ERROR', result.stdout)
        self.assertIn('bad.yaml', result.stdout)
        self.assertIn('line 2', result.stdout.lower() + result.stderr.lower())

    def test_yaml_syntax_error(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        bad_yaml = yaml_dir / 'syntax.yaml'
        bad_yaml.write_text("a: 1\n  b: 2\n")

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ERROR', result.stdout)
        self.assertIn('syntax.yaml', result.stdout)

    def test_schema_include_cycle(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text('version: "1.0"\nincludes: ["schema_cycle.yaml"]\nroot_class:\n  - f1: {type: str}\n')
        (yaml_dir / 'schema_cycle.yaml').write_text('version: "1.0"\nincludes: ["schema.yaml"]\n')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ERROR', result.stdout)
        self.assertIn('cycle', result.stdout.lower())

    def test_schema_shared_include_is_not_a_cycle(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nincludes: ["schema_left.yaml", "schema_right.yaml"]\n'
            'root_class:\n  - f1: {type: str}\n'
        )
        (yaml_dir / 'schema_left.yaml').write_text(
            'version: "1.0"\nincludes: ["schema_shared.yaml"]\n'
        )
        (yaml_dir / 'schema_right.yaml').write_text(
            'version: "1.0"\nincludes: ["schema_shared.yaml"]\n'
        )
        (yaml_dir / 'schema_shared.yaml').write_text('version: "1.0"\n')

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_schema_include_cannot_escape_yaml_directory(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nincludes: ["../outside.yaml"]\n'
            'root_class:\n  - f1: {type: str}\n'
        )
        (self.root_dir / 'outside.yaml').write_text('version: "1.0"\n')

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('outside yaml directory', result.stdout.lower())

    def test_schema_include_rejects_symlink_inside_yaml_directory(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nincludes: ["schema_link.yaml"]\n'
            'root_class:\n  - f1: {type: str}\n'
        )
        (yaml_dir / 'schema_real.yaml').write_text('version: "1.0"\n')
        (yaml_dir / 'schema_link.yaml').symlink_to('schema_real.yaml')

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('symlink', result.stdout.lower())

    def test_schema_include_requires_mapping_root(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nincludes: ["schema_extra.yaml"]\n'
            'root_class:\n  - f1: {type: str}\n'
        )
        (yaml_dir / 'schema_extra.yaml').write_text('- not\n- a\n- mapping\n')

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('mapping', result.stdout.lower())
        self.assertIn('schema_extra.yaml', result.stdout)

    def test_schema_include_name_must_follow_convention(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nincludes: ["other.yaml"]\n'
            'root_class:\n  - f1: {type: str}\n'
        )
        (yaml_dir / 'other.yaml').write_text('version: "1.0"\n')

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('schema_[a-z0-9_]+.yaml', result.stdout)

    def test_duplicate_schema_declaration_fails(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nincludes: ["schema_extra.yaml"]\n'
            'root_class:\n  - first: {type: str}\n'
        )
        (yaml_dir / 'schema_extra.yaml').write_text(
            'version: "1.0"\nroot_class:\n  - second: {type: str}\n'
        )

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('duplicate declaration', result.stdout)

    def test_invalid_descriptor_keyword_fails(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nroot_class:\n'
            '  - first: {type: str, not_a_keyword: true}\n'
        )

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('not_a_keyword', result.stdout)

    def test_malformed_class_entry_fails_actionably(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nroot_class:\n  - invalid\n'
        )

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('one-entry mapping', result.stdout)
        self.assertNotIn('Traceback', result.stderr)

    def test_schema_registry_reference_must_resolve(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nroot_class:\n  - op: {type: expression}\n'
            'expression: {registry: missing_registry}\n'
        )

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('missing_registry', result.stdout)

    def test_schema_unknown_type(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text('version: "1.0"\nroot_class:\n  - f1: {type: unknown_type}\n')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ERROR', result.stdout)
        if 'unknown_type' not in result.stdout:
            print("STDOUT:", result.stdout)
            print("STDERR:", result.stderr)
        self.assertIn('unknown_type', result.stdout)

    def test_schema_column_types_are_not_schema_builtins(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\nroot_class:\n  - f1: {type: date}\n'
        )

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unknown_type 'date'", result.stdout)

    def test_registry_name_is_not_a_schema_type(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "1.0"\n'
            'root_class:\n  - f1: {type: operations}\n'
            'operation: {registry: operations}\n'
            'operations:\n  one: {type: str}\n'
        )

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unknown_type 'operations'", result.stdout)

    def test_schema_version_mismatch(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text('version: "1.0"\nincludes: ["other.yaml"]\nroot_class:\n  - f1: {type: str}\n')
        (yaml_dir / 'other.yaml').write_text('version: "2.0"\n')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ERROR', result.stdout)
        self.assertIn('version', result.stdout.lower())

    def test_schema_bundle_version_is_not_hardcoded(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text(
            'version: "2.0"\nincludes: ["schema_extra.yaml"]\n'
            'root_class:\n  - value: {type: str}\n'
        )
        (yaml_dir / 'schema_extra.yaml').write_text('version: "2.0"\n')

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_example_layout_missing_files(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'bad-example'
        ex_dir.mkdir(parents=True, exist_ok=True)
        # missing everything
        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('README.md', result.stdout)

    def test_example_layout_negative_missing_how_to_fix(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'negative-bad'
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / 'README.md').write_text('# bad')
        (ex_dir / 'spec.yaml').write_text('{}')
        (ex_dir / 'input').mkdir()
        (ex_dir / 'expected').mkdir()
        (ex_dir / 'expected' / 'error.yaml').write_text('{}')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('How to fix', result.stdout)

    def test_example_layout_positive_has_error(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'positive-bad'
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / 'README.md').write_text('# bad')
        (ex_dir / 'spec.yaml').write_text('{}')
        (ex_dir / 'input').mkdir()
        (ex_dir / 'expected').mkdir()
        (ex_dir / 'expected' / 'error.yaml').write_text('{}')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('error.yaml', result.stdout)

    def test_example_index_stale(self):
        ex_dir = self.root_dir / 'yaml' / 'examples'
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / 'README.md').write_text('# Index\n\n| [`stale`](stale/) | stale desc |\n')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('stale', result.stdout)

    def test_example_index_link_must_match_directory(self):
        examples_dir = self.root_dir / 'yaml' / 'examples'
        ex_dir = examples_dir / 'good'
        ex_dir.mkdir(parents=True)
        (ex_dir / 'README.md').write_text('# Good: description')
        (examples_dir / 'README.md').write_text(
            '| [`good`](wrong-target/) | description |\n'
        )

        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(self.root_dir)],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('wrong-target', result.stdout)

    def test_example_index_missing(self):
        ex_dir = self.root_dir / 'yaml' / 'examples'
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / 'README.md').write_text('# Index\n')
        good_ex = ex_dir / 'good'
        good_ex.mkdir()
        (good_ex / 'README.md').write_text('# Good: description')
        (good_ex / 'spec.yaml').write_text('{}')
        (good_ex / 'input').mkdir()
        (good_ex / 'expected').mkdir()
        (good_ex / 'expected' / 'out.csv').write_text('h1')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('not in index', result.stdout)

    def test_csv_header_uses_output_columns(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'csv-output'
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / 'spec.yaml').write_text(
            'output:\n  columns: [c, a]\n'
            'columns:\n  - name: a\n  - name: b\n  - name: c\n'
        )
        (ex_dir / 'expected').mkdir()
        (ex_dir / 'expected' / 'out.csv').write_text('c,a\n3,1\n')

        errors, _ = VALIDATOR.validate_examples_csv(self.root_dir)

        self.assertEqual(errors, [])

    def test_csv_header_mismatch(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'csv-bad'
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / 'README.md').write_text('# CSV: bad header')
        (ex_dir / 'spec.yaml').write_text(
            'output:\n  columns: [a, c]\n'
            'columns:\n  - name: a\n  - name: b\n  - name: c\n'
        )
        (ex_dir / 'input').mkdir()
        (ex_dir / 'expected').mkdir()
        (ex_dir / 'expected' / 'out.csv').write_text('a,b,c\n1,2,3')
        (self.root_dir / 'yaml' / 'examples' / 'README.md').write_text('| [`csv-bad`](csv-bad/) | bad header |\n')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('csv-bad', result.stdout)
        self.assertIn('header', result.stdout.lower())

    def test_spec_structural_unknown_field(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'spec-bad-field'
        ex_dir.mkdir(parents=True, exist_ok=True)
        (self.root_dir / 'yaml' / 'schema.yaml').write_text('''version: "1.0"
root_class:
  - schema_version: {type: str, required: true}
  - domain: {type: str, required: true}
  - datasets: {type: str, required: true}
  - keys: {type: str, required: true}
  - columns: {type: str, required: true}
''')
        (ex_dir / 'README.md').write_text('# Bad: field')
        # Missing domain and datasets which are required, and has an unknown field 'bad_field'
        (ex_dir / 'spec.yaml').write_text('''schema_version: "1.0"
domain: "test"
datasets: {}
keys: []
columns: []
bad_field: "what"
''')
        (ex_dir / 'input').mkdir()
        (ex_dir / 'expected').mkdir()
        (ex_dir / 'expected' / 'out.csv').write_text('a\n1')
        (self.root_dir / 'yaml' / 'examples' / 'README.md').write_text('| [`spec-bad-field`](spec-bad-field/) | field |\n')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('bad_field', result.stdout)

    def test_multiple_spec_variants_are_discovered_and_validated(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'variant-example'
        (ex_dir / 'input').mkdir(parents=True)
        (ex_dir / 'expected').mkdir()
        (ex_dir / 'README.md').write_text('# Variant example\n')
        (ex_dir / 'expected' / 'out.csv').write_text('value\n1\n')
        (ex_dir / 'spec_r.yaml').write_text('value: valid\n')
        (ex_dir / 'spec_py.yaml').write_text(
            'value: valid\nbad_field: true\n'
        )
        env = {
            'version': '1.0',
            'classes': {
                'root_class': [
                    {'value': {'type': 'str', 'required': True}}
                ]
            },
            'aliases': {},
            'registries': {},
        }

        paths = VALIDATOR.example_spec_paths(ex_dir)
        self.assertEqual(
            [path.name for path in paths],
            ['spec_py.yaml', 'spec_r.yaml'],
        )
        self.assertEqual(
            VALIDATOR.validate_examples_layout(self.root_dir), []
        )
        errors = VALIDATOR.validate_examples_structure(self.root_dir, env)
        self.assertTrue(errors)
        self.assertIn('spec_py.yaml', '\n'.join(errors))
        self.assertIn('bad_field', '\n'.join(errors))

    def test_layout_rejects_base_spec_mixed_with_variants(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'mixed-specs'
        (ex_dir / 'input').mkdir(parents=True)
        (ex_dir / 'expected').mkdir()
        (ex_dir / 'README.md').write_text('# Mixed specs\n')
        (ex_dir / 'expected' / 'out.csv').write_text('value\n1\n')
        (ex_dir / 'spec.yaml').write_text('value: base\n')
        (ex_dir / 'spec_r.yaml').write_text('value: variant\n')

        errors = VALIDATOR.validate_examples_layout(self.root_dir)
        self.assertTrue(errors)
        self.assertIn('cannot mix', '\n'.join(errors))

    def test_empty_spec_is_rejected(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'empty-spec'
        ex_dir.mkdir(parents=True)
        (ex_dir / 'spec.yaml').write_text('')
        env = {
            'version': '1.0',
            'classes': {'root_class': []},
            'aliases': {},
            'registries': {},
        }

        errors = VALIDATOR.validate_examples_structure(self.root_dir, env)
        self.assertTrue(errors)
        self.assertIn('mapping', '\n'.join(errors))

    def test_spec_schema_version_must_match_bundle(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'wrong-version'
        ex_dir.mkdir(parents=True)
        (ex_dir / 'spec.yaml').write_text('schema_version: "2.0"\n')
        env = {
            'version': '1.0',
            'classes': {
                'root_class': [
                    {
                        'schema_version': {
                            'type': 'str',
                            'required': True,
                        }
                    }
                ]
            },
            'aliases': {},
            'registries': {},
        }

        errors = VALIDATOR.validate_examples_structure(self.root_dir, env)
        self.assertTrue(errors)
        self.assertIn('2.0', '\n'.join(errors))
        self.assertIn('1.0', '\n'.join(errors))

    def test_negative_spec_rejects_unrelated_structural_errors(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'negative-unrelated'
        (ex_dir / 'expected').mkdir(parents=True)
        (ex_dir / 'spec.yaml').write_text(
            'columns:\n  - name: COUNTRY\n    derivation: valid\n'
            'bad_root_field: true\n'
        )
        (ex_dir / 'expected' / 'error.yaml').write_text(
            'phase: validation\ncondition: invalid_field_type\n'
            'spec_paths: [columns.COUNTRY.derivation]\n'
        )
        env = {
            'classes': {
                'root_class': [
                    {'columns': {'type': 'list[column_class]', 'required': True}}
                ],
                'column_class': [
                    {'name': {'type': 'str', 'required': True}},
                    {'derivation': {'type': 'str', 'required': True}},
                ],
            },
            'aliases': {},
            'registries': {},
        }

        errors = VALIDATOR.validate_examples_structure(self.root_dir, env)
        self.assertTrue(errors)
        self.assertIn('bad_root_field', '\n'.join(errors))

    def test_negative_spec_allows_declared_structural_failure(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'negative-declared'
        (ex_dir / 'expected').mkdir(parents=True)
        (ex_dir / 'spec.yaml').write_text(
            'columns:\n  - name: COUNTRY\n    derivation: {nested: value}\n'
        )
        (ex_dir / 'expected' / 'error.yaml').write_text(
            'phase: validation\ncondition: invalid_field_type\n'
            'spec_paths: [columns.COUNTRY.derivation]\n'
        )
        env = {
            'classes': {
                'root_class': [
                    {'columns': {'type': 'list[column_class]', 'required': True}}
                ],
                'column_class': [
                    {'name': {'type': 'str', 'required': True}},
                    {'derivation': {'type': 'str', 'required': True}},
                ],
            },
            'aliases': {},
            'registries': {},
        }

        errors = VALIDATOR.validate_examples_structure(self.root_dir, env)
        self.assertEqual(errors, [])

    def test_negative_path_matches_list_item(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'negative-list-path'
        (ex_dir / 'expected').mkdir(parents=True)
        (ex_dir / 'spec.yaml').write_text('keys: [wrong]\n')
        (ex_dir / 'expected' / 'error.yaml').write_text(
            'phase: validation\ncondition: invalid_field_type\n'
            'spec_paths: [keys]\n'
        )
        env = {
            'version': '1.0',
            'classes': {
                'root_class': [
                    {'keys': {'type': 'list[int]', 'required': True}}
                ]
            },
            'aliases': {},
            'registries': {},
        }

        errors = VALIDATOR.validate_examples_structure(self.root_dir, env)
        self.assertEqual(errors, [])

    def test_expected_error_contract_rejects_bad_phase_and_path(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'negative-contract'
        (ex_dir / 'expected').mkdir(parents=True)
        (ex_dir / 'spec.yaml').write_text('columns: [{name: A}]\n')
        (ex_dir / 'expected' / 'error.yaml').write_text(
            'phase: someday\ncondition: Bad Name\n'
            'spec_paths: [columns.MISSING]\ncontext: text\n'
        )

        errors = VALIDATOR.validate_expected_error_contracts(self.root_dir)

        message = '\n'.join(errors)
        self.assertIn('phase', message)
        self.assertIn('snake-case', message)
        self.assertIn('does not exist', message)
        self.assertIn('context', message)

    def test_csv_shape_rejects_duplicate_header_and_short_row(self):
        csv_dir = self.root_dir / 'yaml' / 'examples' / 'csv-shape' / 'input'
        csv_dir.mkdir(parents=True)
        (csv_dir / 'input.csv').write_text('A,A\n1\n')

        errors = VALIDATOR.validate_csv_shapes(self.root_dir)

        message = '\n'.join(errors)
        self.assertIn('duplicate header', message)
        self.assertIn('expected 2 fields, got 1', message)

    def test_readme_contract_rejects_schema_vocabulary_and_extra_section(self):
        ex_dir = self.root_dir / 'yaml' / 'examples' / 'readme-contract'
        (ex_dir / 'expected').mkdir(parents=True)
        (ex_dir / 'README.md').write_text(
            '# Test: output\n\nThe schema derivation is shown.\n\n## Notes\n'
        )

        errors = VALIDATOR.validate_example_readmes(self.root_dir)

        message = '\n'.join(errors)
        self.assertIn('schema vocabulary', message)
        self.assertIn('unsupported level-two section', message)

    def test_integration_full_corpus(self):
        real_root = self.tool_path.parent.parent.parent
        result = subprocess.run(
            [sys.executable, str(self.tool_path), '--root', str(real_root)],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('PASS', result.stdout)

if __name__ == '__main__':
    unittest.main()
