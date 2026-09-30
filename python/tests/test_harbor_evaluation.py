from __future__ import annotations

import csv
import importlib.util
import json
import tomllib
from pathlib import Path

import pytest

ROOT = Path(__file__).parents[2]
HARBOR = ROOT / "evaluations" / "harbor"
PILOTS = ("adam-adsl-age-group", "adam-adae-death", "adam-adtte-dor")


def _load(name: str):
    spec = importlib.util.spec_from_file_location(name, HARBOR / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


grade = _load("grade")
build = _load("build")


def _golden(benchmark: str) -> tuple[dict, list[str], list[dict[str, str]]]:
    contract = build.contract_for(ROOT / "benchmarks" / benchmark)
    output = contract["outputs"][0]
    path = ROOT / "benchmarks" / benchmark / "expected" / output["file"]
    with path.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    return contract, output["columns"], rows


def _write(path: Path, columns: list[str], rows: list[dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def _grade(tmp_path: Path, benchmark: str, columns, rows, trajectory=None) -> dict:
    contract, _, _ = _golden(benchmark)
    output = tmp_path / "output"
    _write(output / contract["outputs"][0]["file"], columns, rows)
    trajectory_path = tmp_path / "trajectory.json"
    if trajectory is not None:
        trajectory_path.write_text(json.dumps(trajectory))
    expected = ROOT / "benchmarks" / benchmark / "expected"
    return grade.grade(contract, expected, output, trajectory_path)


@pytest.mark.parametrize("benchmark", PILOTS)
def test_every_pilot_has_a_prompt(benchmark):
    assert (ROOT / "benchmarks" / benchmark / "prompt.md").is_file()


@pytest.mark.parametrize("benchmark", PILOTS)
def test_the_golden_scores_one(tmp_path, benchmark):
    _, columns, rows = _golden(benchmark)
    result = _grade(tmp_path, benchmark, columns, rows)
    assert result["passed"], result["outputs"][0]["problems"]
    assert result["reward"]["reward"] == 1.0


@pytest.mark.parametrize("benchmark", PILOTS)
def test_row_and_column_order_are_not_graded(tmp_path, benchmark):
    _, columns, rows = _golden(benchmark)
    result = _grade(tmp_path, benchmark, columns[::-1], rows[::-1])
    assert result["passed"]
    assert result["outputs"][0]["column_order_matches"] is False


def test_pandas_and_r_spellings_of_the_same_values_pass(tmp_path):
    _, columns, rows = _golden("adam-adae-death")
    styled = [
        {
            **row,
            "AESEQ": f"{float(row['AESEQ']):.1f}",
            "ASTDT": row["ASTDT"] + " 00:00:00",
            "DTHFL": row["DTHFL"] or "NA",
            "DTHDT": row["DTHDT"] or "NA",
        }
        for row in rows
    ]
    assert _grade(tmp_path, "adam-adae-death", columns, styled)["passed"]


def test_a_changed_value_fails(tmp_path):
    _, columns, rows = _golden("adam-adsl-age-group")
    rows[0] = {**rows[0], "AGEGR1N": "0"}
    result = _grade(tmp_path, "adam-adsl-age-group", columns, rows)
    assert not result["passed"]
    assert result["outputs"][0]["diffs"][0]["column"] == "AGEGR1N"
    assert 0 < result["reward"]["cell_accuracy"] < 1


def test_a_value_for_a_missing_category_fails(tmp_path):
    _, columns, rows = _golden("adam-adsl-age-group")
    rows = [{**r, "AGEGR1N": r["AGEGR1N"] or "4"} for r in rows]
    assert not _grade(tmp_path, "adam-adsl-age-group", columns, rows)["passed"]


def test_the_other_reading_of_dor_censoring_fails(tmp_path):
    _, columns, rows = _golden("adam-adtte-dor")
    other = {
        "ADT": "2025-03-24",
        "AVAL": "64",
        "CNSDTDSC": "LAST TUMOUR ASSESSMENT",
        "SRCDOM": "ADRS",
        "SRCVAR": "ADT",
        "SRCSEQ": "2",
    }
    rows = [{**r, **other} if r["USUBJID"].endswith("-102") else r for r in rows]
    result = _grade(tmp_path, "adam-adtte-dor", columns, rows)
    assert not result["passed"]
    assert {d["column"] for d in result["outputs"][0]["diffs"]} == set(other)


def test_a_dropped_row_fails(tmp_path):
    _, columns, rows = _golden("adam-adae-death")
    result = _grade(tmp_path, "adam-adae-death", columns, rows[1:])
    assert not result["passed"]
    assert "missing" in result["outputs"][0]["problems"][0]


def test_a_duplicate_key_fails(tmp_path):
    _, columns, rows = _golden("adam-adae-death")
    result = _grade(tmp_path, "adam-adae-death", columns, rows + rows[:1])
    assert not result["passed"]
    assert any("duplicate" in p for p in result["outputs"][0]["problems"])


def test_an_extra_or_missing_column_fails(tmp_path):
    _, columns, rows = _golden("adam-adsl-age-group")
    extra = [{**r, "AGEGR2": "x"} for r in rows]
    assert not _grade(tmp_path, "adam-adsl-age-group", columns + ["AGEGR2"], extra)[
        "passed"
    ]
    assert not _grade(tmp_path, "adam-adsl-age-group", columns[:-1], rows)["passed"]


def test_an_unwritten_output_fails(tmp_path):
    contract, _, _ = _golden("adam-adsl-age-group")
    expected = ROOT / "benchmarks" / "adam-adsl-age-group" / "expected"
    result = grade.grade(contract, expected, tmp_path, tmp_path / "none.json")
    assert not result["passed"]
    assert result["outputs"][0]["problems"] == ["adsl.csv was not written"]


def test_a_web_tool_call_zeroes_a_correct_answer(tmp_path):
    _, columns, rows = _golden("adam-adsl-age-group")
    trajectory = {"steps": [{"tool_calls": [{"function_name": "webfetch"}]}]}
    result = _grade(tmp_path, "adam-adsl-age-group", columns, rows, trajectory)
    assert not result["passed"]
    assert result["reward"]["web_tool_calls"] == 1.0


def test_reward_json_holds_only_finite_numbers(tmp_path):
    _, columns, rows = _golden("adam-adtte-dor")
    result = _grade(tmp_path, "adam-adtte-dor", columns, rows)
    grade.write_results(result, tmp_path / "verifier")
    reward = json.loads((tmp_path / "verifier" / "reward.json").read_text())
    assert reward["reward"] == 1.0
    assert all(isinstance(v, float) for v in reward.values())


def test_an_unknown_provider_needs_a_host_and_key(tmp_path):
    with pytest.raises(build.BuildError):
        build.job_config(
            [],
            model="acme/model-1",
            api_host=None,
            key_env=None,
            variant=None,
            n_attempts=1,
            n_concurrent=1,
            job_name=None,
            jobs_dir=tmp_path,
        )
    config = build.job_config(
        [],
        model="acme/model-1",
        api_host="llm.acme.example",
        key_env="ACME_API_KEY",
        variant=None,
        n_attempts=1,
        n_concurrent=1,
        job_name=None,
        jobs_dir=tmp_path,
    )
    agent = config["agents"][0]
    assert agent["extra_allowed_hosts"] == ["llm.acme.example"]
    assert agent["env"]["ACME_API_KEY"] == "${ACME_API_KEY}"


def test_built_tasks_and_job_validate_against_harbor(tmp_path):
    task_config = pytest.importorskip("harbor.models.task.config")
    job_module = pytest.importorskip("harbor.models.job.config")
    tasks = [
        build.build_task(ROOT / "benchmarks" / b, tmp_path, build.IMAGE, "test")
        for b in PILOTS
    ]
    for task in tasks:
        config = task_config.TaskConfig.model_validate(
            tomllib.loads((task / "task.toml").read_text())
        )
        assert config.agent.allowed_hosts == []
        assert config.verifier.environment.network_mode.value == "no-network"
        assert (task / "instruction.md").read_text() == (
            ROOT / "benchmarks" / task.name / "prompt.md"
        ).read_text()
    config = build.job_config(
        tasks,
        model="opencode-go/muse-spark-1.3-contributor",
        api_host=None,
        key_env=None,
        variant=None,
        n_attempts=1,
        n_concurrent=1,
        job_name="test",
        jobs_dir=tmp_path,
    )
    job = job_module.JobConfig.model_validate(config)
    assert job.agents[0].extra_allowed_hosts == ["opencode.ai"]


leaderboard = _load("leaderboard")


def _fake_job(root: Path, rewards: dict[str, float]) -> Path:
    job = root / "fake-job"
    job.mkdir()
    (job / "result.json").write_text(json.dumps({"started_at": "2026-09-30T04:00:00Z"}))
    for benchmark, reward in rewards.items():
        trial = job / f"{benchmark}__abc"
        trial.mkdir()
        result = {
            "task_name": f"yamaa/{benchmark}",
            "config": {"agent": {"model_name": "acme/model-1"}},
            "agent_info": {"name": "opencode", "version": "1.18.33"},
            "agent_result": {
                "n_input_tokens": 1200,
                "n_output_tokens": 300,
                "cost_usd": 0.01,
            },
            "verifier_result": {"rewards": {"reward": reward, "cell_accuracy": reward}},
            "exception_info": None,
            "agent_execution": {
                "started_at": "2026-09-30T04:00:00Z",
                "finished_at": "2026-09-30T04:01:30Z",
            },
        }
        (trial / "result.json").write_text(json.dumps(result))
    return job


def test_collect_reads_a_harbor_job(tmp_path):
    run = leaderboard.collect(_fake_job(tmp_path, {"b-one": 1.0, "b-two": 0.0}))
    assert run["model"] == "acme/model-1"
    assert run["agent_version"] == "1.18.33"
    assert [t["benchmark"] for t in run["trials"]] == ["b-one", "b-two"]
    assert run["trials"][0]["agent_seconds"] == 90.0
    page = leaderboard.render([run])
    assert "| 1 / 2 | 50.0% | 2.4k / 600 | $0.020 | 2026-09-30 |" in page
    assert "[b-one](../benchmark/b-one.html) | **pass**, 100.0% |" in page


def test_the_leaderboard_page_matches_the_recorded_results():
    page = leaderboard.render(leaderboard.load_results())
    assert leaderboard.PAGE.read_text() == page, "run leaderboard.py render"
