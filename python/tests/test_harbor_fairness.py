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
    """Grade a temporary Python age derivation against changed-input challenges."""
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
    assert properties["image_reference"]["const"] == board["image_reference"]
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
    task = build.build_task(AGE_GROUP, tmp_path / "tasks", None, "test", "python")
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
    task = build.build_task(AGE_GROUP, tmp_path / "tasks", None, "test", "python")
    job_dir = tmp_path / "jobs" / "test"
    verifier = job_dir / "trial" / "verifier"

    calls = []

    class FakeJob:
        def __init__(self):
            self.callbacks = {}

        def add_hook(self, event, callback):
            self.callbacks[event] = callback

        async def run(self):
            calls.append("run")
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
    # The assertions above live inside FakeJob.run; guard them at top level
    # so a runner that never runs the job cannot pass this test vacuously.
    assert calls == ["run"]
    assert (verifier / "evaluation.json").is_file()
    assert "changed = true" in (verifier / "task.toml").read_text()


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


def test_declared_measurement_columns_scale_only_for_their_benchmark(tmp_path):
    import random

    source = build.BENCHMARKS / "adam-advs-map" / "input"
    staged = tmp_path / "input"
    shutil.copytree(source, staged)
    # Without the benchmark, AVAL is not a built-in measurement: nothing in
    # this input changes.
    assert grade._perturb_values(staged, random.Random(1)) == []
    shutil.rmtree(staged)
    shutil.copytree(source, staged)
    changes = grade._perturb_values(staged, random.Random(1), "adam-advs-map")
    assert changes == [{"file": "advs.csv", "columns": {"AVAL": 8}}]
    _, rows = grade.read_rows(staged / "advs.csv")
    _, originals = grade.read_rows(source / "advs.csv")
    for before, after in zip(originals, rows):
        assert after["USUBJID"] == before["USUBJID"]
        if before["AVAL"] == "":
            assert after["AVAL"] == ""
        else:
            assert float(after["AVAL"]) != float(before["AVAL"])


def test_declared_measurements_name_columns_the_inputs_carry():
    declared = grade._declared_measurements()
    assert declared
    for benchmark, columns in declared.items():
        input_dir = build.BENCHMARKS / benchmark / "input"
        assert input_dir.is_dir(), benchmark
        carried = set()
        for path in input_dir.rglob("*"):
            if path.suffix == ".csv":
                header, _ = grade.read_rows(path)
                carried.update(name.upper() for name in header)
            elif path.suffix == ".parquet":
                _, parquet, _ = grade._parquet()
                carried.update(name.upper() for name in parquet.read_schema(path).names)
        assert columns <= carried, benchmark


@pytest.mark.parametrize(
    ("original", "expected"),
    [
        ("2024-12-27", "2025-01-03"),
        ("2024-02-24", "2024-03-02"),
        ("2024-12-27T08:30", "2025-01-03T08:30"),
        ("2024-12-27T08:30:07", "2025-01-03T08:30:07"),
        ("2024-12-27T08:30:07.0100", "2025-01-03T08:30:07.0100"),
        ("2024-12-27T08:30+05:30", "2025-01-03T08:30+05:30"),
        ("2024-12-27T08:30:07Z", "2025-01-03T08:30:07Z"),
        ("2024-12-27T08:30:07.0100-04:00", "2025-01-03T08:30:07.0100-04:00"),
        ("2024-12", "2024-12"),
        ("2024-02-30T08:30", "2024-02-30T08:30"),
        ("", ""),
    ],
)
def test_date_challenges_preserve_authored_precision_and_timezone(
    tmp_path, original, expected
):
    import random

    path = tmp_path / "vs.csv"
    with path.open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(["USUBJID", "VSDTC"])
        writer.writerow(["SUBJ1", original])
    before = path.read_bytes()
    changes = grade._perturb_values(tmp_path, random.Random(0))
    with path.open(newline="") as handle:
        assert list(csv.DictReader(handle)) == [{"USUBJID": "SUBJ1", "VSDTC": expected}]
    if original == expected:
        assert changes == []
        assert path.read_bytes() == before
    else:
        assert changes == [{"file": "vs.csv", "columns": {"VSDTC": 1}}]


