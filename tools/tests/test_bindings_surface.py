"""The frozen sections of the host interface: vocabularies, tables, records, cli."""

import copy
import json
import re
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools.packaging import bindings_surface as surface
from tools.packaging import generate_bindings as generator


class FacadeCopy:
    """A temporary copy of the facade and the modules its constants name."""

    CORE_MODULES = ("run_report.rs", "odm_items.rs", "dataset/mod.rs")
    ADAPTER_MODULES = ("lint.rs",)

    def __enter__(self):
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        self.facade = root / "facade"
        self.core = root / "core"
        self.adapters = root / "adapters"
        shutil.copytree(surface.FACADE, self.facade)
        for source, copy_root, modules in (
            (surface.CORE, self.core, self.CORE_MODULES),
            (surface.ADAPTERS, self.adapters, self.ADAPTER_MODULES),
        ):
            for module in modules:
                target = copy_root / module
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(source / module, target)
        self.patches = [
            mock.patch.object(surface, "FACADE", self.facade),
            mock.patch.object(surface, "CORE", self.core),
            mock.patch.object(surface, "ADAPTERS", self.adapters),
        ]
        for patch in self.patches:
            patch.start()
        return self

    def edit(self, path, old, new):
        text = path.read_text(encoding="utf-8")
        self_check = text.count(old)
        if self_check != 1:
            raise AssertionError(f"{old!r} occurs {self_check} times in {path}")
        path.write_text(text.replace(old, new), encoding="utf-8")

    def __exit__(self, *exc):
        for patch in self.patches:
            patch.stop()
        self.directory.cleanup()


class FacadeConstantTests(unittest.TestCase):
    def test_every_vocabulary_and_table_lists_its_facade_constant(self):
        definition = generator._load_definition()
        for section, key in (("values", "values"), ("tables", "columns")):
            for name, entry in definition[section].items():
                with self.subTest(section=section, name=name):
                    listed = entry[key]
                    if key == "columns":
                        listed = [column["name"] for column in listed]
                    self.assertEqual(surface.facade_constant(entry["rust"]), listed)

    def test_aliases_are_followed_into_the_core_crate(self):
        self.assertEqual(
            surface.facade_constant("yamaa::REPORT_OUTCOMES"),
            ["succeeded", "failed", "held", "violated"],
        )
        self.assertEqual(len(surface.facade_constant("yamaa::ODM_COLUMNS")), 30)

    def test_aliases_are_followed_into_the_adapters_crate(self):
        findings = surface.facade_constant("yamaa::LINT_FINDINGS")
        self.assertEqual(findings[:2], ["field_order", "root_spacing"])
        self.assertEqual(len(findings), 16)

    def test_a_reference_must_name_one_exported_facade_constant(self):
        for reference, message in (
            ("yamaa::NOT_DECLARED", "does not export"),
            ("yamaa_core::run_report::KINDS", "facade constant as yamaa::NAME"),
            ("yamaa::ISSUE_COLUMNS::X", "facade constant as yamaa::NAME"),
            ("yamaa::VERSION", "unsupported constant value"),
        ):
            with self.subTest(reference=reference):
                with self.assertRaisesRegex(ValueError, message):
                    surface.facade_constant(reference)

    def test_an_undeclared_column_fails_generation(self):
        definition = generator._load_definition()
        with FacadeCopy() as copied:
            copied.edit(
                copied.facade / "diagnostic.rs",
                '    "operand_route",\n];',
                '    "operand_route",\n    "severity",\n];',
            )
            with self.assertRaisesRegex(ValueError, "tables.issues columns differ"):
                surface.validate_tables(definition["tables"], definition["values"])

    def test_an_undeclared_value_fails_generation_through_an_alias(self):
        definition = generator._load_definition()
        with FacadeCopy() as copied:
            copied.edit(
                copied.core / "run_report.rs",
                '"column", "check"];',
                '"column", "check", "warning"];',
            )
            with self.assertRaisesRegex(ValueError, "values.report_kind differs"):
                surface.validate_values(definition["values"])

    def test_an_undeclared_lint_finding_fails_generation_through_an_alias(self):
        definition = generator._load_definition()
        with FacadeCopy() as copied:
            copied.edit(
                copied.adapters / "lint.rs",
                '    "restated_inherited_value",\n];',
                '    "restated_inherited_value",\n    "trailing_space",\n];',
            )
            with self.assertRaisesRegex(ValueError, "values.lint_finding differs"):
                surface.validate_values(definition["values"])

    def test_a_declared_value_the_facade_lacks_fails(self):
        definition = generator._load_definition()
        candidate = copy.deepcopy(definition["values"])
        candidate["phase"]["values"].append("linting")
        with self.assertRaisesRegex(ValueError, "values.phase differs"):
            surface.validate_values(candidate)
        candidate = copy.deepcopy(definition["values"])
        candidate["severity"]["values"].reverse()
        with self.assertRaisesRegex(ValueError, "values.severity differs"):
            surface.validate_values(candidate)


