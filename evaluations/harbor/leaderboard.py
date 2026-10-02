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
import copy
import json
import os
import random
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
    return (
        config.get("timeout_multiplier", 1.0) == 1.0
        and all(config.get(name) in (None, 1.0) for name in PHASE_MULTIPLIERS)
        and all(
            (config.get(phase) or {}).get(key) is None
            for phase in ("agent", "verifier")
            for key in ("override_timeout_sec", "max_timeout_sec")
        )
    )


def _default_resources(config: dict) -> bool:
    environment = config.get("environment") or {}
    return (
        not any(
            value is not None and key.startswith("override_")
            for key, value in environment.items()
        )
        and not any(environment.get(key) for key in ("mounts", "extra_docker_compose"))
        and not environment.get("import_path")
        and not environment.get("kwargs")
        and all(
            environment.get(key, "auto") in ("auto", "limit")
            for key in ("cpu_enforcement_policy", "memory_enforcement_policy")
        )
    )


def _default_network(config: dict) -> bool:
    agent = config.get("agent") or {}
    hosts = agent.get("extra_allowed_hosts") or []
    setup = {
        "raw.githubusercontent.com",
        "github.com",
        "nodejs.org",
        "registry.npmjs.org",
    }
    baseline = (config.get("environment") or {}).get("extra_allowed_hosts") or []
    return (
        len(hosts) <= 1
        and set(baseline) <= setup | set(hosts)
        and not agent.get("skills")
    )


def _default_verifier(config: dict) -> bool:
    verifier = config.get("verifier") or {}
    return not any(verifier.get(k) for k in ("disable", "import_path", "kwargs"))


def _task_metadata(config: dict, trial: Path) -> dict:
    """The `task.toml` metadata of a trial's task. The verifier saves the file
    with its results. A trial that never reached verification uses the
    job's pre-execution snapshot, never a mutable task directory."""
    snapshot = trial / "verifier" / "task.toml"
    if snapshot.is_file():
        return tomllib.loads(snapshot.read_text()).get("metadata", {})
    saved = _read_json(trial.parent / "evaluation.json").get("tasks", {})
    name = _read_json(trial / "result.json").get("task_name")
    entry = (
        saved.get(name)
        or _read_json(trial / "verifier" / "evaluation.json").get("task")
        or {}
    )
    if entry.get("path") != (config.get("task") or {}).get("path"):
        return {}
    return (entry.get("task") or {}).get("metadata", {})


def agent_configuration(config: dict) -> dict:
    """Effective behavior settings, with credentials excluded from evidence."""

    def redact(value):
        if isinstance(value, str) and value.lstrip().startswith(("{", "[")):
            try:
                decoded = json.loads(value)
            except ValueError:
                pass
            else:
                return json.dumps(redact(decoded), sort_keys=True)
        if isinstance(value, list):
            return [redact(v) for v in value]
        if not isinstance(value, dict):
            return value
        result = {}
        for key, item in value.items():
            normalized = re.sub(r"[_-]", "", key).lower()
            secret = normalized in {
                "token",
                "accesstoken",
                "refreshtoken",
                "authorization",
                "cookie",
                "secret",
            } or normalized.endswith(("apikey", "password", "secretkey"))
            result[key] = "<redacted>" if secret else redact(item)
        return result

    agent = config.get("agent") or {}
    return redact(
        {
            "kwargs": agent.get("kwargs") or {},
            "import_path": agent.get("import_path"),
            "skills": agent.get("skills") or [],
            "extra_allowed_hosts": sorted(agent.get("extra_allowed_hosts") or []),
            "env": {
                k: v
                for k, v in (agent.get("env") or {}).items()
                if k.startswith("OPENCODE_")
            },
            "extra_instructions": config.get("extra_instructions") or [],
            "extra_instruction_paths": config.get("extra_instruction_paths") or [],
        }
    )


def _task_commit(config: dict, trial: Path) -> str | None:
    """The yamaa commit a trial's task was built from (`task.toml`
    metadata), preserved in the verifier logs; `None` when unavailable."""
    commit = _task_metadata(config, trial).get("yamaa_commit")
    if not commit:
        return None
    return commit