@pytest.mark.parametrize("line_ending", ["\n", "\r\n"])
def test_subject_rewrites_preserve_padded_shape_and_line_endings(tmp_path, line_ending):
    import random

    source = tmp_path / "source"
    source.mkdir()
    rows = [
        "USUBJID,SUBJID,RFSTDTC",
        "STUDY-SITE-0001,0001,2024-12-27T08:30",
        "STUDY-SITE-0002,0002,2024-12-28T09:15",
        "STUDY-SITE-003,003,2024-12-29T10:00",
        "STUDY-SITE-00004,00004,2024-12-30T10:00",
        "STUDY-SITE-\u00b2,\u00b2,2024-12-30T10:00",
    ]
    (source / "dm.csv").write_bytes((line_ending.join(rows) + line_ending).encode())
    renamed = tmp_path / "renamed"
    assert grade._rename_subjects(source, renamed, 4123456, "-evaluation")
    _, changed = grade.read_rows(renamed / "dm.csv")
    for before, after in zip(rows[1:4], changed[:3]):
        original = before.split(",")[1]
        assert len(after["SUBJID"]) == len(original)
        assert after["SUBJID"].startswith("0")
        assert after["USUBJID"] == "STUDY-SITE-" + after["SUBJID"]
    assert int(changed[1]["SUBJID"]) - int(changed[0]["SUBJID"]) == 1
    assert changed[-1]["SUBJID"] == "\u00b2-evaluation"
    grade._perturb_values(renamed, random.Random(1))
    assert (
        (renamed / "dm.csv")
        .read_bytes()
        .splitlines(keepends=True)[0]
        .endswith(line_ending.encode())
    )
    held = tmp_path / "held"
    grade._subset_inputs(source, held, {"STUDY-SITE-0001"})
    assert (
        (held / "dm.csv")
        .read_bytes()
        .splitlines(keepends=True)[0]
        .endswith(line_ending.encode())
    )


def test_padded_offsets_are_shared_across_tables_and_parquet(tmp_path):
    import pyarrow as arrow
    from pyarrow import parquet

    source = tmp_path / "source"
    source.mkdir()
    (source / "dm.csv").write_text("USUBJID,SUBJID\nS-0001,0001\n")
    parquet.write_table(
        arrow.table({"SubjectKey": ["S-0001", "S-0998"], "PATNUM": [1, 998]}),
        source / "subjects.parquet",
    )
    target = tmp_path / "target"
    grade._rename_subjects(source, target, 4123456, "-evaluation")
    _, rows = grade.read_rows(target / "dm.csv")
    assert rows[0]["USUBJID"] == "S-0002"
    assert rows[0]["SUBJID"] == "0002"
    table = parquet.read_table(target / "subjects.parquet")
    assert table["SubjectKey"].to_pylist() == ["S-0002", "S-0999"]
    assert table["PATNUM"].to_pylist() == [4123457, 4124454]


def test_full_padded_domain_rotates_without_losing_shape_or_joins(tmp_path):
    source = tmp_path / "source"
    source.mkdir()
    (source / "dm.csv").write_text(
        "USUBJID,SUBJID\nS-000,000\nS-001,001\nS-099,099\nZERO-0,0\n"
    )
    (source / "vs.csv").write_text("USUBJID,VSORRES\nS-099,70\nS-000,80\n")
    target = tmp_path / "target"
    grade._rename_subjects(source, target, 4123456, "-evaluation")
    _, rows = grade.read_rows(target / "dm.csv")
    identifiers = [r["SUBJID"] for r in rows[:3]]
    assert len(set(identifiers)) == 3
    assert all(len(value) == 3 and value.startswith("0") for value in identifiers)
    assert identifiers[2] < identifiers[0]  # The full domain wraps at its limit.
    assert rows[-1]["SUBJID"] == "0" and rows[-1]["USUBJID"] == "ZERO-0"
    _, vs = grade.read_rows(target / "vs.csv")
    assert vs[0]["USUBJID"] == rows[2]["USUBJID"]
    assert vs[1]["USUBJID"] == rows[0]["USUBJID"]


