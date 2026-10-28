"""Core's expression table and the schema page's table of expression modules,
generated from the declarations of the expression functions."""

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.commands import repository
from tools.packaging import generate_expression_table as generator
from tools.paths import ROOT
from tools.tests import support

# Each generator that reads a function's declarations, in the order the
# repository checks run them, and the files under the root it reads or writes.
GENERATORS = (
    "tools.packaging.generate_expression_table",
    "tools.packaging.generate_json_schema",
    "tools.packaging.generate_shipped_schema",
    "tools.packaging.generate_style_orders",
)
FILES = (
    "yaml",
    "docs/articles/schema.md",
    "rust/crates/yamaa-core/src/expression_table.rs",
    "rust/crates/yamaa-core/src/schema/versioned_layer.rs",
    "rust/crates/yamaa-adapters/schema",
    "rust/crates/yamaa-adapters/src/shipped_schema/data.rs",
    "rust/crates/yamaa-adapters/src/lint/fields.rs",
)
TABLE = "rust/crates/yamaa-core/src/expression_table.rs"
GRAMMAR = "yaml/grammar/expression.yaml"
STR = "yaml/schema_expression_str.yaml"


def roles(verb):
    """Each argument of a verb with its role, and an operand's form."""
    return [(item.name, item.role, item.form) for item in verb.arguments]


class ExpressionTableTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.declarations = generator.Declarations()
        cls.verbs = {verb.name: verb for verb in cls.declarations.verbs}

    def test_the_committed_files_are_the_declarations(self):
        self.assertEqual(
            (ROOT / generator.MODULE).read_text(encoding="ascii"), generator.render()
        )
        self.assertEqual(
            (ROOT / generator.PAGE).read_text(encoding="ascii"), generator.page()
        )

    def test_the_repository_checks_run_it(self):
        self.assertIn("tools.packaging.generate_expression_table", repository.CHECKS)

    def test_a_name_is_its_words_in_upper_camel_case(self):
        for name, variant in (
            ("LOG10", "Log10"),
            ("STR_CONTAINS", "StrContains"),
            ("NULLIF", "NullIf"),
            ("Q1", "Q1"),
            ("mdy_slash", "MdySlash"),
            ("datetime", "DateTime"),
            ("round_half_away_from_zero", "RoundHalfAwayFromZero"),
        ):
            with self.subTest(case=name):
                self.assertEqual(generator.camel(name), variant)
        with self.assertRaises(SystemExit) as raised:
            generator.camel("2d")
        self.assertEqual(raised.exception.code, "ERROR: '2d' names no Rust variant")

    def test_a_joined_spelling_no_declaration_holds_is_refused(self):
        with mock.patch.dict(generator.JOINED, {"AVGMEAN": "AvgMean"}):
            with self.assertRaises(SystemExit) as raised:
                generator.render()
        self.assertEqual(
            raised.exception.code,
            "ERROR: JOINED names 'AVGMEAN', which no declaration spells",
        )

    def test_a_note_for_an_expression_no_module_registers_alone_is_refused(self):
        # str_pad shares schema_expression_str.yaml with the other text verbs.
        for name in ("str_twice", "str_pad"):
            with self.subTest(case=name):
                with mock.patch.dict(generator.NOTES, {name: "which pads text"}):
                    with self.assertRaises(SystemExit) as raised:
                        generator.page_table()
                self.assertEqual(
                    raised.exception.code,
                    f"ERROR: NOTES names {name!r}, which no module registers alone",
                )

    def test_a_function_s_result_decides_the_contexts_that_admit_it(self):
        lines = generator.functions_rust(
            [
                generator.Function("ABS", 1, 1, "numeric"),
                generator.Function("STR_CONTAINS", 2, 2, "boolean"),
            ],
            ["predicate", "aggregate", "derivation"],
        )
        start = lines.index("    pub const fn contexts(self) -> &'static [Context] {")
        self.assertEqual(
            lines[start : start + 6],
            [
                "    pub const fn contexts(self) -> &'static [Context] {",
                "        match self.result() {",
                "            FunctionResult::Numeric => &[Context::Predicate, "
                "Context::Aggregate, Context::Derivation],",
                "            FunctionResult::Boolean => &[Context::Predicate],",
                "        }",
                "    }",
            ],
        )

    def test_an_argument_s_kind_decides_its_role(self):
        # The kinds of yaml/README.md: `variable`, `filtered_source` and a
        # list of variables are names, `string_template` is an expression,
        # `substitute_value` is a handler's type and `window_selection` the
        # window's; every other type here is a value.
        expected = {
            "diff": [
                ("type", "parameter", None),
                ("start", "operand", "Name"),
                ("end", "operand", "Name"),
                ("unit", "parameter", None),
                ("closed", "parameter", None),
            ],
            "mapping": [
                ("source", "operand", "Binding"),
                ("dict", "parameter", None),
                ("case_sensitive", "parameter", None),
                ("missing", "handler", None),
                ("unmapped", "handler", None),
            ],
            "greatest": [("sources", "operand", "Names")],
            "str_template": [
                ("template", "operand", "Placeholders"),
                ("missing", "handler", None),
            ],
            "shift": [
                ("type", "parameter", None),
                ("source", "operand", "Name"),
                ("days", "operand", "Name"),
                ("amount", "operand", "Name"),
                ("unit", "parameter", None),
            ],
            "rank": [("method", "parameter", None), ("window", "window", None)],
        }
        for verb, arguments in expected.items():
            with self.subTest(case=verb):
                self.assertEqual(roles(self.verbs[verb]), arguments)

    def test_a_verb_s_form_is_its_window_argument_or_its_structure(self):
        forms = {verb: self.verbs[verb].form for verb in self.verbs}
        self.assertEqual(
            [verb for verb, form in forms.items() if form == "structure"],
            list(generator.STRUCTURES),
        )
        self.assertEqual(
            [verb for verb, form in forms.items() if form == "window"],
            ["row_number", "rank", "row_value", "locf", "baseline_flag"],
        )

    def test_a_bare_payload_fills_the_one_required_field_of_its_class(self):
        # SCHEMA-10: `[T, class]` whose class has one required field of type T.
        self.assertEqual(
            {
                verb.name: verb.shorthand
                for verb in self.declarations.verbs
                if verb.shorthand
            },
            {
                "flag": "condition",
                "str_template": "template",
                "aggregate": "expr",
                "odm": "item",
            },
        )

    def test_an_inline_list_of_values_needs_its_enum(self):
        with mock.patch.dict(generator.VALUE_ENUMS, clear=True):
            with self.assertRaises(SystemExit) as raised:
                generator.Declarations()
        self.assertEqual(
            raised.exception.code,
            "ERROR: expressions.cut.closed: VALUE_ENUMS names no enum for closed",
        )

    def test_the_check_refuses_stale_files_and_write_regenerates_them(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for path in ("yaml", generator.PAGE.as_posix()):
                copy(ROOT, root, path)
            support.write(root, generator.MODULE, "// stale\n")
            with mock.patch.object(generator, "ROOT", root):
                self.assertEqual(
                    support.run_main(generator.main),
                    (
                        f"ERROR: {TABLE} differ from the declarations of the "
                        "expression functions; regenerate them with python -m "
                        "tools.packaging.generate_expression_table --write",
                        "",
                        "",
                    ),
                )
                self.assertEqual(
                    support.run_main(generator.main, "--write"), (0, "", "")
                )
                self.assertEqual(
                    support.run_main(generator.main),
                    (
                        0,
                        "PASS: core's expression table and the schema page are "
                        "the declarations'\n",
                        "",
                    ),
                )

    def test_a_new_function_changes_only_its_declaration_and_generated_files(self):
        # The dry run of #3800: a throwaway function added to a copy of its
        # declaration, then every generator that reads the declarations run
        # over the copy. A grammar function reaches the expression table
        # alone; a verb, also the schema page's table, the JSON Schema and
        # the engine's copies of its schema module and the JSON Schema.
        cases = (
            (
                "grammar function",
                GRAMMAR,
                "  function:\n",
                "    TWICE: {min_arguments: 1, max_arguments: 1, result: numeric}\n",
                {GRAMMAR, TABLE},
            ),
            (
                "verb",
                STR,
                "expressions:\n",
                "    str_twice:\n"
                "        - source:\n"
                "            type: variable\n"
                "            required: true\n"
                "            description: Text written twice.\n",
                {
                    STR,
                    TABLE,
                    "docs/articles/schema.md",
                    "yaml/json/spec.schema.json",
                    "rust/crates/yamaa-adapters/schema/schema_expression_str.yaml",
                    "rust/crates/yamaa-adapters/schema/json/spec.schema.json",
                },
            ),
        )
        for label, declaration, after, added, changed in cases:
            with self.subTest(case=label), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                shutil.copytree(
                    ROOT / "tools",
                    root / "tools",
                    ignore=shutil.ignore_patterns("__pycache__", "tests"),
                )
                for path in FILES:
                    copy(ROOT, root, path)
                text = (root / declaration).read_text(encoding="ascii")
                self.assertEqual(text.count(after), 1, label)
                (root / declaration).write_text(
                    text.replace(after, after + added), encoding="ascii"
                )
                for module in GENERATORS:
                    subprocess.run(
                        [sys.executable, "-m", module, "--write"],
                        cwd=root,
                        check=True,
                        capture_output=True,
                    )
                self.assertEqual(differences(ROOT, root), changed, label)


def copy(source, target, relative):
    """Copy a file or a directory tree from one root to the same path in another."""
    if (source / relative).is_dir():
        shutil.copytree(source / relative, target / relative)
    else:
        (target / relative).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / relative, target / relative)


def differences(source, target):
    """The files under :data:`FILES` that differ between the two roots, or
    that one of them lacks."""
    found = set()
    for relative in FILES:
        paths = set()
        for root in (source, target):
            start = root / relative
            if start.is_dir():
                paths.update(
                    path.relative_to(root)
                    for path in start.rglob("*")
                    if path.is_file()
                )
            elif start.is_file():
                paths.add(start.relative_to(root))
        for path in paths:
            before, after = source / path, target / path
            if not (before.is_file() and after.is_file()) or (
                before.read_bytes() != after.read_bytes()
            ):
                found.add(path.as_posix())
    return found


if __name__ == "__main__":
    unittest.main()
