"""Independent M1 owner mutation controls; synthetic reports are never evidence."""

import copy
import importlib.util
import shutil
import unittest
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "committed_cohort_controls", Path(__file__).with_name("test_committed_cohort.py")
)
controls = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(controls)
cohort = controls.cohort
producer = controls.producer


class ProducerOwnerReview(unittest.TestCase):
    def setUp(self):
        self.fixture = controls.CompletePublicTruth("runTest")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)

    def source_owner(self):
        return (
            self.fixture.evidence
            / "python/public-reports/producer-cohort/python"
            / producer.NAME
        )

    def audit_owner(self):
        return self.fixture.output / "raw/python" / producer.NAME

    def prepared_audit(self):
        fixture = self.fixture
        inventory = fixture.compare(True)
        audit = object.__new__(controls.audit_module.Audit)
        audit.directory = fixture.directory / "independent-audit"
        audit.source = {"sha": fixture.revision}
        audit.run = {"id": 7, "run_attempt": 1, "html_url": "unit:unqualified"}
        audit.artifacts = []
        for artifact, filename in (
            ("native-python-ubuntu-24.04-3.14", "unit-owner.whl"),
            ("native-r-ubuntu-24.04", "unit-owner.tar.gz"),
        ):
            path = audit.directory / "artifacts" / artifact / filename
            path.parent.mkdir(parents=True)
            path.write_bytes(b"synthetic mutation control; never qualification")
            audit.artifacts.append({"name": artifact, "expired": False})
        source = audit.directory / "source" / cohort.TRUTH_PATHS[producer.NAME]
        source.parent.mkdir(parents=True)
        shutil.copyfile(controls.ROOT / cohort.TRUTH_PATHS[producer.NAME], source)
        return audit, inventory

    def check_audit(self, audit, inventory):
        return audit.committed_cohort(
            "ubuntu-24.04",
            "3.12",
            "wheel",
            True,
            self.fixture.output,
            inventory,
        )

    def test_qualifier_publication_node_and_flag_types_are_exact(self):
        owner = self.source_owner()
        record = cohort.read(
            self.fixture.evidence / "python/supplemental/supplemental.json"
        )
        expected = cohort.reports.read_report(
            controls.ROOT / cohort.TRUTH_PATHS[producer.NAME]
        )
        saved = owner / "after-1/report.json"
        original = cohort.read(saved)
        for label, change in (
            (
                "boolean node",
                lambda j: j["publication"]["entered"][0].update(node=True),
            ),
            ("float node", lambda j: j["publication"]["entered"][0].update(node=1.0)),
            (
                "integer flag",
                lambda j: j["publication"]["entered"][0].update(published=1),
            ),
        ):
            with self.subTest(label=label):
                changed = copy.deepcopy(original)
                change(changed)
                cohort.write(saved, changed)
                destination = self.fixture.directory / label
                with self.assertRaisesRegex(ValueError, "publication"):
                    producer.retain_owner(
                        self.fixture.evidence / "python",
                        "python",
                        record,
                        expected,
                        destination,
                    )
        cohort.write(saved, original)

    def test_audit_rechecks_publication_node_and_flag_types(self):
        audit, inventory = self.prepared_audit()
        saved = self.audit_owner() / "after-2/report.json"
        original = cohort.read(saved)
        for label, change in (
            (
                "boolean node",
                lambda j: j["publication"]["entered"][0].update(node=True),
            ),
            ("float node", lambda j: j["publication"]["entered"][0].update(node=1.0)),
            (
                "integer flag",
                lambda j: j["publication"]["entered"][0].update(published=1),
            ),
        ):
            with self.subTest(label=label):
                changed = copy.deepcopy(original)
                change(changed)
                cohort.write(saved, changed)
                with self.assertRaisesRegex(ValueError, "publication"):
                    self.check_audit(audit, inventory)
        cohort.write(saved, original)

    def test_audit_raw_reports_must_match_their_actual_host_and_core(self):
        audit, inventory = self.prepared_audit()
        owner = self.audit_owner()
        paths = [
            owner / path
            for path in ("before.json", "after-1/report.json", "after-2/report.json")
        ]
        originals = [cohort.read(path) for path in paths]
        for field, value in (("engine_version", "wrong-core"), ("runtime", "r")):
            with self.subTest(field=field):
                for path, original in zip(paths, originals, strict=True):
                    changed = copy.deepcopy(original)
                    changed[field] = value
                    if field == "runtime":
                        changed["environment"]["lock_source"] = (
                            changed["environment"]["lock_source"].removesuffix(
                                "uv.lock"
                            )
                            + "renv.lock"
                        )
                    cohort.write(path, changed)
                with self.assertRaisesRegex(ValueError, "host|core|owner"):
                    self.check_audit(audit, inventory)
        for path, original in zip(paths, originals, strict=True):
            cohort.write(path, original)

    def test_original_seed_read_cannot_be_reassigned_to_consumer(self):
        owner = self.source_owner()
        raw = cohort.read(owner / "before.json")
        case = (owner / "case-directory.txt").read_text()
        self.assertEqual(len(raw["source_reads"]), 1)
        self.assertEqual(raw["source_reads"][0]["path"], "seed.csv")
        raw["source_reads"][0]["specification"] = case + "/spec.yaml"
        with self.assertRaisesRegex(ValueError, "source|resource|owner|node"):
            producer.project_report(raw, case)


if __name__ == "__main__":
    unittest.main()
