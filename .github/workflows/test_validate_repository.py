import copy
import importlib.util
import math
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

    def test_non_finite_yaml_floats_normalize_to_missing(self):
        spellings = [
            '.inf', '.Inf', '.INF', '+.inf', '+.Inf', '+.INF',
            '-.inf', '-.Inf', '-.INF', '.nan', '.NaN', '.NAN',
        ]
        loaded = yaml.load(
            ''.join(
                f"value_{index}: {spelling}\n"
                for index, spelling in enumerate(spellings)
            ) + "quoted: '.inf'\n",
            Loader=VALIDATOR.UniqueKeyLoader,
        )

        for index, spelling in enumerate(spellings):
            with self.subTest(spelling=spelling):
                self.assertIsNone(loaded[f"value_{index}"])
        self.assertEqual(loaded["quoted"], ".inf")

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


class TestTextSourceBoundary(unittest.TestCase):
    def test_rejects_non_ascii_source_but_allows_unicode_csv_data(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source = root / 'rule.md'
            source.write_bytes(
                b'# Rule\ntext ' + bytes([0xC3, 0xA9]) + b'\n'
            )
            for fixture_type in ('input', 'expected'):
                data = (
                    root / 'yaml' / 'examples' / 'example' / fixture_type
                )
                data.mkdir(parents=True)
                (data / 'values.csv').write_text(
                    'VALUE\n' + chr(0x00E9) + '\n',
                    encoding='utf-8',
                )

            errors = VALIDATOR.validate_ascii_sources(root)
            csv_errors = VALIDATOR.validate_csv_shapes(root)

        self.assertEqual(
            errors,
            ['ERROR: rule.md:2:6: non_ascii_source byte 0xC3'],
        )
        self.assertEqual(csv_errors, [])

    def test_rejects_non_ascii_csv_outside_fixture_directories(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / 'reference.csv').write_text(
                'VALUE\n' + chr(0x00E9) + '\n',
                encoding='utf-8',
            )

            errors = VALIDATOR.validate_ascii_sources(root)

        self.assertEqual(
            errors,
            ['ERROR: reference.csv:2:1: non_ascii_source byte 0xC3'],
        )

    def test_rejects_a_decoded_surrogate(self):
        document = {'value': chr(0xD800)}

        errors = VALIDATOR.validate_unicode_scalars(document, 'spec.yaml')

        self.assertEqual(
            errors,
            [
                'ERROR: spec.yaml.value: invalid_text surrogate U+D800 '
                'at string offset 0'
            ],
        )

    def test_escapes_a_surrogate_mapping_key_in_value_diagnostics(self):
        key = chr(0xD800)
        document = {key: chr(0xD801)}

        errors = VALIDATOR.validate_unicode_scalars(document, 'spec.yaml')

        self.assertEqual(
            errors,
            [
                'ERROR: spec.yaml.<key>: invalid_text surrogate U+D800 '
                'at string offset 0',
                'ERROR: spec.yaml.\\ud800: invalid_text surrogate U+D801 '
                'at string offset 0',
            ],
        )
        for error in errors:
            error.encode('ascii')

    def test_rejects_ill_formed_utf8_data(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            data = (
                Path(temp_dir)
                / 'yaml' / 'examples' / 'example' / 'input'
            )
            data.mkdir(parents=True)
            (data / 'values.csv').write_bytes(
                b'VALUE\n' + bytes([0xFF]) + b'\n'
            )

            errors = VALIDATOR.validate_csv_shapes(Path(temp_dir))

        self.assertEqual(len(errors), 1)
        self.assertIn('invalid_text', errors[0])


class TestPredicateLanguage(unittest.TestCase):
    def test_parses_precedence_compounds_and_all_literal_types(self):
        predicate = (
            "NOT FLAG = 'N' OR "
            "COUNT BETWEEN -1 AND +2.5e1 AND "
            "TERM LIKE '100!!%' ESCAPE '!' AND "
            "DAY >= DATE '2025-01-01' AND "
            "MOMENT < DATETIME '2025-01-02T03:04' AND "
            "OPTION IN ('A', 'B', NULL)"
        )

        ast = VALIDATOR.parse_predicate(predicate)
        types = {
            'FLAG': 'str',
            'COUNT': 'int',
            'TERM': 'str',
            'DAY': 'date',
            'MOMENT': 'datetime',
            'OPTION': 'str',
        }
        errors = VALIDATOR.validate_predicate_types(ast, types.get)

        self.assertEqual(ast['kind'], 'or')
        self.assertEqual(errors, [])

    def test_rejects_syntax_outside_the_closed_grammar(self):
        invalid = [
            'VALUE != 1',
            'VALUE + 1 > 2',
            'VALUE',
            'VALUE IN ()',
            "VALUE = 'unterminated",
            "DAY = DATE '2025-02-30'",
            "TERM LIKE 'abc!' ESCAPE '!'",
        ]

        for predicate in invalid:
            with self.subTest(predicate=predicate):
                with self.assertRaises(VALIDATOR.PredicateError):
                    VALIDATOR.parse_predicate(predicate)

    def test_allows_an_escaped_escape_at_the_end_of_like_pattern(self):
        ast = VALIDATOR.parse_predicate("TERM LIKE 'abc!!' ESCAPE '!'")
        errors = VALIDATOR.validate_predicate_types(
            ast, {'TERM': 'str'}.get
        )
        self.assertEqual(errors, [])

    def test_rejects_unknown_and_incompatible_operands(self):
        ast = VALIDATOR.parse_predicate(
            "UNKNOWN = 1 OR DAY = '2025-01-01' OR COUNT LIKE '1%'"
        )

        errors = VALIDATOR.validate_predicate_types(
            ast, {'DAY': 'date', 'COUNT': 'int'}.get
        )
        message = '\n'.join(errors)

        self.assertIn("unknown identifier 'UNKNOWN'", message)
        self.assertIn("'date' and 'str'", message)
        self.assertIn("LIKE requires str operands", message)

    def test_validates_predicate_sites_in_a_specification(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            example_dir = Path(temp_dir)
            source = example_dir / 'dm.csv'
            source.write_text('USUBJID,AGE\n01,40\n')
            spec_path = example_dir / 'spec.yaml'
            spec = {
                'domain': 'ADSL',
                'datasets': {
                    'DM': {
                        'path': 'dm.csv',
                        'types': {'AGE': 'int'},
                    }
                },
                'base': 'DM',
                'keys': ['USUBJID'],
                'output': {'columns': ['USUBJID', 'AGE']},
                'columns': [
                    {
                        'name': 'USUBJID',
                        'type': 'str',
                        'derivation': {'source': 'DM.USUBJID'},
                    },
                    {
                        'name': 'AGE',
                        'type': 'int',
                        'derivation': {
                            'case': {
                                'branches': [
                                    {
                                        'when': 'DM.AGE >= 18',
                                        'then': {'source': 'DM.AGE'},
                                    }
                                ]
                            }
                        },
                    },
                ],
                'verifications': [
                    {
                        'predicate': {
                            'id': 'known-age',
                            'assert': 'MISSING > 0',
                        }
                    }
                ],
            }

            errors = VALIDATOR.validate_spec_predicates(
                spec, 'example/spec.yaml', spec_path
            )

        self.assertEqual(len(errors), 1)
        self.assertIn('verifications[0].predicate.assert', errors[0])
        self.assertIn("unknown identifier 'MISSING'", errors[0])

    def test_validates_a_grouped_row_count_filter(self):
        spec = {
            'domain': 'ADLB',
            'datasets': {'LB': 'lb.csv'},
            'base': 'LB',
            'keys': ['USUBJID'],
            'output': {'columns': ['USUBJID']},
            'columns': [
                {
                    'name': 'USUBJID',
                    'type': 'str',
                    'derivation': {'source': 'LB.USUBJID'},
                },
                {
                    'name': 'ABLFL',
                    'type': 'str',
                    'derivation': {'literal': 'Y'},
                },
            ],
            'verifications': [
                {
                    'row_count': {
                        'id': 'one-baseline-per-subject',
                        'group_by': ['USUBJID'],
                        'filter': "ABLFL = 'Y'",
                        'max': 1,
                    }
                },
                {
                    'row_count': {
                        'id': 'unknown-operand',
                        'group_by': ['USUBJID'],
                        'filter': "NOSUCHCOL = 'Y'",
                        'max': 1,
                    }
                },
                {
                    'row_count': {
                        'id': 'malformed-predicate',
                        'group_by': ['USUBJID'],
                        'filter': 'ABLFL =',
                        'max': 1,
                    }
                },
            ],
        }

        errors = VALIDATOR.validate_spec_predicates(spec, 'example/spec.yaml')

        self.assertEqual(len(errors), 2)
        self.assertIn('verifications[1].row_count.filter', errors[0])
        self.assertIn("unknown identifier 'NOSUCHCOL'", errors[0])
        self.assertIn('verifications[2].row_count.filter', errors[1])
        self.assertIn('invalid predicate', errors[1])


class TestProjectFunctionEnvironment(unittest.TestCase):
    def setUp(self):
        root = TOOL_PATH.parents[2]
        self.spec_schema, spec_errors = VALIDATOR.build_schema_env(root)
        self.environment_schema, environment_errors = (
            VALIDATOR.build_schema_env(root, 'schema_environment.yaml')
        )
        self.assertEqual(spec_errors, [])
        self.assertEqual(environment_errors, [])

    def contract(self):
        return {
            'contract_version': '1.0.0',
            'implementation_version': '2026.1',
            'description': 'Test a numeric value.',
            'params': [
                {
                    'name': 'x',
                    'type': 'float',
                    'accepts_missing': False,
                },
                {
                    'name': 'enabled',
                    'type': 'bool',
                    'required': False,
                    'default': True,
                },
            ],
            'returns': 'float',
            'binding': {
                'call': 'projectstats::test_value',
                'args': {'x': 'x', 'enabled': 'enabled'},
            },
            'conformance': 'conformance/test-value.yaml',
        }

    def write_project(self, root, contract=None):
        root.mkdir(parents=True, exist_ok=True)
        conformance = root / 'conformance'
        conformance.mkdir()
        (conformance / 'test-value.yaml').write_text(
            'schema_version: "1.0"\n'
            'function: test_value\n'
            'contract_version: "1.0.0"\n'
            'cases:\n'
            '  - id: ordinary\n'
            '    covers: [normal, numeric-comparison, boolean-true:enabled]\n'
            '    args: {x: 1.0, enabled: true}\n'
            '    result: 1.0\n'
            '  - id: boundary\n'
            '    covers: [boundary, numeric-comparison, boolean-false:enabled]\n'
            '    args: {x: 0.0, enabled: false}\n'
            '    result: 0.0\n'
            '  - id: default-enabled\n'
            '    covers: [default:enabled, numeric-comparison]\n'
            '    args: {x: 2.0}\n'
            '    result: 2.0\n'
            '  - id: missing-x-short-circuits\n'
            '    covers: [short-circuit-missing:x]\n'
            '    args: {x: null, enabled: true}\n'
            '    result: null\n'
            '  - id: missing-enabled-short-circuits\n'
            '    covers: [short-circuit-missing:enabled]\n'
            '    args: {x: 1.0, enabled: null}\n'
            '    result: null\n'
        )
        document = {
            'schema_version': '1.0',
            'version': '2026.1',
            'runtime': {
                'language': 'r',
                'artifact': {
                    'reference': 'org.example/test-r:2026.1',
                    'digest': 'sha256:' + ('0' * 64),
                },
            },
            'functions': {'test_value': contract or self.contract()},
        }
        environment_path = root / 'environment.yaml'
        environment_path.write_text(yaml.safe_dump(document, sort_keys=False))
        return document, environment_path

    def test_validates_environment_and_missing_short_circuit_vectors(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            document, environment_path = self.write_project(Path(temp_dir))

            errors = VALIDATOR.validate_project_environment(
                document,
                'environment.yaml',
                environment_path,
                self.environment_schema,
            )

        self.assertEqual(errors, [])

    def test_rejects_optional_without_default_and_implicit_widening(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            contract = self.contract()
            contract['params'][0]['required'] = False
            contract['params'][1]['type'] = 'float'
            document, environment_path = self.write_project(
                Path(temp_dir), contract
            )

            errors = VALIDATOR.validate_project_environment(
                document,
                'environment.yaml',
                environment_path,
                self.environment_schema,
            )

        message = '\n'.join(errors)
        self.assertIn('optional parameter requires', message)
        self.assertIn("expected exact type 'float', got 'bool'", message)

    def test_rejects_runtime_specific_or_incomplete_binding(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            contract = self.contract()
            contract['binding'] = {
                'call': 'projectstats.test_value',
                'args': {'x': 'x'},
            }
            document, environment_path = self.write_project(
                Path(temp_dir), contract
            )

            errors = VALIDATOR.validate_project_environment(
                document,
                'environment.yaml',
                environment_path,
                self.environment_schema,
            )

        message = '\n'.join(errors)
        self.assertIn('not fully qualified for runtime', message)
        self.assertIn("missing=['enabled']", message)

    def test_contract_fingerprint_excludes_implementation_binding(self):
        first = self.contract()
        second = copy.deepcopy(first)
        second['implementation_version'] = '2026.2'
        second['description'] = 'A differently worded description.'
        second['binding']['call'] = 'otherproject::test_value'

        first_id = VALIDATOR.function_contract_fingerprint(
            'test_value', first
        )
        second_id = VALIDATOR.function_contract_fingerprint(
            'test_value', second
        )
        second['comparison_decimals'] = 5
        changed_id = VALIDATOR.function_contract_fingerprint(
            'test_value', second
        )

        self.assertEqual(first_id, second_id)
        self.assertNotEqual(first_id, changed_id)

    def test_call_requires_exact_version_and_argument_type(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            project = Path(temp_dir)
            self.write_project(project)
            spec_path = project / 'spec.yaml'
            spec = {
                'columns': [
                    {'name': 'A', 'type': 'int'},
                    {
                        'name': 'B',
                        'type': 'float',
                        'derivation': {
                            'function': {
                                'name': 'test_value',
                                'contract_version': '2.0.0',
                                'args': {'x': 'A'},
                            }
                        },
                    },
                ]
            }

            errors = VALIDATOR.validate_spec_functions(
                spec, 'spec.yaml', spec_path, self.spec_schema
            )

        message = '\n'.join(errors)
        self.assertIn('function_contract_mismatch', message)
        self.assertIn("expected exact type 'float', got 'int'", message)

    def test_function_call_can_defer_implementation_environment(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            spec_path = Path(temp_dir) / 'spec.yaml'
            spec = {
                'columns': [
                    {
                        'name': 'RESULT',
                        'type': 'float',
                        'derivation': {
                            'function': {
                                'name': 'non_finite_value',
                                'contract_version': '1.0.0',
                                'args': {
                                    'kind': {'literal': 'positive-infinity'}
                                },
                            }
                        },
                    }
                ]
            }

            errors = VALIDATOR.validate_spec_functions(
                spec, 'spec.yaml', spec_path, self.spec_schema
            )

        self.assertEqual(errors, [])

    def test_function_arguments_do_not_admit_nested_expressions(self):
        expression = {
            'function': {
                'name': 'test_value',
                'contract_version': '1.0.0',
                'args': {'x': {'compute': {'expr': '1 + 1'}}},
            }
        }

        errors = VALIDATOR.validate_type(
            expression, ['expression'], self.spec_schema, 'derivation'
        )

        self.assertTrue(errors)

    def test_malformed_environment_stops_before_semantic_validation(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            document, environment_path = self.write_project(Path(temp_dir))
            document['runtime']['language'] = []

            errors = VALIDATOR.validate_project_environment(
                document,
                'environment.yaml',
                environment_path,
                self.environment_schema,
            )

        self.assertTrue(errors)
        self.assertIn('expected str', '\n'.join(errors))

    def test_malformed_argument_keys_report_without_sorting_crash(self):
        errors = VALIDATOR.validate_function_arguments(
            {1: 1.0, 'other': 1.0, 'x': 1.0},
            [{'name': 'x', 'type': 'float'}],
            'function.args',
        )

        self.assertEqual(len(errors), 1)
        self.assertIn("unknown argument 'other'", errors[0])

    def test_short_circuit_vector_must_return_missing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            document, environment_path = self.write_project(root)
            vector_path = root / 'conformance' / 'test-value.yaml'
            vectors = yaml.safe_load(vector_path.read_text())
            vectors['cases'][3]['result'] = 1.0
            vector_path.write_text(
                yaml.safe_dump(vectors, sort_keys=False)
            )

            errors = VALIDATOR.validate_project_environment(
                document,
                'environment.yaml',
                environment_path,
                self.environment_schema,
            )

        self.assertIn(
            'short-circuiting case must return missing', '\n'.join(errors)
        )

    def test_requires_inferable_conformance_coverage(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            document, environment_path = self.write_project(root)
            vector_path = root / 'conformance' / 'test-value.yaml'
            vectors = yaml.safe_load(vector_path.read_text())
            vectors['cases'] = vectors['cases'][:1]
            vector_path.write_text(
                yaml.safe_dump(vectors, sort_keys=False)
            )

            errors = VALIDATOR.validate_project_environment(
                document,
                'environment.yaml',
                environment_path,
                self.environment_schema,
            )

        message = '\n'.join(errors)
        self.assertIn('missing required coverage', message)
        self.assertIn('default:enabled', message)
        self.assertIn('short-circuit-missing:x', message)

    def test_accepting_missing_and_nullable_output_have_distinct_coverage(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            contract = self.contract()
            contract['params'] = [
                {'name': 'x', 'type': 'float', 'accepts_missing': True}
            ]
            contract['may_return_missing'] = True
            contract['binding']['args'] = {'x': 'x'}
            document, environment_path = self.write_project(root, contract)
            vectors = {
                'schema_version': '1.0',
                'function': 'test_value',
                'contract_version': '1.0.0',
                'cases': [
                    {
                        'id': 'ordinary',
                        'covers': ['normal', 'numeric-comparison'],
                        'args': {'x': 1.0},
                        'result': 1.0,
                    },
                    {
                        'id': 'boundary',
                        'covers': ['boundary', 'numeric-comparison'],
                        'args': {'x': 0.0},
                        'result': 0.0,
                    },
                    {
                        'id': 'accepted-missing',
                        'covers': ['accepted-missing:x'],
                        'args': {'x': None},
                        'result': 0.0,
                    },
                    {
                        'id': 'nullable-result',
                        'covers': ['nullable-output'],
                        'args': {'x': 2.0},
                        'result': None,
                    },
                ],
            }
            vector_path = root / 'conformance' / 'test-value.yaml'
            vector_path.write_text(
                yaml.safe_dump(vectors, sort_keys=False)
            )

            errors = VALIDATOR.validate_project_environment(
                document,
                'environment.yaml',
                environment_path,
                self.environment_schema,
            )

        self.assertEqual(errors, [])

    def test_python_host_arguments_are_identifiers_and_not_keywords(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            document, environment_path = self.write_project(Path(temp_dir))
            document['runtime']['language'] = 'python'
            binding = document['functions']['test_value']['binding']
            binding['call'] = 'projectstats.test_value'
            binding['args'] = {'x': 'class', 'enabled': 'lower.tail'}

            errors = VALIDATOR.validate_project_environment(
                document,
                'environment.yaml',
                environment_path,
                self.environment_schema,
            )

        message = '\n'.join(errors)
        self.assertIn("host argument 'class'", message)
        self.assertIn("host argument 'lower.tail'", message)
        self.assertTrue(
            VALIDATOR.valid_host_argument_name('r', 'lower.tail')
        )
        self.assertFalse(VALIDATOR.valid_host_argument_name('r', 'function'))

    def test_repository_compares_same_name_and_version_fingerprints(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            examples = root / 'yaml' / 'examples'
            self.write_project(examples / 'project-a')
            changed = self.contract()
            changed['comparison_decimals'] = 5
            self.write_project(examples / 'project-b', changed)

            errors = (
                VALIDATOR.validate_repository_function_fingerprints(
                    root, self.environment_schema
                )
            )

        self.assertEqual(len(errors), 1)
        self.assertIn('function_contract_mismatch', errors[0])

    def test_fingerprint_distinguishes_exact_default_scalar_types(self):
        float_contract = self.contract()
        float_contract['params'][1] = {
            'name': 'threshold',
            'type': 'float',
            'required': False,
            'default': 1.0,
        }
        int_contract = copy.deepcopy(float_contract)
        int_contract['params'][1]['type'] = 'int'
        int_contract['params'][1]['default'] = 1

        float_id = VALIDATOR.function_contract_fingerprint(
            'test_value', float_contract
        )
        int_id = VALIDATOR.function_contract_fingerprint(
            'test_value', int_contract
        )

        self.assertNotEqual(float_id, int_id)

    def test_fingerprint_has_a_stable_cross_language_golden_value(self):
        contract = {
            'contract_version': '1.0.0',
            'implementation_version': 'ignored',
            'description': 'Ignored by the logical fingerprint.',
            'params': [
                {
                    'name': 'threshold',
                    'type': 'float',
                    'required': False,
                    'default': 1.0,
                    'accepts_missing': False,
                },
                {
                    'name': 'enabled',
                    'type': 'bool',
                    'required': False,
                    'default': True,
                    'accepts_missing': True,
                },
            ],
            'returns': 'float',
            'comparison_decimals': 4,
        }

        fingerprint = VALIDATOR.function_contract_fingerprint(
            'score', contract
        )

        self.assertEqual(
            fingerprint,
            'sha256:7ef6d151e88fe84544f0afbeedaa2239'
            '2b6ccb74683bb5c4437e57868d8c1bfe',
        )
        self.assertEqual(
            VALIDATOR.canonical_function_value(1.0, 'float'),
            {'type': 'float', 'value': '3ff0000000000000'},
        )

    def test_non_finite_function_values_are_missing(self):
        for value in (math.inf, -math.inf, math.nan):
            with self.subTest(value=value):
                self.assertIsNone(VALIDATOR.function_value_type(value))
                self.assertFalse(
                    VALIDATOR.function_value_matches(value, 'float')
                )
                self.assertTrue(
                    VALIDATOR.function_value_matches(
                        value, 'float', accepts_missing=True
                    )
                )
                self.assertEqual(
                    VALIDATOR.canonical_function_value(value, 'float'),
                    {'type': 'missing'},
                )

    def test_non_finite_default_has_the_missing_fingerprint(self):
        non_finite = self.contract()
        non_finite['params'][1] = {
            'name': 'threshold',
            'type': 'float',
            'required': False,
            'default': math.inf,
            'accepts_missing': True,
        }
        missing = copy.deepcopy(non_finite)
        missing['params'][1]['default'] = None

        self.assertEqual(
            VALIDATOR.function_contract_fingerprint('score', non_finite),
            VALIDATOR.function_contract_fingerprint('score', missing),
        )


class TestRuleMetadata(unittest.TestCase):
    def test_requires_normative_rule_and_index_status(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            rules = root / 'yaml' / 'rules'
            rules.mkdir(parents=True)
            (rules / 'README.md').write_text(
                '| ID | Rule | Status | Owns | Depends on |\n'
                '|---|---|---|---|---|\n'
                '| R001 | Rule | proposed | Topic | -- |\n'
            )
            (rules / 'R001-rule.md').write_text(
                '---\nid: R001\ntitle: Rule\nstatus: proposed\n---\n'
            )

            errors = VALIDATOR.validate_rule_metadata(root)

        self.assertEqual(len(errors), 2)
        self.assertTrue(all('normative' in error for error in errors))


class TestSpecificationInheritance(unittest.TestCase):
    def setUp(self):
        self.env, schema_errors = VALIDATOR.build_schema_env(
            TOOL_PATH.parents[2]
        )
        self.assertEqual(schema_errors, [])

    def resolve(self, path):
        with open(path, 'r', encoding='utf-8') as handle:
            spec = yaml.load(handle, Loader=VALIDATOR.UniqueKeyLoader)
        return VALIDATOR.resolve_spec_inheritance(
            spec, 'example/spec.yaml', path, self.env
        )

    def test_resolves_shallow_diamond_and_minimal_ordered_spec(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            layers = root / 'layers'
            inputs = root / 'input'
            layers.mkdir()
            inputs.mkdir()
            (inputs / 'dm.csv').write_text('USUBJID,AGE\n01,40\n')
            (layers / 'common.yaml').write_text(
                'schema_version: "1.0"\n'
                'datasets:\n'
                '  DM: ../input/dm.csv\n'
                '  UNUSED: ../input/missing.csv\n'
                'base: DM\n'
                'record_lookups:\n'
                '  - id: unused_lookup\n'
                '    dataset: UNUSED\n'
                'columns:\n'
                '  - name: RESULT\n'
                '    type: str\n'
                '    label: Common label\n'
                '    metadata: {owner: common}\n'
                '    derivation: {literal: common}\n'
                '  - name: DEPENDENT\n'
                '    type: str\n'
                '    label: Dependent\n'
                '    derivation: {source: LATE}\n'
                '  - name: UNUSED\n'
                '    type: str\n'
                '    label: Unused\n'
                '    derivation: {source: MISSING}\n'
                '  - name: AUDIT\n'
                '    type: str\n'
                '    label: Verification input\n'
                '    derivation: {literal: audit}\n'
                'verifications:\n'
                '  unique: {columns: [AUDIT]}\n'
            )
            (layers / 'a.yaml').write_text(
                'schema_version: "1.0"\n'
                'parents: common.yaml\n'
                'metadata: {owner: a}\n'
                'columns:\n'
                '  - name: RESULT\n'
                '    label: Parent A label\n'
            )
            (layers / 'b.yaml').write_text(
                'schema_version: "1.0"\n'
                'parents: common.yaml\n'
                'metadata: {owner: b}\n'
                'datasets:\n'
                '  DM:\n'
                '    types: {AGE: int}\n'
                'columns:\n'
                '  - name: RESULT\n'
                '    derivation: {literal: parent-b}\n'
                '    metadata: null\n'
                '  - name: LATE\n'
                '    type: str\n'
                '    label: Late dependency\n'
                '    derivation: {literal: ready}\n'
            )
            spec_path = root / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: [layers/a.yaml, layers/b.yaml]\n'
                'domain: ADSL\n'
                'keys: [RESULT]\n'
                'output: {columns: [RESULT, DEPENDENT]}\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertNotIn('parents', resolved)
        self.assertEqual(resolved['metadata'], {'owner': 'b'})
        self.assertEqual(
            resolved['datasets'],
            {'DM': {'path': 'input/dm.csv', 'types': {'AGE': 'int'}}},
        )
        self.assertNotIn('record_lookups', resolved)
        self.assertEqual(
            [column['name'] for column in resolved['columns']],
            ['RESULT', 'AUDIT', 'LATE', 'DEPENDENT'],
        )
        result = resolved['columns'][0]
        self.assertEqual(result['label'], 'Parent A label')
        self.assertNotIn('metadata', result)
        self.assertEqual(
            result['derivation'], {'value': {'literal': 'parent-b'}}
        )

    def test_merges_each_keyed_collection_at_the_member_boundary(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / 'parent.yaml'
            parent.write_text(
                'schema_version: "1.0"\n'
                'datasets:\n'
                '  DM: input/dm.csv\n'
                '  REF: input/ref.csv\n'
                'base: DM\n'
                'record_lookups:\n'
                '  - id: ref\n'
                '    dataset: REF\n'
                '    source: DM.KEY\n'
                '    key: KEY\n'
                'columns:\n'
                '  - name: X\n'
                '    type: str\n'
                '    label: Value\n'
                '    derivation: {source: ref.VALUE}\n'
                'rows:\n'
                '  - id: main\n'
                '    dataset: DM\n'
                '    derivations:\n'
                '      X: {literal: parent}\n'
                '      UNUSED: {literal: parent}\n'
            )
            spec_path = root / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: parent.yaml\n'
                'domain: TEST\n'
                'keys: [X]\n'
                'output: {columns: [X]}\n'
                'record_lookups:\n'
                '  - id: ref\n'
                '    unmatched: missing\n'
                'rows:\n'
                '  - id: main\n'
                '    derivations:\n'
                '      X: {literal: child}\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertEqual(
            resolved['record_lookups'][0],
            {
                'id': 'ref',
                'dataset': 'REF',
                'source': ['DM.KEY'],
                'key': ['KEY'],
                'unmatched': 'missing',
            },
        )
        self.assertEqual(
            resolved['rows'][0],
            {
                'id': 'main',
                'dataset': 'DM',
                'derivations': {
                    'X': {'value': {'literal': 'child'}},
                },
            },
        )

    def test_final_error_names_the_contributing_parent_file(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / 'parent.yaml'
            parent.write_text(
                'schema_version: "1.0"\n'
                'datasets: {DM: input/dm.csv}\n'
                'base: DM\n'
                'columns:\n'
                '  - name: A\n'
                '    label: Incomplete parent column\n'
            )
            spec_path = root / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: parent.yaml\n'
                'domain: TEST\n'
                'keys: [A]\n'
                'output: {columns: [A]}\n'
            )
            with open(spec_path, 'r', encoding='utf-8') as handle:
                spec = yaml.load(handle, Loader=VALIDATOR.UniqueKeyLoader)

            errors = VALIDATOR.validate_spec_document(
                spec, 'example/spec.yaml', spec_path, self.env
            )

        self.assertTrue(errors)
        self.assertIn(
            f'(contributed by {parent.resolve()})', '\n'.join(errors)
        )

    def test_output_order_by_keeps_an_internal_column_live(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            layers = root / 'layers'
            inputs = root / 'input'
            layers.mkdir()
            inputs.mkdir()
            (inputs / 'dm.csv').write_text('USUBJID,SITEORD\n01,2\n')
            (layers / 'common.yaml').write_text(
                'schema_version: "1.0"\n'
                'datasets:\n'
                '  DM: ../input/dm.csv\n'
                'base: DM\n'
                'columns:\n'
                '  - name: USUBJID\n'
                '    type: str\n'
                '    label: Unique Subject Identifier\n'
                '    derivation: {source: DM.USUBJID}\n'
                '  - name: SITEORD\n'
                '    type: str\n'
                '    label: Site Sort Order\n'
                '    derivation: {source: DM.SITEORD}\n'
                '  - name: UNUSED\n'
                '    type: str\n'
                '    label: Unused\n'
                '    derivation: {literal: idle}\n'
            )
            spec_path = root / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: layers/common.yaml\n'
                'domain: ADSL\n'
                'keys: [USUBJID]\n'
                'output:\n'
                '  columns: [USUBJID]\n'
                '  order_by:\n'
                '    - {variable: SITEORD, direction: desc}\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertEqual(
            [column['name'] for column in resolved['columns']],
            ['USUBJID', 'SITEORD'],
        )

    def test_resolved_column_requires_a_label(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / 'parent.yaml'
            parent.write_text(
                'schema_version: "1.0"\n'
                'datasets: {DM: input/dm.csv}\n'
                'base: DM\n'
                'columns:\n'
                '  - name: A\n'
                '    type: str\n'
                '    derivation: {literal: value}\n'
            )
            spec_path = root / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: parent.yaml\n'
                'domain: TEST\n'
                'keys: [A]\n'
                'output: {columns: [A]}\n'
            )
            with open(spec_path, 'r', encoding='utf-8') as handle:
                spec = yaml.load(handle, Loader=VALIDATOR.UniqueKeyLoader)

            errors = VALIDATOR.validate_spec_document(
                spec, 'example/spec.yaml', spec_path, self.env
            )

        self.assertIn('requires a non-empty label', '\n'.join(errors))

    def test_accepts_an_absolute_parent_path(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / 'parent.yaml'
            parent.write_text('schema_version: "1.0"\n')
            spec_path = root / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                f'parents: "{parent.resolve()}"\n'
                'output: {columns: []}\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertEqual(errors, [])
        self.assertEqual(
            resolved,
            {'schema_version': '1.0', 'output': {'columns': []}},
        )

    def test_rejects_entry_that_inherits_output(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / 'parent.yaml'
            parent.write_text(
                'schema_version: "1.0"\n'
                'output: {columns: [A]}\n'
            )
            spec_path = root / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: parent.yaml\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn('missing_entry_output', '\n'.join(errors))

    def test_rejects_parent_url(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            spec_path = Path(temp_dir) / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: https://example.test/base.yaml\n'
                'output: {columns: []}\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn('invalid_parent_path', '\n'.join(errors))

    def test_rejects_missing_parent(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            spec_path = Path(temp_dir) / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: missing.yaml\n'
                'output: {columns: []}\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn('parent_not_found', '\n'.join(errors))

    def test_rejects_inheritance_cycle(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            spec_path = root / 'spec.yaml'
            parent = root / 'parent.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: parent.yaml\n'
                'output: {columns: []}\n'
            )
            parent.write_text(
                'schema_version: "1.0"\nparents: spec.yaml\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn('inheritance_cycle', '\n'.join(errors))

    def test_rejects_layer_version_mismatch(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            spec_path = root / 'spec.yaml'
            parent = root / 'parent.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: parent.yaml\n'
                'output: {columns: []}\n'
            )
            parent.write_text('schema_version: "2.0"\n')

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn('schema_version_mismatch', '\n'.join(errors))

    def test_rejects_clearing_required_or_absent_fields(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            parent = root / 'parent.yaml'
            parent.write_text(
                'schema_version: "1.0"\n'
                'columns:\n'
                '  - name: A\n'
                '    type: str\n'
                '    derivation: {literal: value}\n'
            )
            spec_path = root / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: parent.yaml\n'
                'output: {columns: [A]}\n'
                'columns:\n'
                '  - name: A\n'
                '    type: null\n'
                '    label: null\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        message = '\n'.join(errors)
        self.assertIn('invalid_clear', message)
        self.assertIn('columns.A.type', message)

    def test_rejects_duplicate_member_within_one_layer(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            spec_path = Path(temp_dir) / 'spec.yaml'
            spec_path.write_text(
                'schema_version: "1.0"\n'
                'parents: []\n'
                'output: {columns: []}\n'
                'columns:\n'
                '  - {name: A}\n'
                '  - {name: A}\n'
            )

            resolved, errors, _ = self.resolve(spec_path)

        self.assertIsNone(resolved)
        self.assertIn('duplicate_identifier', '\n'.join(errors))


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

    def test_accepts_integer_and_month_position_day(self):
        for day in (15, "first", "last"):
            with self.subTest(day=day):
                errors = VALIDATOR.validate_type(
                    {
                        "date_impute": {
                            "source": "AE.AESTDTC",
                            "month": 6,
                            "day": day,
                        }
                    },
                    ["expression"],
                    self.env,
                    "spec.columns.ASTDT.derivation",
                )

                self.assertEqual(errors, [])

    def test_rejects_unknown_day_position(self):
        errors = VALIDATOR.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": "mid",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertTrue(errors)

    def test_accepts_not_before_bound(self):
        errors = VALIDATOR.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": "last",
                    "not_before": "TRTSDT",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertEqual(errors, [])

    def test_rejects_unknown_date_impute_field(self):
        errors = VALIDATOR.validate_type(
            {
                "date_impute": {
                    "source": "AE.AESTDTC",
                    "month": 6,
                    "day": 15,
                    "not_after": "TRTSDT",
                }
            },
            ["expression"],
            self.env,
            "spec.columns.ASTDT.derivation",
        )

        self.assertTrue(errors)


class TestPreviousNonMissingSchema(unittest.TestCase):
    def setUp(self):
        self.env, schema_errors = VALIDATOR.build_schema_env(TOOL_PATH.parents[2])
        self.assertEqual(schema_errors, [])

    def validate(self, payload):
        return VALIDATOR.validate_type(
            {"previous_non_missing": payload},
            ["expression"],
            self.env,
            "spec.columns.PREVIOUS.derivation",
        )

    def test_accepts_closed_window_inputs(self):
        errors = self.validate(
            {
                "source": "AVAL",
                "group_by": ["STUDYID", "USUBJID", "PARAMCD"],
                "order_by": [
                    {
                        "variable": "ADT",
                        "direction": "asc",
                        "nulls": "last",
                    }
                ],
            }
        )

        self.assertEqual(errors, [])

    def test_allows_one_partition_by_omission(self):
        self.assertEqual(
            self.validate({"source": "AVAL", "order_by": ["ADT"]}),
            [],
        )

    def test_rejects_missing_source_and_unregistered_filter(self):
        missing_source = self.validate({"order_by": ["ADT"]})
        filtered = self.validate(
            {
                "source": "AVAL",
                "order_by": ["ADT"],
                "filter": "PARAMCD = 'WEIGHT'",
            }
        )

        self.assertIn("missing required field 'source'", missing_source[0])
        self.assertIn("unknown field 'filter'", filtered[0])


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

    def test_accepts_output_order_by_over_declared_columns(self):
        spec = {
            "domain": "ADSL",
            "datasets": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {
                "columns": ["USUBJID"],
                "order_by": [
                    "USUBJID",
                    {"variable": "SORTORD", "direction": "desc"},
                ],
            },
            "columns": [{"name": "USUBJID"}, {"name": "SORTORD"}],
        }

        self.assertEqual(
            VALIDATOR.validate_spec_names(spec, "example/spec.yaml"), []
        )

    def test_rejects_unknown_and_repeated_output_order_terms(self):
        spec = {
            "domain": "ADSL",
            "datasets": {"DM": "dm.csv"},
            "base": "DM",
            "keys": ["USUBJID"],
            "output": {
                "columns": ["USUBJID"],
                "order_by": [
                    "USUBJID",
                    {"variable": "USUBJID", "nulls": "first"},
                    "DM.USUBJID",
                    "MISSING",
                ],
            },
            "columns": [{"name": "USUBJID"}],
        }

        errors = VALIDATOR.validate_spec_names(spec, "example/spec.yaml")

        message = "\n".join(errors)
        self.assertIn("output.order_by: duplicate order term 'USUBJID'", message)
        self.assertIn(
            "output.order_by[2]: undeclared column 'DM.USUBJID'", message
        )
        self.assertIn(
            "output.order_by[3]: undeclared column 'MISSING'", message
        )

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

    def test_accepts_grouped_row_count(self):
        spec = {
            "domain": "ADLB",
            "datasets": {"LB": "lb.csv"},
            "base": "LB",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {"name": "USUBJID", "derivation": {"source": "LB.USUBJID"}},
                {"name": "ABLFL", "derivation": {"literal": "Y"}},
            ],
            "verifications": [
                {
                    "row_count": {
                        "id": "one-baseline-per-subject",
                        "group_by": ["USUBJID"],
                        "filter": "ABLFL = 'Y'",
                        "max": 1,
                    }
                }
            ],
        }

        self.assertEqual(
            VALIDATOR.validate_spec_contracts(spec, "example/spec.yaml"), []
        )

    def test_rejects_grouped_row_count_without_id_or_known_columns(self):
        spec = {
            "domain": "ADLB",
            "datasets": {"LB": "lb.csv"},
            "base": "LB",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {"name": "USUBJID", "derivation": {"source": "LB.USUBJID"}}
            ],
            "verifications": [
                {
                    "row_count": {
                        "group_by": ["USUBJID", "USUBJID", "MISSING"],
                        "max": 1,
                    }
                },
                {"row_count": {"id": "empty-grain", "group_by": [], "max": 1}},
            ],
        }

        errors = VALIDATOR.validate_spec_contracts(spec, "example/spec.yaml")

        message = "\n".join(errors)
        self.assertIn("a grouped row_count requires a verification id", message)
        self.assertIn("group_by: duplicate column 'USUBJID'", message)
        self.assertIn("group_by: unknown column 'MISSING'", message)
        self.assertIn("group_by: requires at least one column", message)

    def test_rejects_duplicate_row_count_verification_id(self):
        spec = {
            "domain": "ADLB",
            "datasets": {"LB": "lb.csv"},
            "base": "LB",
            "keys": ["USUBJID"],
            "output": {"columns": ["USUBJID"]},
            "columns": [
                {"name": "USUBJID", "derivation": {"source": "LB.USUBJID"}}
            ],
            "verifications": [
                {
                    "row_count": {
                        "id": "one-row-per-subject",
                        "group_by": ["USUBJID"],
                        "max": 1,
                    }
                },
                {
                    "predicate": {
                        "id": "one-row-per-subject",
                        "assert": "TRUE",
                    }
                },
            ],
        }

        errors = VALIDATOR.validate_spec_contracts(spec, "example/spec.yaml")

        self.assertIn(
            "duplicate dataset verification id", "\n".join(errors)
        )

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


class TestProducingSpecs(unittest.TestCase):
    VALID_PRODUCER_SPEC = '''schema_version: "1.0"
domain: DM
datasets:
  RAW: raw.csv
base: RAW
keys: [STUDYID]
output:
  columns: [STUDYID, AGE]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: {source: RAW.STUDYID}
  - name: AGE
    type: int
    label: Age
    derivation: {source: RAW.AGE}
'''

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
        (self.input_dir / "raw.csv").write_text(
            "STUDYID,AGE\nSTUDY1,42\n", encoding="utf-8"
        )

    def tearDown(self):
        self.test_dir.cleanup()

    def write_producer_spec(self, content=None):
        if content is None:
            content = self.VALID_PRODUCER_SPEC
        (self.input_dir / "dm.schema.yaml").write_text(
            content, encoding="utf-8"
        )

    def validate(self, source=None):
        if source is None:
            source = {
                "path": "input/dm.csv",
                "schema": "input/dm.schema.yaml",
            }
        return VALIDATOR.validate_producing_specs(
            {"datasets": {"DM": source}},
            "example/spec.yaml",
            self.spec_path,
            self.env,
            {self.spec_path.resolve()},
        )

    def test_accepts_complete_producing_spec(self):
        self.write_producer_spec()

        self.assertEqual(self.validate(), [])

    def test_rejects_inline_types_with_producing_spec(self):
        self.write_producer_spec()

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
            "combined with a producing specification",
            "\n".join(empty_types_errors),
        )

    def test_rejects_producer_version_mismatch(self):
        self.write_producer_spec(
            self.VALID_PRODUCER_SPEC.replace(
                'schema_version: "1.0"', 'schema_version: "2.0"'
            )
        )

        message = "\n".join(self.validate())

        self.assertIn("does not match bundle version", message)

    def test_rejects_reordered_producer_output(self):
        self.write_producer_spec(
            '''schema_version: "1.0"
domain: DM
datasets: {RAW: raw.csv}
base: RAW
keys: [STUDYID]
output:
  columns: [AGE, STUDYID]
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: {source: RAW.STUDYID}
  - name: AGE
    type: int
    label: Age
    derivation: {source: RAW.AGE}
'''
        )

        message = "\n".join(self.validate())

        self.assertIn("match the artifact header exactly", message)
        self.assertIn("['STUDYID', 'AGE']", message)
        self.assertIn("['AGE', 'STUDYID']", message)

    def test_rejects_empty_producer_artifact(self):
        self.write_producer_spec()
        (self.input_dir / "dm.csv").write_text("", encoding="utf-8")

        message = "\n".join(self.validate())

        self.assertIn("stored CSV artifact is empty", message)

    def test_rejects_legacy_field_map(self):
        self.write_producer_spec(
            '''version: "1.0"
fields: {STUDYID: string, AGE: integer}
'''
        )

        message = "\n".join(self.validate())

        self.assertIn("missing required field 'schema_version'", message)
        self.assertIn("unknown field 'version'", message)
        self.assertIn("unknown field 'fields'", message)

    def test_rejects_metadata_only_contract(self):
        self.write_producer_spec(
            '''schema_version: "1.0"
domain: DM
datasets: {}
keys: [STUDYID]
output: {columns: [STUDYID, AGE]}
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
  - name: AGE
    type: int
    label: Age
'''
        )

        message = "\n".join(self.validate())

        self.assertIn("base is required", message)
        self.assertIn("column has no derivation", message)

    def test_rejects_unlabeled_producer_output(self):
        self.write_producer_spec(
            self.VALID_PRODUCER_SPEC.replace("label: Age", "label: ''")
        )

        message = "\n".join(self.validate())

        self.assertIn(
            "stored producer column requires a non-empty label", message
        )

    def test_rejects_missing_producer_input(self):
        self.write_producer_spec()
        (self.input_dir / "raw.csv").unlink()

        message = "\n".join(self.validate())

        self.assertIn("source path does not exist: raw.csv", message)

    def test_rejects_producer_dependency_cycle(self):
        self.write_producer_spec(
            '''schema_version: "1.0"
domain: DM
datasets:
  LOOP:
    path: dm.csv
    schema: ../spec.yaml
base: LOOP
keys: [STUDYID]
output: {columns: [STUDYID, AGE]}
columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: {source: LOOP.STUDYID}
  - name: AGE
    type: int
    label: Age
    derivation: {source: LOOP.AGE}
'''
        )

        message = "\n".join(self.validate())

        self.assertIn("producer workflow dependency cycle", message)

    def test_rejects_missing_producing_spec(self):
        message = "\n".join(self.validate())

        self.assertIn("producing specification does not exist", message)


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
