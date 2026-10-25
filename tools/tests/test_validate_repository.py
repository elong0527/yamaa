"""Static repository integrity checks; Rust and installed hosts own language behavior."""

import copy
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.tests import support
from tools.validation import _repository_yaml as OWNER_REPOSITORY_YAML
from tools.validation import check_documentation, validate_repository
from tools.validation import editorial as OWNER_EDITORIAL
from tools.validation import grammar_checks as OWNER_GRAMMAR_CHECKS
from tools.validation import repository_artifacts as OWNER_REPOSITORY_ARTIFACTS
from tools.validation import repository_manifests as OWNER_REPOSITORY_MANIFESTS
from tools.validation import repository_structure as OWNER_REPOSITORY_STRUCTURE

# Directories the repository walkers skip: a nested checkout, Rust build output,
# a cache and the root build output. Authored files beside them, at the root and
# in a nested directory named build, are still checked.
SKIPPED_LOCATIONS = (".worktrees/x/", "rust/target/", ".cache/", "build/")
CHECKED_LOCATIONS = ("", "docs/build/")


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
            Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
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
            Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
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
            ".inf",
            ".Inf",
            ".INF",
            "+.inf",
            "+.Inf",
            "+.INF",
            "-.inf",
            "-.Inf",
            "-.INF",
            ".nan",
            ".NaN",
            ".NAN",
        ]
        loaded = yaml.load(
            "".join(
                f"value_{index}: {spelling}\n"
                for index, spelling in enumerate(spellings)
            )
            + "quoted: '.inf'\n",
            Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
        )

        for index, spelling in enumerate(spellings):
            with self.subTest(spelling=spelling):
                self.assertIsNone(loaded[f"value_{index}"])
        self.assertEqual(loaded["quoted"], ".inf")

    def test_yaml_beyond_the_repository_subset_is_rejected(self):
        for case, document in [
            ("alias", "first: &value text\nsecond: *value\n"),
            ("merge key", "base: &base {a: 1}\nmerged: {<<: *base}\n"),
            ("explicit tag", "value: !!str 123\n"),
        ]:
            with self.subTest(case=case):
                with self.assertRaises(yaml.YAMLError):
                    yaml.load(document, Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader)

    def test_merge_keys_report_the_intended_reason(self):
        # The merge-key guard lives in flatten_mapping, which only runs when
        # construct_mapping calls it. This anchor-free merge key is the only
        # shape that reaches the guard (anchors/aliases die at the composer),
        # so it pins the intended message instead of the accidental
        # "could not determine a constructor for the tag
        # 'tag:yaml.org,2002:merge'".
        with self.assertRaisesRegex(yaml.YAMLError, "YAML merge keys are not allowed"):
            yaml.load(
                "merged:\n  <<: just_a_scalar\n",
                Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
            )

    def test_unhashable_keys_are_rejected_as_yaml_errors(self):
        # A sequence or mapping used as a key is valid YAML syntax, so the
        # loader must report it the way SafeLoader does (a YAMLError the
        # validation callers catch), not crash with a raw TypeError from
        # the duplicate-key membership test in construct_mapping.
        for document in ("? [a, b]\n: value\n", "? {a: 1}\n: value\n"):
            with self.subTest(document=document):
                with self.assertRaisesRegex(yaml.YAMLError, "unhashable key"):
                    yaml.load(
                        document,
                        Loader=OWNER_REPOSITORY_YAML.UniqueKeyLoader,
                    )


