"""Installed public producer builds against independently authored study truth."""

import gc
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import yamaa_benchmarks as program
from host_fixtures import load as load_truth

import yamaa
from yamaa import _locked_functions as host
from yamaa import _native as native

TRUTH = load_truth(__file__, "producer-graph")

ROOT = Path(__file__).parent / "project-original"
RAW = TRUTH["raw_csv"].encode("utf-8")
PRODUCED = TRUTH["produced_csv"].encode("utf-8")
CONSUMED = TRUTH["consumed_csv"].encode("utf-8")
PRODUCER = TRUTH["producer_yaml"]
CONSUMER = TRUTH["consumer_yaml"]
FUNCTION = TRUTH["function_yaml"].replace(
    "HOST_CALLABLE", "yamaa_benchmarks.project_value"
)
# The issue fields the authored findings state; geometry is not authored here.
FIELDS = ("phase", "condition", "requirement", "spec_paths", "context")


class PublicProducers(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        self.root = Path(work.name).resolve()
        self.write("yamaa-project.yaml", "version: '1.0'\n")
        self.write("producer/layers/base.yaml", PRODUCER)
        self.write("producer/p.yaml", TRUTH["producer_parent_yaml"])
        self.write("consumer/root.yaml", CONSUMER)
        self.write("raw/raw.csv", RAW)
        self.write(
            "env/environment.yaml",
            "schema_version: '1.0'\nlock: uv.lock\nfunctions: {id_float: float.yaml}\n",
        )
        self.write("env/float.yaml", FUNCTION)
        self.write(
            "env/uv.lock", (ROOT / "schema-functions/python/uv.lock").read_bytes()
        )
        # The project links the Python environment and lists the consumer,
        # whose `domain` names it; a producer its `schema` links reach is
        # not listed.
        self.write(
            "project.yaml",
            "schema_version: '1.0'\nenvironments: {python: env/environment.yaml}\n"
            "adam: {specs: [consumer/root.yaml]}\n",
        )

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value.encode() if isinstance(value, str) else value)

    def build(self, name="TEST"):
        return yamaa.project(self.root / "project.yaml").spec(name).run()

    def nodes(self, result):
        """The specification of every graph node the run entered, in order."""
        return [
            Path(node["specification"]).relative_to(self.root).as_posix()
            for node in json.loads(result._native.observations())["nodes"]
        ]

    def observe(self, result):
        text = result._native.observations()
        output = os.environ.get("YAMAA_PUBLIC_REPORT_DIR")
        if output:
            # Preserve the complete actual report and separately snapshot disk
            # targets; held artifact content alone cannot prove publication.
            self._witness_count = getattr(self, "_witness_count", 0) + 1
            directory = (
                Path(output)
                / "producer-witnesses/python"
                / self._testMethodName
                / f"{self._witness_count:02d}"
            )
            directory.mkdir(parents=True, exist_ok=True)
            (directory / "report.json").write_text(text, encoding="utf-8")
            saved = []
            for name in ("generated", "published"):
                for path in sorted((self.root / name).glob("*.csv")):
                    if path.is_file():
                        relative = path.relative_to(self.root)
                        target = directory / "targets" / relative
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(path.read_bytes())
                        saved.append(relative.as_posix())
            (directory / "witness.json").write_text(
                json.dumps(
                    {
                        "scope": "supplemental_producer_publication",
                        "truth": "python/tests/installed/installed_public_producers.py",
                        "test": self._testMethodName,
                        "report": "report.json",
                        "saved_targets": saved,
                    },
                    indent=2,
                )
                + "\n",
                encoding="utf-8",
            )
            provenance = os.environ.get("YAMAA_SUPPLEMENTAL_EVIDENCE")
            if provenance:
                (directory / "provenance.json").write_text(provenance, encoding="utf-8")
        return json.loads(text)

    def test_uneven_seven_to_one_graph_uses_both_artifact_profiles(self):
        # Independent source/count truth: seven authored records, one group,
        # then one producer record. Both native artifact routes retain seven and one.
        # REQ-0047: the copied producer count is an explicit scalar group key.
        self.write(
            "raw/raw.csv",
            "ID,VALUE\n" + "".join(f"1,{value}\n" for value in range(1, 8)),
        )
        for profile in ("csv", "parquet"):
            with self.subTest(profile=profile):
                self.write(
                    "producer/p.yaml",
                    f"""schema_version: '1.0'
domain: COUNTED
keys: [ID]
input: {{RAW: {{path: ../raw/raw.csv, types: {{ID: int, VALUE: int}}}}}}
rows:
  - id: counted
    dataset: RAW
    group_by: [RAW.ID]
    derivations: {{ID: RAW.ID, VALUE: {{aggregate: 'COUNT(RAW.*)'}}}}
columns:
  - {{name: ID, type: int, label: Identifier}}
  - {{name: VALUE, type: int, label: Count}}
output: {{path: ../generated/producer.{profile}, columns: [ID, VALUE]}}
""",
                )
                self.write(
                    "consumer/root.yaml",
                    f"""schema_version: '1.0'
domain: ROOT
keys: [ID]
input: {{FIRST: {{path: ../generated/producer.{profile}, schema: ../producer/p.yaml}}}}
rows:
  - id: counted
    dataset: FIRST
    group_by: [FIRST.ID, FIRST.VALUE]
    derivations: {{ID: FIRST.ID, SOURCE_COUNT: FIRST.VALUE, VALUE: {{aggregate: 'COUNT(FIRST.*)'}}}}
columns:
  - {{name: ID, type: int, label: Identifier}}
  - {{name: SOURCE_COUNT, type: int, label: Original source count}}
  - {{name: VALUE, type: int, label: Count}}
output: {{path: ../generated/root.csv, columns: [ID, SOURCE_COUNT, VALUE]}}
""",
                )
                result = yamaa.spec(self.root / "consumer/root.yaml").run()
                self.assertEqual(result.issues.height, 0)
                self.assertEqual(result.output.columns, ["ID", "SOURCE_COUNT", "VALUE"])
                self.assertEqual(result.output.rows(), [(1, 7, 1)])
                (self.root / "generated").mkdir(exist_ok=True)
                for _ in range(2):
                    result.save()
                    self.assertEqual(
                        (self.root / "generated/root.csv").read_bytes(),
                        b"ID,SOURCE_COUNT,VALUE\n1,7,1\n",
                    )
                if profile == "csv":
                    self.assertEqual(
                        (self.root / "generated/producer.csv").read_bytes(),
                        b"ID,VALUE\n1,7\n",
                    )

    def test_absent_project_graph_keeps_no_metadata_and_saves_held_artifacts(self):
        self.write(
            "producer/layers/base.yaml",
            PRODUCER.replace(TRUTH["identity_derivation"], "RAW.VALUE"),
        )
        # Neither file is discovered: a run without a project file has no
        # implementation environment (REQ-1331).
        self.write("env/environment.yaml", "[")
        self.write("project.yaml", "[")
        (self.root / "generated").mkdir()
        with (
            patch.object(
                host,
                "verify_versions",
                side_effect=AssertionError("unexpected activation"),
            ),
            patch.object(
                host,
                "resolve_callable",
                side_effect=AssertionError("unexpected binding"),
            ),
        ):
            result = yamaa.spec(self.root / "consumer/root.yaml").run()
            self.assertTrue(result.issues.is_empty(), result.issues)
            self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])
            observed = self.observe(result)
            self.assertEqual(
                observed["environment"], {"source": None, "lock_source": None}
            )
            self.assertEqual(observed["activation"]["lock"], "not_requested")
            # The only data the graph read is the producer's source.
            self.assertEqual(len(observed["source_reads"]), 1)
            self.assertFalse(
                any(read["path"].endswith(".yaml") for read in observed["source_reads"])
            )
            (self.root / "raw/raw.csv").unlink()
            self.assertTrue(
                yamaa.spec(self.root / "consumer/root.yaml").check().issues.is_empty()
            )
            for name in tuple(TRUTH["retained_specification_files"]):
                (self.root / name).unlink()
            for _ in range(2):
                result.save()
                self.assertEqual(
                    (self.root / "generated/producer.csv").read_bytes(), PRODUCED
                )
                self.assertEqual(
                    (self.root / "generated/root.csv").read_bytes(), CONSUMED
                )

    def test_untyped_date_selector_and_original_producer_filter_cause(self):
        producer = """schema_version: '1.0'
domain: ADSL
keys: [USUBJID]
input: {EX: {path: ../raw/ex.csv}}
base: EX
intermediates:
  - id: EXP
    dataset: EX
    derivations:
      EXSTDT: {to_date: {source: EXSTDTC}}
      EXENDT0: {to_date: {source: EXENDTC}}
      EXENDT: {first_available: {sources: [EXENDT0, EXSTDT]}}
    filter: EX.EXENDT >= EX.EXSTDT
    no_match: null
columns:
  - {name: USUBJID, type: str, label: Subject, derivation: EX.USUBJID}
  - {name: TRTSDT, type: date, label: First Dose Date, derivation: EXP.EXSTDT}
  - {name: TRTEDT, type: date, label: Last Dose Date, derivation: EXP.EXENDT}
output: {path: ../generated/producer.csv, columns: [USUBJID, TRTSDT, TRTEDT]}
"""
        self.write("producer/p.yaml", producer)
        self.write(
            "consumer/root.yaml",
            """schema_version: '1.0'
domain: ADEX
keys: [USUBJID]
input: {ADSL: {path: ../generated/producer.csv, schema: ../producer/p.yaml}}
base: ADSL
columns:
  - {name: USUBJID, type: str, label: Subject, derivation: ADSL.USUBJID}
  - {name: TRTEDT, type: date, label: Last Dose Date, derivation: ADSL.TRTEDT}
output: {path: ../generated/root.csv, columns: [USUBJID, TRTEDT]}
""",
        )
        self.write(
            "raw/ex.csv",
            "USUBJID,EXSTDTC,EXENDTC\nS01,2024-01-01,2024-01-10\nS02,2024-01-05,\n",
        )
        produced = b"USUBJID,TRTSDT,TRTEDT\nS01,2024-01-01,2024-01-10\nS02,2024-01-05,2024-01-05\n"
        consumed = b"USUBJID,TRTEDT\nS01,2024-01-10\nS02,2024-01-05\n"
        (self.root / "generated").mkdir()
        for name, expected in [
            ("producer/p.yaml", produced),
            ("consumer/root.yaml", consumed),
        ]:
            path = self.root / name
            self.assertTrue(yamaa.spec(path).check().issues.is_empty())
            result = yamaa.spec(path).run()
            self.assertTrue(result.issues.is_empty(), result.issues)
            self.assertIsNotNone(result.output)
            self.assertEqual(result.output.height, 2)
            result.save()
            target = "producer.csv" if name.startswith("producer/") else "root.csv"
            self.assertEqual((self.root / "generated" / target).read_bytes(), expected)
        self.assertEqual((self.root / "generated/producer.csv").read_bytes(), produced)

        # This comparison remains invalid even when selector inference is fixed.
        self.write(
            "producer/p.yaml",
            producer.replace("EX.EXENDT >= EX.EXSTDT", "EX.EXSTDT >= 'not-a-date'"),
        )
        prior = {
            "generated/producer.csv": b"previous producer bytes",
            "generated/root.csv": b"previous consumer bytes",
        }
        for name, content in prior.items():
            self.write(name, content)
        failed = yamaa.spec(self.root / "consumer/root.yaml").run()
        self.assertIsNone(failed.output)
        issues = failed.issues.to_dicts()
        self.assertEqual(len(issues), 1)
        actual = {key: issues[0][key] for key in FIELDS}
        actual["context"] = json.loads(actual["context"])
        self.assertEqual(
            actual,
            {
                "phase": "validation",
                "condition": "incompatible_input_type",
                "requirement": "REQ-0190",
                "spec_paths": ["input.ADSL.schema.intermediates[0].filter"],
                "context": {"left_type": "date", "right_type": "str"},
            },
        )
        for _ in range(2):
            with self.assertRaisesRegex(yamaa.Error, "cannot save a failed run"):
                failed.save()
            self.assertEqual(failed.issues.to_dicts(), issues)
            for name, content in prior.items():
                self.assertEqual((self.root / name).read_bytes(), content)

    def test_original_aliases_fresh_union_before_data_and_retained_public_views(self):
        self.write("raw/raw.csv", b"ID,VALUE\n1,99.99\n")
        events = []
        verify, resolve, original = (
            host.verify_versions,
            host.resolve_callable,
            program.project_value,
        )

        def versions(lock, calls):
            events.append(("lock", tuple(calls)))
            return verify(lock, calls)

        def binding(call, params):
            events.append(("bind", call))
            return resolve(call, params)

        def value(x):
            events.append(("invoke", x))
            if x == 0.0:
                self.write("raw/raw.csv", RAW)
            return original(x)

        with (
            patch.object(host, "verify_versions", versions),
            patch.object(host, "resolve_callable", binding),
            patch.object(program, "project_value", value),
        ):
            results = [self.build(), self.build()]
        self.assertEqual(
            events,
            [
                ("lock", ("yamaa_benchmarks.project_value",)),
                ("bind", "yamaa_benchmarks.project_value"),
                ("invoke", 7.25),
                ("invoke", 0.0),
                ("invoke", 1.234),
                ("invoke", 2.345),
            ]
            * 2,
        )
        self.assertFalse((self.root / "generated").exists())
        shutil.rmtree(self.root)
        gc.collect()
        for result in results:
            self.assertEqual(
                result.output.to_dicts(),
                [{"ID": 1, "VALUE": 2.46}, {"ID": 2, "VALUE": 4.70}],
            )
            self.assertTrue(result.issues.is_empty())
            # Every run returns its report, declared as a file or not (REQ-1173).
            run = result.report.row(0, named=True)
            self.assertEqual(
                (run["KIND"], run["OUTCOME"], run["COUNT"]), ("run", "succeeded", 2)
            )
            self.assertEqual(
                self.nodes(result), ["producer/p.yaml", "consumer/root.yaml"]
            )
            observed = self.observe(result)
            self.assertEqual(observed["outcome"], "success")
            # The producer read the source written during activation, not the
            # earlier 99.99 record.
            source = next(
                table
                for table in observed["tables"]
                if (table["name"], table["stage"]) == ("RAW", "source")
            )
            self.assertEqual(
                [row[1]["value"] for row in source["rows"]],
                [TRUTH["raw_first_f64"], TRUTH["raw_second_f64"]],
            )
            self.assertEqual(len(observed["source_reads"]), 1)
            self.assertEqual(
                [item["content"].encode() for item in observed["artifacts"]],
                [PRODUCED, CONSUMED],
            )
            self.assertEqual(observed["activation"]["lock"], "verified")
            self.assertEqual(
                [item["case"] for item in observed["activation"]["tests"]],
                ["normal", "boundary", "missing"],
            )
            self.assertFalse(observed["activation"]["tests"][-1]["invoked"])
            with self.assertRaises(yamaa.Error):
                result.save()
            after = self.observe(result)
            self.assertEqual(after.pop("publication")["entered"], [])
            self.assertEqual(after, observed)
            self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])

    def test_root_output_keeps_unrounded_values_and_projection_order(self):
        text = CONSUMER.replace("output: {path:", TRUTH["root_unrounded_column"])
        text = text.replace(
            "columns: [ID, VALUE], decimals: 2", TRUTH["root_unrounded_projection"]
        )
        self.write("consumer/root.yaml", text)
        result = self.build()
        self.assertTrue(result.issues.is_empty())
        self.assertEqual(result.output.columns, TRUTH["root_unrounded_columns"])
        self.assertEqual(result.output["ROOT_UNROUNDED"].to_list(), [2.464, 4.704])
        # Each node's held artifact, producer first.
        self.assertEqual(
            [
                (item["name"], item["content"].encode())
                for item in self.observe(result)["artifacts"]
            ],
            [
                ("producer", PRODUCED),
                ("root", TRUTH["root_unrounded_csv"].encode("utf-8")),
            ],
        )
        changed = result.output.with_columns(result.output["VALUE"] * 10)
        self.assertEqual(changed["VALUE"].to_list(), [24.6, 47.0])
        self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])

    def test_inherited_diamond_runs_shared_leaf_once_without_prospective_disk_inputs(
        self,
    ):
        bridge = TRUTH["retained_bridge_yaml"]
        self.write("bridge/left.yaml", bridge)
        self.write(
            "bridge/right.yaml",
            bridge.replace(
                "schema: ../producer/p.yaml", "schema: ../producer/./p.yaml"
            ).replace("generated/left.csv", "generated/right.csv"),
        )
        inherited = (
            CONSUMER.replace(", no_match: null", "")
            .replace(
                "../generated/producer.csv, schema: ../producer/p.yaml",
                TRUTH["nested_left_reference"],
            )
            .replace(
                "../generated/producer.csv, schema: ../producer/./p.yaml",
                TRUTH["nested_right_reference"],
            )
            .replace("path: ../generated/root.csv", "path: ../../generated/root.csv")
        )
        self.write("consumer/layers/base.yaml", inherited)
        self.write("consumer/root.yaml", TRUTH["root_parent_yaml"])
        calls = []
        original = program.project_value

        def value(x):
            calls.append(x)
            return original(x)

        with patch.object(program, "project_value", value):
            result = self.build("ROOT")
        self.assertEqual(calls, [7.25, 0.0, 1.234, 2.345])
        self.assertTrue(result.issues.is_empty())
        self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])
        # The shared leaf runs once, before both bridges and the root.
        self.assertEqual(
            self.nodes(result),
            [
                "producer/p.yaml",
                "bridge/left.yaml",
                "bridge/right.yaml",
                "consumer/root.yaml",
            ],
        )
        self.assertEqual(len(self.observe(result)["nodes"]), 4)
        self.assertFalse((self.root / "generated").exists())
        (self.root / "generated").mkdir()
        before = self.observe(result)
        self.assertEqual(len(before["source_reads"]), 1)
        # The project file is the one the caller named; the retired
        # yamaa-project.yaml beside it is never read.
        self.assertEqual(
            before["environment"]["source"], (self.root / "project.yaml").as_posix()
        )
        for directory in tuple(TRUTH["owned_bridge_directories"]):
            shutil.rmtree(self.root / directory)
        gc.collect()
        for _ in range(2):
            result.save()
            self.assertEqual(
                (self.root / "generated/producer.csv").read_bytes(), PRODUCED
            )
            self.assertEqual((self.root / "generated/left.csv").read_bytes(), PRODUCED)
            self.assertEqual((self.root / "generated/right.csv").read_bytes(), PRODUCED)
            self.assertEqual((self.root / "generated/root.csv").read_bytes(), CONSUMED)
            after = self.observe(result)
            self.assertEqual(
                after.pop("publication"),
                {
                    "status": "success",
                    "entered": [
                        {"node": node, "published": True} for node in (3, 1, 2, 0)
                    ],
                    "error": None,
                },
            )
            self.assertEqual(after, before)
            self.assertEqual(calls, [7.25, 0.0, 1.234, 2.345])
            self.assertTrue(result.issues.is_empty())
        shutil.rmtree(self.root)
        self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])

    def test_save_retains_rounded_alias_bytes_and_unrounded_root_after_originals_drop(
        self,
    ):
        text = CONSUMER.replace("output: {path:", TRUTH["root_unrounded_column"])
        text = text.replace(
            "columns: [ID, VALUE], decimals: 2", TRUTH["root_unrounded_projection"]
        )
        self.write("consumer/root.yaml", text)
        (self.root / "generated").mkdir()
        result = self.build()
        before = self.observe(result)
        self.assertEqual(list((self.root / "generated").iterdir()), [])
        self.write("raw/raw.csv", b"ID,VALUE\n1,99.99\n")
        for directory in tuple(TRUTH["owned_directories"]):
            shutil.rmtree(self.root / directory)
        gc.collect()
        with (
            patch.object(
                host,
                "verify_versions",
                side_effect=AssertionError("Save repeated activation"),
            ),
            patch.object(
                host,
                "resolve_callable",
                side_effect=AssertionError("Save rebound a callable"),
            ),
            patch.object(
                program,
                "project_value",
                side_effect=AssertionError("Save replayed a callback"),
            ),
        ):
            for _ in range(2):
                result.save()
                self.assertEqual(
                    (self.root / "generated/producer.csv").read_bytes(), PRODUCED
                )
                self.assertEqual(
                    (self.root / "generated/root.csv").read_bytes(),
                    TRUTH["root_unrounded_csv"].encode("utf-8"),
                )
                self.assertEqual(
                    result.output["ROOT_UNROUNDED"].to_list(), [2.464, 4.704]
                )
                after = self.observe(result)
                self.assertEqual(
                    after.pop("publication")["entered"],
                    [{"node": 1, "published": True}, {"node": 0, "published": True}],
                )
                self.assertEqual(after, before)

    def test_whole_set_preflight_refuses_invalid_late_target_without_producer_write(
        self,
    ):
        (self.root / "generated/root.csv").mkdir(parents=True)
        result = self.build()
        self.assertIsNotNone(result.output)
        with self.assertRaises(yamaa.Error):
            result.save()
        self.assertFalse((self.root / "generated/producer.csv").exists())
        self.assertEqual(self.observe(result)["publication"]["entered"], [])
        self.assertEqual(self.observe(result)["outcome"], "success")
        self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])
        (self.root / "generated/root.csv").rmdir()
        result.save()
        self.assertTrue(result.issues.is_empty())

    def test_actual_producer_or_consumer_save_failure_preserves_entered_prefix_and_prior_targets(
        self,
    ):
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace("../generated/root.csv", "../published/root.csv"),
        )
        for failed in ("producer", "consumer"):
            self.write("generated/producer.csv", b"previous producer bytes")
            self.write("published/root.csv", b"previous consumer bytes")
            result = self.build()
            before = self.observe(result)
            target = self.root / (
                "generated/producer.csv"
                if failed == "producer"
                else "published/root.csv"
            )
            readonly = target if os.name == "nt" else target.parent
            readonly.chmod(0o400 if os.name == "nt" else 0o500)
            try:
                if os.name != "nt":
                    probe = readonly / ".write-permission-probe"
                    try:
                        probe.write_bytes(b"permission probe")
                    except PermissionError:
                        pass
                    else:
                        probe.unlink()
                        self.skipTest(
                            "directory permissions cannot deny writes on this host"
                        )
                with self.assertRaises(yamaa.Error):
                    result.save()
                entered = (
                    [{"node": 1, "published": False}]
                    if failed == "producer"
                    else [
                        {"node": 1, "published": True},
                        {"node": 0, "published": False},
                    ]
                )
                self.assertEqual(
                    self.observe(result)["publication"]["entered"], entered
                )
                self.assertEqual(
                    target.read_bytes(),
                    b"previous producer bytes"
                    if failed == "producer"
                    else b"previous consumer bytes",
                )
                self.assertEqual(
                    (self.root / "generated/producer.csv").read_bytes(),
                    b"previous producer bytes" if failed == "producer" else PRODUCED,
                )
                self.assertEqual(result.output["VALUE"].to_list(), [2.46, 4.70])
                self.assertEqual(self.observe(result)["outcome"], "success")
                self.assertIn("REQ-0764", result.issues["requirement"].to_list())
                self.assertEqual(list(target.parent.glob("*.stage")), [])
            finally:
                readonly.chmod(0o600 if os.name == "nt" else 0o700)
            result.save()
            self.assertTrue(result.issues.is_empty())
            after = self.observe(result)
            after.pop("publication")
            self.assertEqual(after, before)

    @unittest.skipUnless(
        os.name == "nt", "Windows uses ordinal case-insensitive targets"
    )
    def test_absent_windows_case_alias_targets_refuse_every_write(self):
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace("../generated/root.csv", "../generated/PRODUCER.csv"),
        )
        (self.root / "generated").mkdir()
        result = self.build()
        self.assertIsNotNone(result.output)
        with self.assertRaises(yamaa.Error):
            result.save()
        self.assertEqual(list((self.root / "generated").iterdir()), [])
        self.assertEqual(self.observe(result)["publication"]["entered"], [])
        self.assertEqual(self.observe(result)["outcome"], "success")

    @unittest.skipUnless(sys.platform == "darwin", "macOS native parent case policy")
    def test_macos_absent_case_aliases_and_unproven_unicode_targets_preflight(self):
        (self.root / "generated").mkdir()
        self.write("generated/case-probe", b"native case witness")
        sensitive = not (self.root / "generated/CASE-PROBE").exists()
        (self.root / "generated/case-probe").unlink()
        for name in ("PRODUCER.csv", r"\u00e9.csv"):
            self.write(
                "consumer/root.yaml",
                CONSUMER.replace(
                    "../generated/root.csv", '"../generated/' + name + '"'
                ),
            )
            result = self.build()
            self.assertIsNotNone(result.output)
            self.assertEqual(self.observe(result)["outcome"], "success")
            if name == "PRODUCER.csv" and sensitive:
                result.save()
                self.assertEqual(
                    (self.root / "generated/producer.csv").read_bytes(), PRODUCED
                )
                self.assertEqual(
                    (self.root / "generated/PRODUCER.csv").read_bytes(), CONSUMED
                )
                for path in (self.root / "generated").iterdir():
                    path.unlink()
            else:
                with self.assertRaises(yamaa.Error):
                    result.save()
                self.assertEqual(self.observe(result)["publication"]["entered"], [])
                self.assertEqual(list((self.root / "generated").iterdir()), [])

    def test_consumer_only_bad_binding_precedes_all_study_and_runtime_effects(self):
        bad = (
            FUNCTION.replace("params: [{name: x", "params: [{name: z")
            .replace("args: {x:", "args: {z:")
            .replace("short-circuit-missing:x", "short-circuit-missing:z")
        )
        self.write("env/bad.yaml", bad)
        self.write(
            "env/environment.yaml",
            "schema_version: '1.0'\nlock: uv.lock\nfunctions: {id_float: float.yaml, bad: bad.yaml}\n",
        )
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace(TRUTH["sum_derivation"], TRUTH["bad_derivation"]),
        )
        shutil.rmtree(self.root / "raw")
        calls = []
        original = program.project_value

        def value(x):
            calls.append(x)
            return original(x)

        with patch.object(program, "project_value", value):
            result = self.build()
        self.assertEqual(calls, [])
        self.assertIsNone(result.output)
        self.assertEqual(self.nodes(result), [])
        observed = self.observe(result)
        self.assertEqual(observed["source_reads"], [])
        self.assertEqual(observed["activation"]["tests"], [])
        self.assertEqual(
            result.issues["condition"].to_list(), ["project_environment_invalid"]
        )
        self.assertEqual(result.issues["requirement"].to_list(), ["REQ-0695"])

    def test_original_host_failure_and_owned_actual_prefix_survive_file_deletion(self):
        original = RuntimeError("original public producer failure")
        real = program.project_value

        def value(x):
            if x == 1.234:
                raise original
            return real(x)

        with patch.object(program, "project_value", value):
            result = self.build()
        shutil.rmtree(self.root)
        gc.collect()
        self.assertIsNone(result.output)
        self.assertEqual(self.nodes(result), ["producer/p.yaml"])
        self.assertEqual(result.issues["condition"].to_list(), ["function_call_failed"])
        # The original host failure survives as the issue's facts.
        context = json.loads(result.issues["context"][0])
        self.assertEqual(context["host_error"], "RuntimeError")
        self.assertEqual(context["host_message"], str(original))
        observed = self.observe(result)
        self.assertEqual(observed["artifacts"], [])
        self.assertEqual(len(observed["nodes"]), 1)
        self.assertEqual(len(observed["source_reads"]), 1)
        with self.assertRaisesRegex(yamaa.Error, "cannot save a failed run"):
            result.save()

    def test_public_interrupt_propagates_original_and_retains_no_native_run(self):
        original = KeyboardInterrupt("original public graph interrupt")
        real = program.project_value

        def value(x):
            if x == 1.234:
                raise original
            return real(x)

        with patch.object(program, "project_value", value):
            with self.assertRaises(KeyboardInterrupt) as raised:
                self.build()
        self.assertIs(raised.exception, original)
        # No frame of the interrupt's traceback keeps the interrupted run.
        traceback = original.__traceback__
        while traceback is not None:
            self.assertFalse(
                any(
                    isinstance(local, native._Run)
                    for local in traceback.tb_frame.f_locals.values()
                )
            )
            traceback = traceback.tb_next
        self.assertFalse((self.root / "generated").exists())

    def test_long_exception_message_preserves_exact_facts_and_original_failure(
        self,
    ):
        message = "\u00e9\x00" * 30_000 + "complete producer failure"
        original = RuntimeError(message)
        real = program.project_value

        def value(x):
            if x == 1.234:
                raise original
            return real(x)

        with patch.object(program, "project_value", value):
            result = self.build()
        self.assertIsNone(result.output)
        self.assertEqual(self.nodes(result), ["producer/p.yaml"])
        self.assertEqual(result.issues["condition"].to_list(), ["function_call_failed"])
        context = json.loads(result.issues["context"][0])
        self.assertEqual(context["host_error"], "RuntimeError")
        self.assertEqual(context["host_message"], message)
        self.assertNotIn("host_details_truncated", context)

    def test_failed_consumer_verification_has_no_output_from_accepted_producer(self):
        self.write("consumer/root.yaml", CONSUMER + TRUTH["fatal_row_count_yaml"])
        result = self.build()
        self.assertIsNone(result.output)
        observed = self.observe(result)
        self.assertEqual(observed["outcome"], "failure")
        self.assertEqual(self.nodes(result), ["producer/p.yaml", "consumer/root.yaml"])
        # Only the accepted producer holds an artifact.
        self.assertEqual(
            [
                (item["name"], item["content"].encode())
                for item in observed["artifacts"]
            ],
            [("producer", PRODUCED)],
        )
        self.assertEqual(result.issues["condition"].to_list(), ["row_count_failed"])
        self.assertFalse((self.root / "generated").exists())
        shutil.rmtree(self.root)
        self.assertIsNone(result.output)
        with self.assertRaisesRegex(yamaa.Error, "cannot save a failed run"):
            result.save()

    def test_actual_ingestion_failure_stops_before_dependents(self):
        self.write("raw/raw.csv", b"ID,VALUE\n1,bad\n")
        result = self.build()
        self.assertIsNone(result.output)
        self.assertEqual(self.nodes(result), ["producer/p.yaml"])
        # The finding names the value the producer read from its source.
        self.assertEqual(json.loads(result.issues["context"][0])["value"], "bad")
        self.assertEqual(self.observe(result)["artifacts"], [])
        self.assertGreater(result.issues.height, 0)

    def assert_large_projection_admitted(self, result, ids, columns, expected_csv):
        # The fixed table geometry admits these cells and their held bytes.
        self.assertEqual(self.observe(result)["outcome"], "success")
        self.assertEqual(self.nodes(result), ["producer/p.yaml", "consumer/root.yaml"])
        # Admitted projection: no issue and the whole output.
        self.assertEqual(result.issues.height, 0)
        self.assertEqual(result.output.columns, columns)
        self.assertEqual(result.output["ID"].to_list(), ids)
        self.assertEqual(result.output["VALUE"].to_list(), [2.46] * len(ids))
        observed = self.observe(result)
        self.assertEqual(observed["outcome"], "success")
        self.assertIsNone(observed["error"])
        self.assertEqual(len(observed["artifacts"]), 2)
        produced = ("ID,VALUE\n" + "".join(f"{index},1.23\n" for index in ids)).encode()
        self.assertFalse((self.root / "generated").exists())
        held = result.output
        (self.root / "generated").mkdir()
        for directory in TRUTH["owned_directories"]:
            shutil.rmtree(self.root / directory)
        gc.collect()
        for _ in range(2):
            result.save()
            self.assertEqual(
                (self.root / "generated/root.csv").read_bytes(), expected_csv
            )
            self.assertEqual(
                (self.root / "generated/producer.csv").read_bytes(), produced
            )
            self.assertTrue(result.output.equals(held))
            self.assertEqual(self.observe(result)["outcome"], "success")
            self.assertTrue(result.issues.is_empty())

    def test_public_copy_bytes_admit_original_large_geometry_and_held_saves(self):
        payload = "x" * 2048
        extra = f'  - {{name: PAYLOAD, type: str, label: Payload, derivation: {{literal: "{payload}"}}}}\n'
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace("output: {path:", extra + "output: {path:").replace(
                "columns: [ID, VALUE], decimals: 2", TRUTH["payload_projection"]
            ),
        )
        ids = list(range(1, 4096))
        self.write(
            "raw/raw.csv",
            b"ID,VALUE\n" + "".join(f"{index},1.234\n" for index in ids).encode(),
        )
        result = self.build()
        self.assertIsNotNone(result.output)
        self.assertEqual(result.output["PAYLOAD"].to_list(), [payload] * len(ids))
        expected = (
            "ID,VALUE,PAYLOAD\n" + "".join(f"{index},2.46,{payload}\n" for index in ids)
        ).encode()
        self.assert_large_projection_admitted(
            result, ids, ["ID", "VALUE", "PAYLOAD"], expected
        )

    def test_public_copy_cells_admit_original_large_geometry_and_held_saves(self):
        self.write(
            "producer/layers/base.yaml",
            PRODUCER.replace("ID: int", "ID: str").replace(
                "name: ID, type: int", "name: ID, type: str"
            ),
        )
        extras = [f"EXTRA{index}" for index in range(59)]
        extra = "".join(
            f"  - {{name: {name}, type: str, label: Extra value, derivation: {{literal: null}}}}\n"
            for name in extras
        )
        columns = ["ID", "VALUE", *extras]
        names = ", ".join(columns)
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace("name: ID, type: int", "name: ID, type: str")
            .replace("output: {path:", extra + "output: {path:")
            .replace(
                "columns: [ID, VALUE], decimals: 2", f"columns: [{names}], decimals: 2"
            ),
        )
        ids = [str(index) for index in range(1, 23001)]
        # 23,000 records and 63 internal columns make 4,347,000 visits across
        # the three copies, now below the approved public default.
        self.write(
            "raw/raw.csv",
            b"ID,VALUE\n" + "".join(f"{index},1.234\n" for index in ids).encode(),
        )
        result = self.build()
        self.assertIsNotNone(result.output)
        for name in extras:
            self.assertEqual(result.output[name].to_list(), [None] * len(ids))
        expected = (
            ",".join(columns)
            + "\n"
            + "".join(f"{index},2.46" + "," * len(extras) + "\n" for index in ids)
        ).encode()
        self.assert_large_projection_admitted(result, ids, columns, expected)

    def test_invalid_original_producer_metadata_keeps_rejection_and_never_falls_back(
        self,
    ):
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace(
                "schema: ../producer/p.yaml", TRUTH["incompatible_reference"]
            ),
        )
        with (
            patch.object(host, "verify_versions") as verify,
            patch.object(program, "project_value") as invoked,
        ):
            result = self.build()
        verify.assert_not_called()
        invoked.assert_not_called()
        # Refused while prepared: no graph ran, so there is no report.
        self.assertIsNone(result._native.observations())
        self.assertIsNone(result.report)
        self.assertIn("REQ-0523", result.issues["requirement"].to_list())
        before = result.issues.to_dicts()
        shutil.rmtree(self.root)
        self.assertEqual(result.issues.to_dicts(), before)

    def test_early_environment_and_schema_null_failures_keep_original_facts(self):
        self.write("env/environment.yaml", b"schema_version: [\n")
        # The project reads its environment link, so the project itself
        # cannot be read and no specification is selected.
        with patch.object(host, "verify_versions") as verify:
            with self.assertRaises(yamaa.Error) as raised:
                self.build()
        verify.assert_not_called()
        issue = raised.exception.issues.row(0, named=True)
        self.assertEqual(issue["condition"], "invalid_yaml")
        context = json.loads(issue["context"])
        self.assertEqual(
            (context["environment_path"], context["path"]),
            ("environments.python", "env/environment.yaml"),
        )
        self.write(
            "env/environment.yaml",
            "schema_version: '1.0'\nlock: uv.lock\nfunctions: {id_float: float.yaml}\n",
        )
        self.write(
            "consumer/root.yaml",
            CONSUMER.replace("schema: ../producer/p.yaml", "schema: null"),
        )
        # A listing that cannot be read has no name to select, so the
        # refusal comes from the project run, where nothing ran to report.
        rejected = yamaa.project(self.root / "project.yaml").run()
        self.assertEqual(rejected.names, [])
        self.assertEqual(rejected.report.height, 0)
        self.assertIn("REQ-0287", rejected.issues["requirement"].to_list())
        before = rejected.issues.to_dicts()
        shutil.rmtree(self.root)
        self.assertEqual(rejected.issues.to_dicts(), before)


if __name__ == "__main__":
    unittest.main()
