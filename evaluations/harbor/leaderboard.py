"""Turn Harbor jobs into Harbor Hub leaderboard rows.

    python evaluations/harbor/leaderboard.py export <leaderboard> \\
        <job-dir> [<job-dir> ...]

The leaderboards follow Harbor Hub's curated-leaderboard model. Each
`leaderboards/<name>.yaml` names its Hub dataset package, the tasks and
attempts a run must cover and, under `harbor:`, the Hub definition: the
metadata and metrics every row carries, the columns, and the ordered
`rank_by` rules. One row is one job: one agent, model, and variant.

`export` reads each Harbor job directory, refuses a job that does not cover
the leaderboard's tasks at their own timeouts, and writes the configs
`harbor hub leaderboard create` and `harbor hub leaderboard row create` take.
A row's metrics aggregate its trials as Harbor does: the mean counts an
errored trial as 0, and pass@k is Harbor's unbiased estimator averaged over
tasks. Results live on Harbor Hub, not in this repository.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import Counter, defaultdict
from datetime import UTC, datetime
from pathlib import Path

import tomllib

import yaml

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
LEADERBOARDS = HERE / "leaderboards"
EXPORT = (
    Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache")
    / "yamaa-harbor"
    / "hub"
)
PHASE_MULTIPLIERS = (
    "agent_timeout_multiplier",
    "verifier_timeout_multiplier",
    "agent_setup_timeout_multiplier",
    "environment_build_timeout_multiplier",
)


def _seconds(phase: dict | None) -> float | None:
    if not phase or not phase.get("started_at") or not phase.get("finished_at"):
        return None
    start = datetime.fromisoformat(phase["started_at"])
    return (datetime.fromisoformat(phase["finished_at"]) - start).total_seconds()


def _read_json(path: Path) -> dict:
    return json.loads(path.read_text()) if path.is_file() else {}


def _default_timeouts(config: dict) -> bool:
    return config.get("timeout_multiplier", 1.0) == 1.0 and all(
        config.get(name) in (None, 1.0) for name in PHASE_MULTIPLIERS
    )


def _task_commit(config: dict) -> str | None:
    """The yamaa commit a trial's task was built from (`task.toml`
    metadata), shortened; `None` when the task directory is gone."""
    path = Path((config.get("task") or {}).get("path") or "") / "task.toml"
    if not path.is_file():
        return None
    commit = tomllib.loads(path.read_text()).get("metadata", {}).get("yamaa_commit")
    if not commit:
        return None
    sha, _, dirty = commit.partition("+")
    return sha[:8] + (f"+{dirty}" if dirty else "")


def collect(job_dir: Path) -> dict:
    """The facts of one Harbor job that a leaderboard row is built from."""
    trials = []
    agents = set()
    default_timeouts = _default_timeouts(_read_json(job_dir / "config.json"))
    started, commits = [], set()
    for path in sorted(job_dir.glob("*/result.json")):
        result = json.loads(path.read_text())
        config = result.get("config") or {}
        if result.get("started_at"):
            started.append(datetime.fromisoformat(result["started_at"]))
        commits.add(_task_commit(config))
        info = result.get("agent_info") or {}
        agent_config = config.get("agent") or {}
        model = agent_config.get("model_name")
        variant = (agent_config.get("kwargs") or {}).get("variant")
        agents.add((info.get("name"), info.get("version"), model, variant))
        default_timeouts = default_timeouts and _default_timeouts(config)
        rewards = (result.get("verifier_result") or {}).get("rewards") or {}
        usage = result.get("agent_result") or {}
        error = result.get("exception_info") or {}
        trials.append(
            {
                "id": result.get("id"),
                "name": result.get("trial_name"),
                "benchmark": result["task_name"].rpartition("/")[2],
                "reward": float(rewards.get("reward", 0.0)),
                "cell_accuracy": float(rewards.get("cell_accuracy", 0.0)),
                "row_accuracy": float(rewards.get("row_accuracy", 0.0)),
                "web_tool_calls": float(rewards.get("web_tool_calls", 0.0)),
                "input_tokens": usage.get("n_input_tokens"),
                "output_tokens": usage.get("n_output_tokens"),
                "cost_usd": usage.get("cost_usd"),
                "agent_seconds": _seconds(result.get("agent_execution")),
                "error": error.get("exception_type"),
            }
        )
    if not trials:
        raise SystemExit(f"{job_dir} holds no trial results")
    if len(agents) != 1:
        raise SystemExit(
            f"{job_dir} mixes agents, models, or variants: {sorted(agents, key=str)}"
        )
    ((agent, version, model, variant),) = agents
    job = json.loads((job_dir / "result.json").read_text())
    attempts = _read_json(job_dir / "config.json").get("n_attempts")
    lock = _read_json(job_dir / "lock.json")
    return {
        "job": job_dir.name,
        "job_id": job.get("id"),
        # Trial times are UTC; the job's own `started_at` is local time.
        "date": min(started).astimezone(UTC).date().isoformat()
        if started
        else job["started_at"][:10],
        "yamaa_commit": ", ".join(sorted(c or "unknown" for c in commits)),
        "harbor_version": (lock.get("harbor") or {}).get("version"),
        "agent": agent,
        "agent_version": version,
        "model": model,
        # The agent's model variant (OpenCode's reasoning effort), or
        # "default" when the job set none.
        "variant": variant or "default",
        "attempts": attempts or min(Counter(t["benchmark"] for t in trials).values()),
        "default_timeouts": default_timeouts,
        "status": "display",
        "leaderboards": [],
        "trials": sorted(trials, key=lambda t: (t["benchmark"], t["name"] or "")),
    }


def _declared(schema: dict, key: str) -> bool:
    return key in schema.get("properties", {}) or any(
        re.fullmatch(pattern, key) for pattern in schema.get("patternProperties", {})
    )


def board_problems(board: dict) -> list[str]:
    """What keeps a leaderboard definition from exporting."""
    harbor = board["harbor"]
    schemas = {
        "metadata": harbor["metadata_schema"],
        "metrics": harbor["metrics_schema"],
    }
    found = []
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", board.get("package") or ""):
        found.append("needs the Hub dataset package as package: <org>/<name>")
    if not board["tasks"] or board["attempts"] < 1:
        found.append("needs at least one task and one attempt")
    for accessor in [c["accessor"] for c in harbor["columns"]] + [
        r["accessor"] for r in harbor["rank_by"]
    ]:
        root, _, key = accessor.partition(".")
        if not _declared(schemas[root], key):
            found.append(f"{accessor} is not in the {root} schema")
    return found


def load_leaderboards() -> list[dict]:
    boards = []
    for path in sorted(LEADERBOARDS.glob("*.yaml")):
        board = yaml.safe_load(path.read_text(encoding="utf-8"))
        found = board_problems(board)
        if board["harbor"]["name"] != path.stem:
            found.append(f"harbor.name must be {path.stem!r}")
        if found:
            raise SystemExit(f"{path.name}: " + "; ".join(found))
        boards.append(board)
    return boards


def run_problems(run: dict, board: dict) -> list[str]:
    """Why a run cannot be a row of a leaderboard; empty when it can."""
    counts = Counter(t["benchmark"] for t in run["trials"])
    found = [
        f"{task} has {counts[task]} of {board['attempts']} attempts"
        for task in board["tasks"]
        if counts[task] < board["attempts"]
    ]
    if not run["default_timeouts"]:
        found.append("the job changed the tasks' timeouts")
    return found


def _eligible_k(fewest: int) -> list[int]:
    ks = set()
    k = 2
    while k <= fewest:
        ks.add(k)
        k *= 2
    k = 5
    while k <= fewest:
        ks.add(k)
        k += 5
    return sorted(ks)


def _pass_at_k_for_task(n: int, c: int, k: int) -> float:
    if n - c < k:
        return 1.0
    product = 1.0
    for i in range(k):
        product *= (n - c - i) / (n - i)
    return 1.0 - product


def pass_at_k(trials: list[dict]) -> dict[int, float]:
    """Harbor's pass@k: the unbiased estimate per task, averaged over tasks.

    Harbor skips pass@k when a verifier writes several rewards, as grade.py
    does, so it is computed here from `reward` alone, with Harbor's k values:
    2, 4, 8, ... and 5, 10, ... up to the fewest attempts on any task.
    """
    successes: dict[str, list[int]] = defaultdict(list)
    for trial in trials:
        if trial["reward"] not in (0.0, 1.0):
            return {}
        successes[trial["benchmark"]].append(int(trial["reward"]))
    if not successes:
        return {}
    fewest = min(len(s) for s in successes.values())
    return {
        k: sum(_pass_at_k_for_task(len(s), sum(s), k) for s in successes.values())
        / len(successes)
        for k in _eligible_k(fewest)
    }


def _mean(values: list[float]) -> float:
    return sum(values) / len(values)


def _total(trials: list[dict], field: str) -> float | None:
    values = [t[field] for t in trials]
    return None if any(v is None for v in values) else sum(values)


def metrics(trials: list[dict]) -> dict:
    """Harbor's aggregation of the trials' rewards, plus usage totals."""
    seconds = [t["agent_seconds"] for t in trials]
    result = {
        "reward": _mean([t["reward"] for t in trials]),
        "cell_accuracy": _mean([t["cell_accuracy"] for t in trials]),
        "row_accuracy": _mean([t["row_accuracy"] for t in trials]),
        "n_trials": len(trials),
        "n_errors": sum(t["error"] is not None for t in trials),
        "input_tokens": _total(trials, "input_tokens"),
        "output_tokens": _total(trials, "output_tokens"),
        "cost_usd": _total(trials, "cost_usd"),
        "agent_seconds": None if None in seconds else _mean(seconds),
    }
    for k, value in pass_at_k(trials).items():
        result[f"pass_at_{k}"] = value
    return result


def rows_for(board: dict, runs: list[dict]) -> list[dict]:
    """The leaderboard's rows, one per run that names it, in Hub row shape
    plus the trials they aggregate."""
    name = board["harbor"]["name"]
    tasks = set(board["tasks"])
    rows, seen = [], set()
    for run in runs:
        if name not in run["leaderboards"]:
            continue
        found = run_problems(run, board)
        if found:
            raise SystemExit(f"{run['job']} on {name}: " + "; ".join(found))
        trials = [t for t in run["trials"] if t["benchmark"] in tasks]
        trial_ids = [t["id"] for t in trials if t["id"]]
        if seen.intersection(trial_ids):
            raise SystemExit(f"{run['job']} on {name}: a trial is on two rows")
        seen.update(trial_ids)
        rows.append(
            {
                "metadata": {
                    "agent": run["agent"],
                    "agent_version": run["agent_version"],
                    "model": run["model"],
                    "variant": run["variant"],
                    "date": run["date"],
                    "attempts": run["attempts"],
                    "job": run["job"],
                    "job_id": run["job_id"],
                    "harbor_version": run["harbor_version"],
                    "yamaa_commit": run.get("yamaa_commit"),
                },
                "metrics": metrics(trials),
                "status": run["status"],
                "trial_ids": trial_ids,
                "trials": trials,
            }
        )
    return rows


def export(board: dict, runs: list[dict], package: str, out: Path) -> list[Path]:
    """Write the Hub create configs for one leaderboard and its rows."""
    name = board["harbor"]["name"]
    out.mkdir(parents=True, exist_ok=True)
    definition = out / f"{name}.leaderboard.yaml"
    definition.write_text(
        yaml.safe_dump({"package": package, **board["harbor"]}, sort_keys=False)
    )
    written = [definition]
    # Harbor Hub ranks rows by the board's `rank_by`; they go up in run order.
    rows = rows_for(board, runs)
    if rows:
        hub_rows = [
            {key: row[key] for key in ("metadata", "metrics", "status", "trial_ids")}
            for row in rows
        ]
        rows_path = out / f"{name}.rows.yaml"
        rows_path.write_text(yaml.safe_dump({"rows": hub_rows}, sort_keys=False))
        written.append(rows_path)
    return written


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    hub = commands.add_parser("export", help="write Harbor Hub create configs")
    hub.add_argument("leaderboard")
    hub.add_argument("job_dirs", nargs="+", type=Path, help="Harbor job directories")
    hub.add_argument(
        "--package", help="Hub dataset package org/name; default: the board's"
    )
    hub.add_argument("--hide", action="store_true", help="export the rows hidden")
    hub.add_argument("--out", type=Path, default=EXPORT)
    args = parser.parse_args()

    by_name = {b["harbor"]["name"]: b for b in load_leaderboards()}
    if args.leaderboard not in by_name:
        sys.exit(f"no leaderboard {args.leaderboard!r}; have {sorted(by_name)}")
    board = by_name[args.leaderboard]
    package = args.package or board["package"]
    runs = []
    for job_dir in args.job_dirs:
        run = collect(job_dir)
        problems = run_problems(run, board)
        if problems:
            sys.exit(f"{run['job']} cannot be ranked: {'; '.join(problems)}")
        run["leaderboards"] = [args.leaderboard]
        run["status"] = "hide" if args.hide else "display"
        runs.append(run)
    paths = export(board, runs, package, args.out)
    for row in rows_for(board, runs):
        m = row["metrics"]
        print(
            f"{row['metadata']['job']}: {row['metadata']['model']} "
            f"({row['metadata']['variant']}) "
            f"reward {m['reward']:.3f}, cells {m['cell_accuracy']:.3f}, "
            f"{m['n_trials']} trial(s), tasks from {row['metadata']['yamaa_commit']}"
        )
    for path in paths:
        print(f"wrote {path}")
    ref = f"{package}/{args.leaderboard}"
    print("upload each job first: harbor upload <job-dir> --org <org> --private")
    print(f"then: harbor hub leaderboard create --config {paths[0]}")
    if len(paths) > 1:
        print(f"then: harbor hub leaderboard row create {ref} --config {paths[1]}")


if __name__ == "__main__":
    main()