class TestTextSourceBoundary(unittest.TestCase):
    def test_preserves_only_named_third_party_unicode_data_and_notices(self):
        """Exact notice/data paths are exempt; neighboring project text stays checked."""
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for relative in (
                "rust/crates/yamaa-core/unicode/18.0.0/DerivedCoreProperties.txt",
                "rust/crates/yamaa-core/unicode/LICENSE.txt",
                "rust/crates/yamaa-python/LICENSE-UNICODE.txt",
                "R/yamaa/inst/LICENSE-UNICODE.txt",
                "python/LICENSE-UNICODE.txt",
                "R/yamaa/inst/LICENSE-THIRD-PARTY.txt",
                "python/LICENSE-THIRD-PARTY.txt",
                "rust/crates/yamaa-cli/LICENSE-THIRD-PARTY.txt",
                "rust/crates/yamaa-cli/LICENSE-MUSL.txt",
            ):
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(chr(0x00A9), encoding="utf-8")
            for relative in (
                "LICENSE-UNICODE.txt",
                "LICENSE-THIRD-PARTY.txt",
                "LICENSE-MUSL.txt",
                "rust/crates/yamaa-core/unicode/notes.md",
                "rust/crates/yamaa-cli/README.md",
            ):
                (root / relative).write_text(chr(0x00A9), encoding="utf-8")
            errors = OWNER_EDITORIAL.validate_ascii_sources(root)
        self.assertEqual(
            errors,
            [
                "ERROR: LICENSE-MUSL.txt:1:1: non_ascii_source byte 0xC2",
                "ERROR: LICENSE-THIRD-PARTY.txt:1:1: non_ascii_source byte 0xC2",
                "ERROR: LICENSE-UNICODE.txt:1:1: non_ascii_source byte 0xC2",
                "ERROR: rust/crates/yamaa-cli/README.md:1:1: non_ascii_source byte 0xC2",
                "ERROR: rust/crates/yamaa-core/unicode/notes.md:1:1: non_ascii_source byte 0xC2",
            ],
        )

    def test_rejects_non_ascii_source_but_allows_unicode_csv_data(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source = root / "rule.md"
            source.write_bytes(b"# Rule\ntext " + bytes([0xC3, 0xA9]) + b"\n")
            for fixture_type in ("input", "expected"):
                data = root / "benchmarks" / "example" / fixture_type
                data.mkdir(parents=True)
                (data / "values.csv").write_text(
                    "VALUE\n" + chr(0x00E9) + "\n",
                    encoding="utf-8",
                )

            errors = OWNER_EDITORIAL.validate_ascii_sources(root)

        self.assertEqual(
            errors,
            ["ERROR: rule.md:2:6: non_ascii_source byte 0xC3"],
        )

    def test_rejects_non_ascii_csv_outside_fixture_directories(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "reference.csv").write_text(
                "VALUE\n" + chr(0x00E9) + "\n",
                encoding="utf-8",
            )

            errors = OWNER_EDITORIAL.validate_ascii_sources(root)

        self.assertEqual(
            errors,
            ["ERROR: reference.csv:2:1: non_ascii_source byte 0xC3"],
        )

    def test_skips_nested_checkouts_build_output_and_caches(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for directory in SKIPPED_LOCATIONS + CHECKED_LOCATIONS:
                source = root / directory / "notes.md"
                source.parent.mkdir(parents=True, exist_ok=True)
                source.write_text(chr(0x00E9), encoding="utf-8")

            errors = OWNER_EDITORIAL.validate_ascii_sources(root)

        self.assertEqual(
            errors,
            [
                "ERROR: docs/build/notes.md:1:1: non_ascii_source byte 0xC3",
                "ERROR: notes.md:1:1: non_ascii_source byte 0xC3",
            ],
        )

    def test_rejects_a_decoded_surrogate(self):
        document = {"value": chr(0xD800)}

        errors = OWNER_EDITORIAL.validate_unicode_scalars(document, "spec.yaml")

        self.assertEqual(
            errors,
            [
                "ERROR: spec.yaml.value: invalid_text surrogate U+D800 "
                "at string offset 0"
            ],
        )

    def test_escapes_a_surrogate_mapping_key_in_value_diagnostics(self):
        key = chr(0xD800)
        document = {key: chr(0xD801)}

        errors = OWNER_EDITORIAL.validate_unicode_scalars(document, "spec.yaml")

        self.assertEqual(
            errors,
            [
                "ERROR: spec.yaml.<key>: invalid_text surrogate U+D800 "
                "at string offset 0",
                "ERROR: spec.yaml.\\ud800: invalid_text surrogate U+D801 "
                "at string offset 0",
            ],
        )
        for error in errors:
            error.encode("ascii")


class TestConditionRegistry(unittest.TestCase):
    def setUp(self):
        self.root = ROOT
        self.registry, load_errors = OWNER_REPOSITORY_MANIFESTS.load_condition_registry(
            self.root
        )
        self.assertEqual(load_errors, [])

    def test_repository_registry_passes(self):
        self.assertEqual(
            OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
                self.root, self.registry
            ),
            [],
        )

    def test_unregistered_condition_is_rejected(self):
        changed = copy.deepcopy(self.registry)
        del changed["conditions"]["aggregate_multiple_records"]

        errors = OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
            self.root, changed
        )

        self.assertEqual(
            errors,
            [
                "ERROR: benchmarks/negative-adlb-duplicate-wbc/"
                "expected/error.yaml.condition: unregistered condition "
                "'aggregate_multiple_records'",
                "ERROR: benchmarks/negative-multiple-baselines/"
                "expected/error.yaml.condition: unregistered condition "
                "'aggregate_multiple_records'",
            ],
        )

    def test_every_condition_has_a_well_formed_message(self):
        for condition, registration in self.registry["conditions"].items():
            self.assertEqual(
                OWNER_REPOSITORY_MANIFESTS.message_errors(registration.get("message")),
                [],
                condition,
            )

    def test_malformed_and_missing_messages_are_rejected(self):
        changed = copy.deepcopy(self.registry)
        conditions = changed["conditions"]
        del conditions["unknown_window"]["message"]
        conditions["window_order_by_required"]["message"] = "Needs an order_by."
        conditions["value_not_permitted"]["message"] = "{operation} takes {{no}"
        conditions["zero_offset"]["message"] = "an offset of {Offset}  is zero\n"
        conditions["xpt_last_row_blank"]["severity"] = "error"

        errors = OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
            self.root, changed
        )

        label = "ERROR: yaml/conditions.yaml.conditions"
        self.assertEqual(
            errors,
            [
                f"{label}.unknown_window.message: expected non-empty text",
                f"{label}.value_not_permitted.message: expected "
                "`{key}` placeholders and doubled literal braces",
                f"{label}.window_order_by_required.message: expected a "
                "lower-case start or a placeholder",
                f"{label}.window_order_by_required.message: expected no final period",
                f"{label}.xpt_last_row_blank.severity: unknown field",
                f"{label}.zero_offset.message: expected printable ASCII on one line",
                f"{label}.zero_offset.message: expected single spaces and no "
                "surrounding space",
                f"{label}.zero_offset.message: expected `{{key}}` placeholders and "
                "doubled literal braces",
            ],
        )

    def test_message_placeholders_name_keys_and_double_literal_braces(self):
        placeholders = OWNER_REPOSITORY_MANIFESTS.message_placeholders
        self.assertEqual(
            placeholders("column {column} reads {dependency}"), ["column", "dependency"]
        )
        self.assertEqual(placeholders("{{literal}} and {spec_path}"), ["spec_path"])
        self.assertEqual(placeholders("{{{key}}}"), ["key"])
        for stray in ("{", "}", "{key}}", "{Key}", "{1key}", "{key"):
            with self.subTest(stray=stray):
                self.assertIsNone(placeholders(stray), stray)

    def test_malformed_history_is_rejected(self):
        changed = copy.deepcopy(self.registry)
        conditions = changed["conditions"]
        conditions["window_order_by_required"]["renamed_from"] = "Order By"
        conditions["xpt_last_row_blank"]["renamed_from"] = "unknown_window"
        conditions["zero_offset"]["renamed_from"] = "schema_version_mismatch"
        retired = changed["retired"]
        retired["day_required"]["retired_in"] = "1.0"
        retired["month_required"]["reason"] = "gone"
        retired["aggregate_multiple_records"] = {"retired_in": "0.6.0"}

        errors = OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
            self.root, changed
        )

        label = "ERROR: yaml/conditions.yaml"
        self.assertEqual(
            errors,
            [
                f"{label}.retired: expected sorted keys",
                f"{label}.retired.day_required.retired_in: expected the release "
                "that retired it, such as '0.6.0'",
                f"{label}.retired.month_required.reason: unknown field",
                f"{label}.retired.aggregate_multiple_records: a registered "
                "condition is not retired",
                f"{label}.conditions.window_order_by_required.renamed_from: "
                "expected a snake-case name",
                f"{label}.conditions.xpt_last_row_blank.renamed_from: "
                "unknown_window is still a condition of the registry",
                f"{label}.conditions.zero_offset.renamed_from: "
                "unsupported_language_version was renamed from "
                "schema_version_mismatch too",
            ],
        )

    def test_a_benchmark_that_expects_a_retired_condition_is_rejected(self):
        changed = copy.deepcopy(self.registry)
        del changed["conditions"]["aggregate_multiple_records"]
        changed["retired"] = {
            "aggregate_multiple_records": {"retired_in": "1.1.0"},
            **changed["retired"],
        }

        errors = OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
            self.root, changed
        )

        self.assertEqual(
            errors,
            [
                f"ERROR: benchmarks/{case}/expected/error.yaml.condition: "
                "condition 'aggregate_multiple_records' is retired"
                for case in (
                    "negative-adlb-duplicate-wbc",
                    "negative-multiple-baselines",
                )
            ],
        )

    def test_wrong_phase_condition_is_rejected(self):
        changed = copy.deepcopy(self.registry)
        changed["conditions"]["aggregate_multiple_records"]["phases"] = ["derivation"]

        errors = OWNER_REPOSITORY_MANIFESTS.validate_condition_registry(
            self.root, changed
        )

        self.assertEqual(
            errors,
            [
                "ERROR: benchmarks/negative-adlb-duplicate-wbc/"
                "expected/error.yaml.phase: condition "
                "'aggregate_multiple_records' is not registered for phase "
                "'row_construction'"
            ],
        )


