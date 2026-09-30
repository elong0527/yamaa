from __future__ import annotations

import copy
import csv
import importlib.util
import json
import tomllib
import uuid
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).parents[2]
HARBOR = ROOT / "evaluations" / "harbor"
PILOTS = ("adam-adsl-age-group", "adam-adae-death", "adam-adtte-dor")
LANGUAGES = ("r", "python")
SCRIPTS = {"r": "result.R", "python": "result.py"}


def _load(name: str):
    spec = importlib.util.spec_from_file_location(name, HARBOR / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


grade = _load("grade")
build = _load("build")


def _golden(
    benchmark: str, language: str = "r"
) -> tuple[dict, list[str], list[dict[str, str]]]:
    contract = build.contract_for(ROOT / "benchmarks" / benchmark, language)
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


def _grade(
    tmp_path: Path,
    benchmark: str,
    columns,
    rows,
    trajectory=None,
    language: str = "r",
    script: bool = True,
) -> dict:
    contract, _, _ = _golden(benchmark, language)
    output = tmp_path / "output"
    _write(output / contract["outputs"][0]["file"], columns, rows)
    if script:
        output.mkdir(parents=True, exist_ok=True)
        (output / contract["script"]).write_text("# agent script\n")
    trajectory_path = tmp_path / "trajectory.json"
    if trajectory is not None:
        trajectory_path.write_text(json.dumps(trajectory))
    expected = ROOT / "benchmarks" / benchmark / "expected"
    return grade.grade(contract, expected, output, trajectory_path)


@pytest.mark.parametrize("benchmark", PILOTS)
def test_every_pilot_has_a_prompt(benchmark):
    assert (ROOT / "benchmarks" / benchmark / "prompt.md").is_file()


@pytest.mark.parametrize("language", LANGUAGES)
def test_every_language_has_a_system_prompt(language):
    assert (HARBOR / build.LANGUAGES[language]["system"]).is_file()


@pytest.mark.parametrize("benchmark", PILOTS)
@pytest.mark.parametrize("language", LANGUAGES)
def test_the_golden_scores_one(tmp_path, benchmark, language):
    _, columns, rows = _golden(benchmark, language)
    result = _grade(tmp_path, benchmark, columns, rows, language=language)
    assert result["passed"], result["outputs"][0]["problems"]
    assert result["reward"]["reward"] == 1.0


@pytest.mark.parametrize("benchmark", PILOTS)
@pytest.mark.parametrize("language", LANGUAGES)
def test_row_and_column_order_are_not_graded(tmp_path, benchmark, language):
    _, columns, rows = _golden(benchmark, language)
    result = _grade(tmp_path, benchmark, columns[::-1], rows[::-1], language=language)
    assert result["passed"]
    assert result["outputs"][0]["column_order_matches"] is False


@pytest.mark.parametrize("language", LANGUAGES)
def test_a_missing_script_fails(tmp_path, language):
    _, columns, rows = _golden("adam-adae-death", language)
    result = _grade(
        tmp_path, "adam-adae-death", columns, rows, language=language, script=False
    )
    assert not result["passed"]
    assert result["script"]["problems"] == [f"{SCRIPTS[language]} was not written"]


@pytest.mark.parametrize("language", LANGUAGES)
def test_an_empty_script_fails(tmp_path, language):
    contract, _, _ = _golden("adam-adsl-age-group", language)
    _, columns, rows = _golden("adam-adsl-age-group", language)
    output = tmp_path / "output"
    _write(output / contract["outputs"][0]["file"], columns, rows)
    (output / contract["script"]).write_text("  \n")
    expected = ROOT / "benchmarks" / "adam-adsl-age-group" / "expected"
    result = grade.grade(contract, expected, output, tmp_path / "none.json")
    assert not result["passed"]
    assert result["script"]["problems"] == [f"{contract['script']} is empty"]


@pytest.mark.parametrize("language", LANGUAGES)
def test_a_non_utf8_script_fails_instead_of_erroring(tmp_path, language):
    contract, _, _ = _golden("adam-adsl-age-group", language)
    _, columns, rows = _golden("adam-adsl-age-group", language)
    output = tmp_path / "output"
    _write(output / contract["outputs"][0]["file"], columns, rows)
    (output / contract["script"]).write_bytes(b"\xff\xfe\x00binary")
    expected = ROOT / "benchmarks" / "adam-adsl-age-group" / "expected"
    result = grade.grade(contract, expected, output, tmp_path / "none.json")
    assert not result["passed"]
    assert result["reward"]["reward"] == 0.0
    (problem,) = result["script"]["problems"]
    assert problem.startswith(f"{contract['script']} cannot be read: ")


def test_an_unknown_language_is_rejected():
    with pytest.raises(build.BuildError):
        build.contract_for(ROOT / "benchmarks" / PILOTS[0], "julia")
    with pytest.raises(build.BuildError):
        build.system_prompt("julia")


def test_default_selection_skips_benchmarks_without_grader_support(tmp_path):
    names = ["adam-adsl-age-group", "adam-adsl-bmi"]
    tasks, skipped = build.build_selection(
        names,
        languages=["r"],
        tasks_dir=tmp_path,
        image=build.IMAGE,
        commit="test",
        strict=False,
    )
    assert [t.name for t in tasks] == ["adam-adsl-age-group-r"]
    assert len(skipped) == 1 and skipped[0].startswith("adam-adsl-bmi-r: ")
    with pytest.raises(build.BuildError):
        build.build_selection(
            names,
            languages=["r"],
            tasks_dir=tmp_path,
            image=build.IMAGE,
            commit="test",
            strict=True,
        )


@pytest.mark.parametrize("language", LANGUAGES)
def test_tasks_carry_the_system_prompt_and_script(tmp_path, language):
    tasks = [
        build.build_task(
            ROOT / "benchmarks" / b, tmp_path, build.IMAGE, "test", language
        )
        for b in PILOTS
    ]
    assert sorted(t.name for t in tasks) == sorted(f"{b}-{language}" for b in PILOTS)
    for task in tasks:
        base = task.name.rpartition("-")[0]
        system = (HARBOR / build.LANGUAGES[language]["system"]).read_text().strip()
        prompt = (ROOT / "benchmarks" / base / "prompt.md").read_text().strip()
        assert (task / "instruction.md").read_text() == f"{system}\n\n---\n\n{prompt}\n"
        config = tomllib.loads((task / "task.toml").read_text())
        assert config["task"]["name"] == f"yamaa/{task.name}"
        assert config["metadata"]["language"] == language
        contract = json.loads((task / "tests" / "contract.json").read_text())
        assert contract["language"] == language
        assert contract["script"] == SCRIPTS[language]
        assert SCRIPTS[language] in (task / "solution" / "solve.sh").read_text()


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
    assert result["script"]["problems"] == ["result.R was not written"]


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
        build.build_task(
            ROOT / "benchmarks" / b, tmp_path, build.IMAGE, "test", language
        )
        for b in PILOTS
        for language in LANGUAGES
    ]
    assert len(tasks) == len(PILOTS) * len(LANGUAGES)
    for task in tasks:
        config = task_config.TaskConfig.model_validate(
            tomllib.loads((task / "task.toml").read_text())
        )
        assert config.agent.allowed_hosts == []
        assert config.verifier.environment.network_mode.value == "no-network"
        base, _, language = task.name.rpartition("-")
        system = (HARBOR / build.LANGUAGES[language]["system"]).read_text().strip()
        prompt = (ROOT / "benchmarks" / base / "prompt.md").read_text().strip()
        assert (task / "instruction.md").read_text() == f"{system}\n\n---\n\n{prompt}\n"
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


