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
            "yes_value: yes\non_value: ON\ntrue_value: true\n",
            Loader=VALIDATOR.UniqueKeyLoader,
        )
        self.assertEqual(loaded["yes_value"], "yes")
        self.assertEqual(loaded["on_value"], "ON")
        self.assertIs(loaded["true_value"], True)

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

    def test_schema_version_mismatch(self):
        yaml_dir = self.root_dir / 'yaml'
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / 'schema.yaml').write_text('version: "1.0"\nincludes: ["other.yaml"]\nroot_class:\n  - f1: {type: str}\n')
        (yaml_dir / 'other.yaml').write_text('version: "2.0"\n')

        result = subprocess.run([sys.executable, str(self.tool_path), '--root', str(self.root_dir)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ERROR', result.stdout)
        self.assertIn('version', result.stdout.lower())

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
