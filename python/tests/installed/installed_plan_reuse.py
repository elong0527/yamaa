"""Installed lazy objects reuse what their first run compiled (#2821).

A spec or project object's first `run()` compiles and keeps its plans. A later
`run()` reuses them once every file they were compiled from is unchanged, and
compiles again when one changed, so no run is stale. Inputs are read afresh by
every run. A compiling run's private measurements start with the
`preparation` phase; a reusing run's start at the run.
"""

import os
import shutil
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "plan-reuse")
STUDIES = Path(__file__).parent / "project-original"


def compiled(run):
    """Whether the run compiled: its measurements hold the preparation phase."""
    return '"phase":"preparation"' in run._native.observations(True)


class PlanReuse(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="yamaa-plan-reuse-")
        self.addCleanup(temporary.cleanup)
        # One resolved spelling of the temp directory for every call.
        self.root = Path(temporary.name).resolve()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(text.encode("utf-8"))
        return path

    def run_study(self, spec, study):
        """Run `spec` on one study's input; check its saved output or issues."""
        self.write("input.csv", study["input_csv"])
        output = self.root / "output.csv"
        output.unlink(missing_ok=True)
        run = spec.run()
        self.assertEqual(run.issues.to_dicts(), study["issues"])
        if study["output_csv"] is None:
            self.assertIsNone(run.output)
            with self.assertRaises(yamaa.Error):
                run.save()
            self.assertFalse(output.exists())
        else:
            run.save()
            self.assertEqual(output.read_bytes(), study["output_csv"].encode("utf-8"))
        return run

    def test_later_runs_reuse_the_plan_and_read_each_input_afresh(self):
        spec = yamaa.spec(self.write("spec.yaml", TRUTH["specification_yaml"]))
        runs = [
            self.run_study(spec, TRUTH["studies"][name]) for name in TRUTH["sequence"]
        ]
        self.assertEqual([compiled(run) for run in runs], [True, False, False])
        # Another object for the same file compiles on its own first run.
        other = yamaa.spec(self.root / "spec.yaml")
        self.assertTrue(compiled(self.run_study(other, TRUTH["studies"]["A"])))

    def test_a_changed_specification_compiles_again_and_is_never_stale(self):
        study = TRUTH["studies"]["A"]
        spec = yamaa.spec(self.write("spec.yaml", TRUTH["specification_yaml"]))
        self.assertTrue(compiled(self.run_study(spec, study)))
        self.write("spec.yaml", TRUTH["changed_specification_yaml"])
        changed = dict(study, output_csv=TRUTH["changed_output_csv"])
        self.assertTrue(compiled(self.run_study(spec, changed)))
        self.assertFalse(compiled(self.run_study(spec, changed)))
        # A specification the engine refuses keeps nothing: the run reports
        # the refusal, and once it is fixed the next run compiles it.
        self.write("spec.yaml", "schema_version: '1.0'\ndomain: TEST\n")
        refused = spec.run()
        self.assertIsNone(refused.output)
        self.assertGreater(refused.issues.height, 0)
        self.write("spec.yaml", TRUTH["specification_yaml"])
        self.assertTrue(compiled(self.run_study(spec, study)))

    def test_a_changed_function_contract_or_project_file_compiles_again(self):
        case = self.root / "schema-functions"
        shutil.copytree(STUDIES / "schema-functions", case)
        previous = Path.cwd()
        self.addCleanup(os.chdir, previous)
        os.chdir(case)
        spec = yamaa.project(case / "project.yaml").spec("TEST")
        expected = (case / "expected/test.csv").read_bytes()

        def saved(run):
            run.save()
            return (case / "test.csv").read_bytes()

        first = spec.run()
        self.assertTrue(compiled(first))
        self.assertEqual(saved(first), expected)
        self.assertFalse(compiled(spec.run()))
        # The function's default number of decimals changes in the
        # environment file: the run compiles the new contract and uses it.
        environment = case / "python/environment.yaml"
        before, after = (
            TRUTH["function_default"]["before"],
            TRUTH["function_default"]["after"],
        )
        original = environment.read_text(encoding="utf-8")
        environment.write_text(original.replace(before, after), encoding="utf-8")
        changed = spec.run()
        self.assertTrue(compiled(changed))
        self.assertEqual(saved(changed), TRUTH["function_output_csv"].encode("utf-8"))
        self.assertFalse(compiled(spec.run()))
        environment.write_text(original, encoding="utf-8")
        restored = spec.run()
        self.assertTrue(compiled(restored))
        self.assertEqual(saved(restored), expected)
        # The project file changes: every plan it holds compiles again.
        project = case / "project.yaml"
        project.write_text(
            project.read_text(encoding="utf-8") + "# reviewed\n", encoding="utf-8"
        )
        self.assertTrue(compiled(spec.run()))
        self.assertFalse(compiled(spec.run()))

    def test_a_project_object_reuses_its_plans_and_runs_every_change(self):
        dm = (
            "schema_version: '1.0'\ndomain: DM\nkeys: [USUBJID]\n"
            "input: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\n"
            "columns:\n"
            "  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n"
            "  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\n"
            "output: {path: ../out/dm.csv, columns: [USUBJID, AGE]}\n"
        )
        adsl = (
            "schema_version: '1.0'\ndomain: ADSL\nkeys: [USUBJID]\n"
            "input: {DM: {path: ../out/dm.csv, schema: ../sdtm/dm.yaml}}\n"
            "columns:\n"
            "  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}\n"
            "  - {name: AGE, type: int, label: Age, derivation: DM.AGE}\n"
            "output: {path: ../out/adsl.csv, columns: [USUBJID, AGE]}\n"
        )
        self.write("raw/dm.csv", "USUBJID,AGE\nS1,30\nS2,41\n")
        self.write("sdtm/dm.yaml", dm)
        self.write("adam/adsl.yaml", adsl)
        self.write(
            "project.yaml",
            "schema_version: '1.0'\nsdtm:\n  specs: [sdtm/dm.yaml]\n"
            "adam:\n  specs: [adam/adsl.yaml]\n",
        )
        (self.root / "out").mkdir()
        project = yamaa.project(self.root / "project.yaml")

        def saved():
            runs = project.run()
            self.assertEqual(runs.names, ["DM", "ADSL"])
            self.assertEqual(runs.issues.height, 0)
            runs.save()
            return (self.root / "out/adsl.csv").read_text(encoding="utf-8")

        self.assertEqual(saved(), "USUBJID,AGE\nS1,30\nS2,41\n")
        self.assertEqual(saved(), "USUBJID,AGE\nS1,30\nS2,41\n")
        # Each run reads the inputs afresh.
        self.write("raw/dm.csv", "USUBJID,AGE\nS1,31\nS2,41\n")
        self.assertEqual(saved(), "USUBJID,AGE\nS1,31\nS2,41\n")
        # A changed specification runs as changed.
        self.write("adam/adsl.yaml", adsl.replace("[USUBJID, AGE]}", "[AGE, USUBJID]}"))
        self.assertEqual(saved(), "AGE,USUBJID\n31,S1\n41,S2\n")
        self.assertEqual(saved(), "AGE,USUBJID\n31,S1\n41,S2\n")
        # A spec object of the project keeps its own plan.
        spec = project.spec("DM")
        self.assertEqual([compiled(spec.run()) for _ in range(2)], [True, False])


if __name__ == "__main__":
    unittest.main()
