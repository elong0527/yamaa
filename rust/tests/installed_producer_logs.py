"""Installed graph log capabilities and complete retained publication witnesses."""

import builtins
import copy
import gc
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import polars as pl
import yamaa

ROOT = Path(__file__).with_name("project-original")
SPECIFICATION = Path(__file__).with_name("specification-original")
IDS = [1, 2, 3, 4, 5, 6, 7, 9223372036854775807]
CSV = ("ID\n" + "\n".join(map(str, IDS)) + "\n").encode("ascii")
KEYS = "[" + ",".join('{"ID":' + str(value) + "}" for value in IDS) + "]"
VHEADER = "REPORT_VERSION,ARTIFACT,SPEC_PATH,VERIFICATION_ID,CHECK,TARGET,REQUIREMENT,SEVERITY,OUTCOME,CONDITION,EVALUATED_COUNT,FAILURE_COUNT,DETAILS\n"
WHEADER = "LOG_VERSION,ARTIFACT,SEVERITY,CONDITION,REQUIREMENT,SPEC_PATH,VERIFICATION_ID,FAILURE_COUNT,OFFENDING_KEYS,DETAILS\n"


def golden(name, warning=False, severity="warning"):
    """Independently authored range records, derived from the normative schema."""
    identifier = "\u00e9\U0001d11e" if name == "root" else name + "-range"
    details = '"{""column"":""ID""}"'
    if warning:
        keys = '"' + KEYS.replace('"', '""') + '"'
        row = f"1.0,generated/{name}.csv,warning,range_failed,REQ-0377,columns.ID.verifications[0].range,{identifier},8,{keys},{details}\n"
        return (WHEADER + row).encode("utf-8")
    row = f"1.0,generated/{name}.csv,columns.ID.verifications[0].range,{identifier},range,ID,REQ-0377,{severity},violated,range_failed,8,8,{details}\n"
    return (VHEADER + row).encode("utf-8")


