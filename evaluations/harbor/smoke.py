"""Run offline oracle/nop checks through Harbor's real Docker verifier.

Use --full for every buildable benchmark; the default covers the three
development tasks in both languages. No model API or key is used.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

import build


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--full", action="store_true")
    parser.add_argument("--image", default=build.IMAGE)
    args = parser.parse_args()
    names = (
        sorted(p.parent.name for p in build.BENCHMARKS.glob("*/prompt.md"))
        if args.full
        else ["adam-adsl-age-group", "adam-adae-death", "adam-adtte-dor"]
    )
    tasks, skipped = build.build_selection(
        names,
        languages=["r", "python"],
        tasks_dir=args.out / "tasks",
        image=args.image,
        commit=build.git_commit(),
        strict=not args.full,
    )
    for note in skipped:
        print(f"skip {note}")
    probe_task = next(t for t in tasks if t.name == "adam-adsl-age-group-python")
    probe_image = "yamaa-harbor-sandbox-probe:local"
    try:
        subprocess.run(
            ["docker", "build", "-q", "-t", probe_image, str(probe_task / "tests")],
            check=True,
        )
        subprocess.run(
            [
                "docker",
                "run",
                "--rm",
                "--network",
                "none",
                "--mount",
                f"type=bind,source={build.HERE / 'probe.py'},target=/tests/probe.py,readonly",
                probe_image,
                "python3",
                "/tests/probe.py",
            ],
            check=True,
        )
    finally:
        subprocess.run(
            ["docker", "image", "rm", probe_image],
            check=False,
            stdout=subprocess.DEVNULL,
        )
    for agent, reward in (("oracle", 1.0), ("nop", 0.0)):
        subprocess.run(
            [
                sys.executable,
                "-c",
                "from harbor.cli.main import app; app()",
                "run",
                "-p",
                str(args.out / "tasks"),
                "-a",
                agent,
                "-o",
                str(args.out / "jobs"),
                "--job-name",
                agent,
                "-n",
                "4",
                "-y",
            ],
            check=True,
        )
        results = list((args.out / "jobs" / agent).glob("*/result.json"))
        if len(results) != len(tasks):
            raise RuntimeError(f"{agent}: {len(results)} trials for {len(tasks)} tasks")
        for path in results:
            result = json.loads(path.read_text())
            rewards = (result.get("verifier_result") or {}).get("rewards") or {}
            if result.get("exception_info") or rewards.get("reward") != reward:
                raise RuntimeError(
                    f"{agent} failed: {path}: {result.get('exception_info')}, {rewards}"
                )
            if agent == "oracle":
                contract = json.loads(
                    (
                        Path(result["config"]["task"]["path"]) / "tests/contract.json"
                    ).read_text()
                )
                if (
                    contract["challenge_required"]
                    and rewards.get("challenge_checked") != 1.0
                ):
                    raise RuntimeError(f"{path}: required challenges were skipped")
    print(f"oracle/nop passed for {len(tasks)} tasks")


if __name__ == "__main__":
    main()
