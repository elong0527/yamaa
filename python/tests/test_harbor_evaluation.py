from __future__ import annotations

import copy
import csv
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
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
brief = _load("brief")
PROMPTED = sorted(p.parent.name for p in build.PROMPTS.glob("*/full.md"))


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
    assert benchmark in PROMPTED


@pytest.mark.parametrize("language", LANGUAGES)
def test_every_language_has_a_system_prompt(language):
    assert (HARBOR / build.LANGUAGES[language]["system"]).is_file()


def test_every_sdtm_and_adam_benchmark_has_its_prompts_with_the_evaluation():
    benchmarks = ROOT / "benchmarks"
    positive = sorted(
        p.name for p in benchmarks.iterdir() if p.name.startswith(("sdtm-", "adam-"))
    )
    # Every sdtm-/adam- benchmark has its prompts; prompt directories may
    # also cover other benchmark prefixes (e.g. bimo-), but each must name
    # a real benchmark. The allowed prefixes stay explicit so a future
    # negative-* benchmark with a prompt directory fails loudly instead
    # of slipping through.
    prompted_prefixes = ("sdtm-", "adam-", "bimo-")
    assert all(name.startswith(prompted_prefixes) for name in PROMPTED)
    assert set(positive) <= set(PROMPTED)
    assert set(PROMPTED) <= {p.name for p in benchmarks.iterdir() if p.is_dir()}
    assert not list(benchmarks.glob("*/prompt.md"))
    for folder in sorted(p for p in build.PROMPTS.iterdir() if p.is_dir()):
        tiers = sorted(p.name for p in folder.iterdir())
        assert tiers == ["brief.md", "conventions.md", "full.md"], folder.name


@pytest.mark.parametrize("benchmark", PROMPTED)
def test_a_brief_prompt_is_its_full_prompt_without_the_rules(benchmark):
    full = (build.PROMPTS / benchmark / "full.md").read_text(encoding="utf-8")
    written = (build.PROMPTS / benchmark / "brief.md").read_text(encoding="utf-8")
    assert written == brief.brief(full), "rerun evaluations/harbor/brief.py"


# What a conventions prompt may name: a quoted value, an upper-case code or
# variable, or a number. Each must come from the full prompt.
LITERALS = re.compile(r'"[^"]*"|\b[A-Z][A-Z0-9_]+\b|\b\d+(?:\.\d+)?\b')


@pytest.mark.parametrize("benchmark", PROMPTED)
def test_a_conventions_prompt_only_narrows_the_full_prompt(benchmark):
    full = (build.PROMPTS / benchmark / "full.md").read_text(encoding="utf-8")
    text = (build.PROMPTS / benchmark / "conventions.md").read_text(encoding="utf-8")
    opening, columns, rules, paths = brief.parts(full)
    paragraphs = [p for p in re.split(r"\n[ \t]*\n", text.strip()) if p.strip()]
    assert paragraphs[:2] == [opening, columns]
    assert paragraphs[-1] == paths
    assert text == "\n\n".join(paragraphs) + "\n"
    assert len(text.splitlines()) <= 20
    kept = " ".join(" ".join(paragraphs[2:-1]).split())
    assert len(kept) <= len(" ".join(" ".join(rules).split()))
    named = set(LITERALS.findall(" ".join(full.split())))
    assert sorted(set(LITERALS.findall(kept)) - named) == []


def test_a_prompt_without_its_four_parts_is_rejected():
    full = (build.PROMPTS / "adam-adex-cumulative-dose" / "full.md").read_text()
    opening, columns, rules, paths = brief.parts(full)
    with pytest.raises(ValueError, match="rules"):
        brief.parts(f"{opening}\n\n{columns}\n\n{paths}")
    with pytest.raises(ValueError, match="last paragraph"):
        brief.parts(f"{opening}\n\n{columns}\n\n{rules[0]}\n\n{rules[0]}")
    with pytest.raises(ValueError, match="second paragraph"):
        brief.parts(f"{opening}\n\n{rules[0]}\n\n{columns}\n\n{paths}")


TIERS = ("full.md", "conventions.md", "brief.md")


@pytest.mark.parametrize("benchmark", PROMPTED)
@pytest.mark.parametrize("tier", TIERS)
def test_every_prompt_tier_asks_for_the_graded_datasets(benchmark, tier):
    try:
        contract = build.contract_for(ROOT / "benchmarks" / benchmark, "r")
    except build.BuildError as exc:
        pytest.skip(str(exc))
    text = " ".join((build.PROMPTS / benchmark / tier).read_text().split())
    asked = re.findall(
        r"in this order: ((?:[A-Z][A-Z0-9_]*, )*[A-Z][A-Z0-9_]*\b)", text
    )
    graded = [", ".join(o["columns"]) for o in contract["outputs"]]
    assert sorted(asked) == sorted(graded)
    saved = re.findall(r"/app/output/([\w-]+\.\w+)", text)
    assert sorted(saved) == sorted(o["file"] for o in contract["outputs"])


# What a prompt never mentions (automation/benchmark_prompt.md).
UNSAID = re.compile(
    r"\b(?:yamaa|yaml|spec|specifications?|schemas?|handlers?|verifications?"
    r"|derivations?)\b|\bR0\d\d\b|\bREQ-\d+",
    re.IGNORECASE,
)


@pytest.mark.parametrize("benchmark", PROMPTED)
@pytest.mark.parametrize("tier", TIERS)
def test_every_prompt_tier_keeps_to_the_recipe(benchmark, tier):
    text = (build.PROMPTS / benchmark / tier).read_text(encoding="utf-8")
    assert [m.group(0) for m in UNSAID.finditer(text)] == []
    assert [line for line in text.splitlines() if len(line) > 79] == []


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
    names = ["adam-adsl-age-group", "adam-adsl-age-quality"]
    tasks, skipped = build.build_selection(
        names,
        languages=["r"],
        tasks_dir=tmp_path,
        image=build.IMAGE,
        commit="test",
        strict=False,
    )
    assert [t.name for t in tasks] == ["adam-adsl-age-group-r"]
    assert len(skipped) == 1 and skipped[0].startswith("adam-adsl-age-quality-r: ")
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
        prompt = (build.PROMPTS / base / "full.md").read_text().strip()
        assert (task / "instruction.md").read_text() == f"{system}\n\n---\n\n{prompt}\n"
        config = tomllib.loads((task / "task.toml").read_text())
        assert config["task"]["name"] == f"yamaa/{task.name}"
        assert config["metadata"]["language"] == language
        contract = json.loads((task / "tests" / "contract.json").read_text())
        assert contract["language"] == language
        assert contract["script"] == SCRIPTS[language]
        assert SCRIPTS[language] in (task / "solution" / "solve.sh").read_text()