def collect(job_dir: Path) -> dict:
    """The facts of one Harbor job that a leaderboard row is built from."""
    trials = []
    protocols, images = set(), set()
    default_resources = True
    default_network = True
    default_verifier = True
    evidence_problems = []
    agents = set()
    configurations = set()
    task_names = set()
    job_config = _read_json(job_dir / "config.json")
    preserved = _read_json(job_dir / "evaluation.json").get("tasks", {})
    expected_tasks = set(preserved)
    default_timeouts = _default_timeouts(job_config)
    started, commits = [], set()
    for path in sorted(job_dir.glob("*/result.json")):
        result = json.loads(path.read_text())
        task_names.add(result["task_name"])
        config = result.get("config") or {}
        if result.get("started_at"):
            started.append(datetime.fromisoformat(result["started_at"]))
        metadata = _task_metadata(config, path.parent)
        commits.add(_task_commit(config, path.parent))
        protocols.add(metadata.get("grading_protocol"))
        images.add(metadata.get("image_reference"))
        default_resources = default_resources and _default_resources(config)
        default_network = default_network and _default_network(config)
        default_verifier = default_verifier and _default_verifier(config)
        info = result.get("agent_info") or {}
        agent_config = config.get("agent") or {}
        model = agent_config.get("model_name")
        variant = (agent_config.get("kwargs") or {}).get("variant")
        agents.add((info.get("name"), info.get("version"), model, variant))
        configurations.add(
            json.dumps(
                agent_configuration(config), sort_keys=True, separators=(",", ":")
            )
        )
        archived = _read_json(path.parent / "verifier" / "evaluation.json")
        entry = preserved.get(result["task_name"]) or archived.get("task") or {}
        recorded_tasks = set(archived.get("expected_tasks") or [])
        if recorded_tasks:
            if expected_tasks and recorded_tasks != expected_tasks:
                evidence_problems.append("the job mixes pre-execution task manifests")
            expected_tasks |= recorded_tasks
        if archived and archived.get("n_attempts") != job_config.get("n_attempts", 1):
            evidence_problems.append("the job changed its pre-execution attempt count")
        if entry.get("path") != (config.get("task") or {}).get("path"):
            evidence_problems.append(
                f"{result.get('trial_name')}: missing pre-execution task evidence"
            )
        snapshot = path.parent / "verifier" / "task.toml"
        if (
            snapshot.is_file()
            and entry
            and tomllib.loads(snapshot.read_text()) != entry.get("task")
        ):
            evidence_problems.append(
                f"{result.get('trial_name')}: task evidence changed during execution"
            )
        default_timeouts = default_timeouts and _default_timeouts(config)
        rewards = (result.get("verifier_result") or {}).get("rewards") or {}
        usage = result.get("agent_result") or {}
        error = result.get("exception_info") or {}
        evidence = _read_json(path.parent / "verifier" / "grade.json")
        if not error and (
            not evidence or not (evidence.get("network") or {}).get("checked")
        ):
            evidence_problems.append(
                f"{result.get('trial_name')}: missing grading or trajectory evidence"
            )
        if (
            not error
            and (rewards.get("reward") == 1 or rewards.get("cell_accuracy", 0) > 0)
            and metadata.get("challenge_required")
            and (not (evidence.get("challenge") or {}).get("checked"))
        ):
            evidence_problems.append(
                f"{result.get('trial_name')}: required challenge was not checked"
            )
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
    if len(configurations) != 1:
        evidence_problems.append("the job mixes effective agent configurations")
    ((agent, version, model, variant),) = agents
    job = json.loads((job_dir / "result.json").read_text())
    attempts = job_config.get("n_attempts", 1)
    stats = job.get("stats") or {}
    complete = (
        bool(job_config)
        and bool(job.get("finished_at"))
        and task_names == expected_tasks
        and isinstance(attempts, int)
        and len(trials) == len(expected_tasks) * attempts
        and job.get("n_total_trials") == len(trials)
        and stats.get("n_completed_trials") == len(trials)
        and not any(
            stats.get(k, 0)
            for k in (
                "n_pending_trials",
                "n_running_trials",
                "n_cancelled_trials",
                "n_retries",
            )
        )
    )
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
        "attempts": attempts,
        "complete": complete,
        "agent_configuration": json.loads(next(iter(configurations))),
        "default_timeouts": default_timeouts,
        "default_resources": default_resources,
        "default_network": default_network,
        "default_verifier": default_verifier,
        "grading_protocols": protocols,
        "image_references": images,
        "evidence_problems": evidence_problems,
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
    if not run.get("complete"):
        found.append(
            "the job must preserve every completed attempt, without retries or cancellation"
        )
    if (
        not isinstance(run.get("attempts"), int)
        or run["attempts"] < 1
        or any(count != run["attempts"] for count in counts.values())
    ):
        found.append("trial counts must match the job's declared attempts")
    if not run["default_timeouts"]:
        found.append("the job changed the tasks' timeouts")
    if not run.get("default_resources", True):
        found.append("the job changed the tasks' resources or mounted extra files")
    if not run.get("default_network", True):
        found.append("the job expanded network access or supplied extra skills")
    if not run.get("default_verifier", True):
        found.append("the job replaced or disabled the task verifier")
    if len({counts[t] for t in board["tasks"] if counts[t]}) > 1:
        found.append("attempt counts must be equal on every board task")
    if board.get("grading_protocol") and run.get("grading_protocols") != {
        board["grading_protocol"]
    }:
        found.append("the job does not use this board's grading protocol")
    if (
        run.get("yamaa_commit") is None
        or "unknown" in run.get("yamaa_commit", "")
        or "," in run.get("yamaa_commit", "")
    ):
        found.append("the job needs one preserved task commit")
    if "+dirty" in run.get("yamaa_commit", ""):
        found.append("ranked tasks must be built from a committed tree")
    if len(run.get("image_references", set())) != 1 or None in run.get(
        "image_references", set()
    ):
        found.append("the job needs one preserved image reference")
    if board.get("image_reference") and run.get("image_references") != {
        board["image_reference"]
    }:
        found.append("the job does not use this board's runtime image reference")
    found.extend(run.get("evidence_problems", []))
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
    """Task-weighted rewards, a task bootstrap interval, and usage totals."""
    seconds = [t["agent_seconds"] for t in trials]
    by_task = defaultdict(list)
    for trial in trials:
        by_task[trial["benchmark"]].append(trial["reward"])
    task_rates = [_mean(values) for values in by_task.values()]
    randomizer = random.Random(0)
    samples = (
        sorted(
            _mean(randomizer.choices(task_rates, k=len(task_rates)))
            for _ in range(2000)
        )
        if len(task_rates) > 1
        else []
    )
    cost = _total(trials, "cost_usd")
    pass_rate = _mean(task_rates)
    passed = sum(t["reward"] == 1.0 for t in trials)
    result = {
        "reward": pass_rate,
        "task_pass_rate_display": f"{pass_rate:.1%}",
        "n_passed_trials": passed,
        "passed_trials_display": f"{passed}/{len(trials)}",
        "trial_reward": _mean([t["reward"] for t in trials]),
        "reward_ci_low": samples[49] if samples else None,
        "reward_ci_high": samples[1949] if samples else None,
        "cell_accuracy": _mean([t["cell_accuracy"] for t in trials]),
        "row_accuracy": _mean([t["row_accuracy"] for t in trials]),
        "n_trials": len(trials),
        "n_errors": sum(t["error"] is not None for t in trials),
        "input_tokens": _total(trials, "input_tokens"),
        "output_tokens": _total(trials, "output_tokens"),
        "cost_usd": cost,
        "cost_per_trial_usd": cost / len(trials) if cost is not None else None,
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
    commits = {run.get("yamaa_commit") for run in runs if name in run["leaderboards"]}
    if len(commits) > 1:
        raise SystemExit(f"{name}: compared runs must use the same task commit")
    for run in runs:
        if name not in run["leaderboards"]:
            continue
        found = run_problems(run, board)
        if found:
            raise SystemExit(f"{run['job']} on {name}: " + "; ".join(found))
        trials = [t for t in run["trials"] if t["benchmark"] in tasks]
        trial_ids = [t["id"] for t in trials if t["id"]]
        if len(trial_ids) != len(trials) or len(set(trial_ids)) != len(trial_ids):
            raise SystemExit(
                f"{run['job']} on {name}: trials need unique preserved ids"
            )
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
                    "agent_configuration": run["agent_configuration"],
                    "date": run["date"],
                    "attempts": min(Counter(t["benchmark"] for t in trials).values()),
                    "job": run["job"],
                    "job_id": run["job_id"],
                    "job_url": f"https://hub.harborframework.com/jobs/{run['job_id']}",
                    "harbor_version": run["harbor_version"],
                    "yamaa_commit": run.get("yamaa_commit"),
                    "grading_protocol": next(iter(run["grading_protocols"])),
                    "image_reference": next(iter(run["image_references"])),
                },
                "metrics": metrics(trials),
                "status": run["status"],
                "trial_ids": trial_ids,
                "trials": trials,
            }
        )
    return rows


