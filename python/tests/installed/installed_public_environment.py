"""Public installed projects and their environments against independent truth.

A project file links one environment file per host; each host reads only its
own entry. Project functions, codelists and locks reach a specification only
through `yamaa.project(path).spec(name)` or `project.run()`.
"""

from __future__ import annotations

import contextlib
import copy
import gc
import json
import os
import shutil
import tempfile
import unittest
import weakref
from pathlib import Path
from unittest.mock import patch

import yaml
from host_fixtures import load as load_truth

import yamaa
from yamaa import _locked_functions as host

TRUTH = load_truth(__file__, "public-environment")

ROOT = Path(__file__).parent / "project-original"
CASES = tuple(TRUTH["original_case_names"])
FUNCTION_SPEC = (
    "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
    "input: {SRC: {path: input.csv, types: {VALUE: float}}}\n"
    "columns:\n  - {name: ID, type: str, derivation: SRC.ID}\n"
    "  - {name: VALUE, type: float, derivation: "
    "{function: {name: project_value, args: {x: SRC.VALUE}}}}\n"
    "output: {path: output.csv, columns: [ID, VALUE]}\n"
)


@contextlib.contextmanager
def no_activation():
    """Fail on any lock audit or binding: the call must run no project code."""
    with (
        patch.object(
            host, "verify_versions", side_effect=AssertionError("unexpected activation")
        ),
        patch.object(
            host, "resolve_callable", side_effect=AssertionError("unexpected binding")
        ),
    ):
        yield


def compact(value, *, sort_keys=True):
    return json.dumps(
        value, ensure_ascii=False, sort_keys=sort_keys, separators=(",", ":")
    )


def issue(condition, rule, spec_paths, context, phase="validation"):
    return {
        "phase": phase,
        "condition": condition,
        "rule": rule,
        "spec_paths": spec_paths,
        "context": compact(context),
        "source_span": None,
        "operand_route": None,
    }


