"""Run offline oracle/nop checks through Harbor's real Docker verifier.

Randomly sample ten buildable tasks, shared by oracle and nop. Save the
sample and seed so the check can be reproduced. No model API or key is used.
"""

from __future__ import annotations

import argparse
import json
import random
import secrets
import subprocess
import sys
from pathlib import Path

import build
import tomllib

PREFLIGHT_TRIALS = 10


def sample_tasks(tasks: list[Path], seed: int) -> list[Path]:
    """Use the same reproducible sample, without replacement, for both agents."""
    candidates = sorted(tasks)
    return sorted(
        random.Random(seed).sample(candidates, min(PREFLIGHT_TRIALS, len(candidates)))
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    # Older launch scripts pass --full. It now uses the same ten-task sample.
    parser.add_argument("--full", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--seed", type=int, help="repeat a saved preflight sample")
    parser.add_argument("--image", default=build.IMAGE)
    parser.add_argument(
        "--prompt", nargs="+", choices=list(build.TIERS), default=["full"]
    )
    parser.add_argument("--n-concurrent", type=int, default=6)
    args = parser.parse_args()
    names = sorted(p.parent.name for p in build.PROMPTS.glob("*/full.md"))
    tasks, skipped = build.build_selection(
        names,
        languages=["r", "python"],
        tasks_dir=args.out / "tasks",
        image=args.image,
        commit=build.git_commit(),
        strict=False,
        tiers=tuple(args.prompt),
    )
    for note in skipped:
        print(f"skip {note}")
    probe_task = next(
        t
        for t in tasks
        if t.name.startswith("adam-adsl-age-group-") and t.name.endswith("-python")
    )
    seed = args.seed if args.seed is not None else secrets.randbits(64)
    candidates = tasks
    tasks = sample_tasks(candidates, seed)
    (args.out / "sample.json").write_text(
        json.dumps(
            {
                "seed": seed,
                "candidate_tasks": len(candidates),
                "tasks": [t.name for t in tasks],
                "agents": ["oracle", "nop"],
                "prompt_tiers": args.prompt,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"sampled {len(tasks)} of {len(candidates)} tasks (seed {seed})")
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
        config_path = args.out / f"{agent}.json"
        config_path.write_text(
            json.dumps(
                {
                    "jobs_dir": str(args.out / "jobs"),
                    "job_name": agent,
                    "n_concurrent_trials": args.n_concurrent,
                    "environment": {"type": "docker"},
                    "agents": [{"name": agent}],
                    "tasks": [{"path": str(t)} for t in tasks],
                }
            )
        )
        subprocess.run(
            [
                sys.executable,
                str(build.HERE / "run.py"),
                "-c",
                str(config_path),
            ],
            check=True,
        )
        job_dir = args.out / "jobs" / agent
        manifest = json.loads((job_dir / "evaluation.json").read_text())["tasks"]
        results = list(job_dir.glob("*/result.json"))
        if len(results) != len(tasks):
            raise RuntimeError(f"{agent}: {len(results)} trials for {len(tasks)} tasks")
        for path in results:
            result = json.loads(path.read_text())
            evidence = json.loads(
                (path.parent / "verifier/evaluation.json").read_text()
            )
            snapshot = tomllib.loads((path.parent / "verifier/task.toml").read_text())
            if (
                evidence["task"] != manifest[result["task_name"]]
                or set(evidence["expected_tasks"]) != set(manifest)
                or evidence["n_attempts"] != 1
                or snapshot != evidence["task"]["task"]
            ):
                raise RuntimeError(f"{path}: pre-execution evidence was not retained")
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