def test_site_parse_reference_still_exercises_four_digit_subjects(tmp_path):
    benchmark = build.BENCHMARKS / "adam-adsl-site-parse"
    app = tmp_path / "app"
    grade._rename_subjects(benchmark / "input", app / "input", 6473686, "-evaluation")
    (app / "output").mkdir()
    reference = tmp_path / "reference.py"
    reference.write_text(
        (build.SOLUTIONS / benchmark.name / "result.py")
        .read_text()
        .replace("/app/", f"{app.as_posix()}/")
    )
    assert _run("python")([sys.executable, str(reference)], app, 60)[0] == 0
    _, original = grade.read_rows(benchmark / "expected/adsl.csv")
    _, changed = grade.read_rows(app / "output/adsl.csv")
    assert (
        sum(bool(row["SITEIDP"]) for row in changed)
        == sum(bool(row["SITEIDP"]) for row in original)
        == 4
    )
    assert any(row["SITEID"] == "UCLA" for row in changed)


def test_measurement_challenges_keep_standardized_results_and_groups_consistent(
    tmp_path,
):
    import random

    (tmp_path / "vs.csv").write_text(
        "USUBJID,VSORRES,VSSTRESN,VSSTRESC,AGE,AGEGR1,HEIGHTBL,WEIGHTBL\n"
        "S-001,70.0,70.0,70.0,64,<65,170,70\n"
        "S-002,180,81.6,81.6,70,>=65,180,81.6\n"
        "S-003,,,,,,,\n"
    )
    (tmp_path / "dm.csv").write_text("USUBJID,AGE\nS-001,64\n")
    changes = grade._perturb_values(tmp_path, random.Random(1))
    assert changes[0]["file"] == "vs.csv"
    assert set(changes[0]["columns"]) == {
        "VSORRES",
        "VSSTRESN",
        "VSSTRESC",
        "HEIGHTBL",
        "WEIGHTBL",
    }
    _, rows = grade.read_rows(tmp_path / "vs.csv")
    assert rows[0]["VSORRES"] == rows[0]["VSSTRESN"] == rows[0]["VSSTRESC"] == "70.7"
    assert rows[1]["VSSTRESN"] == rows[1]["VSSTRESC"] == "82.416"
    assert rows[0]["AGE"] == "64" and rows[0]["AGEGR1"] == "<65"
    assert grade.read_rows(tmp_path / "dm.csv")[1][0]["AGE"] == "64"
    assert rows[2]["VSSTRESN"] == ""


def test_measurement_challenge_keeps_numeric_codes_and_text_results(tmp_path):
    import random

    (tmp_path / "lb.csv").write_text(
        "USUBJID,LBORRES,LBSTRESN,LBSTRESC,QSSTRESN,AVAL,EXDOSE,Value\n"
        "S-001,10,10,POSITIVE,2,1,2,1\n"
    )
    changes = grade._perturb_values(tmp_path, random.Random(1))
    assert changes[0]["columns"] == {"LBORRES": 1, "LBSTRESN": 1}
    _, rows = grade.read_rows(tmp_path / "lb.csv")
    assert rows[0]["LBSTRESC"] == "POSITIVE"
    assert [rows[0][c] for c in ("QSSTRESN", "AVAL", "EXDOSE", "Value")] == [
        "2",
        "1",
        "2",
        "1",
    ]


