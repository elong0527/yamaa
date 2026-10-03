"""Ranked partial scores and job evidence must describe complete derivations."""

from __future__ import annotations

import csv
import json
import shutil
import sys
import tarfile
from pathlib import Path

import pytest
import yaml
from test_harbor_evaluation import (
    _board,
    _fake_job,
    _golden,
    _load,
    _on,
    _rerun,
    _write,
    build,
    grade,
    leaderboard,
)
from test_harbor_hardening import AGE_GROUP, _app, _run


def _age_submission(tmp_path, body):
    app = _app(tmp_path)
    contract = build.contract_for(AGE_GROUP, "python")
    columns = contract["outputs"][0]["columns"]
    script = f"""import csv
with open('/app/input/dm.csv') as handle:
    rows = list(csv.DictReader(handle))
{body}
with open('/app/output/adsl.csv', 'w', newline='') as handle:
    writer = csv.DictWriter(handle, fieldnames={columns!r})
    writer.writeheader()
    writer.writerows(rows)
"""
    submitted = app / "output/result.py"
    submitted.write_text(script.replace("/app/", f"{app.as_posix()}/"))
    reference = tmp_path / "reference.py"
    reference.write_text(
        (build.SOLUTIONS / AGE_GROUP.name / "result.py")
        .read_text()
        .replace("/app/", f"{app.as_posix()}/")
    )
    run = _run("python")
    assert run([sys.executable, str(submitted)], app, 60)[0] == 0
    trajectory = tmp_path / "trajectory.json"
    trajectory.write_text(json.dumps({"steps": []}))
    return grade.grade(
        contract,
        AGE_GROUP / "expected",
        app / "output",
        trajectory,
        rerun=True,
        run=run,
        reference=reference,
        require_trajectory=True,
    )


def test_an_age_keyed_lookup_cannot_earn_a_perfect_score(tmp_path):
    with (AGE_GROUP / "expected/adsl.csv").open() as handle:
        rows = list(csv.DictReader(handle))
    lookup = {r["AGE"]: (r["AGEGR1"], r["AGEGR1N"]) for r in rows}
    result = _age_submission(
        tmp_path,
        f"lookup = {lookup!r}\nfor row in rows:\n"
        "    row['AGEGR1'], row['AGEGR1N'] = lookup[row['AGE']]",
    )
    assert result["outputs"][0]["passed"]
    assert result["reward"]["reproduced"] == 1
    assert result["challenge"]["checked"] and not result["challenge"]["passed"]
    assert result["reward"]["cell_accuracy"] == 0
    assert result["challenge"]["value_changes"][0]["columns"]["AGE"] == 9


def test_partial_derivations_keep_credit_across_changed_inputs(tmp_path):
    result = _age_submission(
        tmp_path,
        "for row in rows:\n    row['AGEGR1'] = ''\n    row['AGEGR1N'] = ''",
    )
    assert result["reward"]["reward"] == 0
    assert result["challenge"]["checked"]
    assert result["reward"]["policy_eligible"] == 1
    assert 0 < result["reward"]["cell_accuracy"] < 1


def test_a_failed_replay_zeroes_partial_credit(tmp_path):
    result = _rerun(tmp_path, "adam-adsl-age-group", lambda *_: (1, "failed"))
    assert result["reward"]["cell_accuracy"] == 0
    assert result["reward"]["row_accuracy"] == 0


def test_output_shape_errors_reduce_partial_credit(tmp_path):
    contract, columns, rows = _golden("adam-adsl-age-group")
    extra = {**rows[0], "USUBJID": "unrequested-subject"}
    _write(tmp_path / "adsl.csv", columns, [*rows, extra])
    result = grade.grade_output(
        contract["outputs"][0], AGE_GROUP / "expected", tmp_path
    )
    assert grade._accuracies([result]) == pytest.approx((10 / 11, 10 / 11))
    _write(tmp_path / "adsl.csv", [c for c in columns if c != "AGEGR1"], rows)
    result = grade.grade_output(
        contract["outputs"][0], AGE_GROUP / "expected", tmp_path
    )
    assert grade._accuracies([result])[1] == 0


def test_an_ill_typed_reference_cannot_validate_itself(tmp_path):
    contract, columns, rows = _golden("adam-adsl-age-group")
    _write(
        tmp_path / "adsl.csv",
        columns,
        [{**rows[0], "AGE": "not-an-integer"}, *rows[1:]],
    )
    with pytest.raises(RuntimeError, match="invalid int value in AGE"):
        grade.grade_output(contract["outputs"][0], tmp_path, tmp_path)


