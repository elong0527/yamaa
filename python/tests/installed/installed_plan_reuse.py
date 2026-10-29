"""Installed lazy objects reuse what their first run compiled (#2821).

A spec or project object's first `run()` compiles and keeps its plans. A later
`run()` reuses them while the object's paths still name the files they were
compiled from and every one is unchanged, and compiles again otherwise, so no
run is stale and none runs a study the paths no longer name (#3069). Inputs are
read afresh by every run. A run's private measurements say how it got its
plan: `compiled`, `refused` or `reused`.
"""

import copy
import json
import os
import pickle
import shutil
import tempfile
import unittest
from pathlib import Path

from host_fixtures import load as load_truth

import yamaa

TRUTH = load_truth(__file__, "plan-reuse")
MEMORY_INPUTS = load_truth(__file__, "memory-inputs")
STUDIES = Path(__file__).parent / "project-original"


def plan_of(run):
    """How the run got its plan, from its private measurements."""
    return json.loads(run._native.observations(True))["measurements"]["plan"]


def compiled(run):
    """Whether the run compiled rather than reused a kept plan."""
    return plan_of(run) != "reused"


PLAIN = (
    'yamaa: "1.0"\ntype: spec\ndomain: TEST\nkeys: [ID]\n'
    "input: {SRC: {path: input.csv, types: {ID: int}}}\n"
    "output: {path: out.csv, columns: [ID]}\n"
    "columns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n"
)


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

    def test_alias_override_fixture_keeps_file_fallback_with_reused_plans(self):
        for name, contents in MEMORY_INPUTS["files"].items():
            self.write(name, contents)
        spec = yamaa.spec(self.root / MEMORY_INPUTS["entry"])
        expected_inputs = {
            alias: MEMORY_INPUTS["file_details"]
            for alias in MEMORY_INPUTS["unknown"]["context"]["allowed"]
        }
        for plan in ("compiled", "reused"):
            with self.subTest(plan=plan):
                run = spec.run()
                run.save()
                inputs = {
                    row["NAME"]: json.loads(row["DETAILS"])
                    for row in run.report.to_pylist()
                    if row["KIND"] == "input"
                }
                self.assertEqual(
                    (
                        run.status,
                        plan_of(run),
                        (self.root / "out.csv").read_bytes(),
                        inputs,
                    ),
                    (
                        "succeeded",
                        plan,
                        MEMORY_INPUTS["file_csv"].encode("utf-8"),
                        expected_inputs,
                    ),
                )

    def run_study(self, spec, study):
        """Run `spec` on one study's input; check its saved output or issues."""
        self.write("input.csv", study["input_csv"])
        output = self.root / "output.csv"
        output.unlink(missing_ok=True)
        run = spec.run()
        # A truth row names its file relative to the study's directory; a
        # location names it by its absolute path (P-ISSUE-1).
        expected = [
            {**row, "file": (self.root / row["file"]).as_posix()}
            for row in study["issues"]
        ]
        self.assertEqual(run.issues.to_pylist(), expected)
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
        self.write("spec.yaml", 'yamaa: "1.0"\ntype: spec\ndomain: TEST\n')
        refused = spec.run()
        self.assertIsNone(refused.output)
        self.assertGreater(refused.issues.num_rows, 0)
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
        # The function's default number of decimals changes in its function
        # file: the run compiles the new contract and uses it.
        function = case / "functions/project_ratio.yaml"
        before, after = (
            TRUTH["function_default"]["before"],
            TRUTH["function_default"]["after"],
        )
        original = function.read_text(encoding="utf-8")
        self.assertEqual(original.count(before), 1)
        function.write_text(original.replace(before, after), encoding="utf-8")
        changed = spec.run()
        self.assertTrue(compiled(changed))
        self.assertEqual(saved(changed), TRUTH["function_output_csv"].encode("utf-8"))
        self.assertFalse(compiled(spec.run()))
        function.write_text(original, encoding="utf-8")
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
        # A section lists only specifications of its model (SUB-3).
        dm = (
            'yamaa: "1.0"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [USUBJID]\n'
            "input: {RAW: {path: ../raw/dm.csv, types: {AGE: int}}}\n"
            "columns:\n"
            "  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}\n"
            "  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}\n"
            "output: {path: ../out/dm.csv, columns: [USUBJID, AGE]}\n"
        )
        adsl = (
            'yamaa: "1.0"\ntype: spec\nmodel: ADaM\ndomain: ADSL\nkeys: [USUBJID]\n'
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
            'yamaa: "1.0"\ntype: project\nsdtm:\n  specs: [sdtm/dm.yaml]\n'
            "adam:\n  specs: [adam/adsl.yaml]\n",
        )
        (self.root / "out").mkdir()
        project = yamaa.project(self.root / "project.yaml")

        def saved():
            runs = project.run()
            self.assertEqual(runs.names, ["DM", "ADSL"])
            self.assertEqual(runs.issues.num_rows, 0)
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

    def studies(self):
        """Studies A and B with the same file names; B has one more row."""
        for name, rows in (("a", "1\n2\n"), ("b", "1\n2\n3\n")):
            self.write(f"{name}/spec.yaml", PLAIN)
            # A section lists only specifications of its model (SUB-3).
            self.write(
                f"{name}/dm.yaml",
                PLAIN.replace("domain: TEST", "model: SDTM\ndomain: DM").replace(
                    "out.csv", "dm.csv"
                ),
            )
            self.write(
                f"{name}/project.yaml",
                'yamaa: "1.0"\ntype: project\nsdtm:\n  specs: [dm.yaml]\n',
            )
            self.write(f"{name}/input.csv", "ID\n" + rows)
        previous = Path.cwd()
        self.addCleanup(os.chdir, previous)

    def test_relative_paths_after_a_working_directory_change_build_the_new_study(self):
        self.studies()
        os.chdir(self.root / "a")
        spec = yamaa.spec("spec.yaml")
        project = yamaa.project("project.yaml")
        member = project.spec("DM")

        def saved(run, study, name):
            run.save()
            return (self.root / study / name).read_text(encoding="utf-8")

        a, b = "ID\n1\n2\n", "ID\n1\n2\n3\n"
        first = spec.run()
        self.assertEqual(
            (plan_of(first), saved(first, "a", "out.csv")), ("compiled", a)
        )
        self.assertEqual(plan_of(spec.run()), "reused")
        self.assertEqual(saved(project.run(), "a", "dm.csv"), a)
        self.assertEqual(plan_of(member.run()), "compiled")
        self.assertEqual(plan_of(member.run()), "reused")
        # The same spellings now name study B: each object builds B and
        # leaves A's saved files as they were.
        os.chdir(self.root / "b")
        moved = spec.run()
        self.assertEqual(
            (plan_of(moved), saved(moved, "b", "out.csv")), ("compiled", b)
        )
        self.assertEqual((self.root / "a/out.csv").read_text(encoding="utf-8"), a)
        self.assertEqual(spec.check().issues.num_rows, 0)
        self.assertEqual(saved(project.run(), "b", "dm.csv"), b)
        self.assertEqual((self.root / "a/dm.csv").read_text(encoding="utf-8"), a)
        # `project.spec(name)` keeps its located specification, A's, with the
        # project's relative path, which now names B's project.
        self.assertEqual(plan_of(member.run()), "compiled")
        self.assertEqual(plan_of(member.run()), "reused")
        # Back in A, the spec object compiles A again: it keeps one plan.
        os.chdir(self.root / "a")
        self.assertEqual(plan_of(spec.run()), "compiled")

    @unittest.skipIf(os.name == "nt", "symbolic links need privileges on Windows")
    def test_a_path_through_a_retargeted_link_builds_the_study_it_now_names(self):
        self.studies()
        link = self.root / "study"
        link.symlink_to(self.root / "a", target_is_directory=True)
        spec = yamaa.spec(link / "spec.yaml")
        project = yamaa.project(link / "project.yaml")
        self.assertEqual(plan_of(spec.run()), "compiled")
        self.assertEqual(plan_of(spec.run()), "reused")
        self.assertEqual(project.run()["DM"].output.num_rows, 2)
        link.unlink()
        link.symlink_to(self.root / "b", target_is_directory=True)
        moved = spec.run()
        self.assertEqual((plan_of(moved), moved.output.num_rows), ("compiled", 3))
        self.assertEqual(project.run()["DM"].output.num_rows, 3)

    def test_objects_pickle_and_copy_and_start_with_nothing_kept(self):
        spec = yamaa.spec(self.write("spec.yaml", PLAIN))
        self.write("input.csv", "ID\n1\n")
        project = yamaa.project(
            self.write(
                "project.yaml",
                'yamaa: "1.0"\ntype: project\nsdtm:\n  specs: [dm.yaml]\n',
            )
        )
        self.write(
            "dm.yaml",
            PLAIN.replace("domain: TEST", "model: SDTM\ndomain: DM").replace(
                "out.csv", "dm.csv"
            ),
        )
        self.assertEqual(plan_of(spec.run()), "compiled")
        for clone in (
            pickle.loads(pickle.dumps(spec)),
            copy.deepcopy(spec),
            copy.copy(spec),
        ):
            with self.subTest(clone=clone):
                self.assertIsNot(clone._plans, spec._plans)
                self.assertEqual(plan_of(clone.run()), "compiled")
                self.assertEqual(plan_of(clone.run()), "reused")
        # The original keeps its own plan.
        self.assertEqual(plan_of(spec.run()), "reused")
        project.run()
        for clone in (pickle.loads(pickle.dumps(project)), copy.deepcopy(project)):
            with self.subTest(clone=clone):
                self.assertEqual(clone.run()["DM"].output.num_rows, 1)
                self.assertEqual(plan_of(clone.spec("DM").run()), "compiled")


if __name__ == "__main__":
    unittest.main()