class PublicEnvironment(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="yamaa-public-environment-")
        self.addCleanup(self.temporary.cleanup)
        # One resolved directory identity for every call: hosts spell the
        # same temp directory differently (the macOS /var symlink, Windows
        # 8.3 short names), and file paths in issues follow the caller's
        # spelling, so chdir and expectations share the resolved form.
        self.directory = Path(self.temporary.name).resolve()
        self.previous = Path.cwd()
        self.addCleanup(os.chdir, self.previous)

    def study(self, name):
        case = self.directory / name
        shutil.copytree(ROOT / name, case)
        os.chdir(case)
        return case

    def value_study(self):
        case = self.study("negative-function-contract")
        source = case / "spec.yaml"
        source.write_text(
            source.read_text().replace("args: {}", "args: {x: SOURCE.VALUE}")
        )
        return case

    def project(self, case=None):
        return yamaa.project((case or Path.cwd()) / "project.yaml")

    def report(self, result):
        return json.loads(result._native.observations())

    def assert_refused_project(self, project, expected):
        """Project problems: issues of check() and of a run that built nothing."""
        self.assertEqual(project.check().issues.to_dicts(), expected)
        runs = project.run()
        self.assertEqual(runs.names, [])
        self.assertEqual(runs.issues.to_dicts(), expected)
        with self.assertRaises(yamaa.Error) as caught:
            runs.save()
        self.assertEqual(caught.exception.issues.to_dicts(), expected)
        with self.assertRaises(yamaa.Error) as caught:
            project.spec("TEST")
        self.assertEqual(caught.exception.issues.to_dicts(), expected)

    def test_missing_metadata_names_each_declared_input_before_entry_and_activation(
        self,
    ):
        os.chdir(self.directory)
        Path("environment.yaml").write_text(
            "schema_version: '1.0'\nlock: missing.lock\n"
            "functions: {id: missing-function.yaml}\n"
        )
        Path("project.yaml").write_text(
            "schema_version: '1.0'\n"
            "environments: {python: environment.yaml, r: environment.yaml}\n"
            "codelists: [missing-codelists.yaml]\nadam: {specs: [absent-spec.yaml]}\n"
        )
        entry = (Path.cwd() / "project.yaml").as_posix()
        expected = [
            issue(
                "project_environment_invalid",
                "FUNC-2",
                [declaration],
                {
                    "cause": "missing",
                    "entry": entry,
                    "environment_path": declaration,
                    "path": path,
                },
            )
            for declaration, path in (
                ("lock", "missing.lock"),
                ("functions.id", "missing-function.yaml"),
                ("codelists[0]", "missing-codelists.yaml"),
            )
        ]
        with no_activation():
            self.assert_refused_project(self.project(), expected)
        self.assertEqual(
            sorted(p.name for p in self.directory.iterdir()),
            ["environment.yaml", "project.yaml"],
        )

    def test_missing_environment_file_precedes_bad_or_absent_entry_without_activation(
        self,
    ):
        os.chdir(self.directory)
        Path("project.yaml").write_text(
            "schema_version: '1.0'\n"
            "environments: {python: missing-environment.yaml, r: missing-environment.yaml}\n"
            "adam: {specs: [spec.yaml]}\n"
        )
        expected = [
            issue(
                "project_environment_invalid",
                "FUNC-2",
                ["environments.python"],
                {
                    "cause": "missing",
                    "entry": (Path.cwd() / "project.yaml").as_posix(),
                    "environment_path": "environments.python",
                    "path": "missing-environment.yaml",
                },
            )
        ]
        with no_activation():
            for content in (None, "[entry must not be read"):
                if content is not None:
                    Path("spec.yaml").write_text(content)
                self.assert_refused_project(self.project(), expected)
        self.assertEqual(
            sorted(p.name for p in self.directory.iterdir()),
            ["project.yaml", "spec.yaml"],
        )

    def test_each_host_reads_only_its_own_environment_entry(self):
        os.chdir(self.directory)
        Path("spec.yaml").write_text(FUNCTION_SPEC)
        # The R entry names a file that does not exist; Python never reads it,
        # so a function call reports the missing Python environment.
        Path("project.yaml").write_text(
            "schema_version: '1.0'\nenvironments: {r: r/absent.yaml}\n"
            "adam: {specs: [spec.yaml]}\n"
        )
        context = {
            "cause": "undeclared",
            "function": "project_value",
            "language": "python",
        }
        path = (Path.cwd() / "spec.yaml").as_posix()
        project = self.project()
        with no_activation():
            spec = project.spec("TEST")
            for result in (spec.check(), spec.run()):
                self.assertEqual(
                    result.issues.to_dicts(),
                    [
                        issue(
                            "project_environment_missing",
                            "FUNC-1",
                            ["columns.VALUE.derivation.function.name"],
                            context,
                        )
                    ],
                )
            # A project result names the specification each issue belongs to.
            self.assertEqual(
                project.check().issues.to_dicts(),
                [
                    issue(
                        "project_environment_missing",
                        "FUNC-1",
                        ["columns.VALUE.derivation.function.name"],
                        {**context, "entry": path, "source": path},
                    )
                ],
            )
            # The same file alone has no project, so the function is unknown.
            self.assertEqual(
                yamaa.spec("spec.yaml").check().issues.to_dicts(),
                [
                    issue(
                        "unknown_project_function",
                        "FUNC-4",
                        ["columns.VALUE.derivation.function.name"],
                        {"function": "project_value"},
                    )
                ],
            )
        self.assertEqual(
            sorted(p.name for p in self.directory.iterdir()),
            ["project.yaml", "spec.yaml"],
        )

    def test_submission_version_schema_admission_precedes_any_artifact_or_study_read(
        self,
    ):
        os.chdir(self.directory)
        Path("spec.yaml").write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "input: {SRC: absent.csv}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n"
            "output: {path: result.csv, columns: [ID]}\n"
        )
        project = self.project()
        for family, guide in (
            ("sdtm", "SDTMIG"),
            ("adam", "ADaMIG"),
            ("send", "SENDIG"),
        ):
            prefix = (
                "schema_version: '1.0'\n"
                "study: {id: STUDY, name: Study, description: Study, protocol_name: Protocol}\n"
                f"{family}:\n  standard: {{name: {guide}, version: '1.0', status: Final}}\n"
                "  specs: [spec.yaml]\n  define: absent/define.xml\n"
            )
            with no_activation():
                for declared in (None, "'1.0'", "'future package build'"):
                    field = (
                        ""
                        if declared is None
                        else f"  source_system_version: {declared}\n"
                    )
                    Path("project.yaml").write_text(prefix + field)
                    checked = project.check()
                    self.assertTrue(checked.issues.is_empty(), checked.issues)
                for declared in tuple(TRUTH["invalid_source_system_versions"]):
                    Path("project.yaml").write_text(
                        prefix + f"  source_system_version: {declared}\n"
                    )
                    checked = project.check()
                    self.assertEqual(checked.issues.height, 1, checked.issues)
                    self.assertEqual(
                        checked.issues["spec_paths"][0].to_list(),
                        [f"{family}.source_system_version"],
                    )
            self.assertEqual(
                sorted(p.name for p in self.directory.iterdir()),
                ["project.yaml", "spec.yaml"],
            )

    def test_functionless_link_reads_its_missing_artifact_without_running_the_producer(
        self,
    ):
        os.chdir(self.directory)
        (self.directory / "producer.yaml").write_text(
            "schema_version: '1.0'\ndomain: PROD\nkeys: [ID]\n"
            "input: {RAW: never-read.csv}\n"
            "columns: [{name: ID, type: int, label: Identifier, derivation: RAW.ID}]\n"
            "output: {path: produced.csv, columns: [ID], decimals: 2}\n"
        )
        (self.directory / "spec.yaml").write_text(
            "schema_version: '1.0'\ndomain: CONS\nkeys: [ID]\n"
            "input: {P: {path: produced.csv, schema: producer.yaml}}\n"
            "columns: [{name: ID, type: int, label: Identifier, derivation: P.ID}]\n"
            "output: {path: consumer.csv, columns: [ID]}\n"
        )
        spec = yamaa.spec("spec.yaml")
        with no_activation():
            checked = spec.check()
            failed = spec.run()
        self.assertTrue(checked.issues.is_empty(), checked.issues)
        # The producer never runs: the consumer reads its own linked path, and
        # that stored artifact does not exist yet.
        self.assertEqual(
            failed.issues["condition"].to_list(), ["resource_path_missing"]
        )
        self.assertEqual(failed.issues["spec_paths"].to_list(), [["input.P.path"]])
        report = self.report(failed)
        self.assertEqual(report["outcome"], "failure")
        self.assertEqual(report["source_reads"], [])
        self.assertIsNone(failed.output)
        with self.assertRaises(yamaa.Error):
            failed.save()
        self.assertFalse((self.directory / "produced.csv").exists())
        self.assertFalse((self.directory / "consumer.csv").exists())

    def test_linked_checks_report_only_the_entry_without_activation_or_data(self):
        os.chdir(self.directory)

        def document(input, base, output, invalid=False):
            checks = (
                '\n    verifications: [{matches: {pattern: "\\u00e9("}}]'
                if invalid
                else ""
            )
            return (
                f"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {input}\n"
                f"columns:\n  - {{name: ID, type: int, label: Identifier, derivation: {base}.ID}}\n"
                f"  - name: VALUE\n    type: str\n    label: Value\n    derivation: {base}.VALUE{checks}\n"
                f"output: {{path: {output}, columns: [ID, VALUE]}}\n"
            )

        Path("parents").mkdir()
        Path("parents/base.yaml").write_text(
            document("{RAW: ../never-read.csv}", "RAW", "../leaf.csv", True)
        )
        Path("leaf.yaml").write_text(
            "schema_version: '1.0'\nparent: parents/base.yaml\ndomain: LEAF\n"
        )
        Path("left.yaml").write_text(
            document(TRUTH["aliased_leaf_reference"], "P", "left.csv")
        )
        Path("right.yaml").write_text(
            document(TRUTH["leaf_reference"], "P", "right.csv")
        )
        Path("spec.yaml").write_text(
            document(TRUTH["diamond_reference"], "L", "consumer.csv", True)
        )
        spec = yamaa.spec("spec.yaml")
        with no_activation():
            for _ in range(2):
                # Linked producers supply output contracts only: their own
                # findings are never compiled, and only the entry is checked.
                rows = spec.check().issues.to_dicts()
                self.assertEqual(
                    rows,
                    [
                        issue(
                            "invalid_regex",
                            "TEXT-6",
                            [TRUTH["verification_pattern_path"]],
                            {"pattern": "\u00e9("},
                        )
                    ],
                )
            # A link types whatever file the consumer names; a path that differs
            # from the producer's own output is admitted. Checking never opens
            # the CSV paths.
            Path("spec.yaml").write_text(
                document(TRUTH["wrong_leaf_reference"], "P", "consumer.csv")
            )
            rows = spec.check().issues.to_dicts()
            self.assertEqual(rows, [])
            Path("spec.yaml").write_text(
                document(TRUTH["incompatible_leaf_reference"], "P", "consumer.csv")
            )
            rows = spec.check().issues.to_dicts()
            self.assertEqual(
                [(r["condition"], r["rule"], r["spec_paths"]) for r in rows],
                [("redundant_field_type", "INPUT-6", ["input.P.types.ID"])],
            )
        for name in tuple(TRUTH["unread_graph_files"]):
            self.assertFalse(Path(name).exists())

    def test_declaring_file_paths_ignore_configuration_and_save_held_bytes(self):
        os.chdir(self.directory)
        for name in tuple(TRUTH["inherited_directories"]):
            Path(name).mkdir()
        Path("yamaa-project.yaml").write_text("[invalid configuration")
        Path("shared/environment.yaml").write_text("schema_version: '1.0'\n")
        Path("shared/project.yaml").write_text(
            "schema_version: '1.0'\n"
            "environments: {python: environment.yaml, r: environment.yaml}\n"
            "adam: {specs: [../entry/spec.yaml]}\n"
        )
        Path("entry/spec.yaml").write_text(
            "schema_version: '1.0'\nparent: ../shared/base.yaml\ndomain: CHILD\n"
        )
        Path("shared/base.yaml").write_text(
            "schema_version: '1.0'\ndomain: BASE\nkeys: [ID]\ninput: {SRC: data.csv}\n"
            "columns: [{name: ID, type: int, derivation: SRC.ID}]\n"
            "output: {path: ../outputs/held.csv, columns: [ID]}\n"
        )
        Path("entry/data.csv").write_text("ID\n999\n")
        Path("shared/data.csv").write_text("ID\n7\n")
        project = yamaa.project("shared/project.yaml")
        with no_activation():
            checked = project.check()
            result = project.spec("CHILD").run()
        self.assertTrue(checked.issues.is_empty(), checked.issues)
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["ID"].to_list(), [7])
        self.assertFalse(Path("outputs/held.csv").exists())
        before = self.report(result)
        self.assertNotIn("yamaa-project.yaml", json.dumps(before))
        # Save uses the already serialized bytes and retained output location.
        # No specification, project, environment or input reread can supply
        # this truth.
        for name in tuple(TRUTH["inherited_source_files"]):
            Path(name).unlink()
        Path("shared/data.csv").write_text("ID\n8\n")
        self.assertEqual(result.save(), [Path("outputs/held.csv").resolve().as_posix()])
        self.assertEqual(Path("outputs/held.csv").read_bytes(), b"ID\n7\n")
        self.assertEqual(self.report(result)["source_reads"], before["source_reads"])

    def test_absolute_local_paths_and_missing_declaring_file_input_have_no_fallback(
        self,
    ):
        os.chdir(self.directory)
        for name in tuple(TRUTH["inherited_directories"]):
            Path(name).mkdir()
        Path("shared/data.csv").write_text("ID\n7\n")
        source = (self.directory / "shared/../shared/./data.csv").as_posix()
        output = (self.directory / "outputs/absolute.csv").as_posix()
        document = (
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "columns: [{name: ID, type: int, derivation: SRC.ID}]\n"
            "input: {SRC: %s}\noutput: {path: %s, columns: [ID]}\n"
        )
        Path("entry/spec.yaml").write_text(
            document % (json.dumps(source), json.dumps(output))
        )
        spec = yamaa.spec("entry/spec.yaml")
        result = spec.run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["ID"].to_list(), [7])
        self.assertFalse(Path(output).exists())
        self.assertEqual(result.save()[-1], Path(output).resolve().as_posix())
        self.assertEqual(Path(output).read_bytes(), b"ID\n7\n")
        Path(output).unlink()
        # A configured data root and a parent file must never rescue the absent
        # path beside the declaring specification.
        Path("yamaa-project.yaml").write_text("version: '1.0'\ndata_roots: [shared]\n")
        Path("data.csv").write_text("ID\n999\n")
        Path("shared/directory.csv").mkdir()
        for input_path, condition in (
            ("data.csv", "resource_path_missing"),
            ("../shared/directory.csv", "resource_path_not_regular_file"),
        ):
            Path("entry/spec.yaml").write_text(
                document % (json.dumps(input_path), json.dumps(output))
            )
            failed = spec.run()
            self.assertIsNone(failed.output)
            self.assertEqual(failed.issues["condition"].to_list(), [condition])
            self.assertEqual(
                failed.issues["spec_paths"].to_list(), [["input.SRC.path"]]
            )
            with self.assertRaises(yamaa.Error):
                failed.save()
            self.assertFalse(Path(output).exists())

    def test_project_codelists_keep_project_locations_and_missing_files_fail_there(
        self,
    ):
        os.chdir(self.directory)
        for name in tuple(TRUTH["terminology_directories"]):
            Path(name).mkdir()
        Path("yamaa-project.yaml").write_text("[ignored configuration")
        Path("entry/spec.yaml").write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "input: {SRC: {path: data.csv, types: {ID: str}}}\n"
            "columns: [{name: ID, type: str, derivation: SRC.ID, "
            "submission: {codelist: IDENTIFIERS, origin: {type: Assigned}}}]\n"
            "output: {path: ../outputs/ids.csv, columns: [ID]}\n"
        )
        Path("entry/data.csv").write_text("ID\n01\n")
        terminology = "codelists: [{id: IDENTIFIERS, name: Identifiers, items: [{value: '01'}]}]\n"
        Path("terminology/ids.yaml").write_text(terminology)
        # An unrelated same-name document cannot change which codelist is used.
        Path("entry/ids.yaml").write_text("[unrelated document")
        project = yamaa.project("project/project.yaml")
        for written in (
            "../terminology/ids.yaml",
            (self.directory / "terminology/ids.yaml").as_posix(),
        ):
            with self.subTest(written=written):
                Path("project/project.yaml").write_text(
                    "schema_version: '1.0'\ncodelists: ["
                    + json.dumps(written)
                    + "]\nsdtm: {specs: [../entry/spec.yaml]}\n"
                )
                self.assertTrue(project.check().issues.is_empty())
                spec = project.spec("TEST")
                checked = spec.check()
                result = spec.run()
                self.assertTrue(checked.issues.is_empty(), checked.issues)
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["ID"].to_list(), ["01"])
                self.assertTrue(self.report(result)["verifications"])
                self.assertFalse(Path("outputs/ids.csv").exists())
                self.assertEqual(
                    result.save()[-1], Path("outputs/ids.csv").resolve().as_posix()
                )
                self.assertEqual(Path("outputs/ids.csv").read_bytes(), b"ID\n01\n")
                Path("outputs/ids.csv").unlink()
        Path("project/project.yaml").write_text(
            "schema_version: '1.0'\ncodelists: [ids.yaml]\n"
            "sdtm: {specs: [../entry/spec.yaml]}\n"
        )
        # Even a valid codelist at the process/entry location must not rescue it.
        Path("ids.yaml").write_text(terminology)
        Path("entry/ids.yaml").write_text(terminology)
        with no_activation():
            checked = project.check().issues.to_dicts()
            runs = project.run()
        self.assertEqual(
            [(row["condition"], row["spec_paths"]) for row in checked],
            [("project_environment_invalid", ["codelists[0]"])],
        )
        self.assertEqual(json.loads(checked[0]["context"])["path"], "ids.yaml")
        self.assertEqual(runs.names, [])
        self.assertEqual(runs.issues.to_dicts(), checked)
        self.assertFalse(Path("outputs/ids.csv").exists())

    def test_catastrophic_pattern_fails_the_same_check_with_and_without_a_project(
        self,
    ):
        # Matching is linear: a nested repetition that cannot match is an
        # ordinary verification failure, not an exhausted step budget.
        os.chdir(self.directory)
        truth = TRUTH["catastrophic_pattern"]
        Path("spec.yaml").write_text(truth["specification"])
        Path("input.csv").write_text(truth["source"])
        Path("environment.yaml").write_text("schema_version: '1.0'\n")
        Path("project.yaml").write_text(
            "schema_version: '1.0'\n"
            "environments: {python: environment.yaml, r: environment.yaml}\n"
            "adam: {specs: [spec.yaml]}\n"
        )
        for spec in (yamaa.spec("spec.yaml"), self.project().spec("TEST")):
            failed = spec.run()
            self.assertIsNone(failed.output)
            rows = failed.issues.to_dicts()
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]["condition"], truth["condition"])
            self.assertEqual(rows[0]["spec_paths"], truth["paths"])
            self.assertEqual(json.loads(rows[0]["context"]), truth["context"])
            with self.assertRaises(yamaa.Error):
                failed.save()
            self.assertFalse(Path("output.csv").exists())

    def precision_study(self, decimals="2", output="rounded.csv"):
        os.chdir(self.directory)
        (self.directory / "input.csv").write_text(
            "ID,VALUE,INT\n1,0.125,7\n2,-0.125,-7\n3,2.675,0\n"
            "4,-0.004,9\n5,1.234,1\n6,2.345,2\n7,,3\n8,25,4\n"
        )
        precision = "" if decimals is None else ", decimals: " + decimals
        (self.directory / "spec.yaml").write_text(
            "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
            "input: {SRC: {path: input.csv, types: {ID: int, VALUE: float, INT: int}}}\n"
            f"output: {{path: {output}, columns: [ID, VALUE, DOUBLE, EMPTY, INT]{precision}}}\n"
            "columns:\n"
            "  - {name: ID, type: int, derivation: SRC.ID}\n"
            "  - {name: VALUE, type: float, derivation: SRC.VALUE}\n"
            "  - {name: DOUBLE, type: float, derivation: {compute: {expr: 'VALUE + VALUE'}}}\n"
            "  - {name: EMPTY, type: str, derivation: {literal: ''}}\n"
            "  - {name: INT, type: int, derivation: SRC.INT}\n"
            "verifications: [{assert: {when: 'ID = 5', require: 'DOUBLE = 2.468'}}]\n"
        )
        return yamaa.spec("spec.yaml")

    def test_csv_precision_changes_saved_bytes_only_and_preserves_derived_values(self):
        expected = TRUTH["rounded_callback_csv"].encode("utf-8")
        spec = self.precision_study()
        with no_activation():
            self.assertTrue(spec.check().issues.is_empty())
            result = spec.run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(
            result.output["VALUE"].to_list(),
            [0.125, -0.125, 2.675, -0.004, 1.234, 2.345, None, 25.0],
        )
        self.assertEqual(
            result.output["DOUBLE"].to_list(),
            [0.25, -0.25, 5.35, -0.008, 2.468, 4.69, None, 50.0],
        )
        before = self.report(result)
        self.assertEqual(before["artifacts"], [])
        self.assertFalse((self.directory / "rounded.csv").exists())
        self.assertEqual(
            result.save()[-1], (self.directory / "rounded.csv").resolve().as_posix()
        )
        self.assertEqual((self.directory / "rounded.csv").read_bytes(), expected)
        self.assertEqual(
            self.report(result)["artifacts"][0]["content"].encode(), expected
        )
        (self.directory / "rounded.csv").unlink()
        ordinary = self.precision_study(None).run()
        self.assertTrue(ordinary.issues.is_empty(), ordinary.issues)
        self.assertEqual(ordinary.output.to_dicts(), result.output.to_dicts())
        # Metadata and every calculation/check observation retain the same truth.
        self.assertEqual(self.report(ordinary), before)

    def test_csv_precision_maximum_and_invalid_bounds_preserve_save_gates(self):
        spec = self.precision_study(TRUTH["csv_precision_maximum"])
        (self.directory / "input.csv").write_text("ID,VALUE,INT\n1,0.125,7\n")
        result = spec.run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        expected = TRUTH["maximum_precision_csv"].encode("utf-8")
        self.assertEqual(
            result.save()[-1], (self.directory / "rounded.csv").resolve().as_posix()
        )
        self.assertEqual((self.directory / "rounded.csv").read_bytes(), expected)
        (self.directory / "rounded.csv").unlink()
        for precision in TRUTH["invalid_csv_precision"]:
            spec = self.precision_study(precision)
            checked = spec.check()
            failed = spec.run()
            self.assertIsNone(failed.output)
            self.assertEqual(checked.issues.to_dicts(), failed.issues.to_dicts())
            self.assertEqual(failed.issues["phase"].to_list(), ["validation"])
            self.assertEqual(
                failed.issues["condition"].to_list(), ["invalid_field_type"]
            )
            self.assertEqual(failed.issues["rule"].to_list(), ["CSV-6"])
            self.assertEqual(
                failed.issues["spec_paths"].to_list(), [["output.decimals"]]
            )
            self.assertEqual(
                json.loads(failed.issues["context"][0]),
                {"expected": TRUTH["csv_precision_expected"], "actual": int(precision)},
            )
            with self.assertRaises(yamaa.Error):
                failed.save()
            self.assertFalse((self.directory / "rounded.csv").exists())

    def test_csv_precision_validation_and_prior_failure_preserve_public_save_gates(
        self,
    ):
        for precision, output, condition, rule, context in (
            (
                "-9223372036854775809",
                "rounded.csv",
                "invalid_field_type",
                "CSV-6",
                {
                    "expected": TRUTH["csv_precision_expected"],
                    "actual": -9223372036854775809,
                },
            ),
            (
                "2",
                "rounded.parquet",
                "decimals_not_applicable",
                "CSV-6",
                {"path": "rounded.parquet", "profile": "parquet"},
            ),
        ):
            result = self.precision_study(precision, output).run()
            self.assertIsNone(result.output)
            self.assertEqual(result.issues["condition"].to_list(), [condition])
            self.assertEqual(result.issues["rule"].to_list(), [rule])
            self.assertEqual(json.loads(result.issues["context"][0]), context)
            self.assertEqual(self.report(result)["diagnostics"][0]["context"], context)
            with self.assertRaises(yamaa.Error):
                result.save()
            self.assertFalse((self.directory / output).exists())
        for invalid in tuple(TRUTH["invalid_significant_digits"]):
            result = self.precision_study(invalid).check()
            self.assertEqual(
                result.issues["spec_paths"].to_list(), [["output.decimals"]]
            )
            self.assertFalse(result.issues.is_empty())
        self.precision_study("-1")
        path = self.directory / "spec.yaml"
        path.write_text(path.read_text().replace("VALUE + VALUE", "1 / 0"))
        failed = yamaa.spec(path).run()
        self.assertIsNone(failed.output)
        self.assertEqual(failed.issues["condition"].to_list(), ["division_by_zero"])
        with self.assertRaises(yamaa.Error):
            failed.save()
        self.assertFalse((self.directory / "rounded.csv").exists())

    def test_original_positive_artifacts_and_fresh_activation_on_every_run(self):
        for name in CASES:
            with self.subTest(name=name):
                case = self.study(name)
                expected = next((case / "expected").glob("*.csv"))
                project = self.project(case)
                self.assertTrue(project.check().issues.is_empty())
                runs = project.run()
                self.assertTrue(runs.issues.is_empty(), runs.issues)
                (domain,) = runs.names
                self.assertIsNotNone(runs[domain].output)
                self.assertFalse((case / expected.name).exists())
                for _ in range(2):
                    result = project.spec(domain).run()
                    self.assertTrue(result.issues.is_empty(), result.issues)
                    self.assertTrue(result.output.equals(runs[domain].output))
                    before = self.report(result)
                    self.assertEqual(before["outcome"], "success")
                    self.assertEqual(
                        before["activation"]["lock"],
                        "not_requested" if name == "adam-adsl-bmi" else "verified",
                    )
                    self.assertTrue(before["activation"]["tests"])
                    self.assertTrue(
                        all(
                            case["outcome"] == "passed"
                            for case in before["activation"]["tests"]
                        )
                    )
                    self.assertEqual(before["artifacts"], [])
                    self.assertFalse((case / expected.name).exists())
                    self.assertEqual(
                        result.save()[-1], (case / expected.name).resolve().as_posix()
                    )
                    self.assertEqual(
                        (case / expected.name).read_bytes(), expected.read_bytes()
                    )
                    after = self.report(result)
                    self.assertEqual(
                        after["artifacts"][0]["content"].encode(), expected.read_bytes()
                    )
                    (case / expected.name).unlink()
                self.assertEqual(
                    runs.save()[-1], (case / expected.name).resolve().as_posix()
                )
                self.assertEqual(
                    (case / expected.name).read_bytes(), expected.read_bytes()
                )

    def test_functions_without_packaging_lock_keep_conformance_and_saved_bytes(self):
        case = self.study("adam-adsl-bmi")
        environment = case / "python/environment.yaml"
        environment.write_text(
            "\n".join(
                line
                for line in environment.read_text().splitlines()
                if not line.startswith("lock:")
            )
            + "\n"
        )
        (case / "python/uv.lock").unlink(missing_ok=True)
        project = self.project(case)
        with patch.object(
            host, "verify_versions", side_effect=AssertionError("unexpected lock audit")
        ):
            self.assertTrue(project.check().issues.is_empty())
            result = project.spec("ADSL").run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(self.report(result)["activation"]["lock"], "not_requested")
        self.assertTrue(self.report(result)["activation"]["tests"])
        expected = next((case / "expected").glob("*.csv"))
        self.assertEqual(result.save()[-1], (case / expected.name).resolve().as_posix())
        self.assertEqual((case / expected.name).read_bytes(), expected.read_bytes())

    def test_negative_static_issue_is_exact_and_never_imports_code_or_reads_data(self):
        case = self.study("negative-function-contract")
        shutil.rmtree(case / "input")
        expected = yaml.safe_load((case / "expected/error.yaml").read_text())
        expected.update(source_span=None, operand_route=None)
        project = self.project(case)
        spec_path = (case / "spec.yaml").as_posix()
        with no_activation():
            spec = project.spec("TEST")
            for result in (spec.check(), spec.run()):
                rows = result.issues.to_dicts()
                self.assertEqual(len(rows), 1)
                rows[0]["context"] = json.loads(rows[0]["context"])
                self.assertEqual(rows[0], expected)
            # A project check names the specification each issue belongs to.
            (row,) = project.check().issues.to_dicts()
            row["context"] = json.loads(row["context"])
            self.assertEqual(
                row,
                {
                    **expected,
                    "context": {
                        **expected["context"],
                        "entry": spec_path,
                        "source": spec_path,
                    },
                },
            )
        self.assertIsNone(result.output)
        self.assertIsNone(result.report)
        with self.assertRaises(yamaa.Error):
            result.save()

    def test_explicit_absolute_project_keeps_the_callers_root_spelling(self):
        case = self.value_study()
        os.chdir(self.previous)
        project = self.project(case)
        self.assertTrue(project.check().issues.is_empty())
        checked = project.spec("TEST").check()
        self.assertTrue(checked.issues.is_empty(), checked.issues)

    def test_valid_static_check_has_no_host_effects_even_with_missing_data(self):
        case = self.value_study()
        shutil.rmtree(case / "input")
        project = self.project(case)
        with no_activation():
            self.assertTrue(project.check().issues.is_empty())
            self.assertTrue(project.spec("TEST").check().issues.is_empty())

    def test_project_check_validates_original_studies_without_study_inputs(self):
        # The caller's path may be any UTF-8 text; the project file is ASCII.
        study = self.directory / "\u00e9tude"
        study.mkdir()
        for name in CASES:
            with self.subTest(name=name):
                case = study / name
                shutil.copytree(ROOT / name, case)
                shutil.rmtree(case / "input")
                domain = yaml.safe_load((case / "spec.yaml").read_text())["domain"]
                project = self.project(case)
                with no_activation():
                    self.assertTrue(project.check().issues.is_empty())
                    checked = project.spec(domain).check()
                self.assertTrue(checked.issues.is_empty(), checked.issues)
                self.assertEqual(list(case.glob("*.csv")), [])

    def test_lock_mismatch_precedes_binding_tests_and_missing_study_data(self):
        case = self.value_study()
        lock = case / "python/uv.lock"
        lock.write_text(
            lock.read_text().replace(
                'name = "yamaa-benchmarks"\nversion = "1.0.0"',
                'name = "yamaa-benchmarks"\nversion = "999999.0"',
            )
        )
        shutil.rmtree(case / "input")
        with patch.object(
            host, "resolve_callable", side_effect=AssertionError("binding")
        ):
            result = self.project(case).spec("TEST").run()
        report = self.report(result)
        self.assertEqual(report["activation"]["lock"], "rejected")
        self.assertEqual(report["source_reads"], [])
        self.assertEqual(report["activation"]["bindings"], [])
        self.assertEqual(
            result.issues["condition"].to_list(), ["runtime_artifact_mismatch"]
        )

    def test_every_failed_case_is_retained_before_data_and_run_repeats_tests(self):
        case = self.value_study()
        shutil.rmtree(case / "input")
        env = case / "python/environment.yaml"
        source = yaml.safe_load(env.read_text())
        tests = source["functions"]["project_value"]["tests"]
        tests[0]["result"] = 9.5
        tests[1]["result"] = 9.0
        project = self.project(case)
        for declared_lock in (True, False):
            with self.subTest(declared_lock=declared_lock):
                if not declared_lock:
                    source.pop("lock")
                env.write_text(yaml.safe_dump(source, sort_keys=False))
                for _ in range(2):
                    result = project.spec("TEST").run()
                    report = self.report(result)
                    self.assertEqual(report["source_reads"], [])
                    self.assertEqual(
                        report["activation"]["lock"],
                        "verified" if declared_lock else "not_requested",
                    )
                    self.assertEqual(
                        [case["outcome"] for case in report["activation"]["tests"]],
                        ["result_mismatch", "result_mismatch", "passed"],
                    )
                    self.assertEqual(
                        result.issues["condition"].to_list(),
                        ["function_conformance_failed"] * 2,
                    )

    def test_project_codelists_bind_and_verify_with_independent_static_findings(self):
        case = self.value_study()
        project_file = case / "project.yaml"
        document = yaml.safe_load(project_file.read_text())
        document["codelists"] = [
            {
                "codelists": [
                    {
                        "id": "IDENTIFIERS",
                        "name": "Identifiers",
                        "items": [{"value": "01"}, {"value": "S2"}, {"value": "S3"}],
                    }
                ]
            }
        ]
        project_file.write_text(yaml.safe_dump(document, sort_keys=False))
        spec_path = case / "spec.yaml"
        spec = yaml.safe_load(spec_path.read_text())
        spec["columns"][0]["submission"] = {
            "codelist": "IDENTIFIERS",
            "origin": {"type": "Assigned"},
        }
        spec_path.write_text(yaml.safe_dump(spec, sort_keys=False))
        project = yamaa.project(project_file.resolve())
        checked = project.spec("TEST").check()
        self.assertTrue(checked.issues.is_empty(), checked.issues)
        result = project.spec("TEST").run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertTrue(self.report(result)["verifications"])
        spec["columns"][0]["submission"]["codelist"] = "ABSENT"
        spec["columns"][1]["derivation"]["function"]["args"] = {}
        spec_path.write_text(yaml.safe_dump(spec, sort_keys=False))
        shutil.rmtree(case / "input")
        with patch.object(
            host, "verify_versions", side_effect=AssertionError("activation")
        ):
            rows = project.spec("TEST").check().issues.to_dicts()
        self.assertEqual({row["rule"] for row in rows}, {"FUNC-5", "CT-1"})

    def test_approved_math_is_identical_in_root_row_and_nested_case_compute(self):
        expression = {"compute": {"expr": "EXP(0) + LN(1) + POWER(2, 3)"}}
        for context in tuple(TRUTH["quota_modes"]):
            with self.subTest(context=context):
                case = (
                    self.value_study()
                    if context == "column"
                    else self.study(
                        "schema-functions" if context == "row" else "schema-non-finite"
                    )
                )
                path = case / "spec.yaml"
                spec = yaml.safe_load(
                    (ROOT / "negative-function-contract/spec.yaml").read_text()
                )
                spec["columns"][1]["derivation"] = copy.deepcopy(expression)
                if context == "row":
                    del spec["columns"][1]["derivation"]
                    spec["rows"] = [
                        {
                            "id": "records",
                            "dataset": "SOURCE",
                            "derivations": {"RESULT": copy.deepcopy(expression)},
                        }
                    ]
                if context == "case":
                    spec["columns"][1]["derivation"] = {
                        "case": [
                            {
                                "when": "ID = '01'",
                                "then": {
                                    "case": [
                                        {
                                            "when": "ID = '01'",
                                            "then": copy.deepcopy(expression),
                                        },
                                        {"otherwise": {"literal": 99.0}},
                                    ]
                                },
                            },
                            {"otherwise": {"literal": 98.0}},
                        ]
                    }
                path.write_text(yaml.safe_dump(spec, sort_keys=False))
                if context != "column":
                    shutil.rmtree(case / "input")
                    shutil.copytree(
                        ROOT / "negative-function-contract/input", case / "input"
                    )
                    shutil.copyfile(
                        ROOT / "negative-function-contract/python/environment.yaml",
                        case / "python/environment.yaml",
                    )
                result = self.project(case).spec("TEST").run()
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(result.output["RESULT"].to_list(), [9.0])
                self.assertEqual(
                    result.save()[-1], (case / "test.csv").resolve().as_posix()
                )
                self.assertEqual((case / "test.csv").read_bytes(), b"ID,RESULT\n01,9\n")

    def test_original_interrupt_survives_activation_in_spec_and_project_runs(self):
        case = self.value_study()
        path = case / "spec.yaml"
        spec = yaml.safe_load(path.read_text())
        spec["output"]["decimals"] = 2
        path.write_text(yaml.safe_dump(spec, sort_keys=False))
        project = self.project(case)
        for call in (lambda: project.spec("TEST").run(), project.run):
            original = KeyboardInterrupt("original project interruption")
            with (
                patch.object(host, "verify_versions", side_effect=original),
                self.assertRaises(KeyboardInterrupt) as seen,
            ):
                call()
            self.assertIs(seen.exception, original)
        self.assertFalse((case / "test.csv").exists())

    def test_row_math_binds_stored_fields_and_orders_completed_dependencies(self):
        case = self.value_study()
        path = case / "spec.yaml"
        spec = yaml.safe_load(path.read_text())
        spec["columns"].insert(
            1, {"name": "EARLIER", "type": "float", "label": "Earlier"}
        )
        del spec["columns"][2]["derivation"]
        spec["rows"] = [
            {
                "id": "records",
                "dataset": "SOURCE",
                "derivations": {
                    "RESULT": {
                        "compute": {
                            "expr": "LN(1) + EXP(0) - 1 + EARLIER + SOURCE.VALUE"
                        }
                    },
                    "EARLIER": {"literal": 2.5},
                },
            }
        ]
        path.write_text(yaml.safe_dump(spec, sort_keys=False))
        result = self.project(case).spec("TEST").run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(result.output["RESULT"].to_list(), [4.0])
        self.assertEqual(result.save()[-1], (case / "test.csv").resolve().as_posix())
        self.assertEqual((case / "test.csv").read_bytes(), b"ID,RESULT\n01,4\n")

    def test_opaque_host_error_is_retained_independently_of_issue_format(self):
        case = self.value_study()
        path = case / "spec.yaml"
        spec = yaml.safe_load(path.read_text())
        spec["output"]["decimals"] = 2
        path.write_text(yaml.safe_dump(spec, sort_keys=False))
        shutil.rmtree(case / "input")

        class LockHostError(RuntimeError):
            pass

        original = LockHostError("original lock host error")
        reference = weakref.ref(original)
        with patch.object(host, "verify_versions", side_effect=original):
            result = self.project(case).spec("TEST").run()
        self.assertIsNone(result.output)
        self.assertIsNone(result.report)
        self.assertIsNone(result._native.observations())
        self.assertFalse(result.issues.is_empty())
        with self.assertRaises(yamaa.Error):
            result.save()
        # The run owns the original host failure exactly as long as it lives.
        del original
        gc.collect()
        self.assertIsNotNone(reference())
        del result
        gc.collect()
        self.assertIsNone(reference())

    def test_lock_format_and_host_language_are_static_independent_findings(self):
        case = self.value_study()
        project = self.project(case)
        entry = (case / "project.yaml").as_posix()

        def lock_kind(environment):
            return issue(
                "project_environment_invalid",
                "FUNC-2",
                ["lock"],
                {
                    "actual_lock_kind": "renv",
                    "entry": entry,
                    "environment_path": "lock",
                    "language": "python",
                    "reason": "lock_kind_mismatch",
                    "source": (case / environment).as_posix(),
                },
            )

        # Captured syntax, rather than the file name, identifies the lock.
        shutil.copyfile(case / "r/renv.lock", case / "python/uv.lock")
        with no_activation():
            self.assertEqual(
                project.check().issues.to_dicts(),
                [lock_kind("python/environment.yaml")],
            )
        shutil.copyfile(
            ROOT / "negative-function-contract/python/uv.lock", case / "python/uv.lock"
        )
        # The environments key states the language: Python reads the R file
        # as a Python environment and refuses its lock and callable.
        (case / "project.yaml").write_text(
            "schema_version: '1.0'\n"
            "environments: {python: r/environment.yaml, r: r/environment.yaml}\n"
            "adam: {specs: [spec.yaml]}\n"
        )
        with no_activation():
            rows = project.check().issues.to_dicts()
        self.assertEqual(rows[0], lock_kind("r/environment.yaml"))
        self.assertEqual(
            [(row["condition"], json.loads(row["context"])["reason"]) for row in rows],
            [
                ("project_environment_invalid", "lock_kind_mismatch"),
                ("project_environment_invalid", "invalid_callable"),
            ],
        )
        # An environment file no longer states its language.
        shutil.copyfile(
            ROOT / "negative-function-contract/project.yaml", case / "project.yaml"
        )
        environment = case / "python/environment.yaml"
        environment.write_text(
            environment.read_text().replace(
                "schema_version: '1.0'\n", "schema_version: '1.0'\nlanguage: python\n"
            )
        )
        with no_activation():
            (row,) = project.check().issues.to_dicts()
        self.assertEqual(
            (row["condition"], row["rule"], row["spec_paths"]),
            ("unknown_field", "SCHEMA-13", ["language"]),
        )
        self.assertEqual(
            json.loads(row["context"])["environment_path"], "environments.python"
        )

    def test_called_only_selection_and_no_code_effects_for_empty_selection(self):
        case = self.value_study()
        env = case / "python/environment.yaml"
        source = yaml.safe_load(env.read_text())
        unused = copy.deepcopy(source["functions"]["project_value"])
        unused["function"] = "absent_package.never_called"
        source["functions"]["unused"] = unused
        env.write_text(yaml.safe_dump(source, sort_keys=False))
        project = self.project(case)
        result = project.spec("TEST").run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(
            [b["function"] for b in self.report(result)["activation"]["bindings"]],
            ["project_value"],
        )
        spec = case / "spec.yaml"
        spec.write_text(
            spec.read_text().replace(
                "function:\n        name: project_value\n        args: {x: SOURCE.VALUE}",
                "literal: 1.0",
            )
        )
        with no_activation():
            result = project.spec("TEST").run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(self.report(result)["activation"]["tests"], [])

    def test_aggregate_callback_keeps_original_current_types_and_selected_record(self):
        case = self.study("negative-function-contract")
        spec = """schema_version: '1.0'
domain: TEST
input:
  SRC: {path: input.csv, types: {ID: int, N: float}}
  B: {path: input.csv, types: {ID: int, N: str}}
base: SRC
keys: [ID, N]
intermediates:
  - {id: CAP, dataset: SRC, key: {N: SRC.N}, order_by: [SRC.ID], keep: first}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: N, type: float, derivation: SRC.N}
  - name: VALUE
    type: float
    derivation:
      aggregate:
        key: {ID: ID}
        derive:
          - {name: BASE, type: float, derivation: B.N}
          - {name: V, type: float, derivation: {function: {name: project_value, args: {x: CAP.N}}}}
        expr: SUM(V)
output: {path: output.csv, columns: [ID, N, VALUE]}
"""
        (case / "spec.yaml").write_text(spec)
        (case / "input.csv").write_text("ID,N\n1,10\n1,30\n2,40\n")
        project = self.project(case)
        result = project.spec("TEST").run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(
            result.output.rows(), [(1, 10.0, 20.0), (1, 30.0, 60.0), (2, 40.0, 40.0)]
        )
        observed = self.report(result)
        self.assertEqual(
            [row["function"] for row in observed["activation"]["bindings"]],
            ["project_value"],
        )
        self.assertEqual(result.save()[-1], (case / "output.csv").resolve().as_posix())
        self.assertEqual(
            (case / "output.csv").read_bytes(),
            b"ID,N,VALUE\n1,10,20\n1,30,60\n2,40,40\n",
        )
        # Saving publishes held bytes; it activates nothing again.
        self.assertEqual(self.report(result)["activation"], observed["activation"])
        (case / "spec.yaml").write_text(
            spec.replace(", order_by: [SRC.ID], keep: first", "")
        )
        with no_activation():
            issues = project.spec("TEST").check().issues.to_dicts()
        self.assertTrue(
            any(
                row["condition"] == "invalid_derive_step" and row["rule"] == "LOOKUP-22"
                for row in issues
            ),
            issues,
        )

    def test_row_callbacks_join_each_driver_by_completed_keys(self):
        case = self.study("negative-function-contract")
        (case / "a.csv").write_text("ID,N\n1,10\n2,20\n3,30\n")
        (case / "b.csv").write_text("ID,N\n2,40\n1,50\n")
        project = self.project(case)
        for driver, other, rows, csv in (
            (
                "SRC",
                "B",
                [(1, 50.0), (2, 40.0), (3, None)],
                b"ID,VALUE\n1,50\n2,40\n3,\n",
            ),
            ("B", "SRC", [(2, 20.0), (1, 10.0)], b"ID,VALUE\n2,20\n1,10\n"),
        ):
            spec = f"""schema_version: '1.0'
domain: TEST
input:
  SRC: {{path: a.csv, types: {{ID: int, N: float}}}}
  B: {{path: b.csv, types: {{ID: int, N: float}}}}
base: SRC
keys: [ID]
columns:
  - {{name: ID, type: int}}
  - {{name: VALUE, type: float}}
rows:
  - id: records
    dataset: {driver}
    derivations:
      ID: {driver}.ID
      VALUE: {{function: {{name: project_value, args: {{x: {other}.N}}}}}}
output: {{path: output.csv, columns: [ID, VALUE]}}
"""
            (case / "spec.yaml").write_text(spec)
            result = project.spec("TEST").run()
            self.assertTrue(result.issues.is_empty(), result.issues)
            self.assertEqual(result.output.rows(), rows)
            activation = self.report(result)["activation"]
            self.assertEqual(
                [row["function"] for row in activation["bindings"]], ["project_value"]
            )
            self.assertEqual(
                result.save()[-1], (case / "output.csv").resolve().as_posix()
            )
            self.assertEqual((case / "output.csv").read_bytes(), csv)
            self.assertEqual(self.report(result)["activation"], activation)

    def test_intermediate_callbacks_read_prior_values_and_selected_donor_types(self):
        case = self.study("negative-function-contract")
        (case / "a.csv").write_text("ID,N\n1,10\n1,30\n2,40\n")
        (case / "b.csv").write_text("ID,N\n1,one\n2,two\n")
        (case / "spec.yaml").write_text("""schema_version: '1.0'
domain: TEST
input:
  SRC: {path: a.csv, types: {ID: int, N: float}}
  B: {path: b.csv, types: {ID: int, N: str}}
base: SRC
keys: [ID, N]
intermediates:
  - {id: CAP, dataset: SRC, key: [ID], order_by: [SRC.N], keep: first}
  - id: DON
    dataset: B
    key: [ID]
    order_by: [B.ID]
    keep: first
    derivations:
      BASE: CAP.N
      VALUE: {function: {name: project_value, args: {x: BASE}}}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - {name: N, type: float, derivation: SRC.N}
  - {name: VALUE, type: float, derivation: DON.VALUE}
output: {path: output.csv, columns: [ID, N, VALUE]}
""")
        result = self.project(case).spec("TEST").run()
        self.assertTrue(result.issues.is_empty(), result.issues)
        self.assertEqual(
            result.output.rows(), [(1, 10.0, 10.0), (1, 30.0, 10.0), (2, 40.0, 40.0)]
        )
        activation = self.report(result)["activation"]
        self.assertEqual(
            [row["function"] for row in activation["bindings"]], ["project_value"]
        )
        self.assertEqual(result.save()[-1], (case / "output.csv").resolve().as_posix())
        self.assertEqual(
            (case / "output.csv").read_bytes(),
            b"ID,N,VALUE\n1,10,10\n1,30,10\n2,40,40\n",
        )
        self.assertEqual(self.report(result)["activation"], activation)


if __name__ == "__main__":
    unittest.main()