class GrammarTests(unittest.TestCase):
    def setUp(self):
        self.definition = generator._load_definition()

    def test_value_and_table_declarations_refuse_malformed_entries(self):
        values = self.definition["values"]
        tables = self.definition["tables"]
        value_mutations = [
            ("invalid vocabulary name", lambda v: v.update({"Phase": v["phase"]})),
            ("missing", lambda v: v["phase"].pop("rust")),
            ("unknown keys", lambda v: v["phase"].update(owner="x")),
            ("lists a value twice", lambda v: v["severity"]["values"].append("error")),
            ("invalid value", lambda v: v["severity"]["values"].append("Error")),
        ]
        for message, mutate in value_mutations:
            with self.subTest(message=message):
                candidate = copy.deepcopy(values)
                mutate(candidate)
                with self.assertRaisesRegex(ValueError, message):
                    surface.validate_values(candidate, check_rust=False)
        column = {"name": "phase", "type": "str"}
        table_mutations = [
            ("names a field twice", lambda t: t["issues"]["columns"].append(column)),
            (
                "nests optional",
                lambda t: t["issues"]["columns"][2].update(
                    type="optional<optional<str>>"
                ),
            ),
            (
                "unknown column type",
                lambda t: t["report"]["columns"][0].update(type="map<str>"),
            ),
            (
                "undeclared vocabulary",
                lambda t: t["report"]["columns"][1].update(values="statuses"),
            ),
            ("unknown keys", lambda t: t["report"]["columns"][1].update(since="1.0.0")),
            ("missing", lambda t: t["odm_items"].pop("columns")),
        ]
        for message, mutate in table_mutations:
            with self.subTest(message=message):
                candidate = copy.deepcopy(tables)
                mutate(candidate)
                with self.assertRaisesRegex(ValueError, message):
                    surface.validate_tables(candidate, values, check_rust=False)

    def test_the_info_record_is_the_truth_both_hosts_read(self):
        # The installed suites check info() against the shared truth, so its
        # fields and the vocabularies its fields name follow the record.
        truth = json.loads(
            (generator.ROOT / "rust/tests/host-fixtures/cases.json").read_text(
                encoding="utf-8"
            )
        )["families"]["info"]
        record = self.definition["records"]["info"]
        self.assertEqual([field["name"] for field in record["fields"]], truth["fields"])
        fields = {field["name"]: field for field in record["fields"]}
        values = self.definition["values"]
        self.assertEqual(
            values[fields["features"]["values"]]["values"], truth["features"]
        )
        self.assertEqual(values[fields["host"]["values"]]["values"], truth["hosts"])

    def test_the_condition_column_may_name_the_registry(self):
        columns = self.definition["tables"]["issues"]["columns"]
        self.assertEqual(columns[2]["values"], surface.REGISTRY_VALUES)
        self.assertTrue((generator.ROOT / surface.REGISTRY_VALUES).is_file())

    def test_records_follow_the_field_grammar(self):
        records, values = self.definition["records"], self.definition["values"]
        surface.validate_records(records, values)
        surface.validate_records({}, {})
        mutations = [
            ("unknown keys", lambda r: r["info"].update(columns=[])),
            (
                "undeclared vocabulary",
                lambda r: r["info"]["fields"][2].update(values="features"),
            ),
            (
                "unknown column type",
                lambda r: r["info"]["fields"][0].update(type="text"),
            ),
            ("invalid record name", lambda r: r.update({"Info": r["info"]})),
        ]
        for message, mutate in mutations:
            with self.subTest(message=message):
                candidate = copy.deepcopy(records)
                mutate(candidate)
                with self.assertRaisesRegex(ValueError, message):
                    surface.validate_records(candidate, values)

    def test_an_empty_command_line_section_declares_no_command(self):
        surface.validate_cli({}, {}, {})
        lines = surface.manifest_lines({**self.definition, "cli": {}})
        self.assertEqual([line for line in lines if line.startswith("cli.")], [])

    def test_the_command_line_grammar_refuses_malformed_commands(self):
        definition = self.definition
        mutations = [
            ("unknown keys", lambda c: c.update(environment="none")),
            ("is missing", lambda c: c.pop("commands")),
            ("cli.binary", lambda c: c.update(binary="yamaa-cli")),
            ("invalid flag name", lambda c: c["global_flags"][0].update(name="-f")),
            ("key must be an integer", lambda c: c["exit_status"].update({"3": "x"})),
            ("outside 0 to 125", lambda c: c["exit_status"].update({130: "signal"})),
            (
                "unknown operation",
                lambda c: c["commands"][0]["operations"].append("lint_all"),
            ),
            (
                "must start with",
                lambda c: c["commands"][1].update(synopsis="yamaa go PATH"),
            ),
            ("names a command twice", lambda c: c["commands"].append(c["commands"][0])),
            (
                "undeclared vocabulary",
                lambda c: c["commands"][1]["flags"][1].update(values="sections"),
            ),
            (
                "invalid JSON key",
                lambda c: c["commands"][0].update(json_keys=["Status"]),
            ),
            (
                "invalid argument name",
                lambda c: c["commands"][0]["args"][0].update(name="path"),
            ),
            # The command line's own vocabularies, which --format names.
            ("names an undeclared vocabulary", lambda c: c.pop("values")),
            (
                "shadows values.file_type",
                lambda c: c["values"].update(file_type=["spec"]),
            ),
            (
                "has an invalid value",
                lambda c: c["values"]["output_format"].append("JSON"),
            ),
            (
                "lists a value twice",
                lambda c: c["values"]["output_format"].append("text"),
            ),
            # A refusal prints the command's own document, whose keys are named.
            (
                "resolve.refusal_keys must be a nonempty list",
                lambda c: c["commands"][3].update(refusal_keys="the issues"),
            ),
        ]
        for message, mutate in mutations:
            with self.subTest(message=message):
                candidate = copy.deepcopy(definition["cli"])
                mutate(candidate)
                with self.assertRaisesRegex(ValueError, message):
                    surface.validate_cli(
                        candidate, definition["values"], definition["operations"]
                    )


