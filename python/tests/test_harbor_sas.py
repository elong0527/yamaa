from __future__ import annotations

import csv
import importlib.util
import json
import shutil
import subprocess
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).parents[2]
HARBOR = ROOT / "evaluations" / "harbor"


def _load(name):
    spec = importlib.util.spec_from_file_location(name, HARBOR / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


build, grade = _load("build"), _load("grade")
PROMPTED = sorted(p.parent.name for p in build.PROMPTS.glob("*/full.md"))
CONFIDENT = sorted(set(PROMPTED) - set(build.SAS_EXCLUSIONS))


def test_sas_selection_builds_all_134_native_input_cases(tmp_path):
    tasks, skipped = build.build_selection(
        PROMPTED,
        languages=["sas"],
        tasks_dir=tmp_path,
        image=build.IMAGE,
        commit="test",
        strict=False,
    )
    assert len(tasks) == len(CONFIDENT) == 134
    assert sorted(t.name for t in tasks) == sorted(f"{name}-sas" for name in CONFIDENT)
    assert len(skipped) == 3
    for name, reason in build.SAS_EXCLUSIONS.items():
        assert any(name in note and reason in note for note in skipped)
    for task in tasks:
        contract = json.loads((task / "tests/contract.json").read_text())
        assert contract["script"] == "result.sas"
        assert "sas /app/output/result.sas" in (task / "solution/solve.sh").read_text()
        reference = build.SOLUTIONS / contract["benchmark"] / "result.sas"
        assert (
            task / "tests/reference/result.sas"
        ).read_bytes() == reference.read_bytes()
        assert all(p.suffix == ".csv" for p in (task / "environment/input").iterdir())


@pytest.mark.parametrize("name", list(build.SAS_EXCLUSIONS))
def test_explicit_unsupported_sas_case_fails_instead_of_silently_staging(
    name, tmp_path
):
    with pytest.raises(build.BuildError, match=build.SAS_EXCLUSIONS[name]):
        build.build_task(build.BENCHMARKS / name, tmp_path, build.IMAGE, "test", "sas")


def test_every_sas_board_ranks_the_same_confident_set():
    for tier in build.TIERS:
        suffix = "sas" if tier == "full" else f"{tier}-sas"
        board = yaml.safe_load(
            (HARBOR / "leaderboards" / f"sdtm-adam-v3-{suffix}.yaml").read_text()
        )
        assert sorted(board["tasks"]) == sorted(
            build.task_name(n, "sas", tier) for n in CONFIDENT
        )
        assert board["image_reference"] == build.IMAGE
        assert board["package"] == build.dataset_name(build.DATASET_PREFIX, "sas", tier)


@pytest.mark.parametrize(
    "text",
    [
        "proc python; submit; endsubmit; run;",
        "PROC LUA; run;",
        "data x; call system('python3 x.py'); run;",
        "%sysexec Rscript x.R;",
        "filename x pipe 'python3 x.py';",
        "x 'Rscript x.R';",
        "systask command 'python3 x.py';",
    ],
)
def test_sas_language_bridges_fail(tmp_path, text):
    (tmp_path / "result.sas").write_text(text)
    assert not grade.grade_script(
        {"script": "result.sas", "language": "sas"}, tmp_path
    )["passed"]


@pytest.mark.parametrize(
    "text",
    [
        '/* proc python; */ data x; message="proc lua;"; run;',
        "* call system('python3 x.py'); data x; label='x'; run;",
        "%* proc lua; data x; label='proc python'; run;",
        "data record; x=1; x+1; run;",
    ],
)
def test_sas_mentions_are_not_language_calls(tmp_path, text):
    (tmp_path / "result.sas").write_text(text)
    assert grade.grade_script({"script": "result.sas", "language": "sas"}, tmp_path)[
        "passed"
    ]


@pytest.mark.parametrize(
    "command", ["python3 x.py", "env Rscript x.R", "uv run x.py", "lua x.lua"]
)
def test_sas_trajectory_rejects_other_language_commands(command):
    assert grade.other_language_calls(command, "sas")


def test_sas_trajectory_allows_its_interpreter_and_text_mentions():
    assert grade.other_language_calls("sas /app/output/result.sas", "sas") == []
    assert (
        grade.other_language_calls("grep 'python3' /app/output/result.sas", "sas") == []
    )


@pytest.mark.parametrize("name", CONFIDENT)
def test_confident_sas_reference_matches_original_golden(tmp_path, name):
    if not shutil.which("sas"):
        pytest.skip("openSAS is not installed; Harbor Oracle runs every case in Docker")
    benchmark = build.BENCHMARKS / name
    shutil.copytree(
        benchmark / "input",
        tmp_path / "input",
        ignore=shutil.ignore_patterns("*.yaml", "*.yml"),
    )
    output = tmp_path / "output"
    output.mkdir()
    script = output / "result.sas"
    script.write_text(
        (build.SOLUTIONS / name / script.name)
        .read_text()
        .replace("/app/", f"{tmp_path.as_posix()}/")
    )
    subprocess.run(["sas", str(script)], cwd=tmp_path, check=True, capture_output=True)
    result = grade.grade(
        build.contract_for(benchmark, "sas"),
        benchmark / "expected",
        output,
        tmp_path / "none.json",
    )
    assert result["passed"], result


def _edge_case(tmp_path, name, inputs, output_name):
    if not shutil.which("sas"):
        pytest.skip("openSAS is not installed")
    source = tmp_path / "input"
    source.mkdir()
    for file, rows in inputs.items():
        with (source / file).open("w", newline="") as handle:
            csv.writer(handle).writerows(rows)
    output = tmp_path / "output"
    output.mkdir()
    script = output / "result.sas"
    script.write_text(
        (build.SOLUTIONS / name / script.name)
        .read_text()
        .replace("/app/", f"{tmp_path.as_posix()}/")
    )
    subprocess.run(["sas", str(script)], cwd=tmp_path, check=True, capture_output=True)
    with (output / output_name).open(newline="") as handle:
        return list(csv.DictReader(handle))


def test_sas_repeated_exposures_keep_one_subject_and_first_nonmissing_treatment(
    tmp_path,
):
    rows = _edge_case(
        tmp_path,
        "sdtm-dm-arms",
        {
            "rand.csv": [
                ["STUDYID", "USUBJID", "RANDCD", "RAND", "SCRNFL"],
                ["S", "S-1", "PBO", "Placebo", ""],
            ],
            "ex.csv": [
                ["STUDYID", "USUBJID", "EXTRT"],
                ["S", "S-1", ""],
                ["S", "S-1", "PLACEBO"],
                ["S", "S-1", "VITAMIN D3"],
            ],
        },
        "dm.csv",
    )
    assert len(rows) == 1
    assert rows[0]["ACTARMCD"] == "PBO"


def test_sas_missing_lab_results_have_no_toxicity_grade(tmp_path):
    rows = _edge_case(
        tmp_path,
        "sdtm-lb-grading",
        {
            "lb_raw.csv": [
                [
                    "STUDYID",
                    "USUBJID",
                    "LBSEQ",
                    "LBTESTCD",
                    "SEX",
                    "LBSTRESN",
                    "REPROGRADE",
                ],
                ["S", "S-1", 1, "ANC", "M", "", ""],
                ["S", "S-1", 2, "HGB", "F", "", ""],
            ],
        },
        "lb.csv",
    )
    assert len(rows) == 2
    assert all(row["LBTOXGR"] == "" for row in rows)


def test_sas_censors_at_last_assessment_without_a_disposition_reason(tmp_path):
    rows = _edge_case(
        tmp_path,
        "adam-adtte-pro-deterioration",
        {
            "adsl.csv": [
                ["STUDYID", "USUBJID", "RANDDT", "DTHDT"],
                ["S", "S-1", "2026-01-15", ""],
            ],
            "qs.csv": [
                ["STUDYID", "USUBJID", "QSSEQ", "AVISIT", "ADT", "AVAL"],
                ["S", "S-1", 1, "BASELINE", "2026-01-15", 80],
                ["S", "S-1", 2, "WEEK 8", "2026-03-12", 75],
            ],
            "ds.csv": [["STUDYID", "USUBJID", "DSDECOD", "DSDTC", "DSSEQ"]],
        },
        "adtte.csv",
    )
    assert len(rows) == 1
    assert rows[0]["ADT"] == "2026-03-12"
    assert rows[0]["CNSR"] == "1"
    assert rows[0]["CNSDTDSC"] == "STUDY COMPLETION"
    assert rows[0]["SRCSEQ"] == "2"