@pytest.mark.parametrize("subjects", [[], [{"USUBJID": "101"}]])
def test_empty_and_key_only_outputs_require_a_real_dataset(tmp_path, subjects):
    spec = {
        "file": "ids.csv",
        "columns": ["USUBJID"],
        "keys": ["USUBJID"],
        "types": {"USUBJID": "str"},
    }
    expected, output = tmp_path / "expected", tmp_path / "output"
    output.mkdir()
    _write(expected / "ids.csv", spec["columns"], subjects)
    result = grade.grade_output(spec, expected, output)
    assert grade._accuracies([result]) == (0, 0)
    _write(output / "ids.csv", spec["columns"], subjects)
    result = grade.grade_output(spec, expected, output)
    assert grade._accuracies([result]) == (1, 1)


def test_balanced_deletion_of_failures_cannot_be_ranked(tmp_path):
    job = _fake_job(tmp_path, {"b-one": [1, 0], "b-two": [1, 0]})
    board = _board(["b-one", "b-two"])
    assert leaderboard.run_problems(leaderboard.collect(job), board) == []
    for path in job.glob("*/result.json"):
        if json.loads(path.read_text())["verifier_result"]["rewards"]["reward"] == 0:
            shutil.rmtree(path.parent)
    problems = leaderboard.run_problems(leaderboard.collect(job), board)
    assert "trial counts must match the job's declared attempts" in problems
    assert any("every completed attempt" in p for p in problems)


def test_error_provenance_never_uses_a_rebuilt_task(tmp_path):
    job = _fake_job(tmp_path, {"b-one": [None]})
    before = leaderboard.collect(job)["yamaa_commit"]
    next(job.glob("*/verifier/task.toml")).unlink()
    (tmp_path / "tasks/task.toml").write_text('[metadata]\nyamaa_commit = "changed"\n')
    assert leaderboard.collect(job)["yamaa_commit"] == before
    assert leaderboard.run_problems(leaderboard.collect(job), _board(["b-one"])) == []
    (job / "evaluation.json").unlink()
    assert leaderboard.collect(job)["yamaa_commit"] == before
    next(job.glob("*/verifier/evaluation.json")).unlink()
    assert leaderboard.collect(job)["yamaa_commit"] == "unknown"


def test_a_row_cannot_mix_tool_permissions(tmp_path):
    job = _fake_job(tmp_path, {"b-one": [1, 0]})
    for index, path in enumerate(sorted(job.glob("*/result.json"))):
        result = json.loads(path.read_text())
        result["config"]["agent"]["kwargs"] = {
            "opencode_config": {"permission": {"bash": "allow" if index else "deny"}}
        }
        path.write_text(json.dumps(result))
    problems = leaderboard.run_problems(leaderboard.collect(job), _board(["b-one"]))
    assert "the job mixes effective agent configurations" in problems


def test_configuration_evidence_redacts_credentials_but_keeps_budgets():
    recorded = leaderboard.agent_configuration(
        {
            "agent": {
                "env": {
                    "OPENCODE_CONFIG_CONTENT": json.dumps(
                        {"apiKey": "private", "max_tokens": 100}
                    )
                },
                "kwargs": {
                    "api_key": "private",
                    "max_tokens": 100,
                    "headers": {"Authorization": "private"},
                },
            }
        }
    )
    assert "private" not in json.dumps(recorded)
    assert recorded["kwargs"]["max_tokens"] == 100
    assert json.loads(recorded["env"]["OPENCODE_CONFIG_CONTENT"])["max_tokens"] == 100


def test_published_boards_pin_the_task_commit_and_runtime(tmp_path):
    job = _fake_job(tmp_path, {"b-one": [1]})
    board = _board(["b-one"])
    run = _on(leaderboard.collect(job), board)
    definition, _rows = leaderboard.export(
        board, [run], "yamaa/example", tmp_path / "export"
    )
    created = yaml.safe_load(definition.read_text())
    properties = created["metadata_schema"]["properties"]
    assert created["name"].endswith(run["yamaa_commit"])
    assert properties["yamaa_commit"]["const"] == run["yamaa_commit"]
    assert properties["image_reference"]["const"] == build.IMAGE
    changed = {**run, "yamaa_commit": "fedcba9876543210fedcba9876543210fedcba98"}
    with pytest.raises(SystemExit, match="same task commit"):
        leaderboard.rows_for(board, [run, changed])
    changed = {**run, "image_references": {"another-image"}}
    assert (
        "the job does not use this board's runtime image reference"
        in leaderboard.run_problems(changed, board)
    )