def _fake_job(root: Path, rewards: dict[str, list[float | None]], **config) -> Path:
    # One trial per reward; None stands for a trial that errored before grading.
    job = root / "fake-job"
    job.mkdir()
    job_id = str(uuid.uuid4())
    (job / "result.json").write_text(
        json.dumps({"id": job_id, "started_at": "2026-09-30T04:00:00Z"})
    )
    attempts = max(len(r) for r in rewards.values())
    (job / "config.json").write_text(json.dumps({"n_attempts": attempts, **config}))
    (job / "lock.json").write_text(json.dumps({"harbor": {"version": "0.23.0"}}))
    for benchmark, values in rewards.items():
        for attempt, reward in enumerate(values):
            name = f"{benchmark}__{attempt}"
            trial = job / name
            trial.mkdir()
            graded = {"reward": reward, "cell_accuracy": reward}
            result = {
                "id": str(uuid.uuid4()),
                "trial_name": name,
                "task_name": f"yamaa/{benchmark}",
                "config": {"agent": {"model_name": "acme/model-1"}, **config},
                "agent_info": {"name": "opencode", "version": "1.18.33"},
                "agent_result": {
                    "n_input_tokens": 1200,
                    "n_output_tokens": 300,
                    "cost_usd": 0.01,
                },
                "verifier_result": None if reward is None else {"rewards": graded},
                "exception_info": (
                    {"exception_type": "AgentTimeoutError"} if reward is None else None
                ),
                "agent_execution": {
                    "started_at": "2026-09-30T04:00:00Z",
                    "finished_at": "2026-09-30T04:01:30Z",
                },
            }
            (trial / "result.json").write_text(json.dumps(result))
    return job