def test_a_measurement_keyed_bmi_lookup_fails_changed_inputs(tmp_path):
    benchmark = build.BENCHMARKS / "adam-advs-bmi"
    app = _app(tmp_path, benchmark)
    contract = build.contract_for(benchmark, "python")
    fingerprint = """def fingerprints(folder):
    vs = list(csv.DictReader(open(folder + '/vs.csv', newline='')))
    height = {r['USUBJID']: r['HEIGHTBL'] for r in csv.DictReader(open(folder + '/adsl.csv', newline=''))}
    values = {}
    for row in vs:
        values.setdefault(row['USUBJID'], []).append([row['VSTESTCD'], row['VISIT'], row['VSSTRESN']])
    return {json.dumps([height.get(u, ''), sorted(v)]): u for u, v in values.items()}
"""
    _, subjects = grade.read_rows(benchmark / "input/adsl.csv")
    heights = {row["USUBJID"]: row["HEIGHTBL"] for row in subjects}
    _, readings = grade.read_rows(benchmark / "input/vs.csv")
    values = {}
    for row in readings:
        values.setdefault(row["USUBJID"], []).append(
            [row["VSTESTCD"], row["VISIT"], row["VSSTRESN"]]
        )
    known = {
        json.dumps([heights.get(subject, ""), sorted(results)]): subject
        for subject, results in values.items()
    }
    golden = (benchmark / "expected/advs.csv").read_text()
    script = f"""import csv, json
{fingerprint}
known = {known!r}
golden = list(csv.DictReader({golden!r}.splitlines()))
with open('/app/output/advs.csv', 'w', newline='') as handle:
    writer = csv.DictWriter(handle, fieldnames=list(golden[0]))
    writer.writeheader()
    for key, subject in fingerprints('/app/input').items():
        writer.writerows({{**row, 'USUBJID': subject}} for row in golden if row['USUBJID'] == known.get(key))
"""
    submitted = app / "output/result.py"
    submitted.write_text(script.replace("/app/", f"{app.as_posix()}/"))
    reference = tmp_path / "reference.py"
    reference.write_text(
        (build.SOLUTIONS / benchmark.name / "result.py")
        .read_text()
        .replace("/app/", f"{app.as_posix()}/")
    )
    run = _run("python")
    assert run([sys.executable, str(submitted)], app, 60)[0] == 0
    result = grade.grade(
        contract,
        benchmark / "expected",
        app / "output",
        tmp_path / "none.json",
        rerun=True,
        run=run,
        reference=reference,
    )
    assert result["outputs"][0]["passed"] and result["held_out"]["passed"]
    assert result["challenge"]["checked"] and not result["challenge"]["passed"]
    assert result["reward"]["reward"] == 0
    assert any(
        "VSSTRESN" in item["columns"] for item in result["challenge"]["value_changes"]
    )


@pytest.mark.parametrize("domain", ["LB", "VS"])
@pytest.mark.parametrize("reverse", [False, True])
def test_small_csv_measurements_keep_nonzero_values_and_matching_text(
    tmp_path, domain, reverse
):
    import random
    from decimal import Decimal

    numeric, textual = domain + "STRESN", domain + "STRESC"
    columns = [numeric, textual] if not reverse else [textual, numeric]
    originals = ["0.0000004", "-0.0000004", "1e-20", "-1e-20", "0", ""]
    _write(
        tmp_path / "measurements.csv",
        columns,
        [{numeric: value, textual: value} for value in originals],
    )
    changes = grade._perturb_values(tmp_path, random.Random(1))
    _, rows = grade.read_rows(tmp_path / "measurements.csv")
    assert changes == [
        {"file": "measurements.csv", "columns": {column: 4 for column in columns}}
    ]
    for original, row in zip(originals[:4], rows[:4], strict=True):
        assert Decimal(row[numeric]) == Decimal(original) * Decimal("1.01")
        assert row[textual] == row[numeric]
    assert rows[4] == {numeric: "0", textual: "0"}
    assert rows[5] == {numeric: "", textual: ""}


