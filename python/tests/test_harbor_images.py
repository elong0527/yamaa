"""Language runtime pairs and provenance must stay aligned throughout a job."""

import json
import tomllib

import pytest
from test_harbor_evaluation import (
    ROOT,
    _board,
    _fake_job,
    _on,
    build,
    grade,
    leaderboard,
)


@pytest.mark.parametrize("language", list(build.LANGUAGES))
def test_task_uses_the_correct_agent_and_verifier_pair(tmp_path, language):
    task = build.build_task(
        ROOT / "benchmarks/adam-adsl-age-group", tmp_path, None, "test", language
    )
    metadata = tomllib.loads((task / "task.toml").read_text())["metadata"]
    assert metadata["image_reference"] == build.AGENT_IMAGES[language]
    assert metadata["verifier_image_reference"] == build.VERIFIER_IMAGES[language]
    assert (task / "environment/Dockerfile").read_text().splitlines()[
        0
    ] == f"FROM {build.AGENT_IMAGES[language]}"
    assert (task / "tests/Dockerfile").read_text().splitlines()[
        0
    ] == f"FROM {build.VERIFIER_IMAGES[language]}"
    config = build.job_config(
        [task],
        model="opencode-go/muse-spark-1.3-contributor",
        api_host=None,
        key_env=None,
        variant=None,
        n_attempts=1,
        n_concurrent=1,
        job_name="test",
        jobs_dir=tmp_path,
    )
    agent = config["agents"][0]
    assert agent["kwargs"]["opencode_config"]["permission"] == json.loads(
        agent["env"]["OPENCODE_PERMISSION"]
    )
    assert agent["kwargs"]["opencode_config"]["permission"]["bash"]["node *"] == "deny"
    assert (
        agent["kwargs"]["opencode_config"]["permission"]["bash"].get(
            build.LANGUAGES[language]["runner"] + " *"
        )
        != "deny"
    )


@pytest.mark.parametrize("replacement", [None, "another-verifier:1"])
def test_wrong_or_missing_verifier_reference_cannot_be_ranked(tmp_path, replacement):
    job = _fake_job(tmp_path, {"b-one": [1.0]})
    snapshot = next(job.glob("*/verifier/task.toml"))
    before = snapshot.read_text()
    line = next(
        line
        for line in before.splitlines()
        if line.startswith("verifier_image_reference")
    )
    snapshot.write_text(
        before.replace(
            line,
            ""
            if replacement is None
            else f'verifier_image_reference = "{replacement}"',
        )
    )
    problems = leaderboard.run_problems(leaderboard.collect(job), _board(["b-one"]))
    assert any("verifier image reference" in problem for problem in problems)


def test_export_pins_both_runtime_references(tmp_path):
    run = leaderboard.collect(_fake_job(tmp_path, {"b-one": [1.0]}))
    board = _board(["b-one"])
    rows = leaderboard.rows_for(board, [_on(run, board)])
    definition = leaderboard.definition_for(board, rows, board["package"])
    for key in ("image_reference", "verifier_image_reference"):
        assert (
            definition["metadata_schema"]["properties"][key]["const"]
            == rows[0]["metadata"][key]
        )


@pytest.mark.parametrize("language", list(build.LANGUAGES))
@pytest.mark.parametrize(
    "command",
    ["node --version", "bun -e '1'", "env perl -v", "timeout 10 gcc --version"],
)
def test_helper_runtime_policy_is_consistent_across_tracks(language, command):
    assert grade.other_language_calls(command, language)
    assert not grade.other_language_calls(
        "grep 'node python R opensas' /app/input/dm.csv", language
    )


@pytest.mark.parametrize(
    "language, body",
    [
        ("r", 'system("node --version")'),
        ("python", 'import subprocess\nsubprocess.run(["node", "--version"])'),
    ],
)
def test_final_script_cannot_bridge_to_a_helper_runtime(tmp_path, language, body):
    contract = build.contract_for(ROOT / "benchmarks/adam-adsl-age-group", language)
    (tmp_path / contract["script"]).write_text(body)
    assert not grade.grade_script(contract, tmp_path)["passed"]
