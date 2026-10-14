"""Authored API/fragment controls; these fixtures never claim hosted qualification."""

import contextlib
import copy
import importlib.util
import sys
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import test_audit_public_performance as fixtures

auditor, write = fixtures.auditor, fixtures.write

with mock.patch.object(sys, "path", [str(Path(auditor.__file__).parent), *sys.path]):
    SPEC = importlib.util.spec_from_file_location(
        "performance_aggregate",
        Path(auditor.__file__).with_name("aggregate_public_performance.py"),
    )
    aggregate = importlib.util.module_from_spec(SPEC)
    SPEC.loader.exec_module(aggregate)
aggregate.auditor = auditor


class BoundedReaders(unittest.TestCase):
    def setUp(self):
        fixtures.CollectionGates.setUp(self)
        self.run.update(status="completed", conclusion="success")
        write(self.root / "run.json", self.run)
        jobs = auditor.read(self.root / "jobs.json")
        for number, job in enumerate(jobs):
            job["id"] = number + 100
        write(self.root / "jobs.json", jobs)
        write(self.root / "input.json", self.payload)
        self.catalog = auditor.replay_catalog(self.root / "input.json")
        self.reader = {
            "revision": "c" * 40,
            "repository": self.payload["repository"],
            "run_id": 500,
            "run_attempt": 3,
        }
        self.reader_run = dict(
            self.run,
            id=500,
            run_attempt=3,
            head_sha="c" * 40,
            path=auditor.READER_WORKFLOW,
            event="workflow_run",
            status="in_progress",
            conclusion=None,
            created_at="2026-10-14T02:00:00Z",
        )
        self.make_fragments()

    def make_fragments(self):
        self.fragment_entries, self.reader_jobs, self.reader_artifacts = [], [], []
        self.fragments = []
        for number, unit in enumerate(self.catalog["units"]):
            with (
                mock.patch.object(auditor, "reader_identity", return_value=self.reader),
                mock.patch.object(
                    auditor, "admit_collection", side_effect=self.admitted_values
                ),
            ):
                fragment = auditor.read_unit(
                    self.root / "input.json", self.root, unit["selector"]
                )
            self.fragments.append(fragment)
            name = "performance-unit-" + unit["key"]
            directory = self.root / name
            directory.mkdir()
            write(directory / "performance-unit-audit.json", fragment)
            self.fragment_entries.append(
                {
                    "selector": unit["selector"],
                    "artifact_id": number + 5000,
                    "artifact_name": name,
                }
            )
            self.reader_jobs.append(
                {
                    "id": number + 9000,
                    "name": "performance-replay-unit (" + unit["key"] + ")",
                    "run_id": 500,
                    "run_attempt": 3,
                    "head_sha": "c" * 40,
                    "status": "completed",
                    "conclusion": "success",
                    "started_at": "2026-10-14T02:01:00Z",
                    "completed_at": "2026-10-14T02:20:00Z",
                }
            )
            self.reader_artifacts.append(
                {
                    "id": number + 5000,
                    "name": name,
                    "expired": False,
                    "size_in_bytes": 100,
                    "workflow_run": {"id": 500, "head_sha": "c" * 40},
                    "created_at": "2026-10-14T02:10:00Z",
                }
            )
        self.aggregate_input = {
            "protocol": aggregate.PROTOCOL,
            "original_manifest": "input.json",
            "reader_run": "reader-run.json",
            "reader_jobs": "reader-jobs.json",
            "reader_artifacts": "reader-artifacts.json",
            "fragments": self.fragment_entries,
        }
        self.persist()

    def admitted_values(self, directory, lane, role, source, *_):
        selected = [
            cases
            for variant, cases in auditor.variants().items()
            if directory.parent.name.endswith("-" + variant)
        ]
        boundaries = [
            boundary
            for boundary in auditor.lifecycle.BOUNDARIES[1:]
            if role != "producer-root-build-baseline"
            or not boundary.startswith("held_save_")
        ]
        names = [
            f"{mode}.{boundary}.median_ns"
            for mode in ("cold", "warm")
            for boundary in boundaries
        ] + [
            "cold.package_import.median_ns",
            "cold.child_peak_rss.median_bytes",
            "warm.child_peak_rss.median_bytes",
        ]
        metrics = {}
        for name in names:
            count = (
                1
                if name.startswith("warm.child")
                else 10
                if name.startswith("cold.")
                else 30
            )
            raw = [1000 if "child_peak_rss" in name else 10] * count
            unit = "bytes" if "child_peak_rss" in name else "nanoseconds"
            metrics[name] = (
                auditor.stats(raw, unit)
                if count > 1
                else {
                    "count": 1,
                    "raw": raw,
                    "unit": unit,
                    "median": raw[0],
                    "min": raw[0],
                    "max": raw[0],
                    "quartiles": None,
                }
            )
        return {
            case: {
                "status": "pass",
                "metrics": copy.deepcopy(metrics),
                "unavailable": list(auditor.lifecycle.unavailable(False))
                if role == "candidate"
                else [],
                "actual_function_calls": [],
                "actual_callback_counts": [23]
                if role == "candidate" and case.startswith("callback-")
                else [],
                "measurement_host": lane["host"] if role == "candidate" else "python",
                "source_revision": source if role == "candidate" else "b" * 40,
            }
            for case in auditor.role_case_names(role)
            if not selected or case in selected[0]
        }

    def persist(self):
        for filename, value in (
            ("reader-run.json", self.reader_run),
            ("reader-jobs.json", self.reader_jobs),
            ("reader-artifacts.json", self.reader_artifacts),
            ("aggregate.json", self.aggregate_input),
        ):
            write(self.root / filename, value)

    def save_fragment(self, index=0):
        entry = self.fragment_entries[index]
        write(
            self.root / entry["artifact_name"] / "performance-unit-audit.json",
            self.fragments[index],
        )

    def report(self, budgets=None, revision="c" * 40):
        self.persist()
        with mock.patch.object(
            auditor, "reader_identity", return_value={"revision": "d" * 40}
        ):
            return aggregate.aggregate(self.root / "aggregate.json", revision, budgets)

    def approved_budgets(self):
        schema = auditor.read(Path(auditor.__file__).with_suffix(".schema.json"))
        value = {
            "protocol": "public-performance-budgets/1",
            "status": "approved",
            "source_revision": "a" * 40,
            "decision": {
                "issue": 1973,
                "item": 10,
                "reference": "authored fixture only; no release decision",
            },
            "rules": [
                {
                    "tuple": "*",
                    "case": "*",
                    "metric": name,
                    "comparison": "absolute",
                    "limit": 100000,
                }
                for name in schema["$defs"]["rule"]["properties"]["metric"]["enum"]
            ],
            "dispositions": [
                {
                    "tuple": "*",
                    "case": "*",
                    "metric": "*",
                    "decision": "incomparable",
                    "reason": "authored protocol fixture",
                    "reference": "fixture only",
                }
            ],
        }
        path = self.root / "budgets.json"
        write(path, value)
        return path

    def test_whole_catalog_and_complete_unit_preserve_exact_original_roles(self):
        self.assertEqual(len(self.catalog["units"]), 21)
        self.assertEqual(
            sum(
                len(unit["cases_by_role"]["candidate"])
                for unit in self.catalog["units"]
            ),
            567,
        )
        self.assertEqual(
            self.fragments[0]["unit_coverage"],
            {"required": 51, "passed": 51, "missing_or_failed": 0},
        )
        self.assertEqual(self.fragments[0]["reader"]["revision"], "c" * 40)
        self.assertEqual(self.fragments[0]["original"]["source_revision"], "a" * 40)
        self.assertFalse(self.fragments[0]["accepted"])
        self.assertEqual(self.fragments[0]["budgets"], {"status": "not_evaluated"})

    def test_segmentation_catalog_requires_exact45_units_with_three_sizes(self):
        fixtures.Segments.setUp(self)
        self.run.update(status="completed", conclusion="success")
        write(self.root / "run.json", self.run)
        write(self.root / "input.json", self.payload)
        catalog = auditor.replay_catalog(self.root / "input.json")
        self.catalog = catalog
        self.make_fragments()
        report = self.report()
        self.assertEqual(report["collection_gate"]["status"], "pass")
        self.assertEqual(report["coverage"]["passed"], 567)
        self.assertEqual(report["comparator_coverage"]["passed"], 504)
        self.assertEqual(len(catalog["units"]), 45)
        self.assertEqual(
            sum(
                sum(len(cases) for cases in unit["cases_by_role"].values())
                for unit in catalog["units"]
            ),
            1071,
        )
        for unit in catalog["units"][18:]:
            self.assertEqual(len(unit["cases_by_role"]["candidate"]), 3)

    def test_selector_cannot_override_source_paths_cases_or_drop_original_inventory(
        self,
    ):
        selector = self.catalog["units"][0]["selector"]
        for key, value in (
            ("source_revision", "c" * 40),
            ("cases", []),
            ("path", "other"),
        ):
            with (
                self.subTest(key=key),
                self.assertRaisesRegex(ValueError, "selector.*fields"),
            ):
                auditor.unit_key(dict(selector, **{key: value}))
        self.payload["entries"].pop()
        write(self.root / "input.json", self.payload)
        with self.assertRaisesRegex(ValueError, "missing original logical tuple"):
            auditor.replay_catalog(self.root / "input.json")

    def test_python_segments_keep189_complete_units_and567_504_unpooled_cells(self):
        import jsonschema

        fixtures.Segments.setUp(self, include_python=True)
        self.run.update(status="completed", conclusion="success")
        write(self.root / "run.json", self.run)
        write(self.root / "input.json", self.payload)
        self.catalog = auditor.replay_catalog(self.root / "input.json")
        self.assertEqual(len(self.catalog["units"]), 189)
        self.assertEqual(len({unit["key"] for unit in self.catalog["units"]}), 189)
        self.assertEqual(
            sum(
                len(unit["cases_by_role"]["candidate"])
                for unit in self.catalog["units"]
            ),
            567,
        )
        self.assertEqual(
            sum(
                len(cases)
                for unit in self.catalog["units"]
                for role, cases in unit["cases_by_role"].items()
                if role != "candidate"
            ),
            504,
        )
        for unit in self.catalog["units"]:
            self.assertEqual(len(unit["cases_by_role"]["candidate"]), 3)
            self.assertEqual(
                [case.split("-")[1] for case in unit["cases_by_role"]["candidate"]],
                ["1000", "10000", "100000"],
            )
        self.make_fragments()
        report = self.report()
        self.assertEqual(report["collection_gate"]["status"], "pass")
        self.assertEqual(report["coverage"]["passed"], 567)
        self.assertEqual(report["comparator_coverage"]["passed"], 504)
        self.assertEqual(len(report["reader_job_admission"]["admitted_units"]), 189)
        for cell in report["cells"]:
            self.assertEqual(
                cell["candidate"]["metrics"]["warm.child_peak_rss.median_bytes"][
                    "count"
                ],
                1,
            )
            association = cell["candidate"]["performance_collection_association"]
            self.assertIn(cell["case"], auditor.variants()[association["variant"]])
            self.assertEqual(
                association["runner_name"],
                f"original-runner-{association['artifact_id']}",
            )
        self.assertFalse(report["accepted"])
        validator = jsonschema.Draft202012Validator(
            auditor.read(Path(auditor.__file__).with_suffix(".schema.json"))
        )
        for value in (
            self.payload,
            self.catalog,
            self.fragments[0],
            self.aggregate_input,
        ):
            validator.validate(value)
        self.fragment_entries.append(copy.deepcopy(self.fragment_entries[0]))
        self.assertEqual(self.report()["collection_gate"]["status"], "fail")
        self.fragment_entries.pop()
        self.fragment_entries.pop(0)
        self.assertEqual(self.report()["coverage"]["missing"], 3)

    def test_python_segment_cases_roles_and_actual_producing_attempt_are_strict(self):
        fixtures.Segments.setUp(self, include_python=True)
        self.run.update(status="completed", conclusion="success")
        write(self.root / "run.json", self.run)
        original = copy.deepcopy(self.payload["entries"][0])
        mutations = [
            lambda entry: entry["segments"].pop(),
            lambda entry: entry["segments"].append(copy.deepcopy(entry["segments"][0])),
            lambda entry: entry["segments"][0].update(variant="lookup-2"),
            lambda entry: entry["segments"][0]["cases"].pop(),
            lambda entry: entry["segments"][1].update(
                cases=entry["segments"][0]["cases"]
            ),
            lambda entry: entry["segments"][0].update(run_attempt=1),
            lambda entry: entry["segments"][0]["collections"].pop(),
        ]
        for mutate in mutations:
            self.payload["entries"][0] = copy.deepcopy(original)
            mutate(self.payload["entries"][0])
            write(self.root / "input.json", self.payload)
            with self.assertRaises(ValueError):
                auditor.replay_catalog(self.root / "input.json")
        self.payload["entries"][0] = original
        write(self.root / "input.json", self.payload)
        jobs = auditor.read(self.root / "jobs.json")
        good = copy.deepcopy(jobs[0])
        for changes in (
            {"run_attempt": 1},
            {"conclusion": "failure"},
            {"conclusion": "cancelled"},
            {"head_sha": "b" * 40},
            {"name": "performance-python (ubuntu-24.04, 3.12, wheel)"},
        ):
            jobs[0] = dict(good, **changes)
            write(self.root / "jobs.json", jobs)
            with self.assertRaises(ValueError):
                auditor.replay_catalog(self.root / "input.json")

    def test_unit_retains_failed_original_raw_and_cannot_skip_missing_baseline(self):
        def failed(*args):
            values = self.admitted_values(*args)
            if args[2] == "baseline":
                values[next(iter(values))] = {
                    "status": "fail",
                    "reason": "original raw child failure",
                }
            return values

        with (
            mock.patch.object(auditor, "reader_identity", return_value=self.reader),
            mock.patch.object(auditor, "admit_collection", side_effect=failed),
        ):
            fragment = auditor.read_unit(
                self.root / "input.json",
                self.root,
                self.catalog["units"][0]["selector"],
            )
        self.assertEqual(fragment["unit_gate"]["status"], "fail")
        self.assertEqual(fragment["unit_coverage"]["missing_or_failed"], 1)
        self.assertFalse(fragment["accepted"])

    def test_authentic_metadata_complete567_504_only_collection_passes(self):
        report = self.report()
        self.assertEqual(report["collection_gate"]["status"], "pass")
        self.assertEqual(report["coverage"]["passed"], 567)
        self.assertEqual(report["comparator_coverage"]["passed"], 504)
        self.assertEqual(report["reader_job_admission"]["status"], "pass")
        self.assertEqual(report["reader_completion"]["status"], "incomplete")
        self.assertEqual(report["aggregate_revision"], "d" * 40)
        self.assertFalse(report["accepted"])
        self.assertEqual(report["migration_acceptance"]["status"], "pending")

    def test_versioned_schema_admits_exact_catalog_unit_fragment_and_aggregate(self):
        import jsonschema

        schema = auditor.read(Path(auditor.__file__).with_suffix(".schema.json"))
        jsonschema.Draft202012Validator.check_schema(schema)
        validator = jsonschema.Draft202012Validator(schema)
        for value in (
            self.catalog,
            self.catalog["units"][0]["selector"],
            self.fragments[0],
            self.aggregate_input,
        ):
            with self.subTest(protocol=value["protocol"]):
                validator.validate(value)
        with self.assertRaises(jsonschema.ValidationError):
            validator.validate(
                dict(self.catalog["units"][0]["selector"], source_revision="a" * 40)
            )

    def test_later_numeric_acceptance_needs_separate_exact_reader_completion(self):
        original_reader = (self.root / "reader-run.json").read_bytes()
        original_source = (self.root / "run.json").read_bytes()
        budgets = self.approved_budgets()
        self.assertEqual(self.report(budgets)["budgets"]["status"], "pass")
        self.assertFalse(self.report(budgets)["accepted"])
        self.aggregate_input["reader_completion_run"] = "reader-completion-run.json"
        completion = dict(self.reader_run, status="completed", conclusion="success")
        write(self.root / "reader-completion-run.json", completion)
        report = self.report(budgets)
        self.assertTrue(report["accepted"])
        self.assertEqual(report["aggregate_revision"], "d" * 40)
        self.assertEqual(report["reader_job_admission"]["reviewed_revision"], "c" * 40)
        self.assertEqual((self.root / "reader-run.json").read_bytes(), original_reader)
        self.assertEqual((self.root / "run.json").read_bytes(), original_source)
        for mutation in (
            {"run_attempt": 4},
            {"head_sha": "d" * 40},
            {"created_at": "2026-10-14T02:01:00Z"},
            {"status": "in_progress", "conclusion": None},
            {"conclusion": "failure"},
            {"conclusion": "cancelled"},
        ):
            write(
                self.root / "reader-completion-run.json", dict(completion, **mutation)
            )
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.report(budgets)

    def test_failed_original_or_reader_attempt_cannot_be_rescued_by_fragments(self):
        for state in ("failure", "cancelled"):
            self.reader_run.update(status="completed", conclusion=state)
            with (
                self.subTest(reader=state),
                self.assertRaisesRegex(ValueError, "reader attempt"),
            ):
                self.report()
        self.reader_run.update(status="in_progress", conclusion=None)
        for state in ("failure", "cancelled", None):
            write(self.root / "run.json", dict(self.run, conclusion=state))
            with self.subTest(source=state), self.assertRaises(ValueError):
                self.report()
        write(self.root / "run.json", self.run)
        with self.assertRaisesRegex(ValueError, "reviewed source"):
            self.report(revision="e" * 40)

    def test_only_actual_reviewed_workflow_events_and_source_branches_admit(self):
        original = copy.deepcopy(self.reader_run)
        for event, branch in (
            ("workflow_dispatch", "main"),
            ("workflow_call", "main"),
            ("workflow_run", "untrusted"),
            ("push", "main"),
            ("push", "evidence/other"),
        ):
            self.reader_run.update(event=event, head_branch=branch)
            with (
                self.subTest(event=event, branch=branch),
                self.assertRaises(ValueError),
            ):
                self.report()
        self.reader_run.update(event="push", head_branch="evidence/issue-1998-replay")
        self.assertEqual(self.report()["collection_gate"]["status"], "pass")
        self.reader_run = original

    def test_failed_wrong_attempt_and_unproven_reader_job_cannot_supply_metrics(self):
        original = self.reader_jobs[0]
        for mutation in (
            {"status": "in_progress"},
            {"conclusion": "failure"},
            {"conclusion": "cancelled"},
            {"run_attempt": 2},
            {"head_sha": "a" * 40},
            {"id": True},
            {"name": "unreviewed-local-reader"},
        ):
            self.reader_jobs[0] = dict(original, **mutation)
            with self.subTest(mutation=mutation):
                report = self.report()
                self.assertEqual(report["reader_job_admission"]["status"], "fail")
                self.assertEqual(report["coverage"]["missing"], 27)
                self.assertFalse(report["accepted"])
        self.reader_jobs[0] = original

    def test_artifact_id_name_expiry_size_and_producing_interval_bind(self):
        original = self.reader_artifacts[0]
        for mutation in (
            {"id": 999},
            {"name": "arbitrary-fragment"},
            {"expired": True},
            {"expired": 0},
            {"size_in_bytes": True},
            {"size_in_bytes": 0},
            {"created_at": "2026-10-14T01:00:00Z"},
            {"created_at": "2026-10-14T02:10:00"},
            {"workflow_run": {"id": 501, "head_sha": "c" * 40}},
        ):
            self.reader_artifacts[0] = dict(original, **mutation)
            with self.subTest(mutation=mutation):
                self.assertEqual(self.report()["collection_gate"]["status"], "fail")
        self.reader_artifacts[0] = original

    def test_same_named_old_attempt_artifact_remains_retained_but_cannot_substitute(
        self,
    ):
        selected = self.reader_artifacts[0]
        previous = dict(selected, id=6000, created_at="2026-10-14T00:10:00Z")
        self.reader_artifacts.append(previous)
        original_snapshot = copy.deepcopy(self.reader_artifacts)
        self.assertEqual(self.report()["collection_gate"]["status"], "pass")
        self.assertEqual(
            auditor.read(self.root / "reader-artifacts.json"), original_snapshot
        )
        self.fragment_entries[0]["artifact_id"] = previous["id"]
        self.assertEqual(self.report()["reader_job_admission"]["status"], "fail")
        self.fragment_entries[0]["artifact_id"] = selected["id"]
        previous["created_at"] = selected["created_at"]
        self.assertEqual(self.report()["reader_job_admission"]["status"], "fail")

    def test_duplicate_missing_extra_unit_and_compatible_baseline_never_deduplicate(
        self,
    ):
        self.fragment_entries.append(copy.deepcopy(self.fragment_entries[0]))
        self.assertEqual(self.report()["collection_gate"]["status"], "fail")
        self.fragment_entries.pop()
        missing = self.fragment_entries.pop()
        self.assertEqual(self.report()["coverage"]["missing"], 27)
        self.fragment_entries.append(missing)
        first = self.fragments[0]
        original = copy.deepcopy(first)
        first["cells"].append(copy.deepcopy(first["cells"][0]))
        self.save_fragment()
        self.assertEqual(self.report()["coverage"]["missing"], 27)
        self.fragments[0] = original
        self.fragments[0]["cells"][0]["baseline"] = {"status": "missing"}
        self.save_fragment()
        self.assertFalse(self.report(self.approved_budgets())["accepted"])

    def test_fragment_source_sampling_reader_and_catalog_forgeries_refuse(self):
        original = self.fragments[0]
        for key, mutation in (
            ("reader", dict(self.reader, revision="d" * 40)),
            ("original", dict(original["original"], source_revision="c" * 40)),
            ("sampling", {"cold": 1, "warm": 1, "warmups": 0}),
            ("accepted", True),
            ("unit", dict(original["unit"], cases_by_role={"candidate": []})),
            ("unit_gate", {"status": "pass", "key": "other"}),
        ):
            self.fragments[0] = dict(original, **{key: mutation})
            self.save_fragment()
            with self.subTest(key=key):
                self.assertEqual(self.report()["collection_gate"]["status"], "fail")
        self.fragments[0] = original

    def test_raw_statistic_count_scope_and_single_warm_rss_refuse_tampering(self):
        original = self.fragments[0]
        mutations = [
            lambda v: v["metrics"]["cold.public_build.median_ns"].update(median=999),
            lambda v: v["metrics"]["warm.child_peak_rss.median_bytes"].update(
                count=30, raw=[1000] * 30
            ),
            lambda v: v["metrics"]["cold.public_build.median_ns"]["raw"].__setitem__(
                0, True
            ),
            lambda v: v["metrics"].pop("cold.public_build.median_ns"),
            lambda v: v["performance_collection_association"].update(
                runner_name="other"
            ),
            lambda v: v["native_package_association"].update(run_attempt=9),
        ]
        for mutate in mutations:
            self.fragments[0] = copy.deepcopy(original)
            mutate(self.fragments[0]["cells"][0]["candidate"])
            self.save_fragment()
            self.assertEqual(self.report()["reader_job_admission"]["status"], "fail")
        self.fragments[0] = original

    def test_original_fragment_zip_bytes_size_and_provider_cleanup(self):
        for index, selected in enumerate(self.fragment_entries):
            archive = self.root / f"fragment-{index}.zip"
            with zipfile.ZipFile(archive, "w") as output:
                output.write(
                    self.root
                    / selected["artifact_name"]
                    / "performance-unit-audit.json",
                    "performance-unit-audit.json",
                )
            selected["archive"] = archive.name
            self.reader_artifacts[index]["size_in_bytes"] = archive.stat().st_size
        entry = self.fragment_entries[0]
        archive = self.root / entry["archive"]
        original = archive.read_bytes()
        completed = []

        @contextlib.contextmanager
        def provider(selected, root):
            try:
                yield root
            finally:
                completed.append(selected["artifact_id"])

        self.persist()
        with mock.patch.object(
            auditor, "reader_identity", return_value={"revision": "d" * 40}
        ):
            report = aggregate.aggregate(
                self.root / "aggregate.json", "c" * 40, entry_provider=provider
            )
        self.assertEqual(report["coverage"]["passed"], 567)
        self.assertEqual(report["comparator_coverage"]["passed"], 504)
        self.assertEqual(
            completed, [selected["artifact_id"] for selected in self.fragment_entries]
        )
        self.assertEqual(archive.read_bytes(), original)
        self.reader_artifacts[0]["size_in_bytes"] += 1
        self.assertEqual(self.report()["coverage"]["passed"], 540)

    def test_phase_ledger_includes_all_three_actual_warmups(self):
        value = copy.deepcopy(self.fragments[0]["cells"][0]["candidate"])
        value["unavailable"] = list(auditor.lifecycle.unavailable(True))
        phases = (
            "preparation",
            "build_service",
            "input",
            "execution_admission",
            "execution_derivation",
            "output_keys",
            "verification",
            "output_preparation",
            "held_save_1",
            "held_save_2",
        )
        for mode in ("cold", "warm"):
            for phase in phases:
                value["metrics"][f"{mode}.phase.{phase}.median_ns"] = auditor.stats(
                    [10] * (10 if mode == "cold" else 30), "nanoseconds"
                )
        case = self.fragments[0]["cells"][0]["case"]
        value["actual_function_calls"] = [
            {
                "child": f"{case}-cold-{i}",
                "mode": "cold",
                "index": i,
                "warmup": False,
                "calls": [
                    {
                        "node": None,
                        "stage": "execution_derivation",
                        "entered": 1,
                        "succeeded": 1,
                        "failed": 0,
                    }
                ],
            }
            for i in range(10)
        ] + [
            {
                "child": f"{case}-warm-0",
                "mode": "warm",
                "index": i,
                "warmup": i < 3,
                "calls": [
                    {
                        "node": None,
                        "stage": "execution_derivation",
                        "entered": 1,
                        "succeeded": 1,
                        "failed": 0,
                    }
                ],
            }
            for i in range(33)
        ]
        aggregate.validate_vectors(value, "candidate", case)
        value["actual_function_calls"].pop(10)
        with self.assertRaisesRegex(ValueError, "warmup/function"):
            aggregate.validate_vectors(value, "candidate", case)

    def test_cli_always_retains_refusal_and_forbids_budgeted_fragments(self):
        selection = self.root / "selector.json"
        write(selection, self.catalog["units"][0]["selector"])
        for flags in (
            ("--unit", str(selection), "--check"),
            ("--check-unit",),
            ("--catalog", "--check-collection"),
            ("--unit", str(selection), "--budgets", "fake"),
        ):
            with (
                self.subTest(flags=flags),
                contextlib.redirect_stderr(__import__("io").StringIO()),
                self.assertRaises(SystemExit),
            ):
                auditor.main(
                    [
                        "--input",
                        str(self.root / "input.json"),
                        "--output",
                        str(self.root / "forbidden"),
                        *flags,
                    ]
                )
        self.reader_jobs[0]["conclusion"] = "failure"
        self.persist()
        with mock.patch.object(
            auditor, "reader_identity", return_value={"revision": "d" * 40}
        ):
            code = aggregate.main(
                [
                    "--input",
                    str(self.root / "aggregate.json"),
                    "--reviewed-auditor-revision",
                    "c" * 40,
                    "--output",
                    str(self.root / "refused"),
                    "--check-collection",
                ]
            )
        self.assertEqual(code, 1)
        self.assertTrue((self.root / "refused/performance-audit.json").is_file())
        self.assertTrue((self.root / "refused/performance-audit.md").is_file())


if __name__ == "__main__":
    unittest.main()