def _board(tasks: list[str], attempts: int = 1) -> dict:
    board = copy.deepcopy(leaderboard.load_leaderboards()[0])
    board["tasks"], board["attempts"] = tasks, attempts
    return board


def _board_named(name: str) -> dict:
    for board in leaderboard.load_leaderboards():
        if board["harbor"]["name"] == name:
            return copy.deepcopy(board)
    raise AssertionError(f"no leaderboard {name!r}")


def test_two_language_boards_cover_the_pilots():
    boards = {b["harbor"]["name"]: b for b in leaderboard.load_leaderboards()}
    assert set(boards) == {"adam-pilot-python", "adam-pilot-r"}
    cases = (("r", boards["adam-pilot-r"]), ("python", boards["adam-pilot-python"]))
    for language, board in cases:
        assert sorted(board["tasks"]) == sorted(f"{b}-{language}" for b in PILOTS)


def _on(run: dict, board: dict) -> dict:
    run["leaderboards"] = [board["harbor"]["name"]]
    return run


def test_collect_reads_a_harbor_job(tmp_path):
    run = leaderboard.collect(_fake_job(tmp_path, {"b-one": [1.0], "b-two": [0.0]}))
    assert run["model"] == "acme/model-1"
    assert run["agent_version"] == "1.18.33"
    assert run["harbor_version"] == "0.23.0"
    assert run["attempts"] == 1 and run["default_timeouts"]
    assert [t["benchmark"] for t in run["trials"]] == ["b-one", "b-two"]
    assert all(uuid.UUID(t["id"]) for t in run["trials"])
    assert run["trials"][0]["agent_seconds"] == 90.0

    board = _board(["b-one", "b-two"])
    page = leaderboard.render([board], [_on(run, board)])
    assert (
        "| 1 | opencode | 1.18.33 | acme/model-1 | 50.0% | 50.0% | 0 "
        "| 2.4k | 600 | $0.020 | 2026-09-30 | [fake-job](" in page
    )
    assert "[b-one](../benchmark/b-one.html) | **pass**, 100.0% |" in page


def test_an_errored_trial_counts_as_a_failure(tmp_path):
    # Harbor's mean treats a trial without a reward as 0.
    run = leaderboard.collect(_fake_job(tmp_path, {"b-one": [1.0], "b-two": [None]}))
    metrics = leaderboard.metrics(run["trials"])
    assert metrics["reward"] == 0.5
    assert metrics["n_errors"] == 1 and metrics["n_trials"] == 2


def test_pass_at_k_averages_the_unbiased_estimate_over_tasks(tmp_path):
    rewards = {"b-one": [1.0, 0.0, 1.0, 0.0, 0.0], "b-two": [1.0] * 5}
    run = leaderboard.collect(_fake_job(tmp_path, rewards))
    # b-one: 1 - C(3, 2) / C(5, 2) = 0.7 at k = 2; b-two always passes.
    assert leaderboard.pass_at_k(run["trials"]) == pytest.approx(
        {2: 0.85, 4: 1.0, 5: 1.0}
    )
    assert leaderboard.metrics(run["trials"])["pass_at_2"] == pytest.approx(0.85)


def test_pass_at_k_matches_harbor():
    harbor = pytest.importorskip("harbor.utils.pass_at_k")
    assert leaderboard._eligible_k(20) == harbor._eligible_k_values(20)
    for n, c, k in [(5, 2, 2), (5, 0, 4), (10, 3, 5), (8, 8, 2)]:
        assert leaderboard._pass_at_k_for_task(n, c, k) == pytest.approx(
            harbor._pass_at_k_for_task(n, c, k)
        )


def _row(reward, cells, cost):
    return {
        "metadata": {"model": f"m-{reward}-{cells}-{cost}"},
        "metrics": {"reward": reward, "cell_accuracy": cells, "cost_usd": cost},
    }


RANK_ROWS = [
    _row(0.5, 0.9, 0.2),
    _row(1.0, 1.0, None),
    _row(1.0, 1.0, 0.3),
    _row(None, 1.0, 0.1),
    _row(1.0, 0.8, 0.1),
]


