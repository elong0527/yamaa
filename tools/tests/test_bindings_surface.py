"""The frozen sections of the host interface: vocabularies, tables, records, cli."""

import copy
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.packaging import bindings_surface as surface
from tools.packaging import generate_bindings as generator

DRAFT = generator.DEFINITION.with_name("bindings-1.0.yaml")


def draft():
    """The 1.0 draft, which later pull requests move into bindings.yaml.

    Only its records, cli, values and operations are read here, so a plain
    load suffices: its public section still has a YAML 1.1 boolean key, which
    the closed loader refuses.
    """
    with DRAFT.open(encoding="utf-8") as stream:
        return yaml.safe_load(stream)


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
        self.assertEqual(len(findings), 12)

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
                '    "scalar_as_text",\n];',
                '    "scalar_as_text",\n    "trailing_space",\n];',
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

    def test_the_condition_column_may_name_the_registry(self):
        columns = self.definition["tables"]["issues"]["columns"]
        self.assertEqual(columns[2]["values"], surface.REGISTRY_VALUES)
        self.assertTrue((generator.ROOT / surface.REGISTRY_VALUES).is_file())

    def test_records_follow_the_field_grammar_of_the_draft(self):
        planned = draft()
        surface.validate_records(planned["records"], planned["values"])
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
                candidate = copy.deepcopy(planned["records"])
                mutate(candidate)
                with self.assertRaisesRegex(ValueError, message):
                    surface.validate_records(candidate, planned["values"])

    def test_the_command_line_grammar_accepts_the_draft(self):
        planned = draft()
        surface.validate_cli(planned["cli"], planned["values"], planned["operations"])
        self.assertEqual(self.definition["cli"], {})
        surface.validate_cli({}, {}, {})
        self.assertEqual(
            surface.cli_names(planned["cli"]),
            {
                "check",
                "run",
                "submission",
                "resolve",
                "read-odm",
                "lint",
                "schema",
                "selftest",
                "--format",
                "--version",
                "--help",
            },
        )

    def test_the_command_line_grammar_refuses_malformed_commands(self):
        planned = draft()
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
        ]
        for message, mutate in mutations:
            with self.subTest(message=message):
                candidate = copy.deepcopy(planned["cli"])
                mutate(candidate)
                with self.assertRaisesRegex(ValueError, message):
                    surface.validate_cli(
                        candidate, planned["values"], planned["operations"]
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

    def test_the_command_line_and_records_of_the_draft_render_as_lines(self):
        planned = draft()
        definition = generator._load_definition()
        definition["records"] = planned["records"]
        definition["cli"] = planned["cli"]
        lines = surface.manifest_lines(definition)
        for expected in (
            "records.info.fields[0]: version: str",
            "records.info.fields[2]: features: list<str>, values feature",
            "cli.global_flags.--format: values output_format; default text",
            "cli.exit_status.2: " + planned["cli"]["exit_status"][2],
            "cli.commands.lint: yamaa lint [--fix] PATH...",
            "cli.commands.lint.args[0]: PATH: path, repeated",
            "cli.commands.read-odm.flags.--max-xml-depth: N; default 64",
            "cli.commands.check.json_keys.status",
            "cli.json.issue_keys.operand_route",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, lines)
        self.assertEqual(len(lines), len(set(lines)))


if __name__ == "__main__":
    unittest.main()
