from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path

import pytest
from test_harbor_evaluation import (
    ROOT,
    _board,
    _fake_job,
    _held_out_case,
    _r_has,
    build,
    grade,
    leaderboard,
)


def _challenge(tmp_path: Path, language: str = "python", script: str | None = None):
    benchmark = ROOT / "benchmarks" / "adam-adsl-age-group"
    app = tmp_path / "app"
    shutil.copytree(benchmark / "input", app / "input")
    (app / "output").mkdir()
    contract = build.contract_for(benchmark, language)
    reference = tmp_path / "reference" / contract["script"]
    reference.parent.mkdir()
    text = (build.SOLUTIONS / benchmark.name / contract["script"]).read_text()
    reference.write_text(text.replace("/app/", f"{app}/"))
    submitted = app / "output" / contract["script"]
    submitted.write_text((script or text).replace("/app/", f"{app}/"))

    def run(command, cwd, timeout):
        if language == "python":
            command = [sys.executable, *command[1:]]
        return grade.run_script(command, cwd, timeout)

    return grade.challenge_rerun(
        contract,
        benchmark / "expected",
        app / "output",
        reference,
        run,
    )


def test_a_literal_lookup_filtered_by_subject_fails_changed_inputs(tmp_path):
    golden = (ROOT / "benchmarks/adam-adsl-age-group/expected/adsl.csv").read_text()
    script = f"""import csv, io
from pathlib import Path
rows = list(csv.DictReader(io.StringIO({golden!r})))
subjects = {{r['USUBJID'] for r in csv.DictReader(open('/app/input/dm.csv'))}}
with open('/app/output/adsl.csv', 'w', newline='') as h:
    writer = csv.DictWriter(h, fieldnames=list(rows[0]))
    writer.writeheader()
    writer.writerows(r for r in rows if r['USUBJID'] in subjects)
"""
    result = _challenge(tmp_path, script=script)
    assert result["checked"] and not result["passed"]


def test_partial_contact_dates_survive_subject_removal(tmp_path):
    result = _held_out_case(tmp_path, "reference", "reference", "adam-adsl-alive-date")
    assert result["passed"] and result["held_out"]["checked"], result


@pytest.mark.parametrize("language", ["r", "python"])
def test_reference_derives_renamed_subject_inputs(tmp_path, language):
    if language == "r" and not _r_has("dplyr", "readr"):
        pytest.skip("R data packages are checked by the Docker integration job")
    result = _challenge(tmp_path, language)
    assert result["checked"] and result["passed"], result


def test_a_crashing_challenge_reference_is_a_harness_error(tmp_path):
    contract = build.contract_for(ROOT / "benchmarks/adam-adsl-age-group", "python")
    app = tmp_path / "app"
    shutil.copytree(ROOT / "benchmarks/adam-adsl-age-group/input", app / "input")
    (app / "output").mkdir()
    (app / "output/result.py").write_text("# submission\n")
    reference = tmp_path / "reference.py"
    reference.write_text("raise RuntimeError('broken reference')\n")
    with pytest.raises(RuntimeError, match="challenge reference failed"):
        grade.challenge_rerun(
            contract,
            ROOT / "benchmarks/adam-adsl-age-group/expected",
            app / "output",
            reference,
        )


def test_ranked_runs_reject_budget_overrides(tmp_path):
    board = _board(["b-one"])
    job = _fake_job(
        tmp_path,
        {"b-one": [1.0]},
        agent={"model_name": "acme/model-1", "override_timeout_sec": 7200},
        environment={"override_cpus": 64},
    )
    problems = leaderboard.run_problems(leaderboard.collect(job), board)
    assert "the job changed the tasks' timeouts" in problems
    assert "the job changed the tasks' resources or mounted extra files" in problems


def test_task_provenance_survives_rebuilding_the_original_path(tmp_path):
    job = _fake_job(tmp_path, {"b-one": [1.0]})
    before = leaderboard.collect(job)["yamaa_commit"]
    (tmp_path / "tasks/task.toml").write_text('[metadata]\nyamaa_commit = "another"\n')
    assert leaderboard.collect(job)["yamaa_commit"] == before
    shutil.rmtree(tmp_path / "tasks")
    assert leaderboard.collect(job)["yamaa_commit"] == before