@pytest.mark.parametrize("domain", ["LB", "VS"])
@pytest.mark.parametrize("reverse", [False, True])
@pytest.mark.parametrize("kind", ["int8", "int64", "uint8", "uint64"])
def test_integer_parquet_measurements_change_within_schema_and_match_text(
    tmp_path, domain, reverse, kind
):
    import random

    arrow = pytest.importorskip("pyarrow")
    parquet = pytest.importorskip("pyarrow.parquet")
    numeric, textual = domain + "STRESN", domain + "STRESC"
    dtype = getattr(arrow, kind)()
    unsigned = kind.startswith("u")
    upper = (1 << (dtype.bit_width - int(not unsigned))) - 1
    originals = [70, 1, 0, upper, None]
    expected = [71, 2, 0, upper - 1, None]
    if not unsigned:
        lower = -(1 << (dtype.bit_width - 1))
        originals += [-70, -1, lower]
        expected += [-71, -2, lower + 1]
    arrays = {
        numeric: arrow.array(originals, type=dtype),
        textual: arrow.array(
            [str(value) if value is not None else None for value in originals]
        ),
    }
    if reverse:
        arrays = dict(reversed(list(arrays.items())))
    table = arrow.table(arrays)
    path = tmp_path / "measurements.parquet"
    parquet.write_table(table, path)
    changes = grade._perturb_values(tmp_path, random.Random(1))
    actual = parquet.read_table(path)
    assert actual.schema == table.schema
    assert actual[numeric].to_pylist() == expected
    assert actual[textual].to_pylist() == [
        str(value) if value is not None else None for value in expected
    ]
    assert changes == [
        {
            "file": path.name,
            "columns": {column: len(originals) - 2 for column in arrays},
        }
    ]


@pytest.mark.parametrize("domain", ["LB", "VS"])
def test_decimal_parquet_measurements_stay_decimal_and_match_text(tmp_path, domain):
    import random
    from decimal import Decimal

    arrow = pytest.importorskip("pyarrow")
    parquet = pytest.importorskip("pyarrow.parquet")
    numeric, textual = domain + "STRESN", domain + "STRESC"
    dtype = arrow.decimal128(10, 2)
    originals = [Decimal("12.34"), Decimal("-12.34"), Decimal("0.00"), None]
    expected = [Decimal("12.46"), Decimal("-12.46"), Decimal("0.00"), None]
    arrays = {
        numeric: arrow.array(originals, type=dtype),
        textual: arrow.array(
            [str(value) if value is not None else None for value in originals]
        ),
    }
    table = arrow.table(arrays)
    path = tmp_path / "measurements.parquet"
    parquet.write_table(table, path)
    changes = grade._perturb_values(tmp_path, random.Random(1))
    actual = parquet.read_table(path)
    assert actual.schema == table.schema
    assert actual[numeric].to_pylist() == expected
    assert actual[textual].to_pylist() == [
        str(value) if value is not None else None for value in expected
    ]
    assert changes == [{"file": path.name, "columns": {column: 2 for column in arrays}}]