class ProducerLogs(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name).resolve()
        self.write("yamaa-project.yaml", "version: '1.0'\n")
        self.write(
            "environment.yaml",
            "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions: {}\n",
        )
        self.write("uv.lock", (ROOT / "schema-functions/python/uv.lock").read_bytes())
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
        self.names = (
            ["producer", "left", "right", "root"] if diamond else ["producer", "root"]
        )
        for name in self.names:
            if name == "producer":
                source, base = "{SRC: {path: input.csv, types: {ID: int}}}", "SRC"
            elif name in {"left", "right"}:
                source, base = (
                    "{P: {path: generated/producer.csv, schema: producer.yaml}}",
                    "P",
                )
            elif diamond:
                source, base = (
                    "{A: {path: generated/left.csv, schema: left.yaml}, B: {path: generated/right.csv, schema: right.yaml}}",
                    "A",
                )
            else:
                source, base = (
                    "{A: {path: generated/producer.csv, schema: producer.yaml}, B: {path: generated/producer.csv, schema: ./producer.yaml}}",
                    "A",
                )
            intermediate = (
                "intermediates: [{id: SHARED, dataset: B, no_match: null}]\n"
                if name == "root"
                else ""
            )
            severity = "error" if name == fatal else "warning"
            warning = (
                "blocked/root-warnings.csv"
                if blocked and name == "root"
                else f"generated/{name}-warnings.csv"
            )
            identifier = r'"\u00e9\U0001d11e"' if name == "root" else name + "-range"
            self.write(
                name + ".yaml",
                f"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\nbase: {base}\ninput: {source}\n{intermediate}columns:\n"
                f"  - name: ID\n    type: int\n    label: Identifier\n    derivation: {base}.ID\n"
                f"    verifications: [{{range: {{max: 0, severity: {severity}, id: {identifier}}}}}]\n"
                f"output: {{path: generated/{name}.csv, columns: [ID], verification_log: generated/{name}-checks.csv, warning_log: {warning}}}\n",
            )
        self.paths = [
            path
            for name in self.names
            for path in (
                f"generated/{name}-checks.csv",
                "blocked/root-warnings.csv"
                if blocked and name == "root"
                else f"generated/{name}-warnings.csv",
                f"generated/{name}.csv",
            )
        ]

    def build(self):
        return yamaa.domain(
            self.work / "root.yaml", environment=self.work / "environment.yaml"
        )

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

    def test_alias_and_diamond_logs_use_actual_owned_views_and_repeat_held_bytes(self):
        for diamond in (False, True):
            with self.subTest(diamond=diamond):
                self.fixture(diamond=diamond)
                (self.work / "generated").mkdir(exist_ok=True)
                result = self.build()
                self.assertTrue(result.issues.is_empty(), result.issues)
                self.assertEqual(
                    result.output.to_dicts(), [{"ID": value} for value in IDS]
                )
                verification, warning = result.verification_log, result.warning_log
                self.assertEqual(
                    verification["VERIFICATION_ID"].to_list(), ["\u00e9\U0001d11e"]
                )
                self.assertEqual(verification["EVALUATED_COUNT"].to_list(), [8])
                self.assertEqual(warning["FAILURE_COUNT"].to_list(), [8])
                self.assertEqual(warning["OFFENDING_KEYS"].to_list(), [KEYS])
                self.assertEqual(warning.schema["FAILURE_COUNT"], pl.Int64)
                self.assertEqual(verification.schema["EVALUATED_COUNT"], pl.Int64)
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
                verification.replace_column(0, pl.Series("REPORT_VERSION", ["mutated"]))
                self.assertNotEqual(
                    result.verification_log["REPORT_VERSION"].to_list(), ["mutated"]
                )
                self.remove_inputs()
                gc.collect()
                held = None
                for attempt in (1, 2):
                    self.assertTrue(result.save())
                    report = self.retain(result, f"save-{diamond}-{attempt}")
                    self.assertEqual(self.execution(report), self.execution(before))
                    entries = report["publication"]["entered"]
                    order = (
                        ["producer", "left", "right", "root"]
                        if diamond
                        else ["producer", "root"]
                    )
                    expected = [
                        f"generated/{name}{suffix}.csv"
                        for name in order
                        for suffix in ("-checks", "-warnings", "")
                    ]
                    self.assertEqual([entry["path"] for entry in entries], expected)
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
                                actual[path], golden(name, "-warnings" in path)
                            )
                    if held is not None:
                        self.assertEqual(actual, held)
                    held = actual
                for path in self.paths:
                    (self.work / path).unlink()

    def test_failed_leaf_and_dependent_publish_only_completed_verification_prefix(self):
        for fatal in ("producer", "root"):
            self.fixture(fatal=fatal)
            result = self.build()
            self.assertIsNone(result.output)
            self.assertIsNone(result.warning_log)
            self.assertEqual(result.verification_log is None, fatal == "producer")
            issues = result.issues.clone()
            self.assertEqual(issues["condition"].to_list(), ["range_failed"])
            before = self.retain(result, "failed-build-" + fatal)
            for path in self.paths:
                self.write(path, b"earlier sentinel")
            self.remove_inputs()
            for attempt in (1, 2):
                with self.assertRaises(yamaa.DomainError):
                    result.save()
                report = self.retain(result, f"failed-save-{fatal}-{attempt}")
                self.assertTrue(result.issues.equals(issues))
                self.assertEqual(self.execution(report), self.execution(before))
                self.assertEqual(report["publication"]["mode"], "diagnostic")
                self.assertEqual(
                    [entry["field"] for entry in report["publication"]["entered"]],
                    ["verification_log"] * (1 if fatal == "producer" else 2),
                )
                for path in self.paths:
                    expected_log = path.endswith("-checks.csv") and (
                        fatal == "root" or "producer" in path
                    )
                    if expected_log:
                        name = Path(path).name.split("-")[0]
                        self.assertEqual(
                            (self.work / path).read_bytes(),
                            golden(
                                name, severity="error" if name == fatal else "warning"
                            ),
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
        self.assertFalse(result.save())
        refused = self.retain(result, "parent-refused")
        self.assertEqual(refused["publication"]["entered"], [])
        self.assertEqual(self.execution(refused), self.execution(before))
        (self.work / "generated").mkdir()
        self.assertTrue(result.save())
        self.assertTrue(result.issues.is_empty())
        saved = self.retain(result, "parent-retry")
        self.assertEqual(len(saved["artifacts"]), 6)
        self.assertEqual(self.execution(saved), self.execution(before))

    def test_unchanged_original_log_document_and_goldens_run_as_shared_producer(self):
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
                "  A: {path: adsl.csv, schema: producer.yaml}\n  B: {path: adsl.csv, schema: ./producer.yaml}",
            )
            .replace("DM.", "A.")
        )
        self.write("root.yaml", consumer)
        self.paths = [
            "adsl-checks.csv",
            "adsl-warnings.csv",
            "adsl.csv",
            "root-checks.csv",
            "root-warnings.csv",
            "root.csv",
        ]
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
            name: (source / "expected" / name).read_bytes() for name in self.paths[:3]
        }
        expected.update(
            {
                name.replace("adsl", "root"): content.replace(b"adsl.csv", b"root.csv")
                for name, content in tuple(expected.items())
            }
        )
        for attempt in (1, 2):
            self.assertTrue(result.save())
            report = self.retain(result, f"original-save-{attempt}")
            self.assertEqual(self.execution(report), self.execution(before))
            self.assertEqual(
                [entry["path"] for entry in report["publication"]["entered"]],
                self.paths,
            )
            self.assertEqual(
                [entry["published"] for entry in report["publication"]["entered"]],
                [True] * 6,
            )
            for name, content in expected.items():
                self.assertEqual((self.work / name).read_bytes(), content)

    def test_lexical_collisions_and_invalid_sidecars_precede_study_access(self):
        for replacement in (
            "generated/../generated/producer.csv",
            "generated/root-checks.arrow",
        ):
            self.fixture()
            entry = self.work / "root.yaml"
            entry.write_text(
                entry.read_text().replace("generated/root-checks.csv", replacement)
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
            with self.assertRaises(yamaa.DomainError):
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
        for name in ("generated/PRODUCER-checks.csv", "GENERATED/producer-checks.csv"):
            self.fixture()
            entry = self.work / "root.yaml"
            entry.write_text(
                entry.read_text().replace("generated/root-checks.csv", name)
            )
            self.assertFalse((self.work / "generated/producer-checks.csv").exists())
            if name.startswith("GENERATED"):
                self.assertTrue(
                    (self.work / "GENERATED").samefile(self.work / "generated")
                )
            result = self.build()
            if os.name == "nt":
                self.assertIsNone(result.output)
                self.assertFalse(result.issues.is_empty())
                with self.assertRaises(yamaa.DomainError):
                    result.save()
            else:
                self.assertIsNotNone(result.output)
                self.assertFalse(result.save())
                phase = (
                    "case-parent-alias"
                    if name.startswith("GENERATED")
                    else "case-entry-alias"
                )
                report = self.retain(result, phase)
                self.assertEqual(report["publication"]["entered"], [])
                self.assertEqual(
                    report["publication"]["error"]["field"], "verification_log"
                )
            self.assertEqual(list((self.work / "generated").iterdir()), [])

    @unittest.skipUnless(os.name == "posix", "Unix denied-write capability witness")
    def test_actual_late_warning_publication_failure_keeps_prefix_and_retries(self):
        self.fixture(blocked=True)
        (self.work / "generated").mkdir()
        self.write("blocked/root-warnings.csv", b"older warning")
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
        self.assertFalse(result.save())
        failed = self.retain(result, "late-failure")
        self.assertEqual(self.execution(failed), self.execution(before))
        self.assertEqual(
            [entry["published"] for entry in failed["publication"]["entered"]],
            [True, True, True, True, False],
        )
        self.assertEqual(failed["publication"]["error"]["field"], "warning_log")
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
            (0, "blocked/root-warnings.csv", 5, 4),
        )
        self.assertEqual(len(failed["artifacts"]), 4)
        self.assertEqual((self.work / "generated/producer.csv").read_bytes(), CSV)
        self.assertEqual(
            (self.work / "generated/producer-checks.csv").read_bytes(),
            golden("producer"),
        )
        self.assertEqual(
            (self.work / "generated/producer-warnings.csv").read_bytes(),
            golden("producer", True),
        )
        self.assertEqual(
            (self.work / "generated/root-checks.csv").read_bytes(), golden("root")
        )
        self.assertEqual(
            (self.work / "generated/root.csv").read_bytes(), b"older primary"
        )
        self.assertEqual(
            (self.work / "blocked/root-warnings.csv").read_bytes(), b"older warning"
        )
        blocked.chmod(0o700)
        self.assertTrue(result.save())
        self.assertTrue(result.issues.is_empty())
        retry = self.retain(result, "late-retry")
        self.assertEqual(self.execution(retry), self.execution(before))
        self.assertEqual(len(retry["artifacts"]), 6)


if __name__ == "__main__":
    unittest.main()