@pytest.mark.parametrize("tier", list(build.TIERS))
def test_a_task_carries_the_prompt_of_its_tier(tmp_path, tier):
    benchmark = ROOT / "benchmarks" / "adam-adtte-dor"
    task = build.build_task(benchmark, tmp_path, build.IMAGE, "test", "r", tier)
    name = "adam-adtte-dor-r" if tier == "full" else f"adam-adtte-dor-{tier}-r"
    assert task.name == name
    assert build.task_language(task) == "r"
    assert build.task_tier(task) == tier
    system = (HARBOR / "system-r.md").read_text().strip()
    prompt = (build.PROMPTS / "adam-adtte-dor" / f"{tier}.md").read_text().strip()
    assert (task / "instruction.md").read_text() == f"{system}\n\n---\n\n{prompt}\n"
    config = tomllib.loads((task / "task.toml").read_text())
    assert config["task"]["name"] == f"yamaa/{name}"
    assert config["metadata"]["prompt"] == tier
    readme = (task / "README.md").read_text()
    assert readme.startswith(f"# yamaa/{name}\n")
    assert f"| Prompt | {tier}: `evaluations/harbor/prompts/adam-adtte-dor/" in readme
    assert f"then the benchmark's {tier}\n  prompt verbatim" in readme
    # The reference was written from the full prompt; on another tier the
    # page says a passing oracle does not show that tier can be solved.
    unproven = "does not\nshow that this prompt alone can be solved"
    assert (unproven in readme) == (tier != "full")


def test_an_unknown_prompt_tier_is_rejected(tmp_path):
    benchmark = ROOT / "benchmarks" / PILOTS[0]
    with pytest.raises(build.BuildError):
        build.prompt_file(benchmark, "terse")
    with pytest.raises(build.BuildError):
        build.build_task(benchmark, tmp_path, build.IMAGE, "test", "r", "terse")


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
            ROOT / "benchmarks" / b, tmp_path, build.IMAGE, "test", language, tier
        )
        for b in PILOTS
        for language in LANGUAGES
        for tier in build.TIERS
    ]
    assert len(tasks) == len(PILOTS) * len(LANGUAGES) * len(build.TIERS)
    for task in tasks:
        raw = tomllib.loads((task / "task.toml").read_text())
        config = task_config.TaskConfig.model_validate(raw)
        assert config.agent.allowed_hosts == []
        assert config.verifier.environment.network_mode.value == "no-network"
        meta = raw["metadata"]
        system = (HARBOR / build.LANGUAGES[meta["language"]]["system"]).read_text()
        tier_file = build.PROMPTS / meta["benchmark"] / f"{meta['prompt']}.md"
        prompt = tier_file.read_text().strip()
        expected = f"{system.strip()}\n\n---\n\n{prompt}\n"
        assert (task / "instruction.md").read_text() == expected
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
    assert job.agents[0].override_setup_timeout_sec == build.SETUP_TIMEOUT_SEC


leaderboard = _load("leaderboard")


