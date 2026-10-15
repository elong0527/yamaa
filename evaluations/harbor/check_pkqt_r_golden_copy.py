"""Reject literal golden writers with the actual R grader and changed inputs.

Run after the canonical evaluation image's R packages have been installed.
Each PK/QT case is checked under every prompt tier. The submitted R program
must reproduce the original golden successfully before the held-out and
changed-input checks reject it. A missing runtime or failed rerun is an error.
"""

from __future__ import annotations

import argparse
import json
import shutil
from pathlib import Path

import build
import grade

BENCHMARKS = (
    "adam-adeg-placebo-corrected-change",
    "adam-adeg-replicate-qtc-average",
    "adam-adeg-time-matched-baseline",
    "adam-adeg-time-matched-concentration",
    "adam-adpp-accumulation-ratio",
    "adam-adpp-carryover-exclusion",
    "adam-adpp-comparison-sets",
    "adam-adpp-dose-normalized",
)


def rejection_problems(result: dict) -> list[str]:
    """Require rejection for changed answers, after a valid original rerun."""
    problems = []
    if not result["outputs"] or not all(o["passed"] for o in result["outputs"]):
        problems.append("the literal writer did not match the original golden")
    if not result["script"]["passed"]:
        problems.append("the submitted script was invalid")
    if not result["rerun"]["passed"] or result["rerun"].get("returncode") != 0:
        problems.append("the original-input rerun did not succeed")
    for name in ("held_out", "challenge"):
        check = result[name]
        if not check["checked"] or not check.get("executed"):
            problems.append(f"the {name} check did not execute")
        if check["passed"] or not check["problems"]:
            problems.append(f"the {name} check did not reject the literal writer")
    if result["reward"]["policy_eligible"] != 1.0:
        problems.append("the script was rejected by policy instead of its answers")
    if result["passed"] or result["reward"]["reward"] != 0.0:
        problems.append("the literal golden writer was awarded credit")
    return problems


def check_case(name: str, tier: str, language: str, out: Path, commit: str) -> dict:
    """Build an owned task and run its literal writer against its real reference."""
    benchmark = build.BENCHMARKS / name
    task = build.build_task(
        benchmark, out / "tasks" / tier, None, commit, language, tier=tier
    )
    contract = json.loads((task / "tests/contract.json").read_text())
    if not contract["challenge_required"]:
        raise RuntimeError(f"{name}: changed-input challenges must remain required")
    directory = out / "checks" / name / tier
    app = directory / "app"
    shutil.copytree(task / "tests/input", app / "input")
    output = app / "output"
    output.mkdir()
    script = contract["script"]
    reference = directory / "reference" / script
    reference.parent.mkdir()
    reference.write_text(
        (task / "tests/reference" / script)
        .read_text()
        .replace("/app/", f"{app.as_posix()}/"),
        encoding="utf-8",
    )
    literal = build.oracle_script(
        language, [task / "tests/expected" / o["file"] for o in contract["outputs"]]
    )
    submitted = output / script
    submitted.write_text(
        literal.replace("/app/", f"{app.as_posix()}/"), encoding="utf-8"
    )
    code, log = grade.run_script(
        [grade.INTERPRETERS[language], str(submitted)], app, grade.RERUN_TIMEOUT_SEC
    )
    (directory / "initial-run.json").write_text(
        json.dumps({"returncode": code, "log": log}, indent=2) + "\n"
    )
    if code != 0:
        raise RuntimeError(f"{name}/{tier}: literal writer failed: {log}")
    result = grade.grade(
        contract,
        task / "tests/expected",
        output,
        directory / "absent-trajectory.json",
        rerun=True,
        reference=reference,
    )
    grade.write_results(result, directory)
    problems = rejection_problems(result)
    return {
        "benchmark": name,
        "language": language,
        "tier": tier,
        "reward": result["reward"]["reward"],
        "held_out_checked": result["held_out"]["checked"],
        "challenge_checked": result["challenge"]["checked"],
        "passed": not problems,
        "problems": problems,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if shutil.which("Rscript") is None:
        parser.error("Rscript is required; these controls must not be skipped")
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    commit = build.git_commit()
    results = []
    for name in BENCHMARKS:
        for tier in build.TIERS:
            try:
                result = check_case(name, tier, "r", args.out, commit)
            except Exception as error:  # noqa: BLE001 - retain every selected failure
                result = {
                    "benchmark": name,
                    "language": "r",
                    "tier": tier,
                    "passed": False,
                    "problems": [str(error)],
                }
            results.append(result)
            print(json.dumps(result), flush=True)
    failures = [r for r in results if not r["passed"]]
    (args.out / "summary.json").write_text(
        json.dumps(
            {"source_commit": commit, "controls": results, "failures": failures},
            indent=2,
        )
        + "\n"
    )
    if failures:
        raise SystemExit(f"{len(failures)} of {len(results)} R controls failed")
    print(f"All {len(results)} R literal-golden rejection controls passed")


if __name__ == "__main__":
    main()
