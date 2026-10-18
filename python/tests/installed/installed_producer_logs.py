"""Installed producer-graph run reports and complete retained publication witnesses."""

import builtins
import copy
import gc
import io
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import polars as pl
from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "producer-logs")

SPECIFICATION = Path(__file__).with_name("specification-original")
IDS = [int(value) for value in TRUTH["ids_decimal"]]
CSV = TRUTH["input_csv"].encode("utf-8")
KEYS = TRUTH["offending_keys_json"]
ROOT_ID = "\u00e9\U0001d11e"
HEADER = (
    "ARTIFACT,KIND,SPEC_PATH,NAME,ID,TARGET,RULE,SEVERITY,OUTCOME,CONDITION,"
    "COUNT,TOTAL,KEYS,DETAILS\n"
)
# The inputs each node reports, in declaration order, as (dataset, declared
# path, declared schema link). A linked input reads its producer's held
# artifact, which holds the same IDs and so the same bytes as the raw source.
INPUTS = {
    "producer": [("SRC", "input.csv", None)],
    "left": [("P", "generated/producer.csv", "producer.yaml")],
    "right": [("P", "generated/producer.csv", "producer.yaml")],
    "root": [
        ("A", "generated/producer.csv", "producer.yaml"),
        ("B", "generated/producer.csv", "./producer.yaml"),
    ],
    "diamond root": [
        ("A", "generated/left.csv", "left.yaml"),
        ("B", "generated/right.csv", "right.yaml"),
    ],
}


def cell(text):
    """One CSV field holding `text`, which carries quotes."""
    return '"' + text.replace('"', '""') + '"'


def golden(name, *, diamond=False, fatal=None):
    """The authored run report of one graph node (REQ-0392, REQ-0393).

    Its rows are the run; each input with the file facts of REQ-1176; the
    base row template, driven by the first input; the ID column, for a
    successful run only; and the range check with every offending key.
    """
    failed = name == fatal
    count = len(IDS)
    inputs = INPUTS["diamond root" if diamond and name == "root" else name]
    rows = [
        "run,output,,,,,,failed,range_failed,,,,{}"
        if failed
        else f"run,output,,,,,,succeeded,,{count},,,{{}}"
    ]
    for dataset, path, schema in inputs:
        link = "" if schema is None else f',"schema":"{schema}"'
        facts = f'{{"bytes":{len(CSV)},"path":"{path}","profile":"csv"{link}}}'
        rows.append(f"input,input.{dataset},{dataset},,,,,,,{count},,,{cell(facts)}")
    rows.append(f"rows,base,{inputs[0][0]},,,,,,,{count},{count},,{{}}")
    if not failed:
        rows.append(f"column,columns.ID,ID,,,,,,,0,{count},,{{}}")
    identifier = ROOT_ID if name == "root" else name + "-range"
    severity = "error" if failed else "warning"
    rows.append(
        f"check,columns.ID.verifications[0].range,range,{identifier},ID,REQ-0377,"
        f"{severity},violated,range_failed,{count},{count},{cell(KEYS)},"
        + cell('{"column":"ID"}')
    )
    artifact = f"generated/{name}.csv"
    return (HEADER + "".join(f"{artifact},{row}\n" for row in rows)).encode("utf-8")


def frame(report):
    """Read report bytes with the run report column types (REQ-0392)."""
    return pl.read_csv(
        io.BytesIO(report),
        schema_overrides={"COUNT": pl.Int64, "TOTAL": pl.Int64},
        infer_schema=False,
    )