def test_run_snapshot_survives_rebuilding_tasks(tmp_path):
    runner = _load("run")
    task = build.build_task(
        AGE_GROUP, tmp_path / "tasks", build.IMAGE, "test", "python"
    )
    config = {"tasks": [{"path": str(task)}]}
    saved = runner.preserve_tasks(config, tmp_path / "job")
    shutil.rmtree(task)
    assert (Path(saved["tasks"][0]["path"]) / "task.toml").is_file()
    assert (tmp_path / "job/evaluation.json").is_file()
    with pytest.raises(FileExistsError):
        runner.preserve_tasks(config, tmp_path / "job")


def test_runtime_uses_the_pinned_binary_catalog():
    dockerfile = (build.HERE / "Dockerfile").read_text()
    assert "https://models.dev/api.json" not in dockerfile
    config = build.job_config(
        [],
        model="openai/gpt-5",
        api_host=None,
        key_env=None,
        variant=None,
        n_attempts=1,
        n_concurrent=1,
        job_name="test",
        jobs_dir=build.OUT / "jobs",
    )
    assert config["agents"][0]["env"]["OPENCODE_DISABLE_MODELS_FETCH"] == "1"
    assert "OPENCODE_MODELS_PATH" not in config["agents"][0]["env"]


def test_runner_restores_evidence_after_verifier_reset(tmp_path, monkeypatch):
    import asyncio
    from types import SimpleNamespace

    job_module = pytest.importorskip("harbor.job")
    hooks = pytest.importorskip("harbor.trial.hooks")
    runner = _load("run")
    task = build.build_task(
        AGE_GROUP, tmp_path / "tasks", build.IMAGE, "test", "python"
    )
    job_dir = tmp_path / "jobs" / "test"
    verifier = job_dir / "trial" / "verifier"

    class FakeJob:
        def __init__(self):
            self.callbacks = {}

        def add_hook(self, event, callback):
            self.callbacks[event] = callback

        async def run(self):
            event = SimpleNamespace(
                task_name="yamaa/adam-adsl-age-group-python", trial_name="trial"
            )
            await self.callbacks[hooks.TrialEvent.START](event)
            before = (verifier / "evaluation.json").read_text()
            shutil.rmtree(verifier)
            verifier.mkdir()
            (verifier / "task.toml").write_text("[metadata]\nchanged = true\n")
            await self.callbacks[hooks.TrialEvent.END](event)
            assert (verifier / "evaluation.json").read_text() == before
            # Keep a differing verifier snapshot visible to the collector.
            assert "changed = true" in (verifier / "task.toml").read_text()

    async def create(_config):
        return FakeJob()

    monkeypatch.setattr(job_module.Job, "create", create)
    asyncio.run(
        runner.run(
            {
                "jobs_dir": str(tmp_path / "jobs"),
                "job_name": "test",
                "tasks": [{"path": str(task)}],
            }
        )
    )


def test_job_evidence_survives_harbors_archive_allowlist(tmp_path):
    uploader = pytest.importorskip("harbor.upload.uploader")
    job = _fake_job(tmp_path, {"b-one": [None]})
    next(job.glob("*/result.json")).with_name("lock.json").write_text("{}")
    archive = tmp_path / "job.tar.gz"
    uploader._create_job_archive_file(job, archive)
    downloaded = tmp_path / "downloaded"
    with tarfile.open(archive) as handle:
        assert "fake-job/evaluation.json" not in handle.getnames()
        handle.extractall(downloaded, filter="data")
    shutil.rmtree(job)
    shutil.rmtree(tmp_path / "tasks")
    run = leaderboard.collect(downloaded / "fake-job")
    assert leaderboard.run_problems(run, _board(["b-one"])) == []


def test_value_perturbations_preserve_parquet_types_and_partial_dates(tmp_path):
    import random

    arrow = pytest.importorskip("pyarrow")
    parquet = pytest.importorskip("pyarrow.parquet")
    table = arrow.table(
        {
            "USUBJID": ["101", "102"],
            "AGE": arrow.array([17, None], type=arrow.int32()),
            "VISITDT": ["2026-09-01", "2026-09"],
        }
    )
    path = tmp_path / "input.parquet"
    parquet.write_table(table, path)
    changes = grade._perturb_values(tmp_path, random.Random(0))
    changed = parquet.read_table(path)
    assert changed.schema == table.schema
    assert changed["USUBJID"].to_pylist() == ["101", "102"]
    assert changed["AGE"].to_pylist()[0] > 17
    assert changed["AGE"].to_pylist()[1] is None
    assert changed["VISITDT"].to_pylist()[0] != "2026-09-01"
    assert changed["VISITDT"].to_pylist()[1] == "2026-09"
    assert changes[0]["columns"] == {"AGE": 1, "VISITDT": 1}