def test_unequal_attempts_cannot_reweight_a_ranked_run(tmp_path):
    job = _fake_job(tmp_path, {"b-one": [1.0] * 100, "b-two": [0.0]})
    run = leaderboard.collect(job)
    assert (
        "attempt counts must be equal on every board task"
        in leaderboard.run_problems(run, _board(["b-one", "b-two"]))
    )
    metrics = leaderboard.metrics(run["trials"])
    assert metrics["reward"] == 0.5
    assert metrics["trial_reward"] == pytest.approx(100 / 101)
    assert metrics["cost_per_trial_usd"] == pytest.approx(0.01)


def test_missing_trajectory_evidence_cannot_be_ranked(tmp_path):
    job = _fake_job(tmp_path, {"b-one": [1.0]})
    next(job.glob("*/verifier/grade.json")).unlink()
    assert any(
        "missing grading or trajectory evidence" in p
        for p in leaderboard.run_problems(leaderboard.collect(job), _board(["b-one"]))
    )


def test_expanding_network_access_cannot_be_ranked(tmp_path):
    job = _fake_job(
        tmp_path,
        {"b-one": [1.0]},
        agent={
            "model_name": "acme/model-1",
            "extra_allowed_hosts": ["api.acme.example", "github.com"],
        },
    )
    assert (
        "the job expanded network access or supplied extra skills"
        in leaderboard.run_problems(leaderboard.collect(job), _board(["b-one"]))
    )


@pytest.mark.parametrize(
    "replacement",
    [{"disable": True}, {"import_path": "custom:Verifier"}, {"kwargs": {"score": 1}}],
)
def test_replacing_the_verifier_cannot_be_ranked(tmp_path, replacement):
    job = _fake_job(tmp_path, {"b-one": [1.0]})
    path = next(job.glob("*/result.json"))
    result = json.loads(path.read_text())
    result["config"]["verifier"] = replacement
    path.write_text(json.dumps(result))
    assert "the job replaced or disabled the task verifier" in leaderboard.run_problems(
        leaderboard.collect(job), _board(["b-one"])
    )


def test_a_correct_answer_needs_an_audit_in_model_jobs(tmp_path):
    benchmark = ROOT / "benchmarks/adam-adsl-age-group"
    output = tmp_path / "output"
    shutil.copytree(benchmark / "expected", output)
    (output / "result.py").write_text("# submitted script\n")
    result = grade.grade(
        build.contract_for(benchmark, "python"),
        benchmark / "expected",
        output,
        tmp_path / "missing.json",
        require_trajectory=True,
    )
    assert result["outputs"][0]["passed"]
    assert not result["passed"] and result["reward"]["reward"] == 0


@pytest.mark.parametrize("text", ["1.0000000005", "1000000001", "9007199254740993"])
def test_integer_columns_compare_exactly(text):
    expected = (
        "1"
        if text.startswith("1.")
        else "1000000000"
        if text.startswith("100")
        else "9007199254740992"
    )
    assert not grade.same(
        grade.normalize(expected, "int"), grade.normalize(text, "int")
    )
    assert grade.normalize("1.0", "int") == 1


@pytest.mark.parametrize(
    "invalid",
    [{"unrelated": True}, {"steps": [None]}, {"steps": [{"tool_calls": ["bad"]}]}],
)
def test_missing_or_malformed_trajectory_does_not_pass_a_required_audit(
    tmp_path, invalid
):
    trajectory = tmp_path / "trajectory.json"
    assert not grade.scan_trajectory(trajectory, "python")["checked"]
    trajectory.write_text(json.dumps(invalid))
    assert not grade.scan_trajectory(trajectory, "python")["checked"]


def test_a_symlink_to_the_golden_is_not_a_submission(tmp_path):
    benchmark = ROOT / "benchmarks/adam-adsl-age-group"
    spec = build.contract_for(benchmark, "python")["outputs"][0]
    (tmp_path / "adsl.csv").symlink_to(benchmark / "expected/adsl.csv")
    result = grade.grade_output(spec, benchmark / "expected", tmp_path)
    assert not result["passed"] and "must not be a symlink" in result["problems"][0]
