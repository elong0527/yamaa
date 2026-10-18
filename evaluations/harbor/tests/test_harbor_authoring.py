"""The prompt writing flow never hands the writer the expected data."""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

import pytest

import yaml
from evaluations.harbor import author, brief, build, check, packet, scaffold

ROOT = Path(__file__).parents[3]
BENCHMARKS = ROOT / "benchmarks"
DEATH = BENCHMARKS / "adam-adae-death"
PROMPTED = sorted(p.parent.name for p in build.PROMPTS.glob("*/full.md"))
# Several outputs, a record-level override, a project function, a schema-linked
# input, and the pilots.
CASES = (
    "adam-adae-death",
    "adam-adsl-age-group",
    "adam-adtte-dor",
    "adam-adae-severity-override",
    "sdtm-dm-race-ethnicity",
    "adam-adsl-bmi",
    "adam-results-ae-incidence",
)


def _tree(root: Path) -> dict[str, bytes]:
    return {
        p.relative_to(root).as_posix(): p.read_bytes()
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


def _death_trace() -> list[dict]:
    """A trace of adam-adae-death's committed rules, one entry per sentence."""
    _, _, rules, _ = brief.parts((build.PROMPTS / DEATH.name / "full.md").read_text())
    return [
        {"prompt": sentence, "sources": ["spec.yaml: columns.DTHDT.derivation"]}
        for sentence in check.sentences(rules)
    ]


@pytest.mark.parametrize("benchmark", PROMPTED)
def test_the_scaffold_selects_the_outputs_the_build_grades(benchmark):
    case = BENCHMARKS / benchmark
    try:
        graded = [s["output"]["path"] for s in build.output_specs(case)]
    except build.BuildError:
        pytest.skip("not graded yet")
    assert [s["output"]["path"] for s in scaffold.output_specs(case)] == graded


@pytest.mark.parametrize("benchmark", CASES)
def test_a_filled_skeleton_passes_the_checks(benchmark):
    case = BENCHMARKS / benchmark
    skeleton = scaffold.skeleton(case)
    data = packet.check_data(case)
    assert "a skeleton placeholder is left" in check.full_problems(skeleton, data)
    filled = skeleton.replace(scaffold.RULES, "Each value is as collected.")
    filled = check.PLACEHOLDER.sub("subject", filled)
    assert check.full_problems(filled, data) == []


@pytest.mark.parametrize("benchmark", CASES)
def test_the_sources_are_the_same_without_the_expected_data(tmp_path, benchmark):
    # Built from a copy with no expected/ at all, the writer's sources and
    # inputs are byte for byte what the real benchmark gives.
    copy = tmp_path / "benchmarks" / benchmark
    shutil.copytree(
        BENCHMARKS / benchmark, copy, ignore=shutil.ignore_patterns("expected")
    )
    real = packet.write(BENCHMARKS / benchmark, tmp_path / "real")
    blind = packet.write(copy, tmp_path / "blind")
    assert _tree(real) == _tree(blind)
    assert not list(real.rglob("expected"))


def test_the_sources_hold_no_checks_prompts_or_solutions(tmp_path):
    out = packet.write(BENCHMARKS / "adam-adsl-bmi", tmp_path / "packet")
    sources = out / "sources"
    for spec in (sources / "specification").rglob("*.yaml"):
        assert "verifications:" not in spec.read_text()
        assert "row_count" not in spec.read_text()
        assert yaml.safe_load(spec.read_text())
    assert sorted(p.name for p in sources.glob("*.md")) == ["README.md", "skeleton.md"]
    assert not list(out.rglob("result.*"))
    functions = (sources / "functions" / "python.yaml").read_text()
    assert "bmi" in functions and "tests" not in functions
    assert (sources / "functions" / "yamaa_benchmarks.py").is_file()
    # The inputs are the files the evaluated agent gets.
    task = build.build_task(BENCHMARKS / "adam-adsl-bmi", tmp_path, None, "test", "r")
    assert _tree(out / "input") == _tree(task / "environment" / "input")


def test_a_rule_that_names_records_may_say_them():
    unsaid = packet.unsaid(BENCHMARKS / "adam-adae-severity-override")
    # The specification corrects these two events by name; the third
    # subject's identifier is sample data.
    assert "CATH-01-001" not in unsaid and "CATH-01-003" not in unsaid
    assert unsaid["CATH-01-002"] == "a subject identifier in the inputs"


def test_fixture_values_and_specification_internals_are_unsaid():
    unsaid = packet.unsaid(DEATH)
    assert unsaid["KN189-101"] == "a subject identifier in the inputs"
    assert unsaid["2024-02-15"] == "a date in the inputs"
    assert unsaid["DEATHEV"] == "an intermediate result of the specification"
    assert (
        unsaid["DTHAESEQ"] == "a column the output does not hold of the specification"
    )
    assert "first_available" in unsaid
    # Input and output columns are said freely.
    assert not {"AEOUT", "DTHDT", "AESEQ"} & set(unsaid)
    full = (build.PROMPTS / DEATH.name / "full.md").read_text()
    leaked = full.replace("DTHFL is Y when", "DTHFL is Y (DEATHEV) when")
    assert check.unsaid_problems(leaked, unsaid) == [
        "says 'DEATHEV', an intermediate result of the specification"
    ]


def test_a_trace_must_cover_every_sentence_from_a_real_source():
    full = (build.PROMPTS / DEATH.name / "full.md").read_text()
    data = packet.check_data(DEATH)
    trace = _death_trace()
    assert check.trace_problems(full, trace, data) == []
    readme = [{**trace[0], "sources": ['README: "the latest by start date supplies"']}]
    assert check.trace_problems(full, readme + trace[1:], data) == []
    assert check.trace_problems(full, trace[1:], data) == [
        f"no trace entry covers {trace[0]['prompt'][:60]!r}"
    ]
    for source in (
        "spec.yaml: columns.NOSUCH",
        'README: "a sentence the README never says"',
        "input/nosuch.csv: X",
    ):
        wrong = [{**trace[0], "sources": [source]}, *trace[1:]]
        assert check.trace_problems(full, wrong, data) == [
            f"trace entry 1 cites {source!r}, which no source holds"
        ]
    invented = [{"prompt": "DTHFL is always Y.", "sources": trace[0]["sources"]}]
    assert "trace entry 1 quotes text the rules do not hold" in check.trace_problems(
        full, invented + trace, data
    )
    assert check.trace_problems(full, [], data) == ["the trace has no entries"]


def test_the_full_checks_reject_a_changed_request():
    full = (build.PROMPTS / DEATH.name / "full.md").read_text()
    data = packet.check_data(DEATH)
    assert check.full_problems(full.replace("DTHCAUS, DTHDT\n", "DTHDT\n"), data) == [
        "the column lists are not the ones the outputs fix"
    ]
    assert check.full_problems(full.replace("adae.csv", "death.csv"), data) == [
        "the paths are not the ones the outputs fix"
    ]
    assert check.full_problems(full.replace("ADaM", "SDTM", 1), data) == [
        f"the opening does not start {data['opening']!r}"
    ]
    assert check.full_problems(
        full.replace("DTHFL is Y", "The YAML says DTHFL is Y"), data
    )


def test_a_procedure_is_rejected_but_rules_and_literals_pass():
    paraphrase = (
        'Filter AE to records with AEOUT "FATAL", sort the records by ASTDT, '
        "keep the last one per subject, and take the first available date."
    )
    assert check.procedure_problems(paraphrase) == [
        "reads as a procedure: 'Filter'",
        "reads as a procedure: 'sort the records'",
        "reads as a procedure: 'keep the last'",
        "reads as a procedure: 'first available'",
    ]
    rules = (
        'A "FILTER" label stays as collected. TRTSEQP is TRT01P and TRT02P '
        'joined by "/". A year alone takes the first day of that year, and ties '
        "break by earliest ASTDT, then lowest AESEQ."
    )
    assert check.procedure_problems(rules) == []


def test_the_writer_instruction_quotes_no_committed_prompt():
    # The recipe is the writer's instruction. Its examples are invented, so it
    # hands no writer a rule that a committed prompt took from the golden.
    recipe = (build.HERE / "prompts" / "recipe.md").read_text()
    examples = {
        check.words(a or b)
        for a, b in re.findall(r'`([^`\n]+)`|"([^"\n]+)"', recipe)
        if len((a or b).split()) >= 4
    }
    assert examples
    prompts = {
        name: check.words((build.PROMPTS / name / "full.md").read_text())
        for name in PROMPTED
    }
    assert [
        (example, name)
        for example in sorted(examples)
        for name, text in prompts.items()
        if example in text
    ] == []


def test_the_writing_task_runs_the_checks_on_what_it_wrote(tmp_path):
    task_config = pytest.importorskip("harbor.models.task.config")
    task = author.writing_task(DEATH, tmp_path / "tasks", "test")
    config = tomllib.loads((task / "task.toml").read_text())
    task_config.TaskConfig.model_validate(config)
    assert config["metadata"] == {
        "benchmark": DEATH.name,
        "purpose": "write",
        "prompt": "full",
        "yamaa_commit": "test",
    }
    assert config["verifier"]["environment"]["network_mode"] == "no-network"
    # The recipe is the instruction; no other document tells the writer what
    # to do.
    recipe = build.HERE / "prompts" / "recipe.md"
    assert (task / "instruction.md").read_text() == recipe.read_text()
    environment = task / "environment"
    assert (environment / "Dockerfile").read_text() == (
        f"FROM {build.AGENT_IMAGES['python']}\n"
        "COPY --chown=agent:agent sources/ /app/sources/\n"
        "COPY --chown=agent:agent input/ /app/input/\n"
    )
    assert (environment / "sources" / "skeleton.md").read_text() == scaffold.skeleton(
        DEATH
    )
    assert not list(task.rglob("expected")) and not (task / "solution").exists()
    tests = task / "tests"
    assert "check.py full /app/output/full.md" in (tests / "test.sh").read_text()
    # The verifier scores a prompt that fails its checks 0 instead of erroring.
    output = tmp_path / "output"
    output.mkdir()
    shutil.copyfile(build.PROMPTS / DEATH.name / "full.md", output / "full.md")
    command = [
        sys.executable,
        str(tests / "check.py"),
        "full",
        str(output / "full.md"),
        "--data",
        str(tests / "check.json"),
        "--trace",
        str(output / "trace.json"),
        "--reward",
        str(tmp_path / "reward.json"),
    ]
    result = subprocess.run(command, cwd=tests, capture_output=True, text=True)
    assert result.returncode == 0
    assert json.loads((tmp_path / "reward.json").read_text()) == {"reward": 0.0}
    (output / "trace.json").write_text(json.dumps(_death_trace()))
    result = subprocess.run(command, cwd=tests, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout
    assert json.loads((tmp_path / "reward.json").read_text()) == {"reward": 1.0}


def test_the_full_writer_gets_the_benchmark_sources_alone(tmp_path):
    # No earlier candidate, solver result, prompt or solution reaches the
    # writer of a full prompt: only what packet.py renders from the case.
    task = author.writing_task(DEATH, tmp_path / "tasks", "test")
    sources = task / "environment" / "sources"
    assert sorted(p.name for p in sources.iterdir()) == [
        "README.md",
        "brief.py",
        "check.json",
        "check.py",
        "rules",
        "skeleton.md",
        "specification",
    ]
    earlier = tmp_path / "round-1" / DEATH.name
    earlier.mkdir(parents=True)
    (earlier / "full.md").write_text("an earlier candidate\n")
    with pytest.raises(build.BuildError, match="sources alone"):
        author.writing_task(DEATH, tmp_path / "tasks", "test", prompts=earlier.parent)


def test_the_conventions_writer_narrows_the_full_prompt(tmp_path):
    task = author.writing_task(DEATH, tmp_path / "tasks", "test", "conventions")
    full = (build.PROMPTS / DEATH.name / "full.md").read_text()
    sources = task / "environment" / "sources"
    assert sorted(p.name for p in sources.iterdir()) == [
        "brief.py",
        "check.py",
        "full.md",
    ]
    assert (
        (sources / "full.md").read_text()
        == full
        == (task / "tests/full.md").read_text()
    )
    tiers = build.HERE / "prompts" / "README.md"
    assert (task / "instruction.md").read_text() == tiers.read_text()
    candidates = tmp_path / "prompts" / DEATH.name
    candidates.mkdir(parents=True)
    (candidates / "full.md").write_text(full.replace("most recent", "latest"))
    task = author.writing_task(
        DEATH, tmp_path / "tasks", "test", "conventions", prompts=candidates.parent
    )
    assert "latest" in (task / "environment" / "sources" / "full.md").read_text()
    assert "/app/input" not in (task / "environment" / "Dockerfile").read_text()
    test = (task / "tests" / "test.sh").read_text()
    assert (
        "check.py conventions /app/output/conventions.md --full /tests/full.md" in test
    )


def test_a_writer_folder_holds_what_the_writing_task_holds(tmp_path):
    task = author.writing_task(DEATH, tmp_path / "tasks", "test")
    folder = author.writer_folder(DEATH, tmp_path / "folders")
    assert folder.name == "adam-adae-death-write-full"
    assert sorted(p.name for p in folder.iterdir()) == [
        "input",
        "instruction.md",
        "output",
        "sources",
    ]
    for part in ("sources", "input"):
        assert _tree(folder / part) == _tree(task / "environment" / part)
    assert (folder / "instruction.md").read_text() == (
        task / "instruction.md"
    ).read_text()
    assert not list((folder / "output").iterdir())
    conventions = author.writer_folder(DEATH, tmp_path / "folders", "conventions")
    assert sorted(p.name for p in (conventions / "sources").iterdir()) == [
        "brief.py",
        "check.py",
        "full.md",
    ]
    assert not (conventions / "input").exists()
    with pytest.raises(build.BuildError, match="sources alone"):
        author.writer_folder(DEATH, tmp_path / "folders", prompts=tmp_path)


def test_a_solver_folder_holds_only_what_the_evaluated_agent_gets(tmp_path):
    candidate = (build.PROMPTS / DEATH.name / "full.md").read_text()
    folder = author.solver_folder(DEATH, candidate, "python", tmp_path / "folders")
    assert folder.name == "adam-adae-death-candidate-python"
    assert sorted(p.relative_to(folder).as_posix() for p in folder.rglob("*")) == [
        "input",
        "input/ae.csv",
        "input/dm.csv",
        "instruction.md",
        "output",
    ]
    task = build.build_task(
        DEATH, tmp_path / "tasks", None, "test", "python", candidate=candidate
    )
    assert (folder / "instruction.md").read_text() == (
        task / "instruction.md"
    ).read_text()


def test_grading_solver_folders_names_columns_and_never_a_value(tmp_path):
    candidate = (build.PROMPTS / DEATH.name / "full.md").read_text()
    folder = author.solver_folder(DEATH, candidate, "python", tmp_path)
    assert author.grade_folders(tmp_path) == {
        DEATH.name: "0 of 1 solver attempts reproduced every requested dataset.\n"
        "- adae.csv was not written: 1 of 1 attempts\n"
        "- the required script was not saved: 1 of 1 attempts\n"
    }
    output = folder / "output"
    shutil.copyfile(DEATH / "expected" / "adae.csv", output / "adae.csv")
    (output / "result.py").write_text("print('derived')\n")
    assert author.grade_folders(tmp_path) == {
        DEATH.name: "1 of 1 solver attempts reproduced every requested dataset.\n"
    }
    rows = (output / "adae.csv").read_text().splitlines(keepends=True)
    rows[1] = rows[1].replace("HEART ATTACK", "HEART FAILURE", 1)
    (output / "adae.csv").write_text("".join(rows))
    text = author.grade_folders(tmp_path)[DEATH.name]
    assert "- adae.csv AEDECOD differs: 1 of 1 attempts" in text
    assert "HEART" not in text and "KN189" not in text


def test_a_candidate_prompt_is_tried_without_a_reference_and_never_ranked(tmp_path):
    task_config = pytest.importorskip("harbor.models.task.config")
    candidate = (build.PROMPTS / DEATH.name / "full.md").read_text()
    prompts = tmp_path / "prompts" / DEATH.name
    prompts.mkdir(parents=True)
    (prompts / "full.md").write_text(candidate)
    tasks, skipped = author.solver_tasks(
        prompts.parent, tmp_path / "tasks", "test", ["python", "r"]
    )
    assert skipped == []
    assert sorted(t.name for t in tasks) == [
        "adam-adae-death-candidate-python",
        "adam-adae-death-candidate-r",
    ]
    for task in tasks:
        config = tomllib.loads((task / "task.toml").read_text())
        task_config.TaskConfig.model_validate(config)
        assert (
            config["metadata"]["grading_protocol"]
            == f"{build.GRADING_PROTOCOL}-candidate"
        )
        assert config["metadata"]["publicly_indexed"] is False
        assert candidate.strip() in (task / "instruction.md").read_text()
        assert (
            not (task / "solution").exists() and not (task / "tests/reference").exists()
        )
        assert "reference" not in (task / "tests" / "Dockerfile").read_text()
        contract = json.loads((task / "tests" / "contract.json").read_text())
        assert contract["challenge_required"] is False
        assert (
            "a candidate full prompt, not committed" in (task / "README.md").read_text()
        )
    with pytest.raises(build.BuildError, match="full prompt only"):
        build.build_task(DEATH, tmp_path, None, "test", "r", "brief", candidate="x")
    assert (
        build.task_name(DEATH.name, "r", candidate=True)
        == "adam-adae-death-candidate-r"
    )


def test_only_writing_and_candidate_tasks_run_without_publication(tmp_path):
    writing = author.writing_task(DEATH, tmp_path / "tasks", "test")
    author.runnable({"tasks": [{"path": str(writing)}]})
    ranked = build.build_task(DEATH, tmp_path / "tasks", None, "test", "python")
    with pytest.raises(ValueError, match="run evaluations with run.py"):
        author.runnable({"tasks": [{"path": str(writing)}, {"path": str(ranked)}]})
    with pytest.raises(ValueError, match="no tasks"):
        author.runnable({"tasks": []})


def test_a_writing_job_denies_web_tools_and_no_runtime(tmp_path):
    job_config = pytest.importorskip("harbor.models.job.config")
    tasks = [
        author.writing_task(DEATH, tmp_path / "tasks", "test", tier)
        for tier in author.INSTRUCTIONS
    ]
    config = build.job_config(
        tasks,
        model="opencode-go/muse-spark-1.3-contributor",
        api_host=None,
        key_env=None,
        variant=None,
        n_attempts=1,
        n_concurrent=1,
        job_name="write",
        jobs_dir=tmp_path,
        permissions=build.tool_permissions(None),
    )
    job = job_config.JobConfig.model_validate(config)
    assert job.agents[0].kwargs["opencode_config"]["permission"] == {
        "webfetch": "deny",
        "websearch": "deny",
    }
    assert all("source" not in task for task in config["tasks"])


def _trial(
    job: Path, name: str, reward: float, grade: dict | None, files: dict
) -> None:
    trial = job / f"{name}__{len(list(job.iterdir()))}"
    (trial / "verifier").mkdir(parents=True)
    (trial / "result.json").write_text(
        json.dumps(
            {
                "task_name": f"yamaa/{name}",
                "verifier_result": {"rewards": {"reward": reward}},
            }
        )
    )
    if grade is not None:
        (trial / "verifier" / "grade.json").write_text(json.dumps(grade))
    output = trial / "artifacts" / "app" / "output"
    output.mkdir(parents=True)
    for file, text in files.items():
        (output / file).write_text(text)


def test_the_report_names_columns_and_never_a_value(tmp_path):
    job = tmp_path / "job"
    job.mkdir()
    secret_key, secret_value = "KN189-104", "2031-07-04"
    failed = {
        "passed": False,
        "outputs": [
            {
                "file": "adae.csv",
                "problems": ["1 expected row(s) missing, first: ('KN189-104',)"],
                "column_accuracy": {"DTHFL": 1.0, "DTHDT": 0.5},
                "diffs": [
                    {
                        "key": secret_key,
                        "column": "DTHDT",
                        "expected": secret_value,
                        "actual": "",
                    }
                ],
            }
        ],
        "script": {"passed": True},
        "rerun": {"problems": []},
        "network": {},
    }
    _trial(job, "adam-adae-death-candidate-python", 0.0, failed, {})
    _trial(job, "adam-adae-death-candidate-r", 1.0, {"passed": True, "outputs": []}, {})
    _trial(job, "adam-adae-death-candidate-r", 0.0, None, {})
    texts = author.report([job])
    assert texts == {
        "adam-adae-death": "1 of 3 solver attempts reproduced every requested dataset.\n"
        "- adae.csv DTHDT differs: 1 of 3 attempts\n"
        "- adae.csv has missing, extra or repeated rows: 1 of 3 attempts\n"
        "- the attempt was not graded: 1 of 3 attempts\n"
    }
    assert secret_key not in texts["adam-adae-death"]
    assert secret_value not in texts["adam-adae-death"]


def test_collect_copies_what_passing_trials_wrote(tmp_path):
    job = tmp_path / "job"
    job.mkdir()
    _trial(job, "adam-adae-death-write-full", 0.0, None, {"full.md": "rejected\n"})
    _trial(
        job,
        "adam-adae-death-write-full",
        1.0,
        None,
        {"full.md": "first\n", "trace.json": "[]", "questions.md": "why?\n"},
    )
    _trial(
        job,
        "adam-adae-death-write-full",
        1.0,
        None,
        {"full.md": "second\n", "trace.json": "[]"},
    )
    _trial(
        job, "adam-adae-death-candidate-python", 1.0, None, {"result.py": "print(1)\n"}
    )
    _trial(job, "adam-adae-death-candidate-r", 0.0, None, {"result.R": "1\n"})
    into = tmp_path / "prompts"
    copied = author.collect(job, into)
    folder = into / DEATH.name
    assert sorted(p.name for p in copied) == [
        "full.md",
        "questions.md",
        "result.py",
        "trace.json",
    ]
    assert (folder / "full.md").read_text() == "first\n"
    assert not (folder / "result.R").exists()