class TestValidationMetadata(unittest.TestCase):
    def test_repository_metadata_is_complete_and_registered(self):
        fixtures, errors = OWNER_REPOSITORY_MANIFESTS.load_validation_fixtures(ROOT)
        self.assertEqual(errors, [])
        self.assertEqual(
            set(fixtures),
            set(OWNER_REPOSITORY_MANIFESTS.validation_phase_contracts(ROOT)),
        )

    def test_unregistered_validation_conditions_fail(self):
        source = ROOT / "benchmarks/negative-unknown-window"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            case = root / "benchmarks" / source.name
            shutil.copytree(source, case)
            contract_path = case / "expected/error.yaml"
            contract = yaml.safe_load(contract_path.read_text())
            contract["condition"] = "not_registered"
            contract_path.write_text(yaml.safe_dump(contract))
            _, errors = OWNER_REPOSITORY_MANIFESTS.load_validation_fixtures(root)
            self.assertIn("unregistered validation condition", "\n".join(errors))


class TestClosedGrammarContracts(unittest.TestCase):
    """One machine-readable grammar per closed language, read by everyone."""

    @classmethod
    def setUpClass(cls):
        cls.root = ROOT

    def contract_root(self, temp_dir, **replacements):
        """Copy the grammars and rules into a root, applying replacements."""
        root = Path(temp_dir)
        shutil.copytree(self.root / "yaml" / "grammar", root / "yaml" / "grammar")
        shutil.copytree(self.root / "rules", root / "rules")
        for relative, (old, new) in replacements.items():
            path = root / relative
            text = path.read_text()
            self.assertIn(old, text)
            path.write_text(text.replace(old, new, 1))
        return root

    # -- the grammar is one source -----------------------------------------

    def test_every_contract_has_valid_structure(self):
        self.assertEqual(OWNER_GRAMMAR_CHECKS.validate_grammar_contracts(self.root), [])

    def test_every_rule_that_owns_a_grammar_has_a_contract(self):
        for contract in OWNER_GRAMMAR_CHECKS.GRAMMAR_CONTRACTS:
            with self.subTest(contract=contract):
                path = self.root / "yaml" / "grammar" / f"{contract}.yaml"
                self.assertTrue(path.is_file())
                document = yaml.safe_load(path.read_text())
                self.assertEqual(
                    document["rule"],
                    OWNER_GRAMMAR_CHECKS.grammar_rule_id(self.root, contract),
                )

    def test_a_marked_card_block_owns_its_grammar(self):
        text = (
            "### FORMULA-1. Title\n\nStatement.\n\n<!-- grammar: expression -->\n\n"
            "```text\nexpr := term\n```\n"
        )
        self.assertEqual(
            OWNER_GRAMMAR_CHECKS.rule_grammar_block(text, "expression"), "expr := term"
        )
        self.assertIsNone(
            OWNER_GRAMMAR_CHECKS.rule_grammar_block(text, "string-template")
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "rules").mkdir()
            (root / "rules" / "formulas.md").write_text(text)
            self.assertEqual(
                OWNER_GRAMMAR_CHECKS.grammar_rule_id(root, "expression"), "formulas"
            )
            self.assertEqual(
                OWNER_GRAMMAR_CHECKS.grammar_rule_id(root, "string-template"),
                OWNER_GRAMMAR_CHECKS.GRAMMAR_CONTRACTS["string-template"],
            )
            (root / "rules" / "copy.md").write_text(text)
            self.assertIsNone(OWNER_GRAMMAR_CHECKS.grammar_rule_id(root, "expression"))

    def test_a_continuation_aligns_under_its_definition_or_its_bar(self):
        rendered = OWNER_GRAMMAR_CHECKS.render_grammar_block(
            [
                {"name": "comparison", "definition": "a b\n| c d"},
                {"name": "null_test", "definition": "e\nf"},
            ]
        )
        self.assertEqual(
            rendered,
            "comparison := a b\n            | c d\nnull_test  := e\n              f",
        )

    def test_a_rule_block_that_drifts_from_its_grammar_is_reported(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "rules/text.md": (
                        'placeholder := "{" variable "}"',
                        'placeholder := "{" variable "}" | variable',
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(
                root, "string-template"
            )
        self.assertEqual(len(errors), 1)
        self.assertIn("rules/text.md", errors[0])
        self.assertIn("the grammar block is not the one", errors[0])

    # -- the vocabulary is one source --------------------------------------

    # -- the file is machine-checkable -------------------------------------

    def test_a_production_may_not_name_an_undefined_symbol(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/string-template.yaml": (
                        "imports:\n  variable: schema\n",
                        "imports: {}\n",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(
                root, "string-template"
            )
        self.assertEqual(len(errors), 1)
        self.assertIn("placeholder: undefined symbols ['variable']", errors[0])

    def test_an_import_must_name_a_contract_that_defines_it(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/expression.yaml": (
                        "imports: {}\n",
                        "imports:\n  digits: string-template\n",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "expression")
        self.assertEqual(len(errors), 1)
        self.assertIn("defines no 'digits' to import", errors[0])

    # -- each case is read in a declared context ----------------------------

    def test_the_expression_contexts_are_closed(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/expression.yaml": (
                        "contexts: [predicate, aggregate, derivation]\n",
                        "contexts: [predicate, aggregate]\n",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "expression")
        self.assertEqual(len(errors), 1)
        self.assertIn("contexts must be", errors[0])

    def test_every_case_names_a_declared_context(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/expression.yaml": (
                        "    context: predicate\n",
                        "    context: filter\n",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(root, "expression")
        self.assertEqual(len(errors), 1)
        self.assertIn("context must be one of", errors[0])
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(
                temp_dir,
                **{
                    "yaml/grammar/string-template.yaml": (
                        "  - id: ",
                        "  - context: predicate\n    id: ",
                    )
                },
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(
                root, "string-template"
            )
        self.assertEqual(len(errors), 1)
        self.assertIn("read in no contexts", errors[0])

    def test_a_function_declares_its_result_and_a_reducer_its_forms(self):
        for old, new, message in (
            (
                "ABS: {min_arguments: 1, max_arguments: 1, result: numeric}",
                "ABS: {min_arguments: 1, max_arguments: 1}",
                "function ABS must declare min_arguments, max_arguments and result",
            ),
            (
                "ABS: {min_arguments: 1, max_arguments: 1, result: numeric}",
                "ABS: {min_arguments: 1, max_arguments: 1, result: text}",
                "function ABS must return one of",
            ),
            (
                "SUM: {argument: [expression]}",
                "SUM: {argument: [expr]}",
                "reducer SUM must declare its argument forms",
            ),
        ):
            with self.subTest(new=new), tempfile.TemporaryDirectory() as temp_dir:
                root = self.contract_root(
                    temp_dir, **{"yaml/grammar/expression.yaml": (old, new)}
                )
                errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(
                    root, "expression"
                )
                self.assertEqual(len(errors), 1, errors)
                self.assertIn(message, errors[0])

    def test_every_named_category_has_an_authored_case(self):
        self.assertIn(
            "rejection", OWNER_GRAMMAR_CHECKS.GRAMMAR_COVERS["string-template"]
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.contract_root(temp_dir)
            source = root / "yaml" / "grammar" / "string-template.yaml"
            head, separator, cases = source.read_text().partition("cases:\n")
            self.assertTrue(separator)
            kept = [
                block
                for block in cases.split("  - id: ")[1:]
                if "parse: reject" not in block
            ]
            source.write_text(
                head + separator + "".join(f"  - id: {block}" for block in kept)
            )
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contract(
                root, "string-template"
            )
        self.assertEqual(len(errors), 1)
        self.assertIn("no case covers ['rejection']", errors[0])

    def test_a_missing_grammar_fails_validation(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            errors = OWNER_GRAMMAR_CHECKS.validate_grammar_contracts(Path(temp_dir))
        self.assertEqual(len(errors), len(OWNER_GRAMMAR_CHECKS.GRAMMAR_CONTRACTS))
        for error in errors:
            self.assertIn("missing machine-readable grammar", error)

    # -- the R parser reads the same files ----------------------------------

    def test_the_r_runner_reads_the_same_contracts(self):
        stager = self.root / "tools" / "staging" / "stage_grammar_inputs.py"
        text = stager.read_text()
        for contract, rule_id in OWNER_GRAMMAR_CHECKS.GRAMMAR_CONTRACTS.items():
            with self.subTest(contract=contract):
                self.assertIn(contract, text)
                self.assertIn(rule_id, text)
        self.assertIn('"yaml/grammar"', text)
        runner = self.root / "R" / "yamaa" / "tests" / "public_grammar.R"
        self.assertIn('"grammar-original"', runner.read_text())
        self.assertIn("yamaa_spec(", runner.read_text())


class TestValidatorCLI(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Every repository carries the shared regex fixtures and the closed
        # grammars, which are read against their rules, so a synthetic root
        # that expects a clean run needs all of them too. No test writes to
        # them, so each test's root links the one copy that the class makes.
        directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(directory.cleanup)
        cls.shared = Path(directory.name)
        conformance = cls.shared / "yaml" / "conformance"
        conformance.mkdir(parents=True)
        shutil.copy(ROOT / "yaml" / "conformance" / "regex.yaml", conformance)
        shutil.copytree(ROOT / "yaml" / "grammar", cls.shared / "yaml" / "grammar")
        shutil.copytree(ROOT / "rules", cls.shared / "rules")

    def setUp(self):
        self.test_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.test_dir.cleanup)
        self.root_dir = Path(self.test_dir.name)
        shutil.copytree(
            self.shared, self.root_dir, copy_function=os.link, dirs_exist_ok=True
        )

    def validate(self):
        """Run the repository validator on this root: (status, stdout, stderr)."""
        return support.run_main(validate_repository.main, "--root", str(self.root_dir))

    def check_documentation(self):
        """Run the documentation lint on this root: (status, stdout, stderr)."""
        return support.run_main(check_documentation.main, "--root", str(self.root_dir))

    def test_cli_help(self):
        status, output, _ = support.run_main(validate_repository.main, "--help")
        self.assertEqual(status, 0)
        self.assertIn("--root", output)

    def test_cli_missing_schema_fails(self):
        status, output, errors = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn(
            "ERROR: yaml/schema_spec.yaml: missing repository schema source",
            output.splitlines(),
        )

    def test_a_yaml_file_that_does_not_load_is_an_error_naming_it(self):
        for case, name, text, fragments in [
            (
                "duplicate keys",
                "bad.yaml",
                "a: 1\na: 2\n",
                ("ERROR", "bad.yaml", "line 2"),
            ),
            ("syntax error", "syntax.yaml", "a: 1\n  b: 2\n", ("ERROR", "syntax.yaml")),
        ]:
            with self.subTest(case=case):
                path = support.write(self.root_dir, f"yaml/{name}", text)
                status, output, _ = self.validate()
                path.unlink()
                self.assertNotEqual(status, 0)
                for fragment in fragments:
                    self.assertIn(fragment, output)

    def test_yaml_check_skips_nested_checkouts_build_output_and_caches(self):
        for directory in SKIPPED_LOCATIONS + CHECKED_LOCATIONS:
            folder = self.root_dir / directory
            folder.mkdir(parents=True, exist_ok=True)
            (folder / "bad.yaml").write_text("a: 1\na: 2\n")

        status, output, errors = self.validate()

        for directory in CHECKED_LOCATIONS:
            with self.subTest(directory=directory):
                self.assertIn(f"ERROR: {directory}bad.yaml:", output)
        for directory in SKIPPED_LOCATIONS:
            with self.subTest(directory=directory):
                self.assertNotIn(f"ERROR: {directory}", output)

    def test_schema_shared_include_is_not_a_cycle(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema_spec.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_left.yaml", "schema_right.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (yaml_dir / "schema_left.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_shared.yaml"]\n'
        )
        (yaml_dir / "schema_right.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_shared.yaml"]\n'
        )
        (yaml_dir / "schema_shared.yaml").write_text('version: "1.0"\n')

        status, output, errors = self.validate()
        self.assertEqual(status, 0, output)

    def test_schema_include_cannot_escape_yaml_directory(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema_spec.yaml").write_text(
            'version: "1.0"\nincludes: ["../outside.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (self.root_dir / "outside.yaml").write_text('version: "1.0"\n')

        status, output, errors = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn("local schema file name", output.lower())

    def test_schema_include_rejects_symlink_inside_yaml_directory(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema_spec.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_link.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (yaml_dir / "schema_real.yaml").write_text('version: "1.0"\n')
        (yaml_dir / "schema_link.yaml").symlink_to("schema_real.yaml")

        status, output, errors = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn("regular file", output.lower())

    def test_schema_include_requires_mapping_root(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema_spec.yaml").write_text(
            'version: "1.0"\nincludes: ["schema_extra.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (yaml_dir / "schema_extra.yaml").write_text("- not\n- a\n- mapping\n")

        status, output, errors = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn("mapping", output.lower())
        self.assertIn("schema_extra.yaml", output)

    def test_schema_include_name_must_follow_convention(self):
        yaml_dir = self.root_dir / "yaml"
        yaml_dir.mkdir(exist_ok=True)
        (yaml_dir / "schema_spec.yaml").write_text(
            'version: "1.0"\nincludes: ["other.yaml"]\n'
            "root_class:\n  - f1: {type: str}\n"
        )
        (yaml_dir / "other.yaml").write_text('version: "1.0"\n')

        status, output, errors = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn("local schema file name", output)

    def test_documentation_missing_readme(self):
        ex_dir = self.root_dir / "benchmarks" / "bad-example"
        ex_dir.mkdir(parents=True, exist_ok=True)
        # missing everything
        status, output, errors = self.check_documentation()
        self.assertNotEqual(status, 0)
        self.assertIn("README.md", output)

    def test_documentation_negative_missing_how_to_fix(self):
        ex_dir = self.root_dir / "benchmarks" / "negative-bad"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text("# bad")
        (ex_dir / "spec.yaml").write_text("{}")
        (ex_dir / "input").mkdir()
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "error.yaml").write_text("{}")

        status, output, errors = self.check_documentation()
        self.assertNotEqual(status, 0)
        self.assertIn("How to fix", output)

    def test_example_layout_positive_has_error(self):
        ex_dir = self.root_dir / "benchmarks" / "positive-bad"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text("# bad")
        (ex_dir / "spec.yaml").write_text("{}")
        (ex_dir / "input").mkdir()
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "error.yaml").write_text("{}")

        status, output, errors = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn("error.yaml", output)

    def test_an_index_row_without_its_directory_fails_the_documentation_lint(self):
        for case, files, fragment in [
            (
                "stale row",
                {"README.md": "# Index\n\n| [`stale`](stale/) | stale desc |\n"},
                "stale",
            ),
            (
                "link to another directory",
                {
                    "good/README.md": "# Good: description",
                    "README.md": "| [`good`](wrong-target/) | description |\n",
                },
                "wrong-target",
            ),
        ]:
            with self.subTest(case=case):
                for relative, text in files.items():
                    support.write(self.root_dir / "benchmarks", relative, text)
                status, output, _ = self.check_documentation()
                shutil.rmtree(self.root_dir / "benchmarks")
                self.assertNotEqual(status, 0)
                self.assertIn(fragment, output)

    def test_example_index_extra_columns_keep_link_title_and_duplicate_guards(self):
        examples = self.root_dir / "benchmarks"
        case = examples / "good"
        case.mkdir(parents=True)
        (case / "README.md").write_text("# Good: description\n")
        index = examples / "README.md"
        row = "| [`good`](good/) | description | lifecycle | Python, R |\n"
        index.write_text(row)
        self.assertEqual(OWNER_EDITORIAL.validate_examples_index(self.root_dir), [])
        index.write_text(row + row)
        self.assertIn(
            "duplicate index entry for good",
            "\n".join(OWNER_EDITORIAL.validate_examples_index(self.root_dir)),
        )
        index.write_text(row.replace("(good/)", "(wrong-target/)"))
        self.assertIn(
            "wrong-target",
            "\n".join(OWNER_EDITORIAL.validate_examples_index(self.root_dir)),
        )
        index.write_text(row.replace("description", "wrong description"))
        self.assertTrue(OWNER_EDITORIAL.validate_examples_index(self.root_dir))

    def test_example_index_accepts_full_title_with_colon(self):
        # The derived index (build_index) uses the benchmark README's full
        # title as the description, so a title containing a colon must not
        # be flagged just because the after-colon part differs from it.
        examples = self.root_dir / "benchmarks"
        case = examples / "good"
        case.mkdir(parents=True)
        (case / "README.md").write_text("# Good: description\n")
        index = examples / "README.md"
        index.write_text("| [`good`](good/) | Good: description |\n")
        self.assertEqual(OWNER_EDITORIAL.validate_examples_index(self.root_dir), [])
        index.write_text("| [`good`](good/) | Other: description |\n")
        self.assertTrue(OWNER_EDITORIAL.validate_examples_index(self.root_dir))

    def test_example_index_is_derived_not_committed(self):
        # The committed README carries no index table; the index is derived
        # from the case directories, so a case absent from the committed
        # README is still indexed.
        ex_dir = self.root_dir / "benchmarks"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text("# Index\n\n## Benchmarks\n")
        good_ex = ex_dir / "good"
        good_ex.mkdir()
        readme = good_ex / "README.md"
        readme.write_text(
            "# Good description\n"
            "[![Lifecycle: draft](https://example.com/badge)](https://example.com/lifecycle)\n"
        )
        (good_ex / "spec.yaml").write_text("{}")
        (good_ex / "benchmark.yaml").write_text(
            "lifecycle: draft\nstandard: TEST\ndomain: TEST\n"
        )
        self.assertEqual(OWNER_EDITORIAL.validate_examples_index(self.root_dir), [])
        # A case the derived index cannot render is reported, not dropped.
        (good_ex / "benchmark.yaml").unlink()
        readme.write_text("# Good description\n")
        errors = OWNER_EDITORIAL.validate_examples_index(self.root_dir)
        self.assertEqual(len(errors), 1)
        self.assertIn("benchmark.yaml", errors[0])
        self.assertIn("cannot derive benchmark index", errors[0])

    def test_csv_header_uses_output_columns(self):
        ex_dir = self.root_dir / "benchmarks" / "csv-output"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "spec.yaml").write_text(
            "output:\n  path: out.csv\n  columns: [c, a]\n"
            "columns:\n  - name: a\n  - name: b\n  - name: c\n"
        )
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "out.csv").write_text("c,a\n3,1\n")

        errors = OWNER_REPOSITORY_ARTIFACTS.validate_examples_csv(self.root_dir)

        self.assertEqual(errors, [])

    def test_csv_header_mismatch(self):
        ex_dir = self.root_dir / "benchmarks" / "csv-bad"
        ex_dir.mkdir(parents=True, exist_ok=True)
        (ex_dir / "README.md").write_text("# CSV: bad header")
        (ex_dir / "spec.yaml").write_text(
            "output:\n  path: out.csv\n  columns: [a, c]\n"
            "columns:\n  - name: a\n  - name: b\n  - name: c\n"
        )
        (ex_dir / "input").mkdir()
        (ex_dir / "expected").mkdir()
        (ex_dir / "expected" / "out.csv").write_text("a,b,c\n1,2,3")
        (self.root_dir / "benchmarks" / "README.md").write_text(
            "| [`csv-bad`](csv-bad/) | bad header |\n"
        )

        status, output, errors = self.validate()
        self.assertNotEqual(status, 0)
        self.assertIn("csv-bad", output)
        self.assertIn("header", output.lower())

    def write_producer_example(self, name):
        ex_dir = self.root_dir / "benchmarks" / name
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "spec_dm.yaml").write_text(
            "output:\n  path: dm.csv\n  columns: [a]\ncolumns:\n  - name: a\n"
        )
        (ex_dir / "spec_supp.yaml").write_text(
            "input:\n  DM: {path: dm.csv, schema: spec_dm.yaml}\n"
            "output:\n  path: supp.csv\n  columns: [b]\n"
            "columns:\n  - name: b\n"
        )
        (ex_dir / "expected" / "dm.csv").write_text("a\n1\n")
        (ex_dir / "expected" / "supp.csv").write_text("b\n2\n")
        return ex_dir

    def test_csv_golden_of_a_sibling_producer_is_accepted(self):
        self.write_producer_example("producer-output")

        errors = OWNER_REPOSITORY_ARTIFACTS.validate_examples_csv(self.root_dir)

        self.assertEqual(errors, [])

    def test_csv_golden_no_spec_declares_is_still_rejected(self):
        ex_dir = self.write_producer_example("producer-stray")
        (ex_dir / "expected" / "other.csv").write_text("c\n3\n")

        errors = OWNER_REPOSITORY_ARTIFACTS.validate_examples_csv(self.root_dir)

        message = "\n".join(errors)
        self.assertIn(
            "producer-stray/expected/other.csv: no authored output declares this artifact",
            message,
        )
        self.assertNotIn("dm.csv: expected artifact name", message)
        self.assertNotIn("supp.csv: expected artifact name", message)

    def test_csv_golden_of_a_declared_producer_entry_is_accepted(self):
        ex_dir = self.root_dir / "benchmarks" / "producer-entry"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "input").mkdir()
        (ex_dir / "input" / "dm.schema.yaml").write_text(
            "output:\n  path: dm.csv\n  columns: [a]\ncolumns:\n  - name: a\n"
        )
        (ex_dir / "spec.yaml").write_text(
            "input:\n  DM: {path: input/dm.csv, schema: input/dm.schema.yaml}\n"
            "output:\n  path: out.csv\n  columns: [b]\ncolumns:\n  - name: b\n"
        )
        (ex_dir / "expected" / "dm.csv").write_text("a\n1\n")
        (ex_dir / "expected" / "out.csv").write_text("b\n2\n")
        calls = (
            "  - {specification: input/dm.schema.yaml, environment: null}\n"
            "  - {specification: spec.yaml, environment: null}\n"
        )
        metadata = ex_dir / "benchmark.yaml"
        metadata.write_text(
            "lifecycle: draft\nstandard: TEST\ndomain: TEST\nentries:\n"
            f"  python:\n{calls}  r:\n{calls}"
        )

        errors = OWNER_REPOSITORY_ARTIFACTS.validate_examples_csv(self.root_dir)

        self.assertEqual(errors, [])
        # Without the entry, nothing declares the producer's artifact.
        metadata.write_text("lifecycle: draft\nstandard: TEST\ndomain: TEST\n")
        errors = OWNER_REPOSITORY_ARTIFACTS.validate_examples_csv(self.root_dir)
        self.assertIn(
            "producer-entry/expected/dm.csv: no authored output declares this artifact",
            "\n".join(errors),
        )

    def test_layout_rejects_base_spec_mixed_with_variants(self):
        ex_dir = self.root_dir / "benchmarks" / "mixed-specs"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "README.md").write_text(
            "# Mixed specs\n"
            "\n"
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/mixed-specs.html)\n"
        )
        (ex_dir / "expected" / "out.csv").write_text("value\n1\n")
        (ex_dir / "spec.yaml").write_text("value: base\n")
        (ex_dir / "spec_r.yaml").write_text("value: variant\n")

        errors = OWNER_REPOSITORY_STRUCTURE.validate_examples_layout(self.root_dir)
        self.assertTrue(errors)
        self.assertIn("cannot mix", "\n".join(errors))

    def test_readme_is_a_data_contract_without_badge_configuration(self):
        ex_dir = self.root_dir / "benchmarks" / "metadata-owned"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "README.md").write_text(
            "# Collected values\n\nThe value is the collected response.\n"
        )
        self.assertEqual(OWNER_EDITORIAL.validate_example_readmes(self.root_dir), [])
        self.assertEqual(
            OWNER_EDITORIAL.validate_examples_readme_presence(self.root_dir), []
        )

    def test_layout_skips_missing_readme_presence_check(self):
        # Issue #184 round 1: a missing README fails only in the docs-lint
        # gate (validate_examples_readme_presence), not in the layout gate.
        ex_dir = self.root_dir / "benchmarks" / "no-readme"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "spec.yaml").write_text("value: valid\n")
        (ex_dir / "expected" / "out.csv").write_text("value\n1\n")
        self.assertEqual(
            OWNER_REPOSITORY_STRUCTURE.validate_examples_layout(self.root_dir), []
        )
        presence = OWNER_EDITORIAL.validate_examples_readme_presence(self.root_dir)
        self.assertIn("missing README.md", "\n".join(presence))

    def test_layout_skips_missing_how_to_fix_check(self):
        # Issue #184 round 1: a negative README without '## How to fix'
        # fails only in the docs-lint gate, not in the layout gate.
        ex_dir = self.root_dir / "benchmarks" / "negative-no-fix"
        (ex_dir / "input").mkdir(parents=True)
        (ex_dir / "expected").mkdir()
        (ex_dir / "spec.yaml").write_text("value: valid\n")
        (ex_dir / "expected" / "error.yaml").write_text("{}")
        badge = (
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)]"
            "(https://elong0527.github.io/yamaa/benchmark/negative-no-fix.html)"
        )
        (ex_dir / "README.md").write_text(f"# No fix\n\n{badge}\n")
        self.assertEqual(
            OWNER_REPOSITORY_STRUCTURE.validate_examples_layout(self.root_dir), []
        )
        presence = OWNER_EDITORIAL.validate_examples_readme_presence(self.root_dir)
        self.assertIn("How to fix", "\n".join(presence))

    def test_readme_width_has_no_configuration_badge_exception(self):
        ex_dir = self.root_dir / "benchmarks" / "badge-width"
        (ex_dir / "expected").mkdir(parents=True)
        badge = (
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)]"
            "(https://elong0527.github.io/yamaa/benchmark/badge-width.html)"
        )
        lifecycle = (
            "[![Lifecycle: finalized]"
            "(https://img.shields.io/badge/Lifecycle-finalized-brightgreen)]"
            "(https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)"
        )
        combined = f"{badge} {lifecycle}"
        self.assertGreater(len(combined), 79)
        (ex_dir / "README.md").write_text(f"# Badge width\n\n{combined}\n")
        self.assertTrue(OWNER_EDITORIAL.validate_example_readmes(self.root_dir))
        (ex_dir / "README.md").write_text(f"# Long prose\n\n{'x' * 80}\n")
        errors = OWNER_EDITORIAL.validate_example_readmes(self.root_dir)
        self.assertIn("maximum is 79", "\n".join(errors))

    def test_expected_error_contract_rejects_bad_phase_and_path(self):
        ex_dir = self.root_dir / "benchmarks" / "negative-contract"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "spec.yaml").write_text("columns: [{name: A}]\n")
        (ex_dir / "expected" / "error.yaml").write_text(
            "phase: someday\ncondition: Bad Name\n"
            "spec_paths: [columns.MISSING]\ncontext: text\n"
        )

        errors = OWNER_REPOSITORY_MANIFESTS.validate_expected_error_contracts(
            self.root_dir
        )

        message = "\n".join(errors)
        self.assertIn("phase", message)
        self.assertIn("snake-case", message)
        self.assertIn("context", message)

    def test_readme_contract_rejects_schema_vocabulary_and_extra_section(self):
        ex_dir = self.root_dir / "benchmarks" / "readme-contract"
        (ex_dir / "expected").mkdir(parents=True)
        (ex_dir / "README.md").write_text(
            "# Test: output\n\nThe schema derivation is shown.\n\n## Notes\n"
        )

        errors = OWNER_EDITORIAL.validate_example_readmes(self.root_dir)

        message = "\n".join(errors)
        self.assertIn("schema vocabulary", message)
        self.assertIn("unsupported level-two section", message)

    def test_integration_full_corpus(self):

        real_root = ROOT
        guard = """
import importlib.abc
import runpy
import sys

class NoNativeDependencies(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        if fullname.split(".", 1)[0] in {"yamaa", "polars", "pyarrow"}:
            raise AssertionError("static validator imported " + fullname)
        return None

sys.meta_path.insert(0, NoNativeDependencies())
sys.argv = ["validate_repository", "--root", sys.argv[1]]
runpy.run_module("tools.validation.validate_repository", run_name="__main__")
"""
        result = subprocess.run(
            [sys.executable, "-c", guard, str(real_root)],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("PASS", result.stdout)


class TestKeyColumnOrder(unittest.TestCase):
    """Populated DOMAIN, STUDYID, USUBJID columns keep that order everywhere."""

    def example(self, raw, name, declared, artifact, header):
        example = Path(raw) / "benchmarks" / name
        (example / "expected").mkdir(parents=True)
        (example / "spec.yaml").write_text(
            'yamaa: "1.0"\ntype: spec\n'
            "domain: SE\n"
            "keys: [USUBJID]\n"
            "input: {}\n"
            f"output:\n  path: {name}.csv\n  columns: [{', '.join(artifact)}]\n"
            "columns:\n"
            + "".join(
                f"  - name: {column}\n    type: str\n    label: {column}\n"
                "    derivation: \"'X'\"\n"
                for column in declared
            )
        )
        (example / "expected" / f"{name}.csv").write_text(
            ",".join(header) + "\n" + ",".join("x" for _ in header) + "\n"
        )
        return Path(raw)

    def findings(self, name, declared, artifact, header):
        with tempfile.TemporaryDirectory() as raw:
            root = self.example(raw, name, declared, artifact, header)
            return OWNER_REPOSITORY_ARTIFACTS.validate_key_column_order(root)

    def test_accepts_ordered_key_columns(self):
        ordered = ["DOMAIN", "STUDYID", "USUBJID", "SESEQ"]
        self.assertEqual(
            self.findings("order-ok", ordered, ordered, ordered),
            [],
        )

    def test_accepts_a_missing_key_column(self):
        cols = ["DOMAIN", "USUBJID", "SESEQ"]
        self.assertEqual(self.findings("order-skip", cols, cols, cols), [])

    def test_rejects_a_swapped_declaration(self):
        declared = ["STUDYID", "DOMAIN", "USUBJID", "SESEQ"]
        ordered = ["DOMAIN", "STUDYID", "USUBJID", "SESEQ"]
        errors = self.findings("order-bad", declared, ordered, ordered)
        self.assertEqual(len(errors), 1)
        self.assertIn("order-bad/spec.yaml: declared columns", errors[0])
        self.assertIn("key columns out of order", errors[0])

    def test_rejects_a_swapped_golden_header(self):
        ordered = ["DOMAIN", "STUDYID", "USUBJID", "SESEQ"]
        header = ["STUDYID", "DOMAIN", "USUBJID", "SESEQ"]
        errors = self.findings("order-golden", ordered, ordered, header)
        self.assertEqual(len(errors), 1)
        self.assertIn("order-golden/expected/order-golden.csv: header", errors[0])