class ProducerLogs(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name).resolve()
        # The retired yamaa-project.yaml marker is never read.
        self.write("yamaa-project.yaml", "version: '1.0'\n")
        destination = os.environ.get("YAMAA_PUBLIC_REPORT_DIR")
        self.evidence = (
            Path(destination) / "producer-graph-logs/python" / self._testMethodName
            if destination
            else self.work / "evidence"
        )
        self.evidence.mkdir(parents=True)
        (self.evidence / "case-directory.txt").write_text(
            str(self.work), encoding="utf-8"
        )
        provenance = os.environ.get("YAMAA_SUPPLEMENTAL_EVIDENCE")
        if provenance:
            (self.evidence / "provenance.json").write_text(provenance, encoding="utf-8")
        original = builtins.__import__

        def reject(name, *args, **kwargs):
            if name in {"yaml", "yaml12", "pydantic", "pyarrow"} or name.startswith(
                (
                    "yamaa.planning",
                    "yamaa.runtime",
                    "yamaa.specification",
                    "yamaa.io",
                    "yamaa.adapters",
                )
            ):
                raise AssertionError("reference semantic import: " + name)
            return original(name, *args, **kwargs)

        guard = patch("builtins.__import__", side_effect=reject)
        guard.start()
        self.addCleanup(guard.stop)

    def write(self, name, value):
        path = self.work / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value.encode("ascii") if isinstance(value, str) else value)

    def fixture(self, fatal=None, diamond=False, blocked=False):
        self.write("input.csv", CSV)
        self.names = TRUTH["diamond_names"] if diamond else ["producer", "root"]
        for name in self.names:
            if name == "producer":
                source, base = TRUTH["raw_source_reference"], "SRC"
            elif name in {"left", "right"}:
                source, base = (
                    TRUTH["producer_reference"],
                    "P",
                )
            elif diamond:
                source, base = (
                    TRUTH["diamond_reference"],
                    "A",
                )
            else:
                source, base = (
                    TRUTH["aliased_producer_reference"],
                    "A",
                )
            intermediate = TRUTH["root_intermediate_yaml"] if name == "root" else ""
            severity = "error" if name == fatal else "warning"
            report = (
                "blocked/root-report.csv"
                if blocked and name == "root"
                else f"generated/{name}-report.csv"
            )
            identifier = r'"\u00e9\U0001d11e"' if name == "root" else name + "-range"
            # The root's ID reads SHARED through first_available so pruning
            # (REQ-0638) keeps the intermediate, and with it the B producer
            # link, in the resolved graph. SHARED's key is a literal: keyed by
            # the ID field it would form a dependency cycle with the ID column
            # that reads it.
            derivation = (
                "{first_available: {sources: [A.ID, SHARED.ID]}}"
                if name == "root"
                else f"{base}.ID"
            )
            self.write(
                name + ".yaml",
                f"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {source}\n{intermediate}columns:\n"
                f"  - name: ID\n    type: int\n    label: Identifier\n    derivation: {derivation}\n"
                f"    verifications: [{{range: {{max: 0, severity: {severity}, id: {identifier}}}}}]\n"
                f"output: {{path: generated/{name}.csv, columns: [ID], report: {report}}}\n",
            )
        self.paths = [
            path
            for name in self.names
            for path in (
                "blocked/root-report.csv"
                if blocked and name == "root"
                else f"generated/{name}-report.csv",
                f"generated/{name}.csv",
            )
        ]

    def build(self):
        return yamaa.spec(self.work / "root.yaml").run()

    def remove_inputs(self):
        for name in ["input.csv", *(name + ".yaml" for name in self.names)]:
            (self.work / name).unlink()
            self.assertFalse((self.work / name).exists())

    def retain(self, result, phase):
        destination = self.evidence / phase
        destination.mkdir()
        raw = result._native.observations()
        self.assertIsInstance(raw, str)
        (destination / "report.json").write_text(raw, encoding="utf-8")
        for path in self.paths:
            actual = self.work / path
            if actual.exists():
                saved = destination / path
                saved.parent.mkdir(parents=True, exist_ok=True)
                saved.write_bytes(actual.read_bytes())
        return json.loads(raw)

    @staticmethod
    def execution(report):
        return {
            key: value
            for key, value in report.items()
            if key not in {"artifacts", "publication"}
        }

    def test_alias_and_diamond_reports_use_actual_owned_views_and_repeat_held_bytes(
        self,
    ):
        for diamond in (False, True):
            with self.subTest(diamond=diamond):
                self.fixture(diamond=diamond)
                (self.work / "generated").mkdir(exist_ok=True)
                result = self.build()
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(
                    result.output.to_dicts(), [{"ID": value} for value in IDS]
                )
                # The graph's result exposes the entry specification's report.
                report = result.report
                self.assertTrue(report.equals(frame(golden("root", diamond=diamond))))
                check = report.filter(pl.col("KIND") == "check")
                self.assertEqual(check["ID"].to_list(), [ROOT_ID])
                self.assertEqual(check["TOTAL"].to_list(), [8])
                self.assertEqual(check["COUNT"].to_list(), [8])
                self.assertEqual(check["KEYS"].to_list(), [KEYS])
                self.assertEqual(report.schema["COUNT"], pl.Int64)
                self.assertEqual(report.schema["TOTAL"], pl.Int64)
                self.assertTrue(
                    all(not (self.work / path).exists() for path in self.paths)
                )
                before = self.retain(result, f"build-{diamond}")
                self.assertEqual(len(before["source_reads"]), 1)
                owners = {record["specification"] for record in before["tables"]}
                # Canonical document identities use forward slashes on every
                # platform (see path_text in file_resources.rs).
                expected_owners = {
                    (self.work / (name + ".yaml")).as_posix() for name in self.names
                }
                self.assertEqual(owners, expected_owners)
                report.replace_column(0, pl.Series("ARTIFACT", ["mutated"] * 6))
                self.assertEqual(
                    result.report["ARTIFACT"].to_list(), ["generated/root.csv"] * 6
                )
                self.remove_inputs()
                gc.collect()
                held = None
                for attempt in (1, 2):
                    result.save()
                    report = self.retain(result, f"save-{diamond}-{attempt}")
                    self.assertEqual(self.execution(report), self.execution(before))
                    entries = report["publication"]["entered"]
                    order = TRUTH["diamond_names"] if diamond else ["producer", "root"]
                    # Each node publishes its report, then its primary artifact.
                    expected = [
                        f"generated/{name}{suffix}.csv"
                        for name in order
                        for suffix in ("-report", "")
                    ]
                    self.assertEqual([entry["path"] for entry in entries], expected)
                    self.assertEqual(
                        [entry["field"] for entry in entries],
                        ["report", "path"] * len(order),
                    )
                    self.assertTrue(all(entry["published"] for entry in entries))
                    self.assertEqual(len(report["artifacts"]), len(expected))
                    actual = {
                        path: (self.work / path).read_bytes() for path in expected
                    }
                    for path, artifact in zip(expected, report["artifacts"]):
                        self.assertEqual(
                            actual[path], artifact["content"].encode("utf-8")
                        )
                        if "-" not in Path(path).name:
                            self.assertEqual(actual[path], CSV)
                        else:
                            name = Path(path).name.split("-")[0]
                            self.assertEqual(
                                actual[path], golden(name, diamond=diamond)
                            )
                    if held is not None:
                        self.assertEqual(actual, held)
                    held = actual
                for path in self.paths:
                    (self.work / path).unlink()

    def test_failed_leaf_and_dependent_save_only_the_reports_of_nodes_that_ran(self):
        for fatal in ("producer", "root"):
            self.fixture(fatal=fatal)
            result = self.build()
            self.assertIsNone(result.output)
            # A failed producer stops the graph: the entry never runs and so
            # has no report (REQ-1330).
            if fatal == "producer":
                self.assertIsNone(result.report)
            else:
                self.assertTrue(
                    result.report.equals(frame(golden("root", fatal="root")))
                )
            issues = result.issues.clone()
            self.assertEqual(issues["condition"].to_list(), ["range_failed"])
            before = self.retain(result, "failed-build-" + fatal)
            for path in self.paths:
                self.write(path, b"earlier sentinel")
            self.remove_inputs()
            for attempt in (1, 2):
                with self.assertRaises(yamaa.Error) as raised:
                    result.save()
                self.assertTrue(raised.exception.issues.equals(issues))
                report = self.retain(result, f"failed-save-{fatal}-{attempt}")
                self.assertTrue(result.issues.equals(issues))
                self.assertEqual(self.execution(report), self.execution(before))
                self.assertEqual(report["publication"]["mode"], "diagnostic")
                self.assertEqual(
                    [entry["field"] for entry in report["publication"]["entered"]],
                    ["report"] * (1 if fatal == "producer" else 2),
                )
                for path in self.paths:
                    expected_report = path.endswith("-report.csv") and (
                        fatal == "root" or "producer" in path
                    )
                    if expected_report:
                        name = Path(path).name.split("-")[0]
                        self.assertEqual(
                            (self.work / path).read_bytes(), golden(name, fatal=fatal)
                        )
                    else:
                        self.assertEqual(
                            (self.work / path).read_bytes(), b"earlier sentinel"
                        )
            for path in self.paths:
                (self.work / path).unlink()

    def test_missing_parent_preflights_every_target_and_retry_needs_no_inputs(self):
        self.fixture()
        result = self.build()
        before = self.retain(result, "parent-build")
        self.assertFalse((self.work / "generated").exists())
        self.remove_inputs()
        with self.assertRaises(yamaa.Error):
            result.save()
        refused = self.retain(result, "parent-refused")
        self.assertEqual(refused["publication"]["entered"], [])
        self.assertEqual(self.execution(refused), self.execution(before))
        (self.work / "generated").mkdir()
        result.save()
        self.assertTrue(result.issues.is_empty())
        saved = self.retain(result, "parent-retry")
        self.assertEqual(len(saved["artifacts"]), 4)
        self.assertEqual(self.execution(saved), self.execution(before))

    def test_unchanged_original_report_document_and_goldens_run_as_shared_producer(
        self,
    ):
        source = SPECIFICATION / "cases/schema-verification-log"
        original = (source / "spec.yaml").read_bytes()
        self.write("producer.yaml", original)
        shutil.copytree(source / "input", self.work / "input")
        consumer = (
            original.decode("ascii")
            .replace("domain: ADSL", "domain: ADSL\nbase: A")
            .replace("adsl", "root")
            .replace(
                "  DM: input/dm.csv",
                TRUTH["verification_producer_aliases"],
            )
            .replace("DM.", "A.")
        )
        self.write("root.yaml", consumer)
        self.paths = ["adsl-checks.csv", "adsl.csv", "root-checks.csv", "root.csv"]
        result = self.build()
        self.assertTrue(result.issues.is_empty(), result.issues)
        truth = json.loads(
            (SPECIFICATION / "expected/schema-verification-log.json").read_text()
        )
        before = self.retain(result, "original-build")
        expected_nodes, expected_checks = [], []
        for name in ("producer", "root"):
            for key, output in (
                ("nodes", expected_nodes),
                ("verifications", expected_checks),
            ):
                records = copy.deepcopy(truth[key])
                for record in records:
                    record["specification"] = (self.work / (name + ".yaml")).as_posix()
                output.extend(records)
        self.assertEqual(before["nodes"], expected_nodes)
        self.assertEqual(before["verifications"], expected_checks)
        expected_leaf = copy.deepcopy(truth["tables"])
        for record in expected_leaf:
            record["specification"] = (self.work / "producer.yaml").as_posix()
        self.assertEqual(
            [
                record
                for record in before["tables"]
                if record["specification"].endswith("/producer.yaml")
            ],
            expected_leaf,
        )
        self.assertEqual(len(before["source_reads"]), 1)
        for name in ("producer.yaml", "root.yaml", "input/dm.csv"):
            (self.work / name).unlink()
            self.assertFalse((self.work / name).exists())
        expected = {
            name: (source / "expected" / name).read_bytes() for name in self.paths[:2]
        }
        # The consumer reports its own artifact and reads only A, the held
        # producer artifact, which pruning keeps; B is never read (REQ-0638).
        # A linked input names its producer (REQ-1176).
        produced = expected["adsl.csv"]
        root_report = (
            expected["adsl-checks.csv"]
            .replace(b"adsl.csv,", b"root.csv,")
            .replace(
                b'input,input.DM,DM,,,,,,,3,,,"{""bytes"":66,""path"":""input/dm.csv"",'
                b'""profile"":""csv""}"',
                b'input,input.A,A,,,,,,,3,,,"{""bytes"":'
                + str(len(produced)).encode()
                + b',""path"":""adsl.csv"",""profile"":""csv"",""schema"":'
                b'""producer.yaml""}"',
            )
            .replace(b"rows,base,DM,", b"rows,base,A,")
        )
        self.assertIn(b"root.csv,input,input.A,A,", root_report)
        expected.update({"root-checks.csv": root_report, "root.csv": produced})
        self.assertTrue(result.report.equals(frame(root_report)))
        for attempt in (1, 2):
            result.save()
            report = self.retain(result, f"original-save-{attempt}")
            self.assertEqual(self.execution(report), self.execution(before))
            self.assertEqual(
                [entry["path"] for entry in report["publication"]["entered"]],
                self.paths,
            )
            self.assertEqual(
                [entry["published"] for entry in report["publication"]["entered"]],
                [True] * 4,
            )
            for name, content in expected.items():
                self.assertEqual((self.work / name).read_bytes(), content)

    def test_lexical_collisions_and_invalid_reports_precede_study_access(self):
        for replacement in (
            "generated/../generated/producer.csv",
            "generated/root-report.arrow",
        ):
            self.fixture()
            entry = self.work / "root.yaml"
            entry.write_text(
                entry.read_text().replace("generated/root-report.csv", replacement)
            )
            (self.work / "input.csv").unlink()
            result = self.build()
            self.assertIsNone(result.output)
            self.assertFalse(result.issues.is_empty())
            self.assertFalse(
                any(
                    "file_not_found" in value
                    for value in result.issues["condition"].to_list()
                )
            )
            with self.assertRaises(yamaa.Error):
                result.save()
            self.assertTrue(all(not (self.work / path).exists() for path in self.paths))

    @unittest.skipUnless(
        sys.platform == "darwin" or os.name == "nt",
        "qualified case-insensitive platform witness",
    )
    def test_absent_entry_case_and_physical_parent_aliases_refuse_before_any_write(
        self,
    ):
        self.fixture()
        self.write("generated/case-probe", b"native case witness")
        sensitive = not (self.work / "generated/CASE-PROBE").exists()
        (self.work / "generated/case-probe").unlink()
        if sensitive:
            self.skipTest("fixture directory is case-sensitive")
        for name in (
            "generated/PRODUCER-report.csv",
            "GENERATED/producer-report.csv",
        ):
            self.fixture()
            entry = self.work / "root.yaml"
            entry.write_text(
                entry.read_text().replace("generated/root-report.csv", name)
            )
            self.assertFalse((self.work / "generated/producer-report.csv").exists())
            if name.startswith("GENERATED"):
                self.assertTrue(
                    (self.work / "GENERATED").samefile(self.work / "generated")
                )
            result = self.build()
            if os.name == "nt":
                self.assertIsNone(result.output)
                self.assertFalse(result.issues.is_empty())
                with self.assertRaises(yamaa.Error):
                    result.save()
            else:
                self.assertIsNotNone(result.output)
                with self.assertRaises(yamaa.Error):
                    result.save()
                phase = (
                    "case-parent-alias"
                    if name.startswith("GENERATED")
                    else "case-entry-alias"
                )
                report = self.retain(result, phase)
                self.assertEqual(report["publication"]["entered"], [])
                self.assertEqual(report["publication"]["error"]["field"], "report")
            self.assertEqual(list((self.work / "generated").iterdir()), [])

    @unittest.skipUnless(os.name == "posix", "Unix denied-write capability witness")
    def test_actual_late_report_publication_failure_keeps_prefix_and_retries(self):
        self.fixture(blocked=True)
        (self.work / "generated").mkdir()
        self.write("blocked/root-report.csv", b"older report")
        self.write("generated/root.csv", b"older primary")
        result = self.build()
        before = self.retain(result, "late-build")
        self.remove_inputs()
        blocked = self.work / "blocked"
        blocked.chmod(0o500)
        self.addCleanup(blocked.chmod, 0o700)
        probe = blocked / "denied-write-probe"
        try:
            probe.write_bytes(b"actual permission probe")
        except PermissionError:
            pass
        else:
            probe.unlink()
            (self.evidence / "permission-control.txt").write_text(
                "host cannot deny writes; failure witness skipped\n"
            )
            self.skipTest("host permissions cannot deny this write")
        with self.assertRaises(yamaa.Error):
            result.save()
        failed = self.retain(result, "late-failure")
        self.assertEqual(self.execution(failed), self.execution(before))
        self.assertEqual(
            [entry["published"] for entry in failed["publication"]["entered"]],
            [True, True, False],
        )
        self.assertEqual(failed["publication"]["error"]["field"], "report")
        normative = result.issues.filter(pl.col("requirement") == "REQ-0764")
        self.assertEqual(normative.height, 1)
        context = json.loads(normative["context"][0])
        self.assertEqual(
            (
                context["node"],
                context["path"],
                context["entered"],
                context["completed"],
            ),
            (0, "blocked/root-report.csv", 3, 2),
        )
        self.assertEqual(len(failed["artifacts"]), 2)
        self.assertEqual((self.work / "generated/producer.csv").read_bytes(), CSV)
        self.assertEqual(
            (self.work / "generated/producer-report.csv").read_bytes(),
            golden("producer"),
        )
        self.assertEqual(
            (self.work / "generated/root.csv").read_bytes(), b"older primary"
        )
        self.assertEqual(
            (self.work / "blocked/root-report.csv").read_bytes(), b"older report"
        )
        blocked.chmod(0o700)
        result.save()
        self.assertTrue(result.issues.is_empty())
        retry = self.retain(result, "late-retry")
        self.assertEqual(self.execution(retry), self.execution(before))
        self.assertEqual(len(retry["artifacts"]), 4)
        self.assertEqual(
            (self.work / "blocked/root-report.csv").read_bytes(), golden("root")
        )


if __name__ == "__main__":
    unittest.main()