@pytest.mark.parametrize("domain", ["LB", "VS"])
@pytest.mark.parametrize("reverse", [False, True])
@pytest.mark.parametrize(
    "storage,precision,scale,originals,expected",
    [
        (
            "decimal128",
            1,
            0,
            ["1", "-1", "9", "-9", "0", None],
            ["2", "-2", "8", "-8", "0", None],
        ),
        (
            "decimal128",
            10,
            2,
            [
                "0.01",
                "-0.01",
                "0.50",
                "-0.50",
                "99999999.99",
                "-99999999.99",
                "0.00",
                None,
            ],
            [
                "0.02",
                "-0.02",
                "0.51",
                "-0.51",
                "99999999.98",
                "-99999999.98",
                "0.00",
                None,
            ],
        ),
        (
            "decimal128",
            38,
            2,
            [
                "1234567890123456789012345678.12",
                "-1234567890123456789012345678.12",
                "999999999999999999999999999999999999.99",
                "0.00",
                None,
            ],
            [
                "1246913569024691356902469134.90",
                "-1246913569024691356902469134.90",
                "999999999999999999999999999999999999.98",
                "0.00",
                None,
            ],
        ),
        (
            "decimal128",
            38,
            30,
            [
                "0.000000000000000000000000000001",
                "-0.000000000000000000000000000001",
                "12.345678901234567890123456789012",
                "-12.345678901234567890123456789012",
                "0",
                None,
            ],
            [
                "0.000000000000000000000000000002",
                "-0.000000000000000000000000000002",
                "12.469135690246913569024691356902",
                "-12.469135690246913569024691356902",
                "0",
                None,
            ],
        ),
        (
            "decimal256",
            76,
            2,
            [
                "123456789012345678901234567890123456789012345678901234567890.12",
                "-123456789012345678901234567890123456789012345678901234567890.12",
                "99999999999999999999999999999999999999999999999999999999999999999999999999.99",
                "0.00",
                None,
            ],
            [
                "124691356902469135690246913569024691356902469135690246913569.02",
                "-124691356902469135690246913569024691356902469135690246913569.02",
                "99999999999999999999999999999999999999999999999999999999999999999999999999.98",
                "0.00",
                None,
            ],
        ),
    ],
)
def test_decimal_parquet_measurements_respect_precision_scale_and_nonzero_changes(
    tmp_path, domain, reverse, storage, precision, scale, originals, expected
):
    import random
    from decimal import ROUND_DOWN, Decimal, Inexact, Rounded, localcontext

    arrow = pytest.importorskip("pyarrow")
    parquet = pytest.importorskip("pyarrow.parquet")
    numeric, textual = domain + "STRESN", domain + "STRESC"
    dtype = getattr(arrow, storage)(precision, scale)
    values = [Decimal(value) if value is not None else None for value in originals]
    expected_values = [
        Decimal(value) if value is not None else None for value in expected
    ]
    arrays = {
        numeric: arrow.array(values, type=dtype),
        textual: arrow.array(originals),
    }
    if reverse:
        arrays = dict(reversed(list(arrays.items())))
    table = arrow.table(arrays)
    path = tmp_path / "measurements.parquet"
    parquet.write_table(table, path)
    # Field quantization intentionally rounds without altering caller context.
    with localcontext() as context:
        context.prec = 6
        context.rounding = ROUND_DOWN
        context.traps[Inexact] = True
        context.traps[Rounded] = True
        changes = grade._perturb_values(tmp_path, random.Random(1))
        assert context.prec == 6
        assert context.rounding == ROUND_DOWN
        assert context.traps[Inexact]
        assert context.traps[Rounded]
    actual = parquet.read_table(path)
    actual_values = actual[numeric].to_pylist()
    assert actual.schema == table.schema
    assert actual_values == expected_values
    for original_text, original, expected_value, text in zip(
        originals, values, actual_values, actual[textual].to_pylist(), strict=True
    ):
        if original:
            assert expected_value and expected_value != original
            assert Decimal(text) == expected_value
        else:
            assert expected_value == original
            assert text == original_text
    assert changes == [
        {
            "file": path.name,
            "columns": {column: len(originals) - 2 for column in arrays},
        }
    ]


@pytest.mark.parametrize("domain", ["LB", "VS"])
@pytest.mark.parametrize("reverse", [False, True])
@pytest.mark.parametrize("kind", ["float32", "float64"])
def test_small_parquet_measurements_match_the_float_value_actually_written(
    tmp_path, domain, reverse, kind
):
    import random

    arrow = pytest.importorskip("pyarrow")
    parquet = pytest.importorskip("pyarrow.parquet")
    numeric, textual = domain + "STRESN", domain + "STRESC"
    dtype = getattr(arrow, kind)()
    numbers = arrow.array([0.0000004, -0.0000004, 1e-20, -1e-20, 0, None], dtype)
    original = numbers.to_pylist()
    arrays = {
        numeric: numbers,
        textual: arrow.array(
            [str(value) if value is not None else None for value in original]
        ),
    }
    if reverse:
        arrays = dict(reversed(list(arrays.items())))
    table = arrow.table(arrays)
    path = tmp_path / "measurements.parquet"
    parquet.write_table(table, path)
    changes = grade._perturb_values(tmp_path, random.Random(1))
    actual = parquet.read_table(path)
    assert actual.schema == table.schema
    for before, after, text in zip(
        original[:4],
        actual[numeric].to_pylist()[:4],
        actual[textual].to_pylist()[:4],
        strict=True,
    ):
        assert abs(after) > abs(before) > 0
        assert after == pytest.approx(before * 1.01, rel=1e-6, abs=0)
        assert float(text) == after
    assert actual[numeric].to_pylist()[4:] == [0, None]
    assert actual[textual].to_pylist()[4:] == ["0.0", None]
    assert changes == [{"file": path.name, "columns": {column: 4 for column in arrays}}]