def definition_for(board: dict, rows: list[dict], package: str) -> dict:
    """Pin a leaderboard definition to the task release and runtime of its rows."""
    release = copy.deepcopy(board["harbor"])
    if rows:
        commit = rows[0]["metadata"]["yamaa_commit"]
        release["name"] += f"-{commit}"
        release["metadata_schema"]["properties"]["yamaa_commit"]["const"] = commit
        release["metadata_schema"]["properties"]["image_reference"]["const"] = rows[0][
            "metadata"
        ]["image_reference"]
    return {"package": package, **release}


def export(board: dict, runs: list[dict], package: str, out: Path) -> list[Path]:
    """Write the Hub create configs for one leaderboard and its rows."""
    name = board["harbor"]["name"]
    out.mkdir(parents=True, exist_ok=True)
    definition = out / f"{name}.leaderboard.yaml"
    written = [definition]
    rows = rows_for(board, runs)
    definition.write_text(
        yaml.safe_dump(definition_for(board, rows, package), sort_keys=False)
    )
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
    ref = f"{package}/{yaml.safe_load(paths[0].read_text())['name']}"
    print("upload each job first: harbor upload <job-dir> --org <org> --private")
    print(f"then: harbor hub leaderboard create --config {paths[0]}")
    if len(paths) > 1:
        print(f"then: harbor hub leaderboard row create {ref} --config {paths[1]}")


if __name__ == "__main__":
    main()