def _fake_job(root: Path, rewards: dict[str, list[float | None]], **config) -> Path:
    # One trial per reward; None stands for a trial that errored before grading.
    job = root / "fake-job"
    job.mkdir()
    job_id = str(uuid.uuid4())
    # Harbor writes the job's started_at in local time without a zone, and
    # each trial's in UTC: 23:59 in New York is already the next day in UTC.
    (job / "result.json").write_text(
        json.dumps(
            {
                "id": job_id,
                "started_at": "2026-09-29T23:59:00",
                "finished_at": "2026-09-30T00:30:00",
                "n_total_trials": sum(map(len, rewards.values())),
                "stats": {"n_completed_trials": sum(map(len, rewards.values()))},
            }
        )
    )
    task_dir = root / "tasks"
    task_dir.mkdir()
    (task_dir / "task.toml").write_text(
        '[metadata]\nyamaa_commit = "0123456789abcdef0123456789abcdef01234567"\n'
    )
    attempts = max(len(r) for r in rewards.values())
    (job / "config.json").write_text(json.dumps({"n_attempts": attempts, **config}))
    (job / "lock.json").write_text(json.dumps({"harbor": {"version": "0.23.0"}}))
    for benchmark, values in rewards.items():
        for attempt, reward in enumerate(values):
            name = f"{benchmark}__{attempt}"
            trial = job / name
            trial.mkdir()
            # The verifier saves the task's task.toml with its results, and
            # grade.json records that the trajectory and challenge were checked.
            (trial / "verifier").mkdir()
            (trial / "verifier" / "task.toml").write_text(
                (task_dir / "task.toml").read_text()
                + f'grading_protocol = "{build.GRADING_PROTOCOL}"\n'
                + f'image_reference = "{build.IMAGE}"\n'
            )
            (trial / "verifier" / "grade.json").write_text(
                json.dumps(
                    {"network": {"checked": True}, "challenge": {"checked": True}}
                )
            )
            graded = {"reward": reward, "cell_accuracy": reward}
            result = {
                "id": str(uuid.uuid4()),
                "trial_name": name,
                "task_name": f"yamaa/{benchmark}",
                "config": {
                    "agent": {"model_name": "acme/model-1"},
                    "task": {"path": str(task_dir)},
                    **config,
                },
                "started_at": "2026-09-30T03:59:00Z",
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
    preserved = {
        f"yamaa/{benchmark}": {
            "path": str(task_dir),
            "task": tomllib.loads(next(job.glob("*/verifier/task.toml")).read_text()),
        }
        for benchmark in rewards
    }
    (job / "evaluation.json").write_text(json.dumps({"tasks": preserved}))
    for path in job.glob("*/result.json"):
        result = json.loads(path.read_text())
        (path.parent / "verifier/evaluation.json").write_text(
            json.dumps(
                {
                    "task": preserved[result["task_name"]],
                    "expected_tasks": sorted(preserved),
                    "n_attempts": attempts,
                }
            )
        )
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


def _board_name(language: str, tier: str) -> str:
    if tier == "full":
        return f"sdtm-adam-v3-{language}"
    return f"sdtm-adam-v3-{tier}-{language}"


def test_each_language_has_a_board_per_prompt_tier():
    boards = {b["harbor"]["name"]: b for b in leaderboard.load_leaderboards()}
    assert set(boards) == {
        _board_name(language, tier) for language in LANGUAGES for tier in build.TIERS
    }
    buildable = []
    for benchmark in PROMPTED:
        # The sdtm-adam boards rank SDTM and ADaM benchmarks only, per the
        # board headers; other prefixes (e.g. bimo-) carry prompts but are
        # not board tasks.
        if not benchmark.startswith(("sdtm-", "adam-")):
            continue
        try:
            build.contract_for(ROOT / "benchmarks" / benchmark, "r")
        except build.BuildError:
            continue
        buildable.append(benchmark)
    for language in LANGUAGES:
        for tier in build.TIERS:
            board = boards[_board_name(language, tier)]
            assert sorted(board["tasks"]) == sorted(
                build.task_name(b, language, tier) for b in buildable
            )
            assert board["harbor"]["rank_by"][0]["accessor"] == "metrics.reward"


def _on(run: dict, board: dict) -> dict:
    run["leaderboards"] = [board["harbor"]["name"]]
    return run


def test_collect_reads_a_harbor_job(tmp_path):
    run = leaderboard.collect(_fake_job(tmp_path, {"b-one": [1.0], "b-two": [0.0]}))
    assert run["model"] == "acme/model-1"
    assert run["variant"] == "default"
    assert run["agent_version"] == "1.18.33"
    assert run["harbor_version"] == "0.23.0"
    assert run["attempts"] == 1 and run["default_timeouts"]
    assert [t["benchmark"] for t in run["trials"]] == ["b-one", "b-two"]
    assert all(uuid.UUID(t["id"]) for t in run["trials"])
    assert run["trials"][0]["agent_seconds"] == 90.0
    assert run["date"] == "2026-09-30"
    assert run["yamaa_commit"] == "0123456789abcdef0123456789abcdef01234567"

    board = _board(["b-one", "b-two"])
    (row,) = leaderboard.rows_for(board, [_on(run, board)])
    assert row["metadata"]["yamaa_commit"] == "0123456789abcdef0123456789abcdef01234567"
    assert row["metadata"]["date"] == "2026-09-30"
    assert row["metrics"]["reward"] == 0.5
    assert row["metrics"]["cost_usd"] == pytest.approx(0.02)


def _set_variants(job: Path, variants: list[str]) -> None:
    for path, variant in zip(sorted(job.glob("*/result.json")), variants, strict=True):
        result = json.loads(path.read_text())
        result["config"]["agent"]["kwargs"] = {"variant": variant}
        path.write_text(json.dumps(result))


def test_a_row_names_its_variant_and_a_job_may_not_mix_them(tmp_path):
    job = _fake_job(tmp_path, {"b-one": [1.0], "b-two": [1.0]})
    _set_variants(job, ["xhigh", "xhigh"])
    run = leaderboard.collect(job)
    assert run["variant"] == "xhigh"
    board = _board(["b-one", "b-two"])
    (row,) = leaderboard.rows_for(board, [_on(run, board)])
    assert row["metadata"]["variant"] == "xhigh"
    _set_variants(job, ["low", "xhigh"])
    with pytest.raises(SystemExit, match="mixes agents, models, or variants"):
        leaderboard.collect(job)


def test_each_board_names_its_language_and_tier_dataset():
    for board in leaderboard.load_leaderboards():
        # sdtm-adam-v3-<language> or sdtm-adam-v3-<tier>-<language>
        parts = board["harbor"]["name"].split("-")
        language, tier = parts[-1], parts[3] if len(parts) == 5 else "full"
        assert board["grading_protocol"] == build.GRADING_PROTOCOL
        package = build.dataset_name(build.DATASET_PREFIX, language, tier)
        assert board["package"] == package
        suffix = f"-{language}" if tier == "full" else f"-{tier}-{language}"
        assert all(task.endswith(suffix) for task in board["tasks"])
    board = _board(["b-one"])
    del board["package"]
    assert leaderboard.board_problems(board) == [
        "needs the Hub dataset package as package: <org>/<name>"
    ]


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


def test_a_run_ranks_only_on_the_tasks_and_timeouts_it_was_given(tmp_path):
    board = _board(["b-one", "b-two"], attempts=2)
    short = leaderboard.collect(_fake_job(tmp_path, {"b-one": [1.0, 1.0]}))
    assert leaderboard.run_problems(short, board) == ["b-two has 0 of 2 attempts"]
    with pytest.raises(SystemExit, match="b-two has 0 of 2 attempts"):
        leaderboard.rows_for(board, [_on(short, board)])

    slow = tmp_path / "slow"
    slow.mkdir()
    rewards = {"b-one": [1.0, 1.0], "b-two": [1.0, 1.0]}
    run = leaderboard.collect(_fake_job(slow, rewards, timeout_multiplier=2.0))
    assert leaderboard.run_problems(run, board) == [
        "the job changed the tasks' timeouts"
    ]


def test_leaderboards_and_rows_match_their_schemas(tmp_path):
    for index, board in enumerate(leaderboard.load_leaderboards()):
        assert leaderboard.board_problems(board) == []
        root_dir = tmp_path / str(index)
        root_dir.mkdir()
        run = leaderboard.collect(
            _fake_job(
                root_dir, {task: [1.0] * board["attempts"] for task in board["tasks"]}
            )
        )
        harbor = board["harbor"]
        (row,) = leaderboard.rows_for(board, [_on(run, board)])
        for root in ("metadata", "metrics"):
            schema = harbor[f"{root}_schema"]
            assert set(schema["required"]) <= set(row[root])
            for key in row[root]:
                assert leaderboard._declared(schema, key), f"{root}.{key}"


def _export(tmp_path: Path, status: str = "display") -> tuple[dict, dict, dict]:
    rewards = {"adam-adae-death-r": [1.0], "adam-adsl-age-group-r": [1.0]}
    rewards["adam-adtte-dor-r"] = [0.0]
    board = _board_named("sdtm-adam-v3-r")
    board["tasks"] = sorted(rewards)
    run = _on(leaderboard.collect(_fake_job(tmp_path, rewards)), board)
    run["status"] = status
    out = tmp_path / "hub"
    definition, rows = leaderboard.export(board, [run], "yamaa/example", out)
    return run, yaml.safe_load(definition.read_text()), yaml.safe_load(rows.read_text())


def test_export_writes_harbor_hub_configs(tmp_path):
    run, created, rows = _export(tmp_path)
    assert created["package"] == "yamaa/example"
    assert created["name"] == f"sdtm-adam-v3-r-{run['yamaa_commit']}"
    (row,) = rows["rows"]
    assert set(row) == {"metadata", "metrics", "status", "trial_ids"}
    assert row["trial_ids"] == [t["id"] for t in run["trials"]]
    assert row["metrics"]["reward"] == pytest.approx(2 / 3)
    assert row["status"] == "display"


def test_a_hidden_run_exports_a_hidden_row(tmp_path):
    _, _, rows = _export(tmp_path, status="hide")
    assert rows["rows"][0]["status"] == "hide"


def test_exported_configs_validate_against_harbor(tmp_path):
    hub = pytest.importorskip("harbor.hub.leaderboards")
    _, created, rows = _export(tmp_path)
    hub.LeaderboardCreateConfig.model_validate(created)
    hub.LeaderboardRowsCreateConfig.model_validate(rows)
    # Every board, each prompt tier's included, is a valid Hub definition.
    for board in leaderboard.load_leaderboards():
        hub.LeaderboardCreateConfig.model_validate(
            {"package": board["package"], **board["harbor"]}
        )


def _export_cli(monkeypatch, *arguments: str) -> None:
    monkeypatch.setattr("sys.argv", ["leaderboard.py", "export", *arguments])
    leaderboard.main()


def _full_job(tmp_path: Path, language: str) -> Path:
    """A fake job with one passing trial on every task of a full board."""
    tasks = _board_named(f"sdtm-adam-v3-{language}")["tasks"]
    return _fake_job(tmp_path, {task: [1.0] for task in tasks})


def test_the_export_command_reads_job_directories(tmp_path, monkeypatch, capsys):
    job = _full_job(tmp_path, "r")
    out = tmp_path / "hub"
    _export_cli(
        monkeypatch,
        "sdtm-adam-v3-r",
        str(job),
        "--package",
        "yamaa/example",
        "--out",
        str(out),
    )
    rows = yaml.safe_load((out / "sdtm-adam-v3-r.rows.yaml").read_text())
    assert rows["rows"][0]["metrics"]["reward"] == 1.0
    assert "harbor hub leaderboard row create yamaa/example/sdtm-adam-v3-r" in (
        capsys.readouterr().out
    )


def test_the_export_command_defaults_to_the_boards_dataset(
    tmp_path, monkeypatch, capsys
):
    job = _full_job(tmp_path, "python")
    out = tmp_path / "hub"
    _export_cli(monkeypatch, "sdtm-adam-v3-python", str(job), "--out", str(out))
    created = yaml.safe_load((out / "sdtm-adam-v3-python.leaderboard.yaml").read_text())
    assert created["package"] == "yamaa/yamaa-sdtm-adam-python"
    assert (
        "row create yamaa/yamaa-sdtm-adam-python/sdtm-adam-v3-python"
        in capsys.readouterr().out
    )


def test_the_export_command_refuses_a_job_off_the_board(tmp_path, monkeypatch):
    job = _fake_job(tmp_path, {"adam-adsl-age-group-r": [1.0]})
    with pytest.raises(SystemExit, match="cannot be ranked"):
        _export_cli(
            monkeypatch,
            "sdtm-adam-v3-r",
            str(job),
            "--out",
            str(tmp_path / "hub"),
        )


def test_no_results_are_kept_in_the_repository():
    assert not (HARBOR / "results").exists()
    assert not (HARBOR / "leaderboard.md").exists()


def test_a_text_value_of_spaces_is_not_missing():
    assert grade.normalize("  ", "str") == "  "
    assert grade.normalize("", "str") is None
    assert grade.normalize(" NA ", "str") is None
    assert grade.normalize("  ", "int") is None


def _rerun(tmp_path: Path, benchmark: str, write) -> dict:
    """Grade a correct submission with a fake interpreter that runs `write`
    against the output directory, as the verifier's rerun would."""
    contract, columns, rows = _golden(benchmark)
    contract["challenge_required"] = False
    output = tmp_path / "output"
    _write(output / contract["outputs"][0]["file"], columns, rows)
    (output / contract["script"]).write_text("# agent script\n")
    calls = []

    def run(command, cwd, timeout):
        calls.append(command)
        return write(output, contract, columns, rows)

    expected = ROOT / "benchmarks" / benchmark / "expected"
    result = grade.grade(
        contract, expected, output, tmp_path / "none.json", rerun=True, run=run
    )
    assert calls == [["Rscript", str(output / contract["script"])]]
    return result


def test_a_rerun_that_reproduces_the_golden_passes(tmp_path):
    def write(output, contract, columns, rows):
        _write(output / contract["outputs"][0]["file"], columns, rows)
        return 0, ""

    result = _rerun(tmp_path, "adam-adsl-age-group", write)
    assert result["passed"] and result["reward"]["reproduced"] == 1.0


def test_a_script_that_cannot_rerun_zeroes_a_correct_answer(tmp_path):
    result = _rerun(tmp_path, "adam-adsl-age-group", lambda *_: (1, "Error"))
    assert not result["passed"]
    assert result["outputs"][0]["passed"], "the submitted dataset was right"
    assert result["reward"]["reproduced"] == 0.0
    assert any("adsl.csv was not written" in p for p in result["rerun"]["problems"])


def test_a_rerun_that_writes_other_values_fails(tmp_path):
    def write(output, contract, columns, rows):
        changed = [{**rows[0], "AGEGR1N": "9"}, *rows[1:]]
        _write(output / contract["outputs"][0]["file"], columns, changed)
        return 0, ""

    result = _rerun(tmp_path, "adam-adsl-age-group", write)
    assert not result["passed"] and result["reward"]["reproduced"] == 0.0


@pytest.mark.parametrize(
    ("language", "command"),
    [("r", "python3 -c 'import pandas'"), ("python", "Rscript -e 'library(dplyr)'")],
)
def test_calling_the_other_language_zeroes_a_correct_answer(
    tmp_path, language, command
):
    _, columns, rows = _golden("adam-adsl-age-group", language)
    trajectory = {
        "steps": [
            {
                "tool_calls": [
                    {"function_name": "bash", "arguments": {"command": command}}
                ]
            }
        ]
    }
    result = _grade(
        tmp_path, "adam-adsl-age-group", columns, rows, trajectory, language
    )
    assert not result["passed"]
    assert result["reward"]["language_violations"] == 1.0


# Shell calls from real trajectories and their variants: the first list runs
# the other language, the second only mentions it.
OTHER_LANGUAGE_CALLS = {
    "r": [
        "python3 -c 'import pandas'",
        'ls -R /app && python3 -c "\nfrom datetime import date\nprint(1)\n"',
        "cd /app; /opt/yamaa-eval/venv/bin/python -V",
        "env FOO=1 python3.12 check.py | head",
        'echo "$(python3 -V)"',
        "bash -c 'pip list'",
        "uv run python -V",
        "timeout 60 python3 x.py",
        "cat <<EOF > notes.txt\nnot a call\nEOF\npython3 -V",
    ],
    "python": [
        "Rscript -e 'library(dplyr)'",
        "R -e 1",
        "R CMD BATCH x.R",
        "sudo -u agent /usr/local/bin/Rscript x.R",
        "echo `Rscript --version`",
    ],
}
OTHER_LANGUAGE_MENTIONS = {
    "r": [
        (
            'grep -inE "reticulate|system\\(|python|install\\.packages" '
            '/app/output/result.R || echo "clean"'
        ),
        'grep -Ei "python|reticulate" /app/output/result.R || echo ok',
        "Rscript /app/output/result.R  # no python here",
        "cat > /tmp/notes.md <<'EOF'\npython3 is not used\nEOF\nRscript x.R",
        "echo 'python3 x.py'",
        "ls /opt/yamaa-eval/venv/bin/python3",
    ],
    "python": [
        (
            "python3 -c \"text = open('/app/output/result.py').read()\n"
            "print('Rscript' in text, 'subprocess' in text)\""
        ),
        'grep -i -E "subprocess|Rscript|rpy" /app/output/result.py || echo none',
        "python3 /app/output/result.py && echo 'R is not used'",
    ],
}


@pytest.mark.parametrize(
    ("language", "command"),
    [(lang, c) for lang, calls in OTHER_LANGUAGE_CALLS.items() for c in calls],
)
def test_a_shell_call_that_runs_the_other_language_is_found(language, command):
    assert grade.other_language_calls(command, language), command


@pytest.mark.parametrize(
    ("language", "command"),
    [(lang, c) for lang, mentions in OTHER_LANGUAGE_MENTIONS.items() for c in mentions],
)
def test_naming_the_other_language_is_not_calling_it(language, command):
    assert grade.other_language_calls(command, language) == [], command


def test_a_shell_calls_description_is_not_scanned(tmp_path):
    trajectory = tmp_path / "trajectory.json"
    call = {
        "function_name": "bash",
        "arguments": {
            "command": "Rscript /app/output/result.R",
            "description": "Run the script; python3 is not used",
        },
    }
    trajectory.write_text(json.dumps({"steps": [{"tool_calls": [call]}]}))
    assert grade.scan_trajectory(trajectory, "r")["language_violations"] == []


@pytest.mark.parametrize(
    ("language", "text", "bridges"),
    [
        ("r", "library(reticulate)\npy_run_file('x.py')\n", True),
        ("r", 'x <- reticulate::import("os")\n', True),
        ("r", 'system("python3 x.py")\n', True),
        ("r", "# Uses dplyr only: no reticulate, no python.\nx <- 1\n", False),
        ("python", "import rpy2.robjects\n", True),
        ("python", 'subprocess.run(["Rscript", "x.R"])\n', True),
        ("python", 'os.system("/usr/local/bin/Rscript x.R")\n', True),
        ("python", '# No Rscript or rpy2 here.\nprint("Rscript" in "")\n', False),
    ],
)
def test_the_script_check_finds_bridges_not_mentions(tmp_path, language, text, bridges):
    script = SCRIPTS[language]
    (tmp_path / script).write_text(text)
    result = grade.grade_script({"script": script, "language": language}, tmp_path)
    assert result["passed"] is not bridges, result


def _held_out_case(
    tmp_path: Path,
    agent: str,
    reference: str | None,
    benchmark_name: str = "adam-adsl-age-group",
) -> dict:
    """Grade, with the verifier's reruns, a Python-track submission of
    `benchmark_name` whose script is `agent` ("reference" or "hardcode")
    against `reference` ("reference", "hardcode", or None). The changed-input
    challenge is off, so the held-out rerun is checked on its own."""
    benchmark = ROOT / "benchmarks" / benchmark_name
    contract = build.contract_for(benchmark, "python")
    contract["challenge_required"] = False
    app = tmp_path / "app"
    shutil.copytree(benchmark / "input", app / "input")
    (app / "output").mkdir()
    golden = [benchmark / "expected" / o["file"] for o in contract["outputs"]]
    texts = {
        "reference": (build.SOLUTIONS / benchmark.name / "result.py").read_text(),
        "hardcode": build.oracle_script("python", golden),
    }

    def at_app(text: str) -> str:
        """Embed the temporary app path in a script without native backslashes."""
        return text.replace("/app/", f"{app.as_posix()}/")

    (app / "output" / "result.py").write_text(at_app(texts[agent]))
    reference_path = None
    if reference:
        reference_path = tmp_path / "reference" / "result.py"
        reference_path.parent.mkdir()
        reference_path.write_text(at_app(texts[reference]))

    def run(command, cwd, timeout):
        return grade.run_script([sys.executable, *command[1:]], cwd, timeout)

    assert run([None, str(app / "output" / "result.py")], app, 60)[0] == 0
    before = {p.name: p.read_bytes() for p in (app / "input").iterdir()}
    result = grade.grade(
        contract,
        benchmark / "expected",
        app / "output",
        tmp_path / "none.json",
        rerun=True,
        run=run,
        reference=reference_path,
    )
    after = {p.name: p.read_bytes() for p in (app / "input").iterdir()}
    assert before == after, "the held-out rerun restores the inputs"
    assert (app / "output" / "result.py").is_file()
    return result


def test_a_dropped_subject_leaves_every_table_under_any_subject_column(tmp_path):
    source, target = tmp_path / "in", tmp_path / "out"
    source.mkdir()
    (source / "dm.csv").write_text("USUBJID,AGE\nS-01-101,30\nS-01-102,40\n")
    (source / "odm.csv").write_text("SubjectKey,Value\nS-01-101,a\nS-01-102,b\n")
    (source / "raw.csv").write_text("SUBJID,X\n101,1\n102,2\n")
    (source / "notes.txt").write_text("kept as is\n")
    grade._subset_inputs(source, target, {"S-01-102"})
    assert (target / "dm.csv").read_text() == "USUBJID,AGE\nS-01-101,30\n"
    assert (target / "odm.csv").read_text() == "SubjectKey,Value\nS-01-101,a\n"
    assert (target / "raw.csv").read_text() == "SUBJID,X\n101,1\n"
    assert (target / "notes.txt").read_text() == "kept as is\n"


def test_the_held_out_rerun_passes_a_script_that_derives(tmp_path):
    result = _held_out_case(tmp_path, "reference", "reference")
    assert result["held_out"]["checked"] and result["passed"]
    assert result["reward"]["held_out"] == 1.0
    assert result["held_out"]["dropped"] == [
        "YAMAA-01-103",
        "YAMAA-01-106",
        "YAMAA-01-109",
    ]


def test_the_held_out_rerun_catches_a_script_that_writes_its_rows(tmp_path):
    result = _held_out_case(tmp_path, "hardcode", "reference")
    assert result["reward"]["reproduced"] == 1.0, "the plain rerun cannot tell"
    assert result["held_out"]["checked"] and not result["held_out"]["passed"]
    assert not result["passed"] and result["reward"]["held_out"] == 0.0


def test_the_held_out_rerun_is_skipped_without_a_reference(tmp_path):
    result = _held_out_case(tmp_path, "hardcode", None)
    assert not result["held_out"]["checked"]
    assert result["held_out"]["notes"] == ["no reference solution"]
    assert "held_out" not in result["reward"]


def test_the_held_out_rerun_is_skipped_when_the_reference_is_not_per_subject(
    tmp_path,
):
    # A reference that ignores its inputs cannot write the restricted golden.
    result = _held_out_case(tmp_path, "reference", "hardcode")
    assert not result["held_out"]["checked"] and result["passed"]
    assert "not per subject" in result["held_out"]["notes"][0]


def test_a_script_that_bridges_to_the_other_language_fails(tmp_path):
    contract, columns, rows = _golden("adam-adsl-age-group", "r")
    output = tmp_path / "output"
    _write(output / contract["outputs"][0]["file"], columns, rows)
    (output / "result.R").write_text("library(reticulate)\npy_run_file('x.py')\n")
    expected = ROOT / "benchmarks" / "adam-adsl-age-group" / "expected"
    result = grade.grade(contract, expected, output, tmp_path / "none.json")
    assert not result["passed"]
    assert "calls another language" in result["script"]["problems"][0]


def test_domains_built_together_are_graded_together():
    contract = build.contract_for(ROOT / "benchmarks" / "sdtm-dm-race-ethnicity", "r")
    assert [o["file"] for o in contract["outputs"]] == ["dm.csv", "suppdm.csv"]


def test_input_schemas_stay_out_of_the_agent_sandbox(tmp_path):
    benchmark = ROOT / "benchmarks" / "adam-adsl-randomization"
    assert (benchmark / "input" / "dm.schema.yaml").is_file()
    task = build.build_task(benchmark, tmp_path, build.IMAGE, "test", "r")
    inputs = sorted(p.name for p in (task / "environment" / "input").iterdir())
    assert inputs == ["dm.parquet", "odm.csv"]


def test_the_python_oracle_writes_the_golden_bytes(tmp_path):
    """The generated Python oracle reproduces both golden datasets byte for byte."""
    benchmark = ROOT / "benchmarks" / "sdtm-dm-race-ethnicity"
    task = build.build_task(
        benchmark, tmp_path / "tasks", build.IMAGE, "test", "python"
    )
    script = build.oracle_script(
        "python", [benchmark / "expected" / n for n in ("dm.csv", "suppdm.csv")]
    )
    out = tmp_path / "out"
    out.mkdir()
    runnable = tmp_path / "result.py"
    runnable.write_text(
        script.replace("/app/output/", f"{out.as_posix()}/"), encoding="utf-8"
    )
    subprocess.run([sys.executable, str(runnable)], check=True)
    for name in ("dm.csv", "suppdm.csv"):
        assert (out / name).read_bytes() == (benchmark / "expected" / name).read_bytes()
    solve = (task / "solution" / "solve.sh").read_text()
    assert "python3 /app/output/result.py" in solve
    assert "--rerun --sandbox" in (task / "tests" / "test.sh").read_text()


# Goldens that stress the oracle's literals: plain text, text with `"""`,
# non-ASCII text, and a binary parquet file.
ORACLE_GOLDENS = (
    "adam-adsl-age-group/expected/adsl.csv",
    "adam-adsl-investigator-comment/expected/adsl.csv",
    "schema-text-functions/expected/adsl.csv",
    "schema-parquet/expected/adsl.parquet",
)
INTERPRETERS = {"r": ["Rscript"], "python": [sys.executable]}


@pytest.mark.parametrize("language", LANGUAGES)
@pytest.mark.parametrize("golden", ORACLE_GOLDENS)
def test_the_oracle_writes_each_golden_byte_for_byte(tmp_path, language, golden):
    """Each oracle preserves the golden's text or binary bytes in either track."""
    if language == "r" and shutil.which("Rscript") is None:
        pytest.skip("Rscript is not installed")
    path = ROOT / "benchmarks" / golden
    script = build.oracle_script(language, [path])
    runnable = tmp_path / SCRIPTS[language]
    runnable.write_text(
        script.replace("/app/output/", f"{tmp_path.as_posix()}/"), encoding="utf-8"
    )
    subprocess.run([*INTERPRETERS[language], str(runnable)], check=True)
    assert (tmp_path / path.name).read_bytes() == path.read_bytes()


@pytest.mark.parametrize("language", LANGUAGES)
def test_the_oracle_shows_a_text_golden_as_text(language):
    path = ROOT / "benchmarks" / "adam-adsl-age-group" / "expected" / "adsl.csv"
    script = build.oracle_script(language, [path])
    for line in path.read_text().splitlines():
        assert line in script
    assert "Not a derivation" in script
    assert "as.raw" not in script and "fromhex" not in script
    parquet = ROOT / "benchmarks" / "schema-parquet" / "expected" / "adsl.parquet"
    binary = build.oracle_script(language, [parquet])
    assert ("as.raw" if language == "r" else "fromhex") in binary


REFERENCES = sorted(
    p.relative_to(build.SOLUTIONS).as_posix()
    for p in build.SOLUTIONS.glob("*/result.*")
    if p.is_file() and p.name in SCRIPTS.values()
)


def test_prepared_sas_solutions_cover_every_prompt():
    # SAS sources are prepared separately until the openSAS Harbor track lands.
    # They must not be sent to the Python interpreter by reference discovery.
    sources = sorted(build.SOLUTIONS.glob("*/result.sas"))
    assert [p.parent.name for p in sources] == PROMPTED
    assert all(p.read_text(encoding="utf-8").strip() for p in sources)


def _r_has(*packages: str) -> bool:
    if shutil.which("Rscript") is None:
        return False
    loads = "; ".join(f"library({p})" for p in packages) or "invisible()"
    check = subprocess.run(["Rscript", "-e", loads], capture_output=True, check=False)
    return check.returncode == 0


def test_every_reference_solution_belongs_to_a_benchmark_track():
    assert REFERENCES
    for reference in REFERENCES:
        benchmark, script = reference.split("/")
        assert script in SCRIPTS.values(), reference
        assert benchmark in PROMPTED, reference
    for benchmark in PILOTS:
        for script in SCRIPTS.values():
            assert f"{benchmark}/{script}" in REFERENCES


@pytest.mark.parametrize("reference", REFERENCES)
def test_a_reference_solution_scores_one(tmp_path, reference):
    """Every available reference solution earns full credit from the grader."""
    benchmark, script = reference.split("/")
    language = "r" if script == "result.R" else "python"
    text = (build.SOLUTIONS / reference).read_text()
    if language == "r":
        libs = sorted(
            set(re.findall(r"library\(([\w.]+)", text))
            | set(re.findall(r"(\w+)::", text))
        )
        if not _r_has(*libs):
            # The solutions workflow has R and sets this, so there a missing
            # package fails the solution instead of skipping it.
            if os.environ.get("YAMAA_REQUIRE_R"):
                pytest.fail(f"Rscript with {', '.join(libs)} is not installed")
            pytest.skip("R with the solution's packages is not installed")
    shutil.copytree(
        ROOT / "benchmarks" / benchmark / "input",
        tmp_path / "input",
        ignore=shutil.ignore_patterns("*.yaml", "*.yml"),
    )
    output = tmp_path / "output"
    output.mkdir()
    runnable = output / script
    runnable.write_text(
        text.replace("/app/input", (tmp_path / "input").as_posix()).replace(
            "/app/output", output.as_posix()
        ),
        encoding="utf-8",
    )
    command = ["Rscript"] if language == "r" else [sys.executable]
    subprocess.run([*command, str(runnable)], check=True)
    contract = build.contract_for(ROOT / "benchmarks" / benchmark, language)
    expected = ROOT / "benchmarks" / benchmark / "expected"
    result = grade.grade(contract, expected, output, tmp_path / "none.json")
    assert result["reward"]["reward"] == 1.0, result


def test_the_evaluation_workflow_runs_the_images_r():
    dockerfile = (HARBOR / "Dockerfile").read_text()
    path = ROOT / ".github" / "workflows" / "harbor-evaluation.yml"
    job = yaml.safe_load(path.read_text())["jobs"]["solutions"]

    def arg(name: str) -> str:
        return re.search(rf"^ARG {name}=(\S+)$", dockerfile, re.MULTILINE).group(1)

    assert re.search(r"^FROM rocker/r-ver:\$\{R_VERSION\}$", dockerfile, re.MULTILINE)
    assert job["container"] == f"rocker/r-ver:{arg('R_VERSION')}"
    assert job["env"]["CRAN_SNAPSHOT"] == arg("CRAN_SNAPSHOT")
    repository = re.search(r"https://p3m\.dev/\S+?\$\{CRAN_SNAPSHOT\}", dockerfile)
    assert repository.group(0) in path.read_text()
    packages = re.search(r"pkgs <- c\(([^)]*)\)", dockerfile).group(1)
    assert job["env"]["R_PACKAGES"].split() == re.findall(r"'([\w.]+)'", packages)
    assert job["env"]["YAMAA_REQUIRE_R"] == "1"


@pytest.mark.parametrize("language", LANGUAGES)
def test_a_task_requires_a_derivation_reference(tmp_path, monkeypatch, language):
    script = SCRIPTS[language]
    pilot = build.build_task(
        ROOT / "benchmarks" / "adam-adtte-dor", tmp_path, build.IMAGE, "test", language
    )
    reference = build.SOLUTIONS / "adam-adtte-dor" / script
    assert (pilot / "solution" / script).read_bytes() == reference.read_bytes()
    assert "is the benchmark's reference solution" in (pilot / "README.md").read_text()
    # The held-out and changed-input reruns need a derivation to compute
    # their answers, so a benchmark without a reference is not built.
    monkeypatch.setattr(build, "SOLUTIONS", tmp_path / "no-solutions")
    with pytest.raises(build.BuildError, match="derivation reference is required"):
        build.build_task(
            ROOT / "benchmarks" / "sdtm-dm-race-ethnicity",
            tmp_path,
            build.IMAGE,
            "test",
            language,
        )


@pytest.mark.parametrize("language", LANGUAGES)
def test_a_task_readme_describes_the_task_outside_the_agent_sandbox(tmp_path, language):
    commit = "afb7fb4e39767f6256fe7a51834c2ed9c436d539"
    benchmark = ROOT / "benchmarks" / "adam-adtte-dor"
    task = build.build_task(benchmark, tmp_path, build.IMAGE, commit, language)
    readme = (task / "README.md").read_text()
    assert readme.startswith(f"# yamaa/adam-adtte-dor-{language}\n")
    assert "## The benchmark: Duration of Response" in readme
    assert "`adrs_raw.csv`, `adsl.csv`, `ds.csv`" in readme
    assert "`/app/output/adtte.csv`, keyed by `STUDYID`, `USUBJID`, `PARAMCD`" in readme
    assert f"`/app/output/{SCRIPTS[language]}`" in readme
    assert f"{build.REPO}/tree/{commit}/benchmarks/adam-adtte-dor" in readme
    prompt = "evaluations/harbor/prompts/adam-adtte-dor/full.md"
    assert f"{build.REPO}/tree/{commit}/{prompt}" in readme
    assert "`afb7fb4e`" in readme
    assert "[![" not in readme
    # The README is for reviewers: it never enters the agent's container.
    assert not list((task / "environment").rglob("README.md"))


def test_a_build_writes_one_dataset_per_language_and_one_job_per_variant(
    tmp_path, monkeypatch
):
    stale = tmp_path / "datasets" / "r" / "dataset.toml"
    stale.parent.mkdir(parents=True)
    stale.write_text("# from an earlier build\n")
    (tmp_path / "configs").mkdir()
    (tmp_path / "configs" / "old.json").write_text("{}")
    monkeypatch.setattr(
        "sys.argv",
        [
            "build.py",
            "--benchmarks",
            *PILOTS,
            "--model",
            "opencode-go/muse-spark-1.3-contributor",
            "--variant",
            "low",
            "xhigh",
            "--job-name",
            "pilot",
            "--out",
            str(tmp_path),
        ],
    )
    build.main()
    for language, other in (("r", "python"), ("python", "r")):
        dataset = tmp_path / "datasets" / language
        assert sorted(p.name for p in dataset.iterdir()) == ["README.md"]
        readme = (dataset / "README.md").read_text()
        assert readme.startswith(f"# yamaa/yamaa-sdtm-adam-{language}\n")
        assert f"`yamaa/yamaa-sdtm-adam-{other}`" in readme
        for benchmark in PILOTS:
            assert f"| `yamaa/{benchmark}-{language}` |" in readme
            assert f"`yamaa/{benchmark}-{other}`" not in readme
    configs = tmp_path / "configs"
    assert sorted(p.name for p in configs.iterdir()) == [
        f"pilot-{language}-{variant}.json"
        for language in ("python", "r")
        for variant in ("low", "xhigh")
    ]
    config = json.loads((configs / "pilot-r-xhigh.json").read_text())
    assert config["job_name"] == "pilot-r-xhigh"
    assert {t["source"] for t in config["tasks"]} == {"yamaa/yamaa-sdtm-adam-r"}
    assert config["agents"][0]["kwargs"]["variant"] == "xhigh"
    assert sorted(Path(t["path"]).name for t in config["tasks"]) == sorted(
        f"{b}-r" for b in PILOTS
    )


def test_a_build_writes_a_dataset_and_job_per_prompt_tier(tmp_path, monkeypatch):
    monkeypatch.setattr(
        "sys.argv",
        [
            "build.py",
            "--benchmarks",
            *PILOTS,
            "--prompt",
            "conventions",
            "brief",
            "--model",
            "opencode-go/muse-spark-1.3-contributor",
            "--variant",
            "low",
            "--job-name",
            "pilot",
            "--out",
            str(tmp_path),
        ],
    )
    build.main()
    assert sorted(p.name for p in (tmp_path / "datasets").iterdir()) == [
        f"{tier}-{language}"
        for tier in ("brief", "conventions")
        for language in ("python", "r")
    ]
    readme = (tmp_path / "datasets" / "brief-r" / "README.md").read_text()
    assert readme.startswith("# yamaa/yamaa-sdtm-adam-brief-r\n")
    assert "`yamaa/yamaa-sdtm-adam-brief-python`" in readme
    assert "dataset `yamaa/yamaa-sdtm-adam-r`." in readme
    for benchmark in PILOTS:
        assert f"| `yamaa/{benchmark}-brief-r` |" in readme
        assert f"`yamaa/{benchmark}-conventions-r`" not in readme
    configs = tmp_path / "configs"
    assert sorted(p.name for p in configs.iterdir()) == [
        f"pilot-{tier}-{language}-low.json"
        for tier in ("brief", "conventions")
        for language in ("python", "r")
    ]
    config = json.loads((configs / "pilot-conventions-python-low.json").read_text())
    assert config["job_name"] == "pilot-conventions-python-low"
    assert {t["source"] for t in config["tasks"]} == {
        "yamaa/yamaa-sdtm-adam-conventions-python"
    }
    assert sorted(Path(t["path"]).name for t in config["tasks"]) == sorted(
        f"{b}-conventions-python" for b in PILOTS
    )


def test_a_build_without_variants_writes_one_job_per_language(tmp_path, monkeypatch):
    monkeypatch.setattr(
        "sys.argv",
        [
            "build.py",
            "--benchmarks",
            "adam-adsl-age-group",
            "--model",
            "opencode-go/muse-spark-1.3-contributor",
            "--out",
            str(tmp_path),
        ],
    )
    build.main()
    configs = sorted(p.name for p in (tmp_path / "configs").iterdir())
    assert configs == [
        "muse-spark-1.3-contributor-python.json",
        "muse-spark-1.3-contributor-r.json",
    ]
    config = json.loads((tmp_path / "configs" / configs[1]).read_text())
    assert "variant" not in config["agents"][0]["kwargs"]


def test_a_build_removes_tasks_left_by_an_earlier_one(tmp_path, monkeypatch):
    stale = tmp_path / "tasks" / "gone-r"
    stale.mkdir(parents=True)
    monkeypatch.setattr(
        "sys.argv",
        [
            "build.py",
            "--benchmarks",
            "adam-adsl-age-group",
            "--language",
            "r",
            "--model",
            "opencode-go/muse-spark-1.3-contributor",
            "--out",
            str(tmp_path),
        ],
    )
    build.main()
    assert sorted(p.name for p in (tmp_path / "tasks").iterdir()) == [
        "adam-adsl-age-group-r"
    ]