def test_rank_orders_rows_by_the_rules_in_turn():
    rank_by = leaderboard.load_leaderboards()[0]["harbor"]["rank_by"]
    ranked = leaderboard.rank(RANK_ROWS, rank_by)
    assert [r["metadata"]["model"] for r in ranked] == [
        "m-1.0-1.0-0.3",
        "m-1.0-1.0-None",
        "m-1.0-0.8-0.1",
        "m-0.5-0.9-0.2",
        "m-None-1.0-0.1",
    ]


def test_rank_matches_harbor():
    rank_by = leaderboard.load_leaderboards()[0]["harbor"]["rank_by"]
    ranked = leaderboard.rank(RANK_ROWS, rank_by)
    hub = pytest.importorskip("harbor.hub.leaderboards")
    expected = hub.sort_rows(
        [hub.LeaderboardRow.from_row(r) for r in RANK_ROWS], rank_by
    )
    assert [r["metadata"] for r in ranked] == [r.metadata for r in expected]


def test_a_run_ranks_only_on_the_tasks_and_timeouts_it_was_given(tmp_path):
    board = _board(["b-one", "b-two"], attempts=2)
    short = leaderboard.collect(_fake_job(tmp_path, {"b-one": [1.0, 1.0]}))
    assert leaderboard.run_problems(short, board) == ["b-two has 0 of 2 attempts"]
    with pytest.raises(SystemExit, match="b-two has 0 of 2 attempts"):
        leaderboard.render([board], [_on(short, board)])

    slow = tmp_path / "slow"
    slow.mkdir()
    rewards = {"b-one": [1.0, 1.0], "b-two": [1.0, 1.0]}
    run = leaderboard.collect(_fake_job(slow, rewards, timeout_multiplier=2.0))
    assert leaderboard.run_problems(run, board) == [
        "the job changed the tasks' timeouts"
    ]


def test_a_hidden_run_stays_off_the_page(tmp_path):
    board = _board(["b-one"])
    run = _on(leaderboard.collect(_fake_job(tmp_path, {"b-one": [1.0]})), board)
    run["status"] = "hide"
    page = leaderboard.render([board], [run])
    assert "No runs are recorded on this leaderboard yet." in page


def test_committed_leaderboards_and_rows_match_their_schemas():
    runs = leaderboard.load_results()
    for board in leaderboard.load_leaderboards():
        assert leaderboard.board_problems(board) == []
        harbor = board["harbor"]
        for row in leaderboard.rows_for(board, runs):
            for root in ("metadata", "metrics"):
                schema = harbor[f"{root}_schema"]
                assert set(schema["required"]) <= set(row[root])
                for key in row[root]:
                    assert leaderboard._declared(schema, key), f"{root}.{key}"


def _export(tmp_path: Path) -> tuple[dict, dict, dict]:
    rewards = {"adam-adae-death-r": [1.0], "adam-adsl-age-group-r": [1.0]}
    rewards["adam-adtte-dor-r"] = [0.0]
    board = _board_named("adam-pilot-r")
    run = _on(leaderboard.collect(_fake_job(tmp_path, rewards)), board)
    out = tmp_path / "hub"
    definition, rows = leaderboard.export(board, [run], "yamaa/benchmarks", out)
    return run, yaml.safe_load(definition.read_text()), yaml.safe_load(rows.read_text())


def test_export_writes_harbor_hub_configs(tmp_path):
    run, created, rows = _export(tmp_path)
    assert created["package"] == "yamaa/benchmarks"
    assert created["name"] == "adam-pilot-r"
    (row,) = rows["rows"]
    assert set(row) == {"metadata", "metrics", "status", "trial_ids"}
    assert row["trial_ids"] == [t["id"] for t in run["trials"]]
    assert row["metrics"]["reward"] == pytest.approx(2 / 3)


def test_exported_configs_validate_against_harbor(tmp_path):
    hub = pytest.importorskip("harbor.hub.leaderboards")
    _, created, rows = _export(tmp_path)
    hub.LeaderboardCreateConfig.model_validate(created)
    hub.LeaderboardRowsCreateConfig.model_validate(rows)


def test_the_leaderboard_page_matches_the_recorded_results():
    page = leaderboard.render(
        leaderboard.load_leaderboards(), leaderboard.load_results()
    )
    assert leaderboard.PAGE.read_text() == page, "run leaderboard.py render"
