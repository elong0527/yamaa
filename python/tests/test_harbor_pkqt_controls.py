"""Exercise the hosted rejection control through the real local grader."""

import importlib.util
import json
import sys
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).parents[2]
HARBOR = ROOT / "evaluations/harbor"


@pytest.fixture
def controls(monkeypatch):
    monkeypatch.syspath_prepend(str(HARBOR))
    # Other Harbor tests load modules from their own test namespace.
    for name in ("build", "grade"):
        monkeypatch.delitem(sys.modules, name, raising=False)
    spec = importlib.util.spec_from_file_location(
        "pkqt_controls", HARBOR / "check_pkqt_r_golden_copy.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_the_r_control_selects_each_batch_case_and_each_prompt_tier(controls):
    assert set(controls.BENCHMARKS) == {
        "adam-adeg-placebo-corrected-change",
        "adam-adeg-replicate-qtc-average",
        "adam-adeg-time-matched-baseline",
        "adam-adeg-time-matched-concentration",
        "adam-adpp-accumulation-ratio",
        "adam-adpp-carryover-exclusion",
        "adam-adpp-comparison-sets",
        "adam-adpp-dose-normalized",
    }
    assert len(controls.BENCHMARKS) == 8
    assert set(controls.build.TIERS) == {"full", "conventions", "brief"}


def test_a_valid_literal_writer_is_rejected_by_both_changed_input_checks(
    controls, tmp_path, monkeypatch
):
    # This local Python probe exercises the same grader orchestration; the
    # hosted command always selects R and requires its installed runtime.
    monkeypatch.setitem(controls.grade.INTERPRETERS, "python", sys.executable)
    result = controls.check_case(
        "adam-adsl-age-group", "full", "python", tmp_path, "test"
    )
    assert result == {
        "benchmark": "adam-adsl-age-group",
        "language": "python",
        "tier": "full",
        "reward": 0.0,
        "held_out_checked": True,
        "challenge_checked": True,
        "passed": True,
        "problems": [],
    }
    grade = json.loads(
        (tmp_path / "checks/adam-adsl-age-group/full/grade.json").read_text()
    )
    grade["rerun"]["returncode"] = 1
    assert "the original-input rerun did not succeed" in controls.rejection_problems(
        grade
    )
    grade["rerun"]["returncode"] = 0
    grade["held_out"]["checked"] = False
    assert "the held_out check did not execute" in controls.rejection_problems(grade)
    grade["held_out"]["checked"] = True
    grade["reward"]["policy_eligible"] = 0.0
    assert (
        "the script was rejected by policy instead of its answers"
        in controls.rejection_problems(grade)
    )


def test_the_r_command_fails_instead_of_skipping_an_absent_runtime(
    controls, monkeypatch, tmp_path
):
    monkeypatch.setattr(controls.shutil, "which", lambda _: None)
    monkeypatch.setattr(sys, "argv", ["check", "--out", str(tmp_path / "out")])
    with pytest.raises(SystemExit) as error:
        controls.main()
    assert error.value.code == 2
    assert not (tmp_path / "out").exists()


def test_the_hosted_control_reuses_r_setup_and_retains_failure_evidence():
    workflow = yaml.safe_load(
        (ROOT / ".github/workflows/harbor-evaluation.yml").read_text()
    )
    steps = workflow["jobs"]["solutions"]["steps"]
    control = next(
        s for s in steps if s.get("name") == "Reject R literal golden writers"
    )
    assert control["if"] == (
        "github.ref == 'refs/heads/validation/issue-2297-harbor' || "
        "github.ref == 'refs/heads/validation/issue-2297-r-rejection'"
    )
    assert (
        "--no-sync python evaluations/harbor/check_pkqt_r_golden_copy.py"
        in control["run"]
    )
    assert steps.index(control) > next(
        i
        for i, step in enumerate(steps)
        if step.get("name") == "Install the evaluation image's R packages"
    )
    artifact = next(s for s in steps if s.get("name") == "Upload R rejection controls")
    assert "always()" in artifact["if"]
    assert artifact["with"]["name"] == "pkqt-r-golden-copy"
    # The additive acceptance renewal must leave the first Docker run alone.
    assert "validation/issue-2297-r-rejection" in workflow[True]["push"]["branches"]
    oracle = yaml.safe_load((ROOT / ".github/workflows/harbor-oracle.yml").read_text())
    assert "validation/issue-2297-r-rejection" not in oracle[True]["push"]["branches"]