class ManifestTests(unittest.TestCase):
    def test_values_tables_and_records_give_one_line_per_element(self):
        lines = surface.manifest_lines(generator._load_definition())
        self.assertIn("values.phase.validation", lines)
        self.assertIn("values.report_outcome.violated", lines)
        self.assertIn("tables.issues.columns[0]: severity: str, values severity", lines)
        self.assertIn("tables.issues.columns[1]: phase: str, values phase", lines)
        self.assertIn("tables.issues.columns[8]: message: str", lines)
        self.assertIn("tables.report.columns[10]: COUNT: optional<int>", lines)
        self.assertIn("tables.odm_items.columns[29]: SourceOrdinal: int", lines)
        self.assertEqual(len(lines), len(set(lines)))
        self.assertFalse([line for line in lines if "meaning" in line or "::" in line])

    def test_the_command_line_and_records_render_as_lines(self):
        definition = generator._load_definition()
        lines = surface.manifest_lines(definition)
        for expected in (
            "records.info.fields[0]: version: str",
            "records.info.fields[2]: features: list<str>, values feature",
            "cli.synopsis: yamaa --help",
            "cli.values.output_format.json",
            "cli.global_flags.--format: values output_format; default text",
            "cli.global_flags.--help: flag",
            "cli.exit_status.2: " + definition["cli"]["exit_status"][2],
            "cli.commands.lint: yamaa lint [--fix] PATH...",
            "cli.commands.lint.args[0]: PATH: path, repeated",
            "cli.commands.schema.args[0]: TYPE: file_type",
            "cli.commands.read-odm.flags.--max-xml-depth: N; default 64",
            "cli.commands.check.json_keys.status",
            "cli.commands.schema.refusal_keys.type",
            "cli.json.issue_keys.operand_route",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, lines)
        # The resolved specification has no fixed keys, so only its refusal's
        # keys give lines.
        self.assertEqual(
            [line for line in lines if re.match(r"cli\.commands\.resolve[.:]", line)],
            [
                "cli.commands.resolve: yamaa resolve PATH [--spec NAME]",
                "cli.commands.resolve.args[0]: PATH: path",
                "cli.commands.resolve.flags.--spec: NAME",
                "cli.commands.resolve.refusal_keys.command",
                "cli.commands.resolve.refusal_keys.path",
                "cli.commands.resolve.refusal_keys.spec",
                "cli.commands.resolve.refusal_keys.status",
                "cli.commands.resolve.refusal_keys.issues",
            ],
        )
        self.assertEqual(len(lines), len(set(lines)))


if __name__ == "__main__":
    unittest.main()
